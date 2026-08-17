//! 多 Session 生命周期与 RPC 连接订阅管理。
//!
//! 本模块是 Rust 端的 `LiveSessionManager` 等价物，但不拥有 socket、Provider 或
//! Agent Loop。它只维护连接与 Session 的附着关系，并通过 [`ManagedSession`] 和
//! [`SessionFactory`] 两个端口调用具体运行时。因此 [`rpc`](../../rpc/src/lib.rs)
//! 仍只依赖共享协议，Runtime 可以替换为内存实现、AgentSession 适配器或持久化实现。

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use protocol::{
    Command, CommandResult, ModelMetadata, ModelRef, ProtocolError, ProtocolErrorCode, ServerEvent,
    ServerSnapshot, SessionMetadata, SessionPhase, SessionSnapshot, ThinkingLevel,
};
use rpc::RpcCommandHandler;

use crate::{
    SessionRuntime,
    agent_session::{
        AgentSession, AgentSessionConfig, JsonlSessionPersistence, PersistenceEvent,
        SessionPersistence,
    },
    provider_ipc::ProviderHostStreamPort,
    provider_runtime::{ProviderStreamDriver, ProviderStreamPort},
};
use agent_loop::AgentLoopEngine;
use session::{JsonlSessionCreateOptions, JsonlSessionListOptions, JsonlSessionRepository};

/// 创建 Session 时由管理器分配的稳定输入。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionCreateParams {
    /// 由管理器生成的 Session 标识。
    pub id: String,
    /// Session 的工作目录。
    pub cwd: String,
    /// 可选显示名称。
    pub name: Option<String>,
    /// 初始模型。
    pub model: ModelRef,
    /// 初始思考等级。
    pub thinking_level: ThinkingLevel,
    /// Unix 毫秒时间戳。
    pub created_at: u64,
}

/// 单个可管理 Session 的业务端口。
///
/// 它刻意不包含 `attach` 或 `detach`：附着状态是连接级视图，属于管理器而不是
/// AgentSession。实现必须返回独立快照，避免调用方修改内部状态。
pub trait ManagedSession: Send {
    /// 返回 Session 的当前权威快照。
    fn snapshot(&self) -> SessionSnapshot;

    /// 执行只属于单一 Session 的命令。
    fn execute(&mut self, command: Command) -> Result<SessionSnapshot, ProtocolError>;

    /// 推进已就绪的 Provider 输出，并取走本 Session 待广播的进度事件。
    ///
    /// 默认实现保留迁移期和测试 Session 的无 Provider 行为。生产实现不得在这里执行阻塞 I/O。
    fn poll(&mut self, _timestamp: u64) -> Result<Vec<ServerEvent>, ProtocolError> {
        Ok(Vec::new())
    }

    /// Session 是否还有进行中的回合；空闲且无订阅者时可被管理器释放。
    fn is_idle(&self) -> bool {
        self.snapshot().phase == SessionPhase::Idle
    }
}

/// 创建或重新打开 Session 的端口。
///
/// `open` 与 `create` 分开，使生产实现可从 Session Store 恢复已有 Session，而测试可
/// 使用同一个内存构造器。错误保持为协议错误，避免把后端错误类型泄露给 RPC。
pub trait SessionFactory: Send {
    /// 创建一个新 Session。
    fn create(
        &mut self,
        params: SessionCreateParams,
    ) -> Result<Box<dyn ManagedSession>, ProtocolError>;

    /// 打开一个已存在的 Session；不存在时返回 `not_found`。
    fn open(&mut self, session_id: &str) -> Result<Box<dyn ManagedSession>, ProtocolError>;
}

/// `SessionRuntime` 的最小迁移期适配器。
///
/// 仅供尚未配置持久化目录的嵌入方使用。生产 Listener 应使用
/// [`AgentSessionFactory`]；它通过完整的 AgentSession 状态机处理 prompt、steer 与 abort。
pub struct SessionRuntimeFactory;

impl SessionFactory for SessionRuntimeFactory {
    fn create(
        &mut self,
        params: SessionCreateParams,
    ) -> Result<Box<dyn ManagedSession>, ProtocolError> {
        Ok(Box::new(SessionRuntime::with_initial_state(
            params.id,
            params.cwd,
            params.model,
            params.name,
            params.thinking_level,
            params.created_at,
        )))
    }

    fn open(&mut self, session_id: &str) -> Result<Box<dyn ManagedSession>, ProtocolError> {
        Err(not_found(format!("找不到已持久化的会话：{session_id}")))
    }
}

impl ManagedSession for SessionRuntime {
    fn snapshot(&self) -> SessionSnapshot {
        SessionRuntime::snapshot(self)
    }

    fn execute(&mut self, command: Command) -> Result<SessionSnapshot, ProtocolError> {
        SessionRuntime::execute(self, command)
    }
}

