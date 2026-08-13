//! Rust AgentSession 的领域编排层。
//!
//! 对应 TypeScript [`AgentSession`](../../../../../packages/coding-agent/src/core/agent-session.ts)，
//! 但不复制其将 Provider、扩展、工具、TUI 与持久化混在一起的实现方式。此处只负责
//! 命令校验、回合状态、稳定 transcript、steer 队列和快照；具体执行由 `AgentLoop`，
//! JSONL 写入由 `SessionPersistence` 提供。

mod jsonl_persistence;
mod ports;
mod state;

pub use jsonl_persistence::JsonlSessionPersistence;
pub use ports::{
    AgentLoop, AgentLoopError, PersistenceEvent, SessionPersistence, SessionPersistenceError,
    session_events_from_rust_agent_loop,
};
pub use state::{AgentSessionState, QueuedSteer, user_text_item};

use protocol::{
    Command, ModelRef, ProtocolError, ProtocolErrorCode, SessionSnapshot, ThinkingLevel,
    TranscriptItem, UserTranscriptItem,
};

/// 领域层向 RPC 或 UI 适配器发出的已排序事件。
///
/// 事件先写入内部队列，调用方通过 [`AgentSession::drain_events`] 获取。这样领域层
/// 不保存回调闭包，也不会因同步监听器重入而破坏状态机。
#[derive(Debug, Clone, PartialEq)]
pub enum AgentSessionEvent {
    /// 状态发生变化后的完整快照。
    Snapshot(SessionSnapshot),
    /// 本地 steer 队列发生变化。
    QueueUpdated {
        /// 尚待 Agent Loop 消费的 steer 项数。
        queued_steer_count: u64,
    },
    /// Agent Loop 已确认本回合结束。
    Settled,
}

/// Agent Loop 向 AgentSession 回传的生命周期事件。
///
/// 事件仅描述 Agent Loop 已完成的事实。流式增量由 RPC 层使用共享协议的
/// `SessionProgress` 直接转发；稳定项目必须经 `TranscriptItemFinished` 回传，
/// 保证 JSONL 中只出现完成态消息，与 TypeScript 的 `message_end` 时机一致。
#[derive(Debug, Clone, PartialEq)]
pub enum AgentLoopEvent {
    /// 一个 transcript 项已到达终态，可写入 Session Store。
    TranscriptItemFinished(TranscriptItem),
    /// Agent Loop 已消费一个 steering 输入。
    SteerConsumed,
    /// 当前回合及其后续工具循环都已结束。
    Settled,
}

/// AgentSession 的构造参数。
#[derive(Debug, Clone)]
pub struct AgentSessionConfig {
    /// 稳定的会话 ID。
    pub id: String,
    /// 会话工作目录。
    pub cwd: String,
    /// 初始模型。
    pub model: ModelRef,
    /// 初始思考等级。
    pub thinking_level: ThinkingLevel,
    /// 创建时间，单位为 Unix 毫秒。
    pub created_at: u64,
}

/// 负责协调一个 Agent 回合的 Rust Session。
///
/// `L` 与 `P` 都是端口实现而非具体服务：前者未来由 Rust Agent Loop 实现，后者
/// 可接 JSONL v4、内存测试存储或其他后端。该泛型边界确保 AgentSession 不反向依赖
/// Provider Host、Tool Runtime、Extension Host 或 TUI。
pub struct AgentSession<L, P> {
    state: AgentSessionState,
    agent_loop: L,
    persistence: P,
    events: Vec<AgentSessionEvent>,
    next_user_message_sequence: u64,
}

