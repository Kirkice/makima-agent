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

    /// 会话是否正处于 Agent Loop 回合中。
    pub fn is_active(&self) -> bool {
        self.phase == SessionPhase::Turn
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