/// 基于 JSONL v4 Store 的完整 AgentSession 工厂。
///
/// 每个创建或打开的 Session 都获取 repository 的单写者租约，并将它封装在
/// [`JsonlSessionPersistence`] 内。因此 SessionManager 释放 runtime 时会同时释放文件锁。
/// 每个 Session 启动一个独立的 TypeScript Provider Host；其 stdout 在 reader 线程中解码，
/// 所以 SessionManager 的 poll 边界不会等待模型输出。
pub struct AgentSessionFactory {
    repository: JsonlSessionRepository,
    provider_host_program: std::ffi::OsString,
    provider_host_args: Vec<std::ffi::OsString>,
    system_prompt: String,
}

impl AgentSessionFactory {
    /// 使用 JSONL repository 根目录及环境配置的 Provider Host 创建生产工厂。
    ///
    /// `PI_PROVIDER_HOST_PROGRAM` 默认 `node`，`PI_PROVIDER_HOST_ENTRY` 必须指向已构建的
    /// Provider Host 入口。可选 `PI_SYSTEM_PROMPT` 会成为每次 Provider 请求的 system prompt。
    pub fn new(sessions_root: impl Into<PathBuf>) -> Result<Self, ProtocolError> {
        let provider_host_entry = std::env::var_os("PI_PROVIDER_HOST_ENTRY").ok_or_else(|| {
            invalid_request("缺少 PI_PROVIDER_HOST_ENTRY；它必须指向已构建的 Provider Host 入口")
        })?;
        JsonlSessionRepository::new(sessions_root)
            .map(|repository| Self {
                repository,
                provider_host_program: std::env::var_os("PI_PROVIDER_HOST_PROGRAM")
                    .unwrap_or_else(|| "node".into()),
                provider_host_args: vec![provider_host_entry],
                system_prompt: std::env::var("PI_SYSTEM_PROMPT").unwrap_or_default(),
            })
            .map_err(session_store_error)
    }

    #[cfg(test)]
    fn new_with_provider_host(
        sessions_root: impl Into<PathBuf>,
        program: impl Into<std::ffi::OsString>,
        args: Vec<std::ffi::OsString>,
        system_prompt: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        JsonlSessionRepository::new(sessions_root)
            .map(|repository| Self {
                repository,
                provider_host_program: program.into(),
                provider_host_args: args,
                system_prompt: system_prompt.into(),
            })
            .map_err(session_store_error)
    }

    fn new_managed_session(
        &self,
        config: AgentSessionConfig,
        persistence: JsonlSessionPersistence,
    ) -> Result<Box<dyn ManagedSession>, ProtocolError> {
        let transport = ProviderHostStreamPort::spawn(
            &self.provider_host_program,
            self.provider_host_args.iter(),
        )
        .map_err(provider_ipc_error)?;
        Ok(Box::new(AgentManagedSession::new(
            config,
            persistence,
            transport,
            self.system_prompt.clone(),
        )))
    }
}

impl SessionFactory for AgentSessionFactory {
    fn create(
        &mut self,
        params: SessionCreateParams,
    ) -> Result<Box<dyn ManagedSession>, ProtocolError> {
        let store = self
            .repository
            .create(JsonlSessionCreateOptions {
                cwd: params.cwd.clone(),
                id: Some(params.id.clone()),
                parent_session_id: None,
                metadata: None,
            })
            .map_err(session_store_error)?;
        let mut config = agent_session_config(&params);
        config.created_at = store.header().created_at;
        let mut persistence = JsonlSessionPersistence::new_leased(store);
        persistence
            .persist(PersistenceEvent::ModelChanged(config.model.clone()))
            .map_err(persistence_error)?;
        persistence
            .persist(PersistenceEvent::ThinkingLevelChanged(
                config.thinking_level,
            ))
            .map_err(persistence_error)?;
        self.new_managed_session(config, persistence)
    }

    fn open(&mut self, session_id: &str) -> Result<Box<dyn ManagedSession>, ProtocolError> {
        let metadata = self
            .repository
            .list(JsonlSessionListOptions::default())
            .map_err(session_store_error)?
            .into_iter()
            .find(|metadata| metadata.id == session_id)
            .ok_or_else(|| not_found(format!("找不到已持久化的会话：{session_id}")))?;
        let store = self
            .repository
            .open(&metadata)
            .map_err(session_store_error)?;
        let model = model_from_store(&store).unwrap_or_else(|| ModelRef {
            provider: "unknown".to_owned(),
            id: "unknown".to_owned(),
        });
        let thinking_level = thinking_level_from_store(&store).unwrap_or(ThinkingLevel::Medium);
        self.new_managed_session(
            AgentSessionConfig {
                id: metadata.id,
                name: store.state().name().map(str::to_owned),
                cwd: metadata.cwd,
                model,
                thinking_level,
                created_at: metadata.created_at,
            },
            JsonlSessionPersistence::new_leased(store),
        )
    }
}

struct AgentManagedSession<P> {
    session: AgentSession<AgentLoopEngine, JsonlSessionPersistence>,
    provider_driver: ProviderStreamDriver<P>,
    tool_runtime: tool_runtime::ToolRuntime,
    pending_provider_events: Vec<ServerEvent>,
}

