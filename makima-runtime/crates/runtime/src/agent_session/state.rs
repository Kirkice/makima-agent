//! AgentSession 的纯领域状态。
//!
//! 该模块不持有端口实现，也不执行 I/O。所有转换均以不可变输入和明确输出
//! 表示，使并发控制、RPC 适配和单元测试都可以围绕同一套规则实现。

use protocol::{
    ModelRef, SessionPhase, SessionSnapshot, ThinkingLevel, TranscriptItem, UserRole,
    UserTranscriptItem,
};

/// 一条尚未被 Agent Loop 消费的 steer 消息。
///
/// 队列保留完整项目用于快照，`queued_steer` 则提供 TypeScript 协议已约定的
/// 文本视图。两者从同一来源投影，避免界面显示与实际队列不一致。
#[derive(Debug, Clone, PartialEq)]
pub struct QueuedSteer {
    /// 待插入运行中回合的用户消息。
    pub message: UserTranscriptItem,
}

/// 一条尚未被 Agent Loop 消费的 follow-up 消息。
///
/// follow-up 只会在当前 assistant、工具和 steering 全部自然完成后消费，因此不能复用
/// steering 队列；保留独立投影能让 RPC 客户端准确呈现其延后执行的语义。
#[derive(Debug, Clone, PartialEq)]
pub struct QueuedFollowUp {
    /// 待在下一轮外层循环中投递的用户消息。
    pub message: UserTranscriptItem,
}

/// AgentSession 的可变领域状态。
#[derive(Debug, Clone)]
pub struct AgentSessionState {
    id: String,
    name: Option<String>,
    cwd: String,
    created_at: u64,
    updated_at: u64,
    phase: SessionPhase,
    model: ModelRef,
    thinking_level: ThinkingLevel,
    attached: bool,
    locked: bool,
    revision: u64,
    transcript: Vec<TranscriptItem>,
    queued_steer: Vec<QueuedSteer>,
    queued_follow_up: Vec<QueuedFollowUp>,
}

impl AgentSessionState {
    /// 创建处于空闲状态的新会话。
    pub fn new(
        id: String,
        name: Option<String>,
        cwd: String,
        model: ModelRef,
        created_at: u64,
        thinking_level: ThinkingLevel,
    ) -> Self {
        Self {
            id,
            name,
            cwd,
            created_at,
            updated_at: created_at,
            phase: SessionPhase::Idle,
            model,
            thinking_level,
            attached: false,
            locked: false,
            revision: 0,
            transcript: Vec::new(),
            queued_steer: Vec::new(),
            queued_follow_up: Vec::new(),
        }
    }

    /// 返回会话标识。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 返回当前执行阶段。
    pub fn phase(&self) -> SessionPhase {
        self.phase
    }

    /// 返回当前模型。
    pub fn model(&self) -> &ModelRef {
        &self.model
    }

    /// 返回当前思考等级。
    pub fn thinking_level(&self) -> ThinkingLevel {
        self.thinking_level
    }

    /// 返回不可变 transcript 视图。
    pub fn transcript(&self) -> &[TranscriptItem] {
        &self.transcript
    }

    /// 会话是否仍在处理当前用户回合。
    ///
    /// `Retry` 虽然暂时没有活动的 Provider 流，但退避结束后会恢复同一份上下文，
    /// 因而对 prompt/steer/follow-up 的并发校验必须和 `Turn` 一样视为活动状态。
    pub fn is_active(&self) -> bool {
        matches!(self.phase, SessionPhase::Turn | SessionPhase::Retry)
    }

    /// 会话是否正在等待一次自动重试的退避时间。
    pub fn is_retrying(&self) -> bool {
        self.phase == SessionPhase::Retry
    }

    /// 变更模型并推进版本。调用方应先完成外部持久化。
    pub fn set_model(&mut self, model: ModelRef, timestamp: u64) {
        self.model = model;
        self.touch(timestamp);
    }

    /// 变更思考等级并推进版本。调用方应先完成外部持久化。
    pub fn set_thinking_level(&mut self, level: ThinkingLevel, timestamp: u64) {
        self.thinking_level = level;
        self.touch(timestamp);
    }

    /// 标记新回合已经被 Agent Loop 接受。
    pub fn start_run(&mut self, timestamp: u64) {
        self.phase = SessionPhase::Turn;
        self.touch(timestamp);
    }

