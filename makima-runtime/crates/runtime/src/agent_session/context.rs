//! AgentSession 的可替换工作上下文。
//!
//! TypeScript `AgentSession.compact()` 与 `navigateTree()` 都会保留完整 Session
//! history，再由 Session Store 重建下一次 LLM 请求可见的消息。本模块把该结果表示为
//! 独立 DTO：领域层只接收已经准备好的 replacement，不依赖 Provider、摘要模型或 JSONL
//! 的具体字段。这样 compaction、分支导航和将来的恢复流程可以共用同一个安全边界。

use protocol::TranscriptItem;

/// 写入 Session Store 的 compaction 事实。
///
/// `summary` 与 `first_kept_entry_id` 对齐 TypeScript compaction entry 的核心字段。
/// 历史消息不会从 Store 删除；该记录只声明后续上下文应从哪个边界重建。摘要生成、
/// token 估算、Provider 认证及扩展回调均属于外部 adapter，不能进入 AgentSession。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionRecord {
    /// 已完成历史的摘要正文。
    pub summary: String,
    /// 压缩边界之后第一条仍保留在上下文中的 entry ID。
    pub first_kept_entry_id: String,
    /// 压缩前由摘要器计算的上下文 token 数。
    pub tokens_before: u64,
    /// 该摘要是否由扩展提供，而非默认摘要器生成。
    pub from_extension: bool,
}

/// 将 Agent Loop 的临时 Provider 工作上下文替换为 Store 重建的稳定消息。
///
/// 此结构刻意不包含完整 Session 历史。`messages` 是 compaction entry、摘要和保留路径
/// 共同投影得到的下一轮请求上下文；原始历史仍由 `SessionPersistence` 保存并可追溯。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionContextReplacement {
    /// 下一次 Provider 请求应看到的权威工作消息序列。
    pub messages: Vec<TranscriptItem>,
}

impl SessionContextReplacement {
    /// 用已经由 Store adapter 构建完成的消息创建替换内容。
    pub fn new(messages: Vec<TranscriptItem>) -> Self {
        Self { messages }
    }
}