impl<P> AgentManagedSession<P>
where
    P: ProviderStreamPort,
{
    fn new(
        config: AgentSessionConfig,
        persistence: JsonlSessionPersistence,
        transport: P,
        system_prompt: impl Into<String>,
    ) -> Self {
        let loop_engine = AgentLoopEngine::new(config.model.clone());
        let mut tool_runtime = tool_runtime::ToolRuntime::new();
        tool_runtime
            .register(
                tool_runtime::ReadTool::new(&config.cwd)
                    .expect("AgentSession cwd 应在创建 Session 时完成有效性校验"),
            )
            .expect("内置 read 工具定义必须保持有效且名称唯一");
        Self {
            session: AgentSession::new(config, loop_engine, persistence),
            provider_driver: ProviderStreamDriver::new(transport, system_prompt),
            tool_runtime,
            pending_provider_events: Vec::new(),
        }
    }
}

impl<P> ManagedSession for AgentManagedSession<P>
where
    P: ProviderStreamPort,
{
    fn snapshot(&self) -> SessionSnapshot {
        self.session.snapshot()
    }

    fn execute(&mut self, command: Command) -> Result<SessionSnapshot, ProtocolError> {
        let is_prompt = matches!(command, Command::Prompt { .. });
        let is_abort = matches!(command, Command::Abort { .. });
        let timestamp = unix_millis();
        let snapshot = self.session.execute_at(command, timestamp)?;
        if is_prompt {
            // 取消状态只属于刚结束的 run。必须在 Session 接受新 prompt 后再重置，避免无效
            // prompt 意外清除仍在执行中的取消信号，也避免 abort 污染同一 Session 的后续回合。
            self.tool_runtime.reset_cancellation();
            let events = self
                .provider_driver
                .start(&mut self.session, &self.tool_runtime, timestamp)
                .map_err(provider_error)?;
            self.pending_provider_events.extend(events);
        }
        if is_abort {
            self.tool_runtime.cancel();
            self.provider_driver.abort().map_err(provider_error)?;
        }
        Ok(snapshot)
    }

    fn poll(&mut self, timestamp: u64) -> Result<Vec<ServerEvent>, ProtocolError> {
        let mut events = std::mem::take(&mut self.pending_provider_events);
        events.extend(
            self.provider_driver
                .poll(&mut self.session, &mut self.tool_runtime, timestamp)
                .map_err(provider_error)?,
        );
        Ok(events)
    }
}

fn agent_session_config(params: &SessionCreateParams) -> AgentSessionConfig {
    AgentSessionConfig {
        id: params.id.clone(),
        name: params.name.clone(),
        cwd: params.cwd.clone(),
        model: params.model.clone(),
        thinking_level: params.thinking_level,
        created_at: params.created_at,
    }
}

fn model_from_store(store: &session::JsonlSessionStore) -> Option<ModelRef> {
    let entry = store
        .state()
        .find_entries(session::EntryQuery {
            entry_type: Some("model_change"),
            ..Default::default()
        })
        .ok()?
        .into_iter()
        .next()?;
    Some(ModelRef {
        provider: entry.get("provider")?.as_str()?.to_owned(),
        id: entry.get("modelId")?.as_str()?.to_owned(),
    })
}

fn thinking_level_from_store(store: &session::JsonlSessionStore) -> Option<ThinkingLevel> {
    let entry = store
        .state()
        .find_entries(session::EntryQuery {
            entry_type: Some("thinking_level_change"),
            ..Default::default()
        })
        .ok()?
        .into_iter()
        .next()?;
    serde_json::from_value(entry.get("thinkingLevel")?.clone()).ok()
}

fn session_store_error(error: session::SessionStoreError) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InternalError,
        message: format!("Session Store 操作失败：{error}"),
        details: None,
    }
}

fn provider_ipc_error(error: crate::provider_ipc::ProviderIpcError) -> ProtocolError {
    provider_error(error.to_string())
}

fn provider_error(message: String) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InternalError,
        message: format!("Provider stream 驱动失败：{message}"),
        details: None,
    }
}

fn persistence_error(error: crate::agent_session::SessionPersistenceError) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InternalError,
        message: format!("Session Store 持久化失败：{}", error.message()),
        details: None,
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

struct LiveSession {
    runtime: Box<dyn ManagedSession>,
    connections: BTreeSet<String>,
}

/// 管理多个活动 Session、其连接订阅及待发送事件。
///
/// 该类型采用显式的 `connection_id` 参数，而不是保存 transport 对象。外层可用
/// [`ConnectionSessionHandler`] 将每条 [`rpc::RpcConnection`] 绑定到同一个管理器；
/// 因而断线时可以精确解除该连接的所有订阅。
pub struct SessionManager<F> {
    server_id: String,
    models: Vec<ModelMetadata>,
    default_cwd: String,
    default_model: ModelRef,
    default_thinking_level: ThinkingLevel,
    factory: F,
    sessions: BTreeMap<String, LiveSession>,
    connection_sessions: BTreeMap<String, BTreeSet<String>>,
    pending_events: BTreeMap<String, Vec<ServerEvent>>,
    revision: u64,
    next_session_sequence: u64,
}

