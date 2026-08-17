//! AgentSession 依赖的外部能力端口。
//!
//! 本模块只定义方向稳定的抽象，不依赖 Provider、工具运行时、TUI 或 RPC。
//! 这样 Agent Loop、持久化实现和上层传输层可以分别演进，不会反向耦合
//! `AgentSession` 的领域状态机。

use agent_loop::{AgentLoopEngine, AgentLoopEvent as RustAgentLoopEvent};
use protocol::UserTranscriptItem;

/// Agent Loop 执行失败的稳定错误表示。
///
/// 错误文本仅用于诊断；对 Host 暴露时由 AgentSession 映射为共享协议错误，
/// 不泄露具体 Provider 或工具实现的错误类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLoopError {
    message: String,
}

impl AgentLoopError {
    /// 用可安全展示的错误消息创建错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// 返回错误说明。
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// `AgentSession` 调用 Agent Loop 所需的最小命令面。
///
/// 该 trait 刻意不规定 Provider 请求、工具执行或流式事件的内部实现。后续
/// Rust Agent Loop 可以实现它；测试则可使用轻量 fake 来验证 Session 的状态
/// 转移，不需要网络、模型密钥或 TUI。
pub trait AgentLoop {
    /// 启动一个新的用户回合。
    fn prompt(&mut self, message: UserTranscriptItem) -> Result<(), AgentLoopError>;

    /// 将用户输入插入当前运行中的回合。
    fn steer(&mut self, message: UserTranscriptItem) -> Result<(), AgentLoopError>;

    /// 请求停止当前回合。停止完成仍由后续 `settled` 事件确认。
    fn abort(&mut self) -> Result<(), AgentLoopError>;
}

/// Rust Agent Loop 到 AgentSession 端口的薄适配器。
///
/// 该适配器只转换错误类型，不吞掉 [`AgentLoopEngine`](../../agent_loop/struct.AgentLoopEngine.html)
/// 产生的生命周期事件。Provider Host 应在每次注入 Provider 事件后读取 engine 的事件，
/// 并映射为 AgentSession 的 `TranscriptItemFinished` 与 `Settled` 通知。
/// 这样 `AgentSession` 继续只依赖本模块定义的最小端口，避免反向绑定具体 Loop crate。
impl AgentLoop for AgentLoopEngine {
    fn prompt(&mut self, message: UserTranscriptItem) -> Result<(), AgentLoopError> {
        AgentLoopEngine::prompt(self, message).map_err(|error| AgentLoopError::new(error.message()))
    }

    fn steer(&mut self, message: UserTranscriptItem) -> Result<(), AgentLoopError> {
        AgentLoopEngine::steer(self, message).map_err(|error| AgentLoopError::new(error.message()))
    }

    fn abort(&mut self) -> Result<(), AgentLoopError> {
        AgentLoopEngine::abort(self).map_err(|error| AgentLoopError::new(error.message()))
    }
}

/// 将 Rust Agent Loop 的稳定生命周期事件投影为 AgentSession 需要的端口事件。
///
/// 流式开始和增量由 RPC/UI 使用原始 Loop 事件展示，不应写入 JSONL；只有终态
/// transcript 项和回合结束会进入 AgentSession，从而保持 TypeScript `message_end`
/// 后持久化、`agent_end` 后 settled 的顺序。
pub fn session_events_from_rust_agent_loop(
    events: impl IntoIterator<Item = RustAgentLoopEvent>,
) -> Vec<crate::agent_session::AgentLoopEvent> {
    events
        .into_iter()
        .filter_map(|event| match event {
            RustAgentLoopEvent::TranscriptItemFinished(item) => {
                Some(crate::agent_session::AgentLoopEvent::TranscriptItemFinished(item))
            }
            RustAgentLoopEvent::AgentEnded { .. } => {
                Some(crate::agent_session::AgentLoopEvent::Settled)
            }
            RustAgentLoopEvent::AgentStarted
            | RustAgentLoopEvent::TurnStarted
            | RustAgentLoopEvent::TranscriptItemStarted(_)
            | RustAgentLoopEvent::TranscriptItemUpdated(_)
            | RustAgentLoopEvent::ToolExecutionStarted { .. }
            | RustAgentLoopEvent::ToolExecutionUpdated { .. }
            | RustAgentLoopEvent::ToolExecutionFinished { .. }
            | RustAgentLoopEvent::ToolResultsReady { .. }
            | RustAgentLoopEvent::TurnEnded { .. } => None,
        })
        .collect()
}

/// 持久化失败的稳定错误表示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPersistenceError {
    message: String,
}

impl SessionPersistenceError {
    /// 用可安全展示的错误消息创建错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// 返回错误说明。
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// AgentSession 写入 Session Store 的业务事件。
///
/// 此枚举避免领域层直接构造 JSONL mutation。JSONL v4、数据库或远端持久化
/// 都可以分别实现 `SessionPersistence`，而不影响 AgentSession 的命令语义。
#[derive(Debug, Clone, PartialEq)]
pub enum PersistenceEvent {
    /// 一条已稳定的 transcript 项；只在结束态消息时写入。
    TranscriptItemFinished(protocol::TranscriptItem),
    /// 模型选择变更。
    ModelChanged(protocol::ModelRef),
    /// 思考等级变更。
    ThinkingLevelChanged(protocol::ThinkingLevel),
}

/// AgentSession 所需的最小持久化端口。
///
/// 方法同步返回，是因为当前 Rust JSONL Store 同步并在返回前落盘。将来若改为
/// 异步实现，应在适配器层调度，而不改变领域状态机对“成功后再发布状态”的约束。
pub trait SessionPersistence {
    /// 持久化一个已提交的业务事件。
    fn persist(&mut self, event: PersistenceEvent) -> Result<(), SessionPersistenceError>;
}
