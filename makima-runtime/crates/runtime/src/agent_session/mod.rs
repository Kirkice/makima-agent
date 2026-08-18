//! Rust AgentSession 的领域编排层。
//!
//! 对应 TypeScript [`AgentSession`](../../../../../packages/coding-agent/src/core/agent-session.ts)，
//! 但不复制其将 Provider、扩展、工具、TUI 与持久化混在一起的实现方式。此处只负责
//! 命令校验、回合状态、稳定 transcript、steer 队列和快照；具体执行由 `AgentLoop`，
//! JSONL 写入由 `SessionPersistence` 提供。

mod context;
mod jsonl_persistence;
mod ports;
mod state;

pub use context::{CompactionRecord, SessionContextReplacement};
pub use jsonl_persistence::JsonlSessionPersistence;
pub use ports::{
    AgentLoop, AgentLoopError, PersistenceEvent, SessionPersistence, SessionPersistenceError,
    session_events_from_rust_agent_loop,
};
pub use state::{AgentSessionState, QueuedSteer, user_text_item};

/// 与 TypeScript `settings.retry` 对齐的自动重试策略。
///
/// `max_retries` 只统计首次 Provider 请求之后的额外尝试次数；退避为
/// `base_delay_ms * 2^(attempt - 1)`。时间等待由 Provider Runtime 注入，领域层只
/// 计算确定性的截止时间，避免在状态机中执行 sleep 或引入线程阻塞。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub enabled: bool,
    pub max_retries: u32,
    pub base_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retries: 3,
            base_delay_ms: 2_000,
        }
    }
}

/// 已接受的 retry 退避计划。运行时应在 `retry_at` 之后调用
/// [`AgentSession::resume_retry_at`]，并且不得在此之前重叠发送 Provider 请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetrySchedule {
    pub attempt: u32,
    pub retry_at: u64,
}

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
        /// 尚待当前回合自然停止后消费的 follow-up 项数。
        queued_follow_up_count: u64,
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
    /// Agent Loop 已消费一个 follow-up 输入。
    FollowUpConsumed,
    /// 当前回合及其后续工具循环都已结束。
    Settled,
}