impl<F> SessionManager<F>
where
    F: SessionFactory,
{
    /// 建立空的 Session 管理器。
    pub fn new(
        server_id: impl Into<String>,
        default_cwd: impl Into<String>,
        default_model: ModelRef,
        models: Vec<ModelMetadata>,
        factory: F,
    ) -> Self {
        Self {
            server_id: server_id.into(),
            models,
            default_cwd: default_cwd.into(),
            default_model,
            default_thinking_level: ThinkingLevel::Medium,
            factory,
            sessions: BTreeMap::new(),
            connection_sessions: BTreeMap::new(),
            pending_events: BTreeMap::new(),
            revision: 0,
            next_session_sequence: 0,
        }
    }

    /// 为连接建立状态槽；可安全地重复调用。
    pub fn connect(&mut self, connection_id: impl Into<String>) {
        let connection_id = connection_id.into();
        self.connection_sessions
            .entry(connection_id.clone())
            .or_default();
        self.pending_events.entry(connection_id).or_default();
    }

    /// 解除连接的全部订阅，并释放不再活跃的空闲 Session。
    pub fn disconnect(&mut self, connection_id: &str) {
        let session_ids = self
            .connection_sessions
            .remove(connection_id)
            .unwrap_or_default();
        self.pending_events.remove(connection_id);
        let mut changed = false;
        for session_id in session_ids {
            if let Some(live) = self.sessions.get_mut(&session_id) {
                changed |= live.connections.remove(connection_id);
            }
            self.maybe_dispose(&session_id);
        }
        if changed {
            self.bump_revision();
        }
    }

    /// 执行某一连接发出的命令。
    pub fn execute(
        &mut self,
        connection_id: &str,
        command: Command,
        timestamp: u64,
    ) -> Result<CommandResult, ProtocolError> {
        self.connect(connection_id);
        match command {
            Command::List => Ok(CommandResult::List {
                sessions: self.list_metadata(),
            }),
            Command::Create {
                cwd,
                name,
                model,
                thinking_level,
            } => self.create_and_attach(connection_id, cwd, name, model, thinking_level, timestamp),
            Command::Attach { session_id } => self.attach_existing(connection_id, session_id),
            Command::Detach { session_id } => {
                self.detach(connection_id, &session_id);
                Ok(CommandResult::Detach { session_id })
            }
            command @ (Command::Prompt { .. }
            | Command::Steer { .. }
            | Command::Abort { .. }
            | Command::SetModel { .. }
            | Command::SetThinking { .. }) => self.execute_session_command(connection_id, command),
        }
    }

    /// 为 hello 构造权威 Server 快照。
    pub fn snapshot(&self) -> ServerSnapshot {
        ServerSnapshot {
            server_id: self.server_id.clone(),
            protocol_version: protocol::PROTOCOL_VERSION,
            revision: self.revision,
            sessions: self.list_metadata(),
            models: self.models.clone(),
        }
    }

    /// 轮询全部 live Session 的非阻塞 Provider 驱动，并向附着连接广播产生的 progress。
    ///
    /// 该方法绝不等待 Provider I/O；实际等待与 frame 解码属于 Provider transport 的 reader
    /// 任务。每个驱动在一次 poll 中结算状态变化后，管理器会额外广播最新 snapshot。
    pub fn poll(&mut self, timestamp: u64) -> Result<(), ProtocolError> {
        let session_ids: Vec<_> = self.sessions.keys().cloned().collect();
        for session_id in session_ids {
            self.poll_session(&session_id, timestamp)?;
        }
        Ok(())
    }

    /// 取走目标连接的待发送事件。
    pub fn drain_events(&mut self, connection_id: &str) -> Vec<ServerEvent> {
        self.pending_events
            .remove(connection_id)
            .unwrap_or_default()
    }

    fn create_and_attach(
        &mut self,
        connection_id: &str,
        cwd: Option<String>,
        name: Option<String>,
        model: Option<ModelRef>,
        thinking_level: Option<ThinkingLevel>,
        timestamp: u64,
    ) -> Result<CommandResult, ProtocolError> {
        self.next_session_sequence += 1;
        let id = format!("{}-session-{}", self.server_id, self.next_session_sequence);
        let runtime = self.factory.create(SessionCreateParams {
            id: id.clone(),
            cwd: cwd.unwrap_or_else(|| self.default_cwd.clone()),
            name,
            model: model.unwrap_or_else(|| self.default_model.clone()),
            thinking_level: thinking_level.unwrap_or(self.default_thinking_level),
            created_at: timestamp,
        })?;
        self.insert_live(id.clone(), runtime)?;
        self.attach(connection_id, &id)?;
        let session = self.snapshot_for_connection(&id, connection_id)?;
        self.broadcast_snapshot(&id)?;
        Ok(CommandResult::Create { session })
    }

    fn attach_existing(
        &mut self,
        connection_id: &str,
        session_id: String,
    ) -> Result<CommandResult, ProtocolError> {
        if !self.sessions.contains_key(&session_id) {
            let runtime = self.factory.open(&session_id)?;
            self.insert_live(session_id.clone(), runtime)?;
        }
        self.attach(connection_id, &session_id)?;
        let session = self.snapshot_for_connection(&session_id, connection_id)?;
        self.broadcast_snapshot(&session_id)?;
        Ok(CommandResult::Attach { session })
    }

    fn execute_session_command(
        &mut self,
        connection_id: &str,
        command: Command,
    ) -> Result<CommandResult, ProtocolError> {
        let session_id =
            command_session_id(&command).expect("session command must have a session id");
        self.require_attached(connection_id, session_id)?;
        let snapshot = self
            .sessions
            .get_mut(session_id)
            .expect("attached session must be live")
            .runtime
            .execute(command.clone())?;
        self.broadcast_snapshot(session_id)?;
        let session = self.with_connection_attachment(snapshot, connection_id);
        Ok(match command {
            Command::Prompt { .. } => CommandResult::Prompt { session },
            Command::Steer { .. } => CommandResult::Steer { session },
            Command::Abort { .. } => CommandResult::Abort { session },
            Command::SetModel { .. } => CommandResult::SetModel { session },
            Command::SetThinking { .. } => CommandResult::SetThinking { session },
            _ => unreachable!("only session commands reach this branch"),
        })
    }

    fn poll_session(&mut self, session_id: &str, timestamp: u64) -> Result<(), ProtocolError> {
        let (connections, before, progress, after) = {
            let live = self
                .sessions
                .get_mut(session_id)
                .ok_or_else(|| not_found(format!("找不到会话：{session_id}")))?;
            let before = live.runtime.snapshot();
            let progress = live.runtime.poll(timestamp)?;
            let after = live.runtime.snapshot();
            (live.connections.clone(), before, progress, after)
        };
        for connection_id in connections {
            self.pending_events
                .entry(connection_id)
                .or_default()
                .extend(progress.iter().cloned());
        }
        if before != after {
            self.broadcast_snapshot(session_id)?;
        }
        self.maybe_dispose(session_id);
        Ok(())
    }

    fn insert_live(
        &mut self,
        session_id: String,
        runtime: Box<dyn ManagedSession>,
    ) -> Result<(), ProtocolError> {
        if runtime.snapshot().id != session_id {
            return Err(invalid_request(format!(
                "Session factory returned {} for requested session {session_id}",
                runtime.snapshot().id
            )));
        }
        self.sessions.insert(
            session_id,
            LiveSession {
                runtime,
                connections: BTreeSet::new(),
            },
        );
        self.bump_revision();
        Ok(())
    }

    fn attach(&mut self, connection_id: &str, session_id: &str) -> Result<(), ProtocolError> {
        let live = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| not_found(format!("找不到会话：{session_id}")))?;
        let added = live.connections.insert(connection_id.to_owned());
        self.connection_sessions
            .entry(connection_id.to_owned())
            .or_default()
            .insert(session_id.to_owned());
        if added {
            self.bump_revision();
        }
        Ok(())
    }

    fn detach(&mut self, connection_id: &str, session_id: &str) {
        let mut changed = false;
        if let Some(session_ids) = self.connection_sessions.get_mut(connection_id) {
            changed = session_ids.remove(session_id);
        }
        if let Some(live) = self.sessions.get_mut(session_id) {
            changed |= live.connections.remove(connection_id);
        }
        self.maybe_dispose(session_id);
        if changed {
            self.bump_revision();
        }
    }

    fn require_attached(&self, connection_id: &str, session_id: &str) -> Result<(), ProtocolError> {
        if !self
            .connection_sessions
            .get(connection_id)
            .is_some_and(|session_ids| session_ids.contains(session_id))
        {
            return Err(invalid_request(format!("连接未附着到会话：{session_id}")));
        }
        if !self.sessions.contains_key(session_id) {
            return Err(not_found(format!("会话已不再活动：{session_id}")));
        }
        Ok(())
    }

    fn list_metadata(&self) -> Vec<SessionMetadata> {
        self.sessions
            .values()
            .map(|live| {
                let snapshot = live.runtime.snapshot();
                SessionMetadata {
                    id: snapshot.id,
                    created_at: snapshot.created_at,
                    updated_at: Some(snapshot.updated_at),
                    parent_session_id: None,
                    session_name: snapshot.name,
                    cwd: Some(snapshot.cwd),
                }
            })
            .collect()
    }

    fn snapshot_for_connection(
        &self,
        session_id: &str,
        connection_id: &str,
    ) -> Result<SessionSnapshot, ProtocolError> {
        let live = self
            .sessions
            .get(session_id)
            .ok_or_else(|| not_found(format!("找不到会话：{session_id}")))?;
        Ok(self.with_connection_attachment(live.runtime.snapshot(), connection_id))
    }

    fn with_connection_attachment(
        &self,
        mut snapshot: SessionSnapshot,
        connection_id: &str,
    ) -> SessionSnapshot {
        snapshot.attached = self
            .connection_sessions
            .get(connection_id)
            .is_some_and(|session_ids| session_ids.contains(&snapshot.id));
        snapshot.locked = true;
        snapshot
    }

    fn broadcast_snapshot(&mut self, session_id: &str) -> Result<(), ProtocolError> {
        let (connections, mut snapshot) = {
            let live = self
                .sessions
                .get(session_id)
                .ok_or_else(|| not_found(format!("找不到会话：{session_id}")))?;
            (live.connections.clone(), live.runtime.snapshot())
        };
        snapshot.attached = !connections.is_empty();
        snapshot.locked = true;
        for connection_id in connections {
            self.pending_events.entry(connection_id).or_default().push(
                ServerEvent::SessionSnapshot {
                    snapshot: snapshot.clone(),
                },
            );
        }
        Ok(())
    }

    fn maybe_dispose(&mut self, session_id: &str) {
        let should_dispose = self
            .sessions
            .get(session_id)
            .is_some_and(|live| live.connections.is_empty() && live.runtime.is_idle());
        if should_dispose {
            self.sessions.remove(session_id);
            self.bump_revision();
            for events in self.pending_events.values_mut() {
                events.push(ServerEvent::SessionRemoved {
                    session_id: session_id.to_owned(),
                });
            }
        }
    }

    fn bump_revision(&mut self) {
        self.revision += 1;
    }
}

