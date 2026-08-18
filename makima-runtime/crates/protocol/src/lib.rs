//! Pi Agent 的 Rust/TypeScript 共享线协议模型。
//!
//! [`packages/protocol/src/schemas.ts`](../../../packages/protocol/src/schemas.ts) 是协议字段、
//! 可选性与版本的事实来源。本 crate 只定义能够穿过 Rust Core 与 TypeScript Host
//! 进程边界的 DTO（数据传输对象），不承载 Agent、TUI 或 Provider 的业务逻辑。
//! 这样可避免两个进程共享内部对象，同时让后续 RPC 层只依赖稳定的 JSON/CBOR 数据。

/// CBOR payload 编解码，使用具体 DTO 类型维护协议边界。
pub mod cbor;
/// 长度前缀帧编解码，独立于具体的 CBOR payload 类型。
pub mod framing;

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// 当前 Rust/TypeScript 共享协议版本。
///
/// `follow_up` command 与快照中的独立 follow-up 队列新增了严格必填字段，旧客户端不能
/// 正确解码；因此本次使用握手版本 2 显式拒绝不兼容的 peer，而不伪造向后兼容。
pub const PROTOCOL_VERSION: u32 = 2;

/// 协议中使用的思考等级，必须与 TypeScript `ThinkingLevelSchema` 同步。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

/// Agent 当前所处的生命周期阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Idle,
    Turn,
    Compaction,
    BranchSummary,
    Retry,
}

/// 模型的稳定外部引用；Provider Host 可据此选择实际 SDK 实现。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    pub provider: String,
    pub id: String,
}

/// 模型调用成本。字段名必须与 TypeScript `ModelCostSchema` 保持一致。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

/// 可由 Host 展示和选择的模型元数据。
///
/// Provider SDK 仍由 TypeScript Host 实现；Rust Core 只传递经协议约束的描述数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMetadata {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub api: String,
    pub reasoning: bool,
    pub input: Vec<ModelInput>,
    pub context_window: u64,
    pub max_tokens: u64,
    pub cost: ModelCost,
    pub supported_thinking_levels: Vec<ThinkingLevel>,
    pub authenticated: bool,
}

/// 模型接受的输入模态。MVP 仅发送文本，但保留 image 以匹配协议契约。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelInput {
    Text,
    Image,
}

/// 用户或工具输出允许的内容块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TextOrImageContent {
    Text { text: String },
    Image { data: String, mime_type: String },
}

/// Assistant 内容块。工具调用的输入只能承载 JSON 值，不能跨边界传递内部对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AssistantContent {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        redacted: Option<bool>,
    },
    ToolCall {
        tool_call_id: String,
        tool_name: String,
        input: Value,
    },
}

/// Provider Host 完成解析后交给 Tool Runtime 的单个工具调用。
///
/// 与 [`AssistantContent::ToolCall`] 保持相同字段，但独立 DTO 避免执行边界依赖
/// assistant transcript 的内容块布局。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolCall {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: Value,
}

/// Tool Runtime 返回给 Agent Loop 的稳定工具执行结果。
///
/// `is_error` 由运行时显式声明，调用方不能通过内容文本猜测成功与否。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: Value,
    pub content: Vec<TextOrImageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    pub is_error: bool,
    pub timestamp: u64,
}

/// 单个工具在同一 assistant 工具批次中的并发约束。
///
/// 默认的 [`ToolExecutionMode::Parallel`] 允许互不依赖的调用同时执行；只要批次中存在
/// `Sequential` 工具，整个批次都必须按 Provider 给出的源顺序串行执行。这与 TypeScript
/// Agent Loop 的保守降级规则一致，可避免同一批有副作用调用产生竞争。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolExecutionMode {
    Parallel,
    Sequential,
}

impl Default for ToolExecutionMode {
    fn default() -> Self {
        Self::Parallel
    }
}

/// Provider 可见工具的声明。输入 schema 必须是可 JSON 序列化的数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    /// 工具批次的执行约束；省略时与 TypeScript 一样默认为并行。
    #[serde(default, skip_serializing_if = "is_parallel_tool_execution")]
    pub execution_mode: ToolExecutionMode,
}

fn is_parallel_tool_execution(mode: &ToolExecutionMode) -> bool {
    *mode == ToolExecutionMode::Parallel
}

/// 发往 TypeScript Provider Host 的不可变请求快照。
///
/// Host 负责认证、SDK 调用与取消；Rust Core 仅提供稳定的模型、上下文和工具描述。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRequest {
    pub request_id: String,
    pub model: ModelRef,
    pub system_prompt: String,
    pub messages: Vec<TranscriptItem>,
    pub tools: Vec<ToolDefinition>,
}