impl<L, P> AgentSession<L, P>
where
    L: AgentLoop,
    P: SessionPersistence,
{
    /// 用外部能力端口创建一个 Session。
    pub fn new(config: AgentSessionConfig, agent_loop: L, persistence: P) -> Self {
        Self {
            state: AgentSessionState::new(
                config.id,
                config.cwd,
                config.model,
                config.created_at,
                config.thinking_level,
            ),
            agent_loop,
            persistence,
            events: Vec::new(),
            next_user_message_sequence: 0,
        }
    }

    /// 返回当前协议快照的独立副本。
    pub fn snapshot(&self) -> SessionSnapshot {
        self.state.snapshot()
    }

    /// 返回只读领域状态，供 Rust Core 内部查询。
    pub fn state(&self) -> &AgentSessionState {
        &self.state
    }

    /// 返回 Agent Loop 实现的只读引用。
    pub fn agent_loop(&self) -> &L {
        &self.agent_loop
    }

    /// 返回 Agent Loop 实现的可变引用，供 Provider adapter 注入已归一化的流事件。
    ///
    /// 领域命令仍必须通过 [`AgentSession::execute_at`] 进入；调用方只能在这里驱动
    /// 已开始的 Loop，并应立即把终态事件回送给 [`AgentSession::handle_agent_loop_event_at`]。
    pub fn agent_loop_mut(&mut self) -> &mut L {
        &mut self.agent_loop
    }

    /// 返回持久化实现的只读引用。
    pub fn persistence(&self) -> &P {
        &self.persistence
    }

    /// 取走按发生顺序累积的领域事件。
    pub fn drain_events(&mut self) -> Vec<AgentSessionEvent> {
        std::mem::take(&mut self.events)
    }

    /// 在指定时刻执行面向协议的 Session 命令。
    ///
    /// `timestamp` 显式作为参数，以便 replay 与测试可以完全确定性地复现快照版本。
    /// 真实运行时适配器应传入当前 Unix 毫秒。
    pub fn execute_at(
        &mut self,
        command: Command,
        timestamp: u64,
    ) -> Result<SessionSnapshot, ProtocolError> {
        self.assert_command_session(&command)?;

        match command {
            Command::SetModel { model, .. } => self.set_model(model, timestamp)?,
            Command::SetThinking { thinking_level, .. } => {
                self.set_thinking(thinking_level, timestamp)?
            }
            Command::Prompt { text, .. } => self.prompt(text, timestamp)?,
            Command::Steer { text, .. } => self.steer(text, timestamp)?,
            Command::Abort { .. } => self.abort(timestamp)?,
            Command::Attach { .. }
            | Command::Detach { .. }
            | Command::List
            | Command::Create { .. } => {
                return Err(invalid_request(
                    "该命令属于 Session Repository 或 Session Runtime，不由单个 AgentSession 执行。",
                ));
            }
        }

        Ok(self.snapshot())
    }

    /// 处理 Agent Loop 已确认完成的事件。
    pub fn handle_agent_loop_event_at(
        &mut self,
        event: AgentLoopEvent,
        timestamp: u64,
    ) -> Result<SessionSnapshot, ProtocolError> {
        match event {
            AgentLoopEvent::TranscriptItemFinished(item) => {
                self.persist(PersistenceEvent::TranscriptItemFinished(item.clone()))?;
                self.state.finish_transcript_item(item, timestamp);
                self.emit_snapshot();
            }
            AgentLoopEvent::SteerConsumed => {
                self.state.consume_steer(timestamp);
                self.events.push(AgentSessionEvent::QueueUpdated {
                    queued_steer_count: self.state.snapshot().queued_steer_count,
                });
                self.emit_snapshot();
            }
            AgentLoopEvent::Settled => {
                self.state.settle(timestamp);
                self.events.push(AgentSessionEvent::Settled);
                self.emit_snapshot();
            }
        }

        Ok(self.snapshot())
    }

    fn set_model(&mut self, model: ModelRef, timestamp: u64) -> Result<(), ProtocolError> {
        self.persist(PersistenceEvent::ModelChanged(model.clone()))?;
        self.state.set_model(model, timestamp);
        self.emit_snapshot();
        Ok(())
    }

    fn set_thinking(&mut self, level: ThinkingLevel, timestamp: u64) -> Result<(), ProtocolError> {
        self.persist(PersistenceEvent::ThinkingLevelChanged(level))?;
        self.state.set_thinking_level(level, timestamp);
        self.emit_snapshot();
        Ok(())
    }

    fn prompt(&mut self, text: String, timestamp: u64) -> Result<(), ProtocolError> {
        if self.state.is_active() {
            return Err(busy(
                "Agent 正在处理请求；请使用 steer 将输入插入当前回合。",
            ));
        }

        let message = self.new_user_message(text, timestamp);
        self.agent_loop.prompt(message).map_err(agent_loop_error)?;
        self.state.start_run(timestamp);
        self.emit_snapshot();
        Ok(())
    }

    fn steer(&mut self, text: String, timestamp: u64) -> Result<(), ProtocolError> {
        if !self.state.is_active() {
            return Err(invalid_request(
                "Agent 当前空闲，不能 steer；请使用 prompt 启动新回合。",
            ));
        }

        let message = self.new_user_message(text, timestamp);
        self.agent_loop
            .steer(message.clone())
            .map_err(agent_loop_error)?;
        self.state.enqueue_steer(message, timestamp);
        self.events.push(AgentSessionEvent::QueueUpdated {
            queued_steer_count: self.state.snapshot().queued_steer_count,
        });
        self.emit_snapshot();
        Ok(())
    }

    fn abort(&mut self, timestamp: u64) -> Result<(), ProtocolError> {
        if !self.state.is_active() {
            return Ok(());
        }

        self.agent_loop.abort().map_err(agent_loop_error)?;
        // 不在这里直接变为 idle：TS `abort()` 会等待 Agent Loop 真正结束。只有
        // `AgentLoopEvent::Settled` 才能结束回合，从而防止工具结果晚到后写入错误分支。
        self.state.mark_abort_requested(timestamp);
        self.emit_snapshot();
        Ok(())
    }

    fn new_user_message(&mut self, text: String, timestamp: u64) -> UserTranscriptItem {
        self.next_user_message_sequence += 1;
        user_text_item(
            format!(
                "{}-user-{}",
                self.state.id(),
                self.next_user_message_sequence
            ),
            text,
            timestamp,
        )
    }

    fn assert_command_session(&self, command: &Command) -> Result<(), ProtocolError> {
        let session_id = match command {
            Command::Attach { session_id }
            | Command::Detach { session_id }
            | Command::Prompt { session_id, .. }
            | Command::Steer { session_id, .. }
            | Command::Abort { session_id }
            | Command::SetModel { session_id, .. }
            | Command::SetThinking { session_id, .. } => Some(session_id),
            Command::List | Command::Create { .. } => None,
        };

        if let Some(session_id) = session_id
            && session_id != self.state.id()
        {
            return Err(ProtocolError {
                code: ProtocolErrorCode::NotFound,
                message: format!("找不到会话：{session_id}"),
                details: None,
            });
        }
        Ok(())
    }

    fn persist(&mut self, event: PersistenceEvent) -> Result<(), ProtocolError> {
        self.persistence.persist(event).map_err(persistence_error)
    }

    fn emit_snapshot(&mut self) {
        self.events
            .push(AgentSessionEvent::Snapshot(self.snapshot()));
    }
}

fn agent_loop_error(error: AgentLoopError) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InternalError,
        message: format!("Agent Loop 执行失败：{}", error.message()),
        details: None,
    }
}

fn persistence_error(error: SessionPersistenceError) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InternalError,
        message: format!("Session Store 持久化失败：{}", error.message()),
        details: None,
    }
}

fn invalid_request(message: impl Into<String>) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InvalidRequest,
        message: message.into(),
        details: None,
    }
}

#[cfg(test)]
mod tests;

fn busy(message: impl Into<String>) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::Busy,
        message: message.into(),
        details: None,
    }
}