/// 将共享 [`SessionManager`] 绑定到一条 RPC 连接的业务适配器。
///
/// 每个 handler 仅持有自己的连接 ID；Session 实例、订阅关系和事件队列留在共享管理器。
/// transport 关闭后外层应调用 [`ConnectionSessionHandler::disconnect`]，再通过
/// [`rpc::RpcConnection::into_handler`] 回收该 handler。
pub struct ConnectionSessionHandler<F> {
    connection_id: String,
    manager: Arc<Mutex<SessionManager<F>>>,
    clock: Box<dyn FnMut() -> u64 + Send>,
}

impl<F> ConnectionSessionHandler<F>
where
    F: SessionFactory,
{
    /// 用确定性时钟创建连接适配器；生产代码可传入 Unix 毫秒时钟，测试可传固定时钟。
    pub fn new(
        connection_id: impl Into<String>,
        manager: Arc<Mutex<SessionManager<F>>>,
        clock: impl FnMut() -> u64 + Send + 'static,
    ) -> Self {
        let connection_id = connection_id.into();
        manager
            .lock()
            .expect("SessionManager mutex must not be poisoned")
            .connect(connection_id.clone());
        Self {
            connection_id,
            manager,
            clock: Box::new(clock),
        }
    }

    /// 解除连接订阅。重复调用安全。
    pub fn disconnect(&mut self) {
        self.manager
            .lock()
            .expect("SessionManager mutex must not be poisoned")
            .disconnect(&self.connection_id);
    }
}