/// Provider Host 归一化后的流事件。
///
/// 该联合刻意不复用 TypeScript Provider SDK 的内部消息类型。增量只负责实时进度；终态
/// 携带 Provider SDK 的稳定消息快照，使 Rust 与 TypeScript 都以同一个 final result 为准，
/// 并完整保留 usage、response model 与 Provider 最后修正的内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProviderStreamEvent {
    Start {
        message_id: String,
        timestamp: u64,
    },
    TextDelta {
        content_index: u64,
        delta: String,
    },
    ThinkingDelta {
        content_index: u64,
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        redacted: Option<bool>,
    },
    ToolCallDelta {
        content_index: u64,
        delta: String,
    },
    ToolCallEnd {
        content_index: u64,
        tool_call: ToolCall,
    },
    Done {
        message_id: String,
        content: Vec<AssistantContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_model: Option<String>,
        usage: Usage,
        timestamp: u64,
        stop_reason: AssistantStopReason,
    },
    Error {
        message_id: String,
        content: Vec<AssistantContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        message: String,
    },
}

/// Rust Core 发往 TypeScript Provider Host 的进程消息。
///
/// 该通道复用协议的 CBOR 与长度前缀 framing，但独立于面向客户端的 RPC 命令。一个
/// `request_id` 在 Host 存活期间只能对应一次 request；`abort` 可重复发送且必须幂等。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProviderHostRequest {
    Request { request: ProviderRequest },
    Abort { request_id: String },
}

/// TypeScript Provider Host 回传给 Rust Core 的进程消息。
///
/// 每个请求先产生零个或多个 `event`，再以唯一的 `complete` 收尾。Host 无法解析或执行
/// 请求时会先发送共享的 `ProviderStreamEvent::Error`，随后仍发送 `complete`；Core 因而
/// 无需为单请求错误建立第二套状态机。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProviderHostResponse {
    Event {
        request_id: String,
        event: ProviderStreamEvent,
    },
    Complete {
        request_id: String,
    },
}

/// 统一用量对象，与 TypeScript `UsageSchema` 的字段和 JSON 命名保持一致。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsageCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<u64>,
    pub total_tokens: u64,
    pub cost: UsageCost,
}

/// 用户 transcript 项。用户只能提交文本或图片内容，且没有运行状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserTranscriptItem {
    pub id: String,
    pub role: UserRole,
    pub content: Vec<TextOrImageContent>,
    pub timestamp: u64,
}

/// Assistant 的流式、完成、异常和中止项。
///
/// `status` 是内部标签；各变体只暴露 TypeScript Schema 对应状态允许出现的字段，
/// 因而无法构造例如 `streaming + stopReason` 这样的无效组合。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AssistantTranscriptItem {
    Streaming {
        id: String,
        role: AssistantRole,
        content: Vec<AssistantContent>,
        model: ModelRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
    },
    Complete {
        id: String,
        role: AssistantRole,
        content: Vec<AssistantContent>,
        model: ModelRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        stop_reason: AssistantStopReason,
    },
    Error {
        id: String,
        role: AssistantRole,
        content: Vec<AssistantContent>,
        model: ModelRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        #[serde(rename = "stopReason")]
        stop_reason: ErrorStopReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
    },
    Aborted {
        id: String,
        role: AssistantRole,
        content: Vec<AssistantContent>,
        model: ModelRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        #[serde(rename = "stopReason")]
        stop_reason: AbortedStopReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
    },
}

/// 工具执行的运行中、完成和异常项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ToolTranscriptItem {
    Running {
        id: String,
        role: ToolRole,
        tool_call_id: String,
        tool_name: String,
        input: Value,
        content: Vec<TextOrImageContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        #[serde(rename = "isError", deserialize_with = "deserialize_false")]
        is_error: bool,
    },
    Complete {
        id: String,
        role: ToolRole,
        tool_call_id: String,
        tool_name: String,
        input: Value,
        content: Vec<TextOrImageContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        #[serde(rename = "isError", deserialize_with = "deserialize_false")]
        is_error: bool,
    },
    Error {
        id: String,
        role: ToolRole,
        tool_call_id: String,
        tool_name: String,
        input: Value,
        content: Vec<TextOrImageContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        #[serde(rename = "isError", deserialize_with = "deserialize_true")]
        is_error: bool,
    },
}

/// 跨进程传输的完整 transcript 判别联合。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TranscriptItem {
    User(UserTranscriptItem),
    Assistant(AssistantTranscriptItem),
    Tool(ToolTranscriptItem),
}