/// AgentSession 的构造参数。
#[derive(Debug, Clone)]
pub struct AgentSessionConfig {
    /// 稳定的会话 ID。
    pub id: String,
    /// 可选显示名称。
    pub name: Option<String>,
    /// 会话工作目录。
    pub cwd: String,
    /// 初始模型。
    pub model: ModelRef,
    /// 初始思考等级。
    pub thinking_level: ThinkingLevel,
    /// 创建时间，单位为 Unix 毫秒。
    pub created_at: u64,
    /// 当前 Session 的自动重试策略；默认值对齐 TypeScript SettingsManager。
    pub retry_policy: RetryPolicy,
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
    retry_policy: RetryPolicy,
    retry_attempt: u32,
    retry_schedule: Option<RetrySchedule>,
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
                config.name,
                config.cwd,
                config.model,
                config.created_at,
                config.thinking_level,
            ),
            agent_loop,
            persistence,
            events: Vec::new(),
            next_user_message_sequence: 0,
            retry_policy: config.retry_policy,
            retry_attempt: 0,
            retry_schedule: None,
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

    /// 返回当前 retry 尝试编号；没有处于重试链时为零。
    pub fn retry_attempt(&self) -> u32 {
        self.retry_attempt
    }

    /// 返回已计划但尚未启动的 retry；由运行时轮询其截止时间。
    pub fn retry_schedule(&self) -> Option<RetrySchedule> {
        self.retry_schedule
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
            Command::FollowUp { text, .. } => self.follow_up(text, timestamp)?,
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
                // 与 TypeScript 在成功 `message_end` 时结束 retry 链一致：只有完整的
                // assistant 回答证明瞬态失败已经恢复。错误 assistant 会保留历史，但不会
                // 清零计数，以便同一回合继续遵守 max_retries 上限。
                if matches!(
                    item,
                    TranscriptItem::Assistant(protocol::AssistantTranscriptItem::Complete { .. })
                ) {
                    self.retry_attempt = 0;
                }
                self.state.finish_transcript_item(item, timestamp);
                self.emit_snapshot();
            }
            AgentLoopEvent::SteerConsumed => {
                self.state.consume_steer(timestamp);
                let snapshot = self.state.snapshot();
                self.events.push(AgentSessionEvent::QueueUpdated {
                    queued_steer_count: snapshot.queued_steer_count,
                    queued_follow_up_count: snapshot.queued_follow_up_count,
                });
                self.emit_snapshot();
            }
            AgentLoopEvent::FollowUpConsumed => {
                self.state.consume_follow_up(timestamp);
                let snapshot = self.state.snapshot();
                self.events.push(AgentSessionEvent::QueueUpdated {
                    queued_steer_count: snapshot.queued_steer_count,
                    queued_follow_up_count: snapshot.queued_follow_up_count,
                });
                self.emit_snapshot();
            }
            AgentLoopEvent::Settled => {
                self.retry_schedule = None;
                self.state.settle(timestamp);
                self.events.push(AgentSessionEvent::Settled);
                self.emit_snapshot();
            }
        }

        Ok(self.snapshot())
    }

    /// 根据刚刚落盘的失败 assistant 计划下一次 retry。
    ///
    /// 错误先通过 AgentSession 持久化，再由该方法从 Agent Loop 的工作上下文移除；这样
    /// 历史完整、下一次 Provider request 又不会把失败 assistant 发送回模型。
    pub fn schedule_retry_at(
        &mut self,
        error_message: &str,
        timestamp: u64,
    ) -> Result<Option<RetrySchedule>, ProtocolError> {
        if !self.state.is_active()
            || !is_retryable_error(error_message)
            || self.retry_attempt >= self.retry_policy.max_retries
            || !self.retry_policy.enabled
        {
            return Ok(None);
        }

        self.retry_attempt += 1;
        let delay_ms = self
            .retry_policy
            .base_delay_ms
            .saturating_mul(2_u64.saturating_pow(self.retry_attempt.saturating_sub(1)));
        let schedule = RetrySchedule {
            attempt: self.retry_attempt,
            retry_at: timestamp.saturating_add(delay_ms),
        };
        self.agent_loop
            .discard_last_error_assistant_for_retry()
            .map_err(agent_loop_error)?;
        self.state.start_retry(timestamp);
        self.retry_schedule = Some(schedule);
        self.emit_snapshot();
        Ok(Some(schedule))
    }

    /// 在到期后恢复 Agent Loop，供 Provider Runtime 发起新的 Provider request。
    pub fn resume_retry_at(&mut self, timestamp: u64) -> Result<bool, ProtocolError> {
        let Some(schedule) = self.retry_schedule else {
            return Ok(false);
        };
        if timestamp < schedule.retry_at || !self.state.is_retrying() {
            return Ok(false);
        }

        self.agent_loop
            .restart_after_retry()
            .map_err(agent_loop_error)?;
        self.retry_schedule = None;
        self.state.resume_retry(timestamp);
        self.emit_snapshot();
        Ok(true)
    }

    /// 持久化已生成的 compaction 边界，并替换下一轮请求的工作上下文。
    ///
    /// 该 API 是 TypeScript `compact()` 中“appendCompaction -> buildSessionContext ->
    /// agent.state.messages”这一同步提交段的领域等价物。摘要计算和分支读取由调用方的
    /// adapter 完成；本方法只在空闲边界提交已验证结果，避免 AgentSession 依赖模型 SDK。
    ///
    /// 提交顺序不可调换：先落盘 compaction 事实，再替换 Loop 上下文。若落盘失败，Loop
    /// 保持原状；若替换失败，历史仍可从已落盘 entry 重建，绝不删除原 transcript。
    pub fn apply_compaction(
        &mut self,
        record: CompactionRecord,
        replacement: SessionContextReplacement,
    ) -> Result<(), ProtocolError> {
        if self.state.is_active() {
            return Err(busy(
                "Agent 正在运行，必须等待回合 settled 后才能替换工作上下文。",
            ));
        }
        if replacement.messages.is_empty() {
            return Err(invalid_request("压缩后的工作上下文不能为空。"));
        }

        self.persist(PersistenceEvent::Compaction(record))?;
        self.agent_loop
            .replace_context(replacement.messages)
            .map_err(agent_loop_error)?;
        Ok(())
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
        let snapshot = self.state.snapshot();
        self.events.push(AgentSessionEvent::QueueUpdated {
            queued_steer_count: snapshot.queued_steer_count,
            queued_follow_up_count: snapshot.queued_follow_up_count,
        });
        self.emit_snapshot();
        Ok(())
    }

    fn follow_up(&mut self, text: String, timestamp: u64) -> Result<(), ProtocolError> {
        if !self.state.is_active() {
            return Err(invalid_request(
                "Agent 当前空闲，不能 follow-up；请使用 prompt 启动新回合。",
            ));
        }

        let message = self.new_user_message(text, timestamp);
        self.agent_loop
            .follow_up(message.clone())
            .map_err(agent_loop_error)?;
        self.state.enqueue_follow_up(message, timestamp);
        let snapshot = self.state.snapshot();
        self.events.push(AgentSessionEvent::QueueUpdated {
            queued_steer_count: snapshot.queued_steer_count,
            queued_follow_up_count: snapshot.queued_follow_up_count,
        });
        self.emit_snapshot();
        Ok(())
    }

    fn abort(&mut self, timestamp: u64) -> Result<(), ProtocolError> {
        if !self.state.is_active() {
            return Ok(());
        }

        // retry 的 sleep 不在领域层执行，取消只需清除计划并让状态稳定回 idle；不会再有
        // Provider 流或工具批次等待结算。这与 TS AbortController 中断 backoff 的语义对应。
        if self.state.is_retrying() {
            self.retry_schedule = None;
            self.retry_attempt = 0;
            self.state.settle(timestamp);
            self.events.push(AgentSessionEvent::Settled);
            self.emit_snapshot();
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
            | Command::FollowUp { session_id, .. }
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

/// 与 TypeScript `isRetryableAssistantError()` 保持同一类瞬态 Provider / transport 信号。
///
/// 上下文溢出含有 "context"、"prompt too long" 等确定性容量信号，不能在这里重试；它应
/// 交给后续 compaction 策略处理。账户配额和 billing 也属于用户动作才能恢复的终态。
fn is_retryable_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    let non_retryable = [
        "insufficient_quota",
        "quota exceeded",
        "out of budget",
        "billing",
        "monthly usage limit reached",
        "available balance",
        "context window",
        "maximum context",
        "prompt too long",
        "input is too long",
        "context length",
    ];
    if non_retryable
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return false;
    }

    [
        "overloaded",
        "rate limit",
        "rate-limit",
        "too many requests",
        "429",
        "500",
        "502",
        "503",
        "504",
        "524",
        "service unavailable",
        "server error",
        "internal error",
        "provider returned error",
        "network error",
        "connection error",
        "connection refused",
        "connection lost",
        "fetch failed",
        "getaddrinfo",
        "enotfound",
        "eai_again",
        "timeout",
        "timed out",
        "socket hang up",
        "websocket closed",
        "ended without",
        "stream ended before",
        "retry delay",
        "please retry",
        "try your request again",
        "resourceexhausted",
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
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