impl<F> RpcCommandHandler for ConnectionSessionHandler<F>
where
    F: SessionFactory,
{
    fn execute(&mut self, command: Command) -> Result<CommandResult, ProtocolError> {
        let timestamp = (self.clock)();
        let mut manager = self
            .manager
            .lock()
            .expect("SessionManager mutex must not be poisoned");
        let result = manager.execute(&self.connection_id, command, timestamp)?;
        manager.poll(timestamp)?;
        Ok(result)
    }

    fn snapshot(&self) -> ServerSnapshot {
        self.manager
            .lock()
            .expect("SessionManager mutex must not be poisoned")
            .snapshot()
    }

    fn drain_events(&mut self) -> Vec<ServerEvent> {
        let timestamp = (self.clock)();
        let mut manager = self
            .manager
            .lock()
            .expect("SessionManager mutex must not be poisoned");
        // provider progress can arrive between RPC requests, so drain is also a polling boundary.
        let _ = manager.poll(timestamp);
        manager.drain_events(&self.connection_id)
    }
}

fn command_session_id(command: &Command) -> Option<&str> {
    match command {
        Command::Attach { session_id }
        | Command::Detach { session_id }
        | Command::Prompt { session_id, .. }
        | Command::Steer { session_id, .. }
        | Command::Abort { session_id }
        | Command::SetModel { session_id, .. }
        | Command::SetThinking { session_id, .. } => Some(session_id),
        Command::List | Command::Create { .. } => None,
    }
}

fn invalid_request(message: impl Into<String>) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InvalidRequest,
        message: message.into(),
        details: None,
    }
}