/// 仅允许的固定 role 值，确保反序列化时拒绝角色与项目类型不匹配的负载。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserRole {
    #[serde(rename = "user")]
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistantRole {
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolRole {
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssistantStopReason {
    Stop,
    Length,
    ToolUse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorStopReason {
    #[serde(rename = "error")]
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbortedStopReason {
    #[serde(rename = "aborted")]
    Aborted,
}

/// 已持久化、可在列表中安全展示的 Session 元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionMetadata {
    pub id: String,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// Rust Core 当前对外公布的完整 Session 快照外壳。
///
/// transcript 与 queuedSteer 中的单项暂以 JSON 值承载，防止在 Agent Loop 尚未
/// 迁移时复制 TypeScript 的内部消息类。RPC 层仍须用 TypeScript schema 校验这些
/// 值；后续可在 provider replay fixture 稳定后将其细化为 Rust 判别联合类型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionSnapshot {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub cwd: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub phase: SessionPhase,
    pub model: ModelRef,
    pub thinking_level: ThinkingLevel,
    pub attached: bool,
    pub locked: bool,
    pub revision: u64,
    pub transcript: Vec<TranscriptItem>,
    pub queued_steer: Vec<UserTranscriptItem>,
    pub queued_steer_count: u64,
    /// 当前回合自然结束后才会投递的输入。它与 steering 分开，确保客户端可以按实际
    /// 调度时机展示队列，而不是把 follow-up 误认为可立即插入下一次 Provider 请求。
    pub queued_follow_up: Vec<UserTranscriptItem>,
    pub queued_follow_up_count: u64,
}

/// Agent 可以接收的命令集合。
///
/// `rename_all_fields` 确保 Rust 的 snake_case 字段在协议中保持既有 camelCase，
/// 例如 `session_id` 编码为 `sessionId`。命令名称则维持已有的 snake_case 字面量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "command",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Command {
    List,
    Create {
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<ModelRef>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thinking_level: Option<ThinkingLevel>,
    },
    Attach {
        session_id: String,
    },
    Detach {
        session_id: String,
    },
    Prompt {
        session_id: String,
        text: String,
    },
    Steer {
        session_id: String,
        text: String,
    },
    /// 在当前回合（含所有工具与 steering）自然完成后继续执行的用户输入。
    FollowUp {
        session_id: String,
        text: String,
    },
    Abort {
        session_id: String,
    },
    SetModel {
        session_id: String,
        model: ModelRef,
    },
    SetThinking {
        session_id: String,
        thinking_level: ThinkingLevel,
    },
}

/// 命令成功后的结果联合，与 TypeScript `CommandResultSchema` 一一对应。
///
/// 每个变体保留原命令名作为判别字段，调用方无需依赖 Rust enum 的内部名称。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "command",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum CommandResult {
    List { sessions: Vec<SessionMetadata> },
    Create { session: SessionSnapshot },
    Attach { session: SessionSnapshot },
    Detach { session_id: String },
    Prompt { session: SessionSnapshot },
    Steer { session: SessionSnapshot },
    FollowUp { session: SessionSnapshot },
    Abort { session: SessionSnapshot },
    SetModel { session: SessionSnapshot },
    SetThinking { session: SessionSnapshot },
}

/// 所有协议错误必须使用有限错误码，禁止把 Rust error enum 名称泄露到 Host。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolErrorCode {
    Version,
    Busy,
    SessionLocked,
    NotFound,
    InvalidRequest,
    NotImplemented,
    InternalError,
}

impl fmt::Display for ProtocolErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Version => "version",
            Self::Busy => "busy",
            Self::SessionLocked => "session_locked",
            Self::NotFound => "not_found",
            Self::InvalidRequest => "invalid_request",
            Self::NotImplemented => "not_implemented",
            Self::InternalError => "internal_error",
        })
    }
}

/// 统一错误对象。`details` 只承载 JSON 安全的数据，不传递堆栈或内部对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolError {
    pub code: ProtocolErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

/// TypeScript Host 发出的版本协商消息，必须是每条连接的第一帧。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientHello {
    #[serde(rename = "type")]
    pub message_type: HelloMessageType,
    pub version: u32,
}

/// 请求 envelope 将调用标识与业务命令分离，支持并发请求和错误关联。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestEnvelope {
    #[serde(rename = "type")]
    pub message_type: RequestMessageType,
    pub id: String,
    pub request: Command,
}

/// 客户端消息的封闭联合，解码未知类型会失败。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    Hello { version: u32 },
    Request { id: String, request: Command },
}