    /// 进入自动重试退避阶段。
    ///
    /// 错误 assistant 已经是稳定历史的一部分，不能从 Session transcript 删除；真正供
    /// Provider 重试的上下文则由 Agent Loop 单独移除该错误项。二者分离与 TypeScript
    /// “保留 session history、从 agent state 移除错误消息”的行为一致。
    pub fn start_retry(&mut self, timestamp: u64) {
        self.phase = SessionPhase::Retry;
        self.touch(timestamp);
    }

    /// 退避结束，重新进入实际执行 Provider 请求的回合阶段。
    pub fn resume_retry(&mut self, timestamp: u64) {
        self.phase = SessionPhase::Turn;
        self.touch(timestamp);
    }

    /// 将已提交到 Agent Loop 的 steer 加入本地展示队列。
    pub fn enqueue_steer(&mut self, message: UserTranscriptItem, timestamp: u64) {
        self.queued_steer.push(QueuedSteer { message });
        self.touch(timestamp);
    }

    /// 当 Agent Loop 已消费一条 steer 时移除最早的本地项。
    ///
    /// 消费通知可能在回合结束后才到达，因此空队列时保持幂等，避免重复事件
    /// 将 Session 置于错误状态。
    pub fn consume_steer(&mut self, timestamp: u64) {
        if !self.queued_steer.is_empty() {
            self.queued_steer.remove(0);
            self.touch(timestamp);
        }
    }

    /// 将已提交到 Agent Loop 的 follow-up 加入本地展示队列。
    pub fn enqueue_follow_up(&mut self, message: UserTranscriptItem, timestamp: u64) {
        self.queued_follow_up.push(QueuedFollowUp { message });
        self.touch(timestamp);
    }

    /// 当 Agent Loop 已消费一条 follow-up 时移除最早的本地项。
    ///
    /// 与 steering 一样保持幂等，以抵御重放或取消边界的重复通知。
    pub fn consume_follow_up(&mut self, timestamp: u64) {
        if !self.queued_follow_up.is_empty() {
            self.queued_follow_up.remove(0);
            self.touch(timestamp);
        }
    }

    /// 追加一个完成态 transcript 项。
    ///
    /// AgentSession 只应在收到 finished 事件后调用它。流式项目由进度事件直接
    /// 发送给 Host，不能提前持久化为稳定历史，保持与 TypeScript `message_end`
    /// 的写入时机一致。
    pub fn finish_transcript_item(&mut self, item: TranscriptItem, timestamp: u64) {
        self.transcript.push(item);
        self.touch(timestamp);
    }

    /// 在 Agent Loop 确认空闲后结束当前回合。
    pub fn settle(&mut self, timestamp: u64) {
        self.phase = SessionPhase::Idle;
        self.touch(timestamp);
    }

    /// 生成不可变协议快照。
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            id: self.id.clone(),
            name: self.name.clone(),
            cwd: self.cwd.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            phase: self.phase,
            model: self.model.clone(),
            thinking_level: self.thinking_level,
            attached: self.attached,
            locked: self.locked,
            revision: self.revision,
            transcript: self.transcript.clone(),
            queued_steer: self
                .queued_steer
                .iter()
                .map(|queued| queued.message.clone())
                .collect(),
            queued_steer_count: self.queued_steer.len() as u64,
            queued_follow_up: self
                .queued_follow_up
                .iter()
                .map(|queued| queued.message.clone())
                .collect(),
            queued_follow_up_count: self.queued_follow_up.len() as u64,
        }
    }

    /// 将时间戳和版本号作为同一个领域提交的一部分更新。
    /// 记录 abort 请求而不结束回合。
    ///
    /// 停止请求只表示已向 Agent Loop 发出取消信号；必须等待 Loop 的 settled
    /// 事件，才能把 phase 切回 idle。
    pub(crate) fn mark_abort_requested(&mut self, timestamp: u64) {
        self.touch(timestamp);
    }

    fn touch(&mut self, timestamp: u64) {
        self.updated_at = timestamp;
        self.revision += 1;
    }
}

/// 创建协议中标准的纯文本用户项。
pub fn user_text_item(id: String, text: String, timestamp: u64) -> UserTranscriptItem {
    UserTranscriptItem {
        id,
        role: UserRole::User,
        content: vec![protocol::TextOrImageContent::Text { text }],
        timestamp,
    }
}