fn not_found(message: impl Into<String>) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::NotFound,
        message: message.into(),
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use protocol::{
        ClientMessage, Command, ModelRef, ProtocolErrorCode, ServerMessage, SessionPhase,
        SessionSnapshot, ThinkingLevel,
        cbor::{decode_cbor, encode_cbor},
        framing::{decode_complete_frame, encode_frame},
    };
    use rpc::{RpcCommandHandler, RpcConnection};

    use super::{
        ConnectionSessionHandler, ManagedSession, SessionCreateParams, SessionFactory,
        SessionManager,
    };

    struct TestSession {
        snapshot: SessionSnapshot,
    }

    impl ManagedSession for TestSession {
        fn snapshot(&self) -> SessionSnapshot {
            self.snapshot.clone()
        }

        fn execute(
            &mut self,
            command: Command,
        ) -> Result<SessionSnapshot, protocol::ProtocolError> {
            match command {
                Command::SetThinking { thinking_level, .. } => {
                    self.snapshot.thinking_level = thinking_level;
                    self.snapshot.revision += 1;
                }
                Command::Prompt { .. } => {
                    self.snapshot.phase = SessionPhase::Turn;
                    self.snapshot.revision += 1;
                }
                Command::Abort { .. } => {
                    self.snapshot.phase = SessionPhase::Idle;
                    self.snapshot.revision += 1;
                }
                _ => {}
            }
            Ok(self.snapshot())
        }
    }

    #[derive(Default)]
    struct TestFactory {
        stored: BTreeMap<String, SessionSnapshot>,
    }

    impl SessionFactory for TestFactory {
        fn create(
            &mut self,
            params: SessionCreateParams,
        ) -> Result<Box<dyn ManagedSession>, protocol::ProtocolError> {
            let snapshot = session_snapshot(&params);
            self.stored.insert(params.id, snapshot.clone());
            Ok(Box::new(TestSession { snapshot }))
        }

        fn open(
            &mut self,
            session_id: &str,
        ) -> Result<Box<dyn ManagedSession>, protocol::ProtocolError> {
            self.stored
                .get(session_id)
                .cloned()
                .map(|snapshot| Box::new(TestSession { snapshot }) as Box<dyn ManagedSession>)
                .ok_or_else(|| protocol::ProtocolError {
                    code: ProtocolErrorCode::NotFound,
                    message: "missing".to_owned(),
                    details: None,
                })
        }
    }

    fn model() -> ModelRef {
        ModelRef {
            provider: "test".to_owned(),
            id: "model".to_owned(),
        }
    }

    fn session_snapshot(params: &SessionCreateParams) -> SessionSnapshot {
        SessionSnapshot {
            id: params.id.clone(),
            name: params.name.clone(),
            cwd: params.cwd.clone(),
            created_at: params.created_at,
            updated_at: params.created_at,
            phase: SessionPhase::Idle,
            model: params.model.clone(),
            thinking_level: params.thinking_level,
            attached: false,
            locked: false,
            revision: 0,
            transcript: Vec::new(),
            queued_steer: Vec::new(),
            queued_steer_count: 0,
        }
    }

    fn manager() -> Arc<Mutex<SessionManager<TestFactory>>> {
        Arc::new(Mutex::new(SessionManager::new(
            "server",
            ".",
            model(),
            Vec::new(),
            TestFactory::default(),
        )))
    }

    #[test]
    fn production_factory_uses_agent_session_for_prompt_steer_and_abort() {
        let root = std::env::temp_dir().join(format!(
            "agent-session-factory-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after epoch")
                .as_nanos()
        ));
        let factory = super::AgentSessionFactory::new_with_provider_host(
            &root,
            "cmd",
            vec!["/C".into(), "more > nul".into()],
            "",
        )
        .expect("factory should initialize");
        let manager = Arc::new(Mutex::new(SessionManager::new(
            "server",
            ".",
            model(),
            Vec::new(),
            factory,
        )));
        let mut handler = ConnectionSessionHandler::new("connection-a", manager, || 100);
        let session_id = match handler
            .execute(Command::Create {
                cwd: None,
                name: Some("production".to_owned()),
                model: None,
                thinking_level: None,
            })
            .expect("create should succeed")
        {
            protocol::CommandResult::Create { session } => {
                assert_eq!(session.name.as_deref(), Some("production"));
                session.id
            }
            result => panic!("unexpected create result: {result:?}"),
        };

        handler.drain_events();
        let prompt = handler
            .execute(Command::Prompt {
                session_id: session_id.clone(),
                text: "hello".to_owned(),
            })
            .expect("prompt should enter AgentSession and start its Provider driver");
        assert!(matches!(
            prompt,
            protocol::CommandResult::Prompt { session } if session.phase == SessionPhase::Turn
        ));
        assert!(matches!(
            handler.drain_events().as_slice(),
            [
                protocol::ServerEvent::SessionSnapshot { snapshot },
                protocol::ServerEvent::SessionProgress { session_id: progress_session_id, .. }
            ] if snapshot.id == session_id && progress_session_id == &session_id
        ));
        let steer = handler
            .execute(Command::Steer {
                session_id: session_id.clone(),
                text: "additional context".to_owned(),
            })
            .expect("steer should enter AgentSession");
        assert!(matches!(
            steer,
            protocol::CommandResult::Steer { session } if session.queued_steer_count == 1
        ));
        let abort = handler
            .execute(Command::Abort {
                session_id: session_id,
            })
            .expect("abort should request AgentSession cancellation");
        assert!(matches!(
            abort,
            protocol::CommandResult::Abort { session } if session.phase == SessionPhase::Turn
        ));

        std::fs::remove_dir_all(root).expect("temporary repository should be removed");
    }

    #[test]
    fn create_attaches_only_the_requesting_connection_and_broadcasts_to_it() {
        let manager = manager();
        let mut handler = ConnectionSessionHandler::new("connection-a", manager.clone(), || 100);
        let result = handler
            .execute(Command::Create {
                cwd: None,
                name: Some("demo".to_owned()),
                model: None,
                thinking_level: None,
            })
            .expect("create succeeds");
        let session_id = match result {
            protocol::CommandResult::Create { session } => {
                assert!(session.attached);
                assert!(session.locked);
                assert_eq!(session.name.as_deref(), Some("demo"));
                session.id
            }
            _ => panic!("expected create result"),
        };

        assert_eq!(manager.lock().expect("mutex").snapshot().sessions.len(), 1);
        assert!(
            matches!(handler.drain_events().as_slice(), [protocol::ServerEvent::SessionSnapshot { snapshot }] if snapshot.id == session_id)
        );
    }

    #[test]
    fn session_commands_require_attachment_and_preserve_response_before_event_boundary() {
        let manager = manager();
        let mut owner = ConnectionSessionHandler::new("owner", manager.clone(), || 100);
        let session_id = match owner
            .execute(Command::Create {
                cwd: None,
                name: None,
                model: None,
                thinking_level: None,
            })
            .expect("create succeeds")
        {
            protocol::CommandResult::Create { session } => session.id,
            _ => panic!("expected create"),
        };
        owner.drain_events();
        let mut stranger = ConnectionSessionHandler::new("stranger", manager, || 101);

        let error = stranger
            .execute(Command::SetThinking {
                session_id: session_id.clone(),
                thinking_level: ThinkingLevel::High,
            })
            .expect_err("unattached connection is rejected");
        assert_eq!(error.code, ProtocolErrorCode::InvalidRequest);

        owner
            .execute(Command::SetThinking {
                session_id,
                thinking_level: ThinkingLevel::High,
            })
            .expect("owner controls attached session");
        assert!(matches!(
            owner.drain_events().as_slice(),
            [protocol::ServerEvent::SessionSnapshot { .. }]
        ));
    }

    #[test]
    fn disconnect_releases_idle_sessions_and_emits_removal_to_remaining_connections() {
        let manager = manager();
        let mut first = ConnectionSessionHandler::new("first", manager.clone(), || 100);
        let session_id = match first
            .execute(Command::Create {
                cwd: None,
                name: None,
                model: None,
                thinking_level: None,
            })
            .expect("create succeeds")
        {
            protocol::CommandResult::Create { session } => session.id,
            _ => panic!("expected create"),
        };
        first.drain_events();
        let mut second = ConnectionSessionHandler::new("second", manager.clone(), || 101);
        second
            .execute(Command::Attach {
                session_id: session_id.clone(),
            })
            .expect("attach succeeds");
        first.drain_events();
        second.drain_events();

        first.disconnect();
        assert!(
            manager
                .lock()
                .expect("mutex")
                .snapshot()
                .sessions
                .iter()
                .any(|item| item.id == session_id)
        );
        second.disconnect();
        assert!(
            manager
                .lock()
                .expect("mutex")
                .snapshot()
                .sessions
                .is_empty()
        );
    }

    #[test]
    fn rpc_connection_routes_create_and_sends_its_response_before_snapshot_event() {
        let handler = ConnectionSessionHandler::new("wire", manager(), || 100);
        let mut connection = RpcConnection::with_max_frame_length("wire", handler, 1024)
            .expect("connection initializes");
        let hello = encode_frame(
            &encode_cbor(&ClientMessage::Hello {
                version: protocol::PROTOCOL_VERSION,
            })
            .expect("hello encodes"),
            1024,
        )
        .expect("hello frames");
        connection.receive(&hello).expect("hello succeeds");

        let request = encode_frame(
            &encode_cbor(&ClientMessage::Request {
                id: "create-1".to_owned(),
                request: Command::Create {
                    cwd: None,
                    name: None,
                    model: None,
                    thinking_level: None,
                },
            })
            .expect("request encodes"),
            1024,
        )
        .expect("request frames");
        let messages: Vec<ServerMessage> = connection
            .receive(&request)
            .expect("request succeeds")
            .iter()
            .map(|frame| {
                decode_cbor(decode_complete_frame(frame, 1024).expect("complete frame"))
                    .expect("server message decodes")
            })
            .collect();

        assert!(matches!(
            messages.as_slice(),
            [ServerMessage::SuccessResponse(_), ServerMessage::Event(_)]
        ));
    }
}