/// 增量 transcript 事件。快照仍是权威状态，进度仅用于低延迟渲染。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TranscriptProgress {
    ItemStarted {
        item: TranscriptItem,
    },
    AssistantDelta {
        message_id: String,
        content_index: u64,
        kind: TranscriptDeltaKind,
        delta: String,
    },
    ItemUpdated {
        item: ActiveTranscriptItem,
    },
    ItemFinished {
        item: FinishedTranscriptItem,
    },
}

/// 可持续更新的 assistant 或工具项目；用户项目不能产生更新事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActiveTranscriptItem {
    Assistant(AssistantTranscriptItem),
    Tool(ToolTranscriptItem),
}

/// 已到达终态的 assistant 或工具项目；运行中的项目不能发出 finished 事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FinishedTranscriptItem {
    AssistantComplete(CompleteAssistantTranscriptItem),
    AssistantError(ErrorAssistantTranscriptItem),
    AssistantAborted(AbortedAssistantTranscriptItem),
    ToolComplete(CompleteToolTranscriptItem),
    ToolError(ErrorToolTranscriptItem),
}

/// `TranscriptProgress::ItemFinished` 使用的完成态 assistant 项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CompleteAssistantTranscriptItem {
    Complete {
        id: String,
        role: AssistantRole,
        content: Vec<AssistantContent>,
        model: ModelRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        stop_reason: AssistantStopReason,
    },
}

/// `TranscriptProgress::ItemFinished` 使用的异常 assistant 项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ErrorAssistantTranscriptItem {
    Error {
        id: String,
        role: AssistantRole,
        content: Vec<AssistantContent>,
        model: ModelRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        #[serde(rename = "stopReason")]
        stop_reason: ErrorStopReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
    },
}

/// `TranscriptProgress::ItemFinished` 使用的中止 assistant 项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AbortedAssistantTranscriptItem {
    Aborted {
        id: String,
        role: AssistantRole,
        content: Vec<AssistantContent>,
        model: ModelRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        #[serde(rename = "stopReason")]
        stop_reason: AbortedStopReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
    },
}

/// `TranscriptProgress::ItemFinished` 使用的完成工具项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CompleteToolTranscriptItem {
    Complete {
        id: String,
        role: ToolRole,
        tool_call_id: String,
        tool_name: String,
        input: Value,
        content: Vec<TextOrImageContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        #[serde(rename = "isError", deserialize_with = "deserialize_false")]
        is_error: bool,
    },
}

/// `TranscriptProgress::ItemFinished` 使用的异常工具项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ErrorToolTranscriptItem {
    Error {
        id: String,
        role: ToolRole,
        tool_call_id: String,
        tool_name: String,
        input: Value,
        content: Vec<TextOrImageContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: u64,
        #[serde(rename = "isError", deserialize_with = "deserialize_true")]
        is_error: bool,
    },
}

/// Assistant 流式增量所影响的内容块种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptDeltaKind {
    Text,
    Thinking,
    ToolCall,
}

/// Server 可推送的 Session 事件。
///
/// progress 的细粒度类型依赖 Agent Loop replay fixture，当前仅保留 JSON 边界，
/// 使协议 crate 不反向依赖尚未迁移的 Agent 消息模型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ServerEvent {
    ServerSnapshot {
        snapshot: ServerSnapshot,
    },
    SessionSnapshot {
        snapshot: SessionSnapshot,
    },
    SessionProgress {
        session_id: String,
        progress: TranscriptProgress,
    },
    SessionRemoved {
        session_id: String,
    },
}

/// 服务端的全局快照，为新连接建立一致的初始状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerSnapshot {
    pub server_id: String,
    pub protocol_version: u32,
    pub revision: u64,
    pub sessions: Vec<SessionMetadata>,
    pub models: Vec<ModelMetadata>,
}

/// Rust Core 对版本协商成功后的应答。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerHello {
    #[serde(rename = "type")]
    pub message_type: HelloMessageType,
    pub version: u32,
    pub connection_id: String,
    pub snapshot: ServerSnapshot,
}

/// 版本协商失败时的服务端应答。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerHelloError {
    #[serde(rename = "type")]
    pub message_type: HelloErrorMessageType,
    pub error: ProtocolError,
}

/// 成功响应的业务内容。
///
/// `ok` 是固定的成功标记；通过构造器创建可避免调用方错误地把成功结果标记为失败。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SuccessResponse {
    #[serde(rename = "type")]
    pub message_type: ResponseMessageType,
    pub id: String,
    #[serde(deserialize_with = "deserialize_true")]
    ok: bool,
    pub result: CommandResult,
}

impl SuccessResponse {
    /// 构造符合协议的成功响应，固定写入 `ok: true`。
    pub fn new(id: impl Into<String>, result: CommandResult) -> Self {
        Self {
            message_type: ResponseMessageType::Response,
            id: id.into(),
            ok: true,
            result,
        }
    }
}

/// 失败响应的业务内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorResponse {
    #[serde(rename = "type")]
    pub message_type: ResponseMessageType,
    pub id: String,
    #[serde(deserialize_with = "deserialize_false")]
    ok: bool,
    pub error: ProtocolError,
}

impl ErrorResponse {
    /// 构造符合协议的失败响应，固定写入 `ok: false`。
    pub fn new(id: impl Into<String>, error: ProtocolError) -> Self {
        Self {
            message_type: ResponseMessageType::Response,
            id: id.into(),
            ok: false,
            error,
        }
    }
}

/// 所有服务端消息的封闭联合。RPC Transport 只能发送此类型，避免漏掉 envelope。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerMessage {
    Hello(ServerHello),
    HelloError(ServerHelloError),
    SuccessResponse(SuccessResponse),
    ErrorResponse(ErrorResponse),
    Event(EventEnvelope),
}

/// 事件 envelope。事件本体由 `ServerEvent` 继续提供精确判别。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    #[serde(rename = "type")]
    pub message_type: EventMessageType,
    pub event: ServerEvent,
}

/// 反序列化成功响应时拒绝 `ok: false`，保持与 TypeScript 判别联合的语义一致。
fn deserialize_true<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = bool::deserialize(deserializer)?;
    if value {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "successful response requires ok: true",
        ))
    }
}

/// 反序列化失败响应时拒绝 `ok: true`，避免错误地把 error 当作成功结果处理。
fn deserialize_false<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = bool::deserialize(deserializer)?;
    if value {
        Err(serde::de::Error::custom(
            "error response requires ok: false",
        ))
    } else {
        Ok(value)
    }
}

/// 线协议中固定的 `type` 值，使用类型而不是裸字符串避免拼写漂移。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HelloMessageType {
    #[serde(rename = "hello")]
    Hello,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestMessageType {
    #[serde(rename = "request")]
    Request,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMessageType {
    #[serde(rename = "response")]
    Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HelloErrorMessageType {
    #[serde(rename = "hello_error")]
    HelloError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventMessageType {
    #[serde(rename = "event")]
    Event,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        AssistantContent, AssistantStopReason, ClientMessage, Command, CommandResult,
        ErrorResponse, EventEnvelope, ModelCost, ModelInput, ModelMetadata, ModelRef,
        ProtocolError, ProtocolErrorCode, ProviderRequest, ProviderStreamEvent, ServerEvent,
        ServerHelloError, ServerMessage, ServerSnapshot, SessionMetadata, SuccessResponse,
        TextOrImageContent, ThinkingLevel, ToolCall, ToolDefinition, ToolExecutionMode,
        TranscriptDeltaKind, TranscriptItem, TranscriptProgress, Usage, UsageCost,
        PROTOCOL_VERSION,
    };

    #[test]
    fn command_uses_typescript_field_names_and_round_trips() {
        let command = Command::SetThinking {
            session_id: "session-1".to_owned(),
            thinking_level: ThinkingLevel::High,
        };

        let encoded = serde_json::to_value(&command).expect("command should serialize");
        assert_eq!(
            encoded,
            json!({
                "command": "set_thinking",
                "sessionId": "session-1",
                "thinkingLevel": "high",
            })
        );
        assert_eq!(
            serde_json::from_value::<Command>(encoded).expect("command should deserialize"),
            command
        );
    }

    #[test]
    fn content_variant_fields_use_typescript_camel_case_names() {
        assert_eq!(
            serde_json::to_value(AssistantContent::ToolCall {
                tool_call_id: "call-1".to_owned(),
                tool_name: "read".to_owned(),
                input: json!({ "path": "hello.txt" }),
            })
            .expect("tool call content should serialize"),
            json!({
                "type": "toolCall",
                "toolCallId": "call-1",
                "toolName": "read",
                "input": { "path": "hello.txt" },
            })
        );
        assert_eq!(
            serde_json::to_value(TextOrImageContent::Image {
                data: "aGVsbG8=".to_owned(),
                mime_type: "image/png".to_owned(),
            })
            .expect("image content should serialize"),
            json!({ "type": "image", "data": "aGVsbG8=", "mimeType": "image/png" })
        );
    }

    #[test]
    fn client_envelope_matches_typescript_discriminated_union() {
        let message = ClientMessage::Request {
            id: "request-1".to_owned(),
            request: Command::Prompt {
                session_id: "session-1".to_owned(),
                text: "hello".to_owned(),
            },
        };

        assert_eq!(
            serde_json::to_value(message).expect("message should serialize"),
            json!({
                "type": "request",
                "id": "request-1",
                "request": { "command": "prompt", "sessionId": "session-1", "text": "hello" },
            })
        );
    }

    #[test]
    fn protocol_error_uses_stable_code_and_omits_absent_details() {
        let error = ProtocolError {
            code: ProtocolErrorCode::NotImplemented,
            message: "not ready".to_owned(),
            details: None,
        };
        assert_eq!(
            serde_json::to_value(error).expect("error should serialize"),
            json!({ "code": "not_implemented", "message": "not ready" })
        );
        assert_eq!(
            ProtocolErrorCode::NotImplemented.to_string(),
            "not_implemented"
        );
    }

    #[test]
    fn server_event_uses_camel_case_session_id() {
        let event = ServerEvent::SessionRemoved {
            session_id: "session-1".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(event).expect("event should serialize"),
            json!({ "type": "session_removed", "sessionId": "session-1" })
        );
    }

    #[test]
    fn metadata_uses_typescript_optional_field_names() {
        let metadata = SessionMetadata {
            id: "session-1".to_owned(),
            created_at: 1,
            updated_at: Some(2),
            parent_session_id: Some("parent-1".to_owned()),
            session_name: Some("Named session".to_owned()),
            cwd: Some("/workspace".to_owned()),
        };
        assert_eq!(
            serde_json::to_value(metadata).expect("metadata should serialize"),
            json!({
                "id": "session-1",
                "createdAt": 1,
                "updatedAt": 2,
                "parentSessionId": "parent-1",
                "sessionName": "Named session",
                "cwd": "/workspace",
            })
        );
    }

    #[test]
    fn server_snapshot_and_response_match_typescript_field_names() {
        let metadata = SessionMetadata {
            id: "session-1".to_owned(),
            created_at: 1,
            updated_at: None,
            parent_session_id: None,
            session_name: None,
            cwd: None,
        };
        let snapshot = ServerSnapshot {
            server_id: "server-1".to_owned(),
            protocol_version: PROTOCOL_VERSION,
            revision: 2,
            sessions: vec![metadata.clone()],
            models: vec![ModelMetadata {
                provider: "test".to_owned(),
                id: "model".to_owned(),
                name: "Test model".to_owned(),
                api: "responses".to_owned(),
                reasoning: true,
                input: vec![ModelInput::Text],
                context_window: 128,
                max_tokens: 64,
                cost: ModelCost {
                    input: 0.0,
                    output: 0.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                },
                supported_thinking_levels: vec![ThinkingLevel::Medium],
                authenticated: true,
            }],
        };
        let response = SuccessResponse::new(
            "request-1",
            CommandResult::List {
                sessions: vec![metadata],
            },
        );

        assert_eq!(
            serde_json::to_value(snapshot).expect("snapshot should serialize"),
            json!({
                "serverId": "server-1",
                "protocolVersion": 2,
                "revision": 2,
                "sessions": [{ "id": "session-1", "createdAt": 1 }],
                "models": [{
                    "provider": "test", "id": "model", "name": "Test model", "api": "responses",
                    "reasoning": true, "input": ["text"], "contextWindow": 128, "maxTokens": 64,
                    "cost": { "input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0 },
                    "supportedThinkingLevels": ["medium"], "authenticated": true,
                }],
            })
        );
        assert_eq!(
            serde_json::to_value(response).expect("response should serialize"),
            json!({
                "type": "response", "id": "request-1", "ok": true,
                "result": { "command": "list", "sessions": [{ "id": "session-1", "createdAt": 1 }] },
            })
        );
    }

    #[test]
    fn rejects_unknown_fields_for_strict_protocol_objects() {
        let command = json!({ "command": "abort", "sessionId": "session-1", "unexpected": true });
        assert!(serde_json::from_value::<Command>(command).is_err());

        let envelope = json!({
            "type": "event",
            "event": { "type": "session_removed", "sessionId": "session-1" },
        });
        assert_eq!(
            serde_json::to_value(EventEnvelope {
                message_type: super::EventMessageType::Event,
                event: ServerEvent::SessionRemoved {
                    session_id: "session-1".to_owned()
                },
            })
            .expect("event envelope should serialize"),
            envelope
        );

        let invalid_error = json!({ "code": "not_found", "message": "missing", "extra": true });
        assert!(serde_json::from_value::<ProtocolError>(invalid_error).is_err());
        let response = ErrorResponse::new(
            "request-2",
            ProtocolError {
                code: ProtocolErrorCode::NotFound,
                message: "missing".to_owned(),
                details: None,
            },
        );
        assert_eq!(
            serde_json::to_value(response).expect("error response should serialize"),
            json!({
                "type": "response", "id": "request-2", "ok": false,
                "error": { "code": "not_found", "message": "missing" },
            })
        );
        assert!(serde_json::from_value::<SuccessResponse>(json!({
            "type": "response", "id": "request-3", "ok": false,
            "result": { "command": "list", "sessions": [] },
        }))
        .is_err());
        assert!(serde_json::from_value::<ErrorResponse>(json!({
            "type": "response", "id": "request-3", "ok": true,
            "error": { "code": "not_found", "message": "missing" },
        }))
        .is_err());
    }

    #[test]
    fn server_message_rejects_mismatched_envelope_discriminators() {
        assert!(serde_json::from_value::<ServerMessage>(json!({
            "type": "response", "id": "request-1", "ok": true,
            "error": { "code": "not_found", "message": "missing" },
        }))
        .is_err());
        assert!(serde_json::from_value::<ServerMessage>(json!({
            "type": "hello_error", "error": { "code": "version", "message": "unsupported" },
            "extra": true,
        }))
        .is_err());
        assert!(serde_json::from_value::<ServerMessage>(json!({
            "type": "unknown",
        }))
        .is_err());
    }

    #[test]
    fn transcript_progress_and_server_message_round_trip() {
        let progress = TranscriptProgress::AssistantDelta {
            message_id: "message-1".to_owned(),
            content_index: 0,
            kind: TranscriptDeltaKind::Text,
            delta: "next".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(&progress).expect("progress should serialize"),
            json!({
                "type": "assistant_delta", "messageId": "message-1", "contentIndex": 0,
                "kind": "text", "delta": "next",
            })
        );

        let message = ServerMessage::Event(EventEnvelope {
            message_type: super::EventMessageType::Event,
            event: ServerEvent::SessionProgress {
                session_id: "session-1".to_owned(),
                progress,
            },
        });
        let encoded = serde_json::to_value(&message).expect("message should serialize");
        assert_eq!(
            encoded,
            json!({
                "type": "event",
                "event": {
                    "type": "session_progress", "sessionId": "session-1",
                    "progress": {
                        "type": "assistant_delta", "messageId": "message-1", "contentIndex": 0,
                        "kind": "text", "delta": "next",
                    },
                },
            })
        );
        assert_eq!(
            serde_json::from_value::<ServerMessage>(encoded).expect("message should deserialize"),
            message
        );

        let hello_error = ServerMessage::HelloError(ServerHelloError {
            message_type: super::HelloErrorMessageType::HelloError,
            error: ProtocolError {
                code: ProtocolErrorCode::Version,
                message: "unsupported version".to_owned(),
                details: None,
            },
        });
        assert_eq!(
            serde_json::to_value(hello_error).expect("hello error should serialize"),
            json!({
                "type": "hello_error",
                "error": { "code": "version", "message": "unsupported version" },
            })
        );
    }

    #[test]
    fn transcript_items_enforce_roles_statuses_and_unknown_fields() {
        let complete_assistant = json!({
            "id": "message-1",
            "role": "assistant",
            "content": [{ "type": "text", "text": "hello" }],
            "model": { "provider": "test", "id": "model" },
            "timestamp": 1,
            "status": "complete",
            "stopReason": "stop",
        });
        assert!(serde_json::from_value::<TranscriptItem>(complete_assistant).is_ok());

        assert!(serde_json::from_value::<TranscriptItem>(json!({
            "id": "message-1", "role": "user", "content": [], "timestamp": 1, "unknown": true,
        }))
        .is_err());
        assert!(serde_json::from_value::<TranscriptItem>(json!({
            "id": "message-1", "role": "assistant", "content": [],
            "model": { "provider": "test", "id": "model" }, "timestamp": 1,
            "status": "streaming", "stopReason": "stop",
        }))
        .is_err());
        assert!(serde_json::from_value::<TranscriptItem>(json!({
            "id": "tool-1", "role": "tool", "toolCallId": "call-1", "toolName": "read",
            "input": {}, "content": [], "timestamp": 1, "status": "complete", "isError": true,
        }))
        .is_err());
    }

    #[test]
    fn transcript_progress_rejects_nonterminal_and_user_finished_items() {
        let streaming_assistant = json!({
            "id": "message-1", "role": "assistant", "content": [],
            "model": { "provider": "test", "id": "model" }, "timestamp": 1,
            "status": "streaming",
        });
        assert!(serde_json::from_value::<TranscriptProgress>(json!({
            "type": "item_finished", "item": streaming_assistant,
        }))
        .is_err());
        assert!(serde_json::from_value::<TranscriptProgress>(json!({
            "type": "item_updated",
            "item": { "id": "message-1", "role": "user", "content": [], "timestamp": 1 },
        }))
        .is_err());
    }

    #[test]
    fn provider_host_dtos_match_typescript_field_names_and_reject_unknown_fields() {
        let request = ProviderRequest {
            request_id: "provider-request-1".to_owned(),
            model: ModelRef {
                provider: "test".to_owned(),
                id: "model".to_owned(),
            },
            system_prompt: "Be concise.".to_owned(),
            messages: Vec::new(),
            tools: vec![ToolDefinition {
                name: "echo".to_owned(),
                description: "Echo text".to_owned(),
                input_schema: json!({ "type": "object" }),
                execution_mode: ToolExecutionMode::Parallel,
            }],
        };
        assert_eq!(
            serde_json::to_value(&request).expect("request should serialize"),
            json!({
                "requestId": "provider-request-1",
                "model": { "provider": "test", "id": "model" },
                "systemPrompt": "Be concise.",
                "messages": [],
                "tools": [{ "name": "echo", "description": "Echo text", "inputSchema": { "type": "object" } }],
            })
        );
        assert!(serde_json::from_value::<ProviderRequest>(json!({
            "requestId": "provider-request-1",
            "model": { "provider": "test", "id": "model" },
            "systemPrompt": "Be concise.",
            "messages": [],
            "tools": [],
            "credential": "secret",
        }))
        .is_err());

        let event = ProviderStreamEvent::ToolCallEnd {
            content_index: 1,
            tool_call: ToolCall {
                tool_call_id: "call-1".to_owned(),
                tool_name: "echo".to_owned(),
                input: json!({ "value": "hello" }),
            },
        };
        assert_eq!(
            serde_json::to_value(&event).expect("event should serialize"),
            json!({
                "type": "tool_call_end",
                "contentIndex": 1,
                "toolCall": { "toolCallId": "call-1", "toolName": "echo", "input": { "value": "hello" } },
            })
        );
        assert!(serde_json::from_value::<ProviderStreamEvent>(json!({
            "type": "done", "timestamp": 3, "stopReason": "error",
        }))
        .is_err());

        let usage = Usage {
            input: 4,
            output: 3,
            cache_read: 2,
            cache_write: 1,
            reasoning: Some(2),
            total_tokens: 10,
            cost: UsageCost {
                input: 0.4,
                output: 0.3,
                cache_read: 0.2,
                cache_write: 0.1,
                total: 1.0,
            },
        };
        let terminal = ProviderStreamEvent::Done {
            message_id: "assistant-1".to_owned(),
            content: vec![AssistantContent::Thinking {
                thinking: "reasoning".to_owned(),
                redacted: Some(false),
            }],
            response_model: Some("resolved-model".to_owned()),
            usage: usage.clone(),
            timestamp: 4,
            stop_reason: AssistantStopReason::Stop,
        };
        let encoded = serde_json::to_value(&terminal).expect("terminal event should serialize");
        assert_eq!(
            encoded,
            json!({
                "type": "done",
                "messageId": "assistant-1",
                "content": [{ "type": "thinking", "thinking": "reasoning", "redacted": false }],
                "responseModel": "resolved-model",
                "usage": {
                    "input": 4,
                    "output": 3,
                    "cacheRead": 2,
                    "cacheWrite": 1,
                    "reasoning": 2,
                    "totalTokens": 10,
                    "cost": {
                        "input": 0.4,
                        "output": 0.3,
                        "cacheRead": 0.2,
                        "cacheWrite": 0.1,
                        "total": 1.0,
                    },
                },
                "timestamp": 4,
                "stopReason": "stop",
            })
        );
        assert_eq!(
            serde_json::from_value::<ProviderStreamEvent>(encoded)
                .expect("terminal event should round-trip"),
            terminal
        );
        assert!(serde_json::from_value::<ProviderStreamEvent>(json!({
            "type": "done",
            "messageId": "assistant-1",
            "content": [],
            "timestamp": 4,
            "stopReason": "stop"
        }))
        .is_err());
        assert!(serde_json::from_value::<ProviderStreamEvent>(json!({
            "type": "done",
            "messageId": "assistant-1",
            "content": [],
            "usage": usage,
            "timestamp": 4,
            "stopReason": "stop",
            "extra": true
        }))
        .is_err());
        assert!(serde_json::from_value::<ProviderStreamEvent>(json!({
            "type": "error",
            "messageId": "assistant-1",
            "content": [],
            "timestamp": 4,
            "message": "failed"
        }))
        .is_ok());
    }

    #[test]
    fn protocol_version_is_explicit() {
        assert_eq!(PROTOCOL_VERSION, 2);
        let model = ModelRef {
            provider: "test".to_owned(),
            id: "model".to_owned(),
        };
        assert_eq!(model.provider, "test");
    }
}
