//! Rust Agent Loop 的可回放回合状态机。
//!
//! 本 crate 不直接调用 Provider SDK、Tool Runtime 或 TUI。它接收已经归一化的 Provider
//! 流事件，生成稳定的生命周期事件与 transcript 项；上层适配器负责把真实网络流逐项送入
//! [`AgentLoopEngine::handle_provider_event`]。这种设计使事件顺序可离线回放和单元测试，
//! 并避免核心状态机耦合 TypeScript Provider Host。

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use protocol::{
    AbortedStopReason, AssistantContent, AssistantRole, AssistantStopReason,
    AssistantTranscriptItem, ErrorStopReason, ModelRef, ProviderStreamEvent, TextOrImageContent,
    ToolCall, ToolResult, ToolRole, ToolTranscriptItem, TranscriptItem, Usage, UserRole,
    UserTranscriptItem,
};

/// Provider 流适配失败的稳定错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLoopError {
    message: String,
}

impl AgentLoopError {
    /// 创建可安全展示给上层的错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// 返回诊断文本。
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Provider Host 归一化后输入 Agent Loop 的事件集合。
///
/// 增量事件仅驱动实时 progress；`Completed` / `Failed` 携带 Provider SDK 的权威终态消息。
/// 状态机在终态替换增量快照，与 TypeScript `response.result()` 的语义保持一致。
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderEvent {
    /// Provider 已接受请求并开始生成 assistant 项。
    Started { message_id: String, timestamp: u64 },
    /// 一个文本增量。`content_index` 决定它在最终 assistant 内容中的位置。
    TextDelta { content_index: u64, text: String },
    /// 一个 thinking 增量。redacted 属性属于内容块，后续增量不得与首次值冲突。
    ThinkingDelta {
        content_index: u64,
        thinking: String,
        redacted: Option<bool>,
    },
    /// 一个尚未完成的工具参数增量。
    ToolCallDelta { content_index: u64, delta: String },
    /// 一个已完成解析、允许执行的工具调用。
    ToolCallEnded {
        content_index: u64,
        tool_call: ToolCall,
    },
    /// 正常结束，并提交 Provider 给出的完整稳定消息。
    Completed {
        message_id: String,
        content: Vec<AssistantContent>,
        response_model: Option<String>,
        usage: Usage,
        timestamp: u64,
        stop_reason: AssistantStopReason,
    },
    /// Provider 错误同样携带可用的稳定消息；Host 自身错误允许没有 usage。
    Failed {
        message_id: String,
        content: Vec<AssistantContent>,
        response_model: Option<String>,
        usage: Option<Usage>,
        timestamp: u64,
        message: String,
    },
}

/// Tool Runtime 反馈给 Agent Loop 的生命周期事件。
///
/// 端口事件只使用共享协议值，不暴露 worker、channel 或 Sandbox。`Updated` 是可丢弃的运行中
/// 快照；只有 `Finished` 可以写入稳定 transcript。同一调用必须且只能产生一个终态。
#[derive(Debug, Clone, PartialEq)]
pub enum ToolRuntimePortEvent {
    Started {
        tool_call: ToolCall,
    },
    Updated {
        tool_call_id: String,
        content: Vec<TextOrImageContent>,
        details: Option<serde_json::Value>,
    },
    Finished {
        result: ToolResult,
    },
}

/// 非阻塞 Tool Runtime 的窄端口。
///
/// `start` 只提交任务，不能等待工具完成；调用方在自己的事件循环中调用 `poll`。Runtime 根据
/// 已注册工具的执行约束选择并行或串行；只要一个调用要求串行，整批都会保守地按源顺序执行。
pub trait ToolRuntimePort {
    fn start(
        &mut self,
        calls: Vec<ToolCall>,
        timestamp: u64,
    ) -> Result<Vec<ToolRuntimePortEvent>, String>;

    fn poll(&mut self, timestamp: u64) -> Vec<ToolRuntimePortEvent>;

    fn cancel(&mut self);

    fn has_active_batch(&self) -> bool;
}

/// 将跨语言 DTO 转换为内部状态机事件。
///
/// 适配层只做字段投影，不重建 Provider 的最终消息。这样 replay、真实 Host 与单元测试
/// 都经过同一条终态路径，不会因某个 adapter 自行拼装内容而产生行为差异。
impl TryFrom<ProviderStreamEvent> for ProviderEvent {
    type Error = AgentLoopError;

    fn try_from(event: ProviderStreamEvent) -> Result<Self, Self::Error> {
        match event {
            ProviderStreamEvent::Start {
                message_id,
                timestamp,
            } => Ok(Self::Started {
                message_id,
                timestamp,
            }),
            ProviderStreamEvent::TextDelta {
                content_index,
                delta,
            } => Ok(Self::TextDelta {
                content_index,
                text: delta,
            }),
            ProviderStreamEvent::ThinkingDelta {
                content_index,
                delta,
                redacted,
            } => Ok(Self::ThinkingDelta {
                content_index,
                thinking: delta,
                redacted,
            }),
            ProviderStreamEvent::Done {
                message_id,
                content,
                response_model,
                usage,
                timestamp,
                stop_reason,
            } => Ok(Self::Completed {
                message_id,
                content,
                response_model,
                usage,
                timestamp,
                stop_reason,
            }),
            ProviderStreamEvent::Error {
                message_id,
                content,
                response_model,
                usage,
                timestamp,
                message,
            } => Ok(Self::Failed {
                message_id,
                content,
                response_model,
                usage,
                timestamp,
                message,
            }),
            ProviderStreamEvent::ToolCallDelta {
                content_index,
                delta,
            } => Ok(Self::ToolCallDelta {
                content_index,
                delta,
            }),
            ProviderStreamEvent::ToolCallEnd {
                content_index,
                tool_call,
            } => Ok(Self::ToolCallEnded {
                content_index,
                tool_call,
            }),
        }
    }
}

/// Agent Loop 交给上层 Session/RPC 的有序生命周期事件。
///
/// 生命周期顺序与 TypeScript [`runAgentLoop()`](../../../packages/agent/src/agent-loop.ts:106)
/// 的无工具路径一致：agent start、turn start、用户 message start/end、assistant
/// message start/update/end、turn end、agent end。
#[derive(Debug, Clone, PartialEq)]
pub enum AgentLoopEvent {
    AgentStarted,
    TurnStarted,
    TranscriptItemStarted(TranscriptItem),
    /// 已累计到当前完整内容的流式 assistant 快照。上层可通过前后快照计算增量，
    /// 从而保持与 TypeScript 单个 `message_update` 事件的语义一致。
    TranscriptItemUpdated(AssistantTranscriptItem),
    TranscriptItemFinished(TranscriptItem),
    /// 已进入工具执行阶段。该事件只对应已完整解析的调用，不能由参数增量触发。
    ToolExecutionStarted {
        tool_call: ToolCall,
    },
    /// 工具运行中的完整快照。更新不会持久化，迟到更新由 Tool Runtime 丢弃。
    ToolExecutionUpdated {
        tool_call: ToolCall,
        content: Vec<TextOrImageContent>,
        details: Option<serde_json::Value>,
    },
    /// 单个工具已产生稳定结果；错误同样作为结果返回，避免中断同批后续调用。
    ToolExecutionFinished {
        result: ToolResult,
    },
    /// 工具结果已写入 transcript，可被 Provider Host 用于构造下一次请求。
    ///
    /// 收到该事件后状态机仍保持 active，直到后续 Provider 回合以非 toolUse 结束。
    ToolResultsReady {
        results: Vec<ToolResult>,
    },
    /// steering 或 follow-up 已按 FIFO 写入 transcript，下一次 Provider 请求现在可以开始。
    ///
    /// 这个事件不携带 messages，避免运行时复制或自行拼装上下文；调用方必须从
    /// [`AgentLoopEngine::messages`] 取得已提交的权威 transcript 快照。
    ProviderContinuationRequested,
    /// AgentSession 可据此移除本地 UI 队列中最早的一条 steering 消息。
    SteerConsumed,
    /// AgentSession 可据此移除本地 UI 队列中最早的一条 follow-up 消息。
    FollowUpConsumed,
    TurnEnded {
        message: TranscriptItem,
    },
    AgentEnded {
        messages: Vec<TranscriptItem>,
    },
}

/// 单个执行中的 assistant 回应。
#[derive(Debug, Clone, PartialEq)]
struct ActiveAssistant {
    id: String,
    timestamp: u64,
    /// 增量可交错到达，必须按 Provider content index 建槽，不能依赖最后一个内容块。
    /// BTreeMap 同时保证每次 progress 快照都按 index 稳定排序。
    content: BTreeMap<u64, AssistantContent>,
    response_model: Option<String>,
    usage: Option<Usage>,
    /// 仅记录尚未收到 terminal event 的参数片段。终态完整快照到达后不再依赖这些片段。
    tool_call_deltas: BTreeMap<u64, String>,
    /// 防止同一个 Provider content index 被重复终结并执行两次。
    completed_tool_call_indexes: BTreeSet<u64>,
}

/// 可回放的 Agent Loop 状态机。
///
/// `prompt`、`steer` 与 `abort` 只改变本地状态并产出事件，不执行网络 I/O。真实运行时
/// 应由 Provider adapter 在 `prompt` 后发出请求，并把流事件交给
/// [`AgentLoopEngine::handle_provider_event`]。这样取消、断线和事件重放都可确定性测试。
pub struct AgentLoopEngine {
    model: ModelRef,
    active: bool,
    abort_requested: bool,
    active_assistant: Option<ActiveAssistant>,
    /// 尚未收到 start 的调用，始终按 Provider 的 source order 保存。
    pending_tool_starts: VecDeque<ToolCall>,
    /// 已启动但尚未产生终态的调用；并行批次可以同时包含多个条目。
    active_tool_calls: BTreeMap<String, ToolCall>,
    /// 所有最终工具结果必须按这个 source order 写入 transcript。
    tool_call_order: Vec<String>,
    /// 已完成但尚未能稳定提交的结果，按调用 ID 索引而非完成时序存放。
    tool_results: BTreeMap<String, ToolResult>,
    /// 已按 source order 提交到 transcript，等待批次结束后一起通知 Provider 的结果。
    finalized_tool_results: Vec<ToolResult>,
    queued_steer: Vec<UserTranscriptItem>,
    queued_follow_up: Vec<UserTranscriptItem>,
    messages: Vec<TranscriptItem>,
    events: Vec<AgentLoopEvent>,
}

impl AgentLoopEngine {
    /// 使用未来 Provider 请求应使用的模型创建状态机。
    pub fn new(model: ModelRef) -> Self {
        Self {
            model,
            active: false,
            abort_requested: false,
            active_assistant: None,
            pending_tool_starts: VecDeque::new(),
            active_tool_calls: BTreeMap::new(),
            tool_call_order: Vec::new(),
            tool_results: BTreeMap::new(),
            finalized_tool_results: Vec::new(),
            queued_steer: Vec::new(),
            queued_follow_up: Vec::new(),
            messages: Vec::new(),
            events: Vec::new(),
        }
    }

    /// 返回当前模型。
    pub fn model(&self) -> &ModelRef {
        &self.model
    }

    /// 仅在一个回合从 prompt 开始到 agent end 完成前返回 true。
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// 返回当前回合内已经稳定的消息。
    pub fn messages(&self) -> &[TranscriptItem] {
        &self.messages
    }

    /// 返回当前流式 assistant 的 Provider message ID。
    ///
    /// Runtime 仅在 Host 违反“终态先于 complete”约束时使用它构造本地错误终态，保证该错误
    /// 可以替换已存在的 partial，而不会因生成另一个 ID 被状态机拒绝。
    pub fn active_assistant_id(&self) -> Option<&str> {
        self.active_assistant
            .as_ref()
            .map(|assistant| assistant.id.as_str())
    }

    /// 在已经结束的失败回合上恢复 Provider 循环。
    ///
    /// retry 不创建新的用户 turn，也不重复提交 prompt；它只让同一份错误前上下文再次
    /// 接受 Provider 流。因此不发出 `AgentStarted`/`TurnStarted`，避免 UI 和持久化把
    /// 同一次用户请求错误显示为两个独立回合。
    pub fn restart_after_retry(&mut self) -> Result<(), AgentLoopError> {
        if self.active {
            return Err(AgentLoopError::new(
                "运行中的 Agent Loop 不能重复启动 retry。",
            ));
        }
        if self.abort_requested {
            return Err(AgentLoopError::new("已取消的 Agent Loop 不能启动 retry。"));
        }
        self.active = true;
        Ok(())
    }

    /// 移除刚刚结束的失败 assistant，以便重试沿用失败前的上下文。
    ///
    /// Session 仍会持久化该错误，供用户追溯失败历史；这里仅修正下一次 Provider request
    /// 的工作上下文，严格对应 TypeScript 在 retry 前从 `agent.state.messages` 弹出 error。
    pub fn discard_last_error_assistant_for_retry(&mut self) -> Result<(), AgentLoopError> {
        match self.messages.last() {
            Some(TranscriptItem::Assistant(AssistantTranscriptItem::Error { .. })) => {
                self.messages.pop();
                Ok(())
            }
            _ => Err(AgentLoopError::new(
                "自动重试前未找到可从 Provider 上下文移除的失败 assistant。",
            )),
        }
    }

    /// 用 Session Store 重建的稳定上下文替换下一次 Provider 请求的工作消息。
    ///
    /// compaction 与树导航都保留完整持久化历史，只改变 Provider 可见的临时消息窗口。
    /// 只能在空闲边界替换：流式 assistant、未完成工具或排队输入仍属于当前回合，若中途
    /// 覆盖会破坏 terminal event 与后续 continuation 的顺序。
    pub fn replace_context(&mut self, messages: Vec<TranscriptItem>) -> Result<(), AgentLoopError> {
        if self.active {
            return Err(AgentLoopError::new(
                "运行中的 Agent Loop 不能替换工作上下文。",
            ));
        }
        if messages.is_empty() {
            return Err(AgentLoopError::new("工作上下文不能为空。"));
        }
        if self.active_assistant.is_some()
            || !self.pending_tool_starts.is_empty()
            || !self.active_tool_calls.is_empty()
            || !self.tool_results.is_empty()
            || !self.finalized_tool_results.is_empty()
            || !self.queued_steer.is_empty()
            || !self.queued_follow_up.is_empty()
        {
            return Err(AgentLoopError::new(
                "存在未完成的回合状态，不能替换工作上下文。",
            ));
        }

        self.messages = messages;
        Ok(())
    }

    /// 返回尚未注入下一次 Provider 请求的 steering 消息。
    pub fn queued_steer(&self) -> &[UserTranscriptItem] {
        &self.queued_steer
    }

    /// 返回等待当前 Agent 完整停止后再投递的 follow-up 消息。
    ///
    /// follow-up 与 steering 分开保存：前者绝不能抢占工具链或当前回合后的 steering，
    /// 这是 TypeScript `runLoop()` 外层循环的关键顺序约束。
    pub fn queued_follow_up(&self) -> &[UserTranscriptItem] {
        &self.queued_follow_up
    }

    /// 启动新回合并产生用户消息的稳定事件。
    pub fn prompt(&mut self, message: UserTranscriptItem) -> Result<(), AgentLoopError> {
        if self.active {
            return Err(AgentLoopError::new(
                "Agent Loop 已在运行，不能直接启动新的 prompt。",
            ));
        }

        self.active = true;
        self.abort_requested = false;
        self.events.push(AgentLoopEvent::AgentStarted);
        self.events.push(AgentLoopEvent::TurnStarted);
        self.commit_message(TranscriptItem::User(message));
        Ok(())
    }

    /// 接收当前回合中的 steering 输入。
    ///
    /// TypeScript 实现在 assistant 当前回合结束、下一次 Provider 请求之前注入 steering。
    /// 因此此处仅入队，绝不改写正在流式生成的 assistant。
    pub fn steer(&mut self, message: UserTranscriptItem) -> Result<(), AgentLoopError> {
        if !self.active {
            return Err(AgentLoopError::new("Agent Loop 空闲时不能接收 steer。"));
        }
        self.queued_steer.push(message);
        Ok(())
    }

    /// 接收一个 follow-up 输入。
    ///
    /// follow-up 仅在当前回合没有待执行工具、也没有 steering 时才会被消费；它不打断
    /// Provider stream，且不会改变当前工具批次的执行次序。
    pub fn follow_up(&mut self, message: UserTranscriptItem) -> Result<(), AgentLoopError> {
        if !self.active {
            return Err(AgentLoopError::new("Agent Loop 空闲时不能接收 follow-up。"));
        }
        self.queued_follow_up.push(message);
        Ok(())
    }

    /// 请求取消。取消本身不结束回合；只有 Provider 的下一事件或显式
    /// [`AgentLoopEngine::settle_abort`] 才会提交 aborted 消息并结束。
    pub fn abort(&mut self) -> Result<(), AgentLoopError> {
        if !self.active {
            return Ok(());
        }
        self.abort_requested = true;
        Ok(())
    }

    /// 当前回合是否已经收到取消请求、正在等待运行时完成逻辑结算。
    pub fn is_abort_requested(&self) -> bool {
        self.abort_requested
    }

    /// 工具批次是否尚未全部产生稳定终态。
    pub fn is_waiting_for_tools(&self) -> bool {
        !self.pending_tool_starts.is_empty() || !self.active_tool_calls.is_empty()
    }

    /// 处理一个不包含工具执行的归一化 Provider 事件。
    ///
    /// 当 Provider 以 `toolUse` 结束时，本方法在提交任何 assistant 消息前返回错误，防止调用方
    /// 在没有 Tool Runtime 的情况下留下无法继续的工具回合。真实运行时应调用
    /// [`AgentLoopEngine::handle_provider_event_with_tools`]。
    pub fn handle_provider_event(
        &mut self,
        event: ProviderEvent,
    ) -> Result<Vec<AgentLoopEvent>, AgentLoopError> {
        self.handle_provider_event_inner(event, None)
    }

    /// 处理 Provider 事件，并在 `toolUse` 终态按源顺序执行已完成的工具调用。
    ///
    /// 此方法只依赖 [`ToolRuntimePort`]，不依赖具体 Tool Runtime、Sandbox 或扩展实现。工具
    /// 结果被追加为完整 transcript 项，但不会结束 Agent 回合；Provider adapter 应使用
    /// [`AgentLoopEngine::messages`] 构造下一次请求，再继续投递新的 `start` 事件。
    pub fn handle_provider_event_with_tools(
        &mut self,
        event: ProviderEvent,
        tool_runtime: &mut impl ToolRuntimePort,
    ) -> Result<Vec<AgentLoopEvent>, AgentLoopError> {
        self.handle_provider_event_inner(event, Some(tool_runtime))
    }

    fn handle_provider_event_inner(
        &mut self,
        event: ProviderEvent,
        tool_runtime: Option<&mut dyn ToolRuntimePort>,
    ) -> Result<Vec<AgentLoopEvent>, AgentLoopError> {
        if !self.active {
            return Err(AgentLoopError::new(
                "Agent Loop 空闲，不能处理 Provider 事件。",
            ));
        }
        if self.abort_requested {
            let timestamp = match event {
                ProviderEvent::Started { timestamp, .. }
                | ProviderEvent::Completed { timestamp, .. }
                | ProviderEvent::Failed { timestamp, .. } => timestamp,
                ProviderEvent::TextDelta { .. }
                | ProviderEvent::ThinkingDelta { .. }
                | ProviderEvent::ToolCallDelta { .. }
                | ProviderEvent::ToolCallEnded { .. } => self
                    .active_assistant
                    .as_ref()
                    .map_or(0, |assistant| assistant.timestamp),
            };
            return Ok(self.settle_abort(timestamp));
        }

        match event {
            ProviderEvent::Started {
                message_id,
                timestamp,
            } => self.start_assistant(message_id, timestamp)?,
            ProviderEvent::TextDelta {
                content_index,
                text,
            } => self.append_text(content_index, text)?,
            ProviderEvent::ThinkingDelta {
                content_index,
                thinking,
                redacted,
            } => self.append_thinking(content_index, thinking, redacted)?,
            ProviderEvent::ToolCallDelta {
                content_index,
                delta,
            } => self.append_tool_call_delta(content_index, delta)?,
            ProviderEvent::ToolCallEnded {
                content_index,
                tool_call,
            } => self.finish_tool_call(content_index, tool_call)?,
            ProviderEvent::Completed {
                message_id,
                content,
                response_model,
                usage,
                timestamp,
                stop_reason,
            } => {
                if stop_reason == AssistantStopReason::ToolUse && tool_runtime.is_none() {
                    return Err(AgentLoopError::new(
                        "收到 toolUse 终态时必须提供 Tool Runtime Port。",
                    ));
                }
                let calls = self.complete_assistant(
                    message_id,
                    content,
                    response_model,
                    usage,
                    timestamp,
                    stop_reason,
                )?;
                if let Some(runtime) = tool_runtime {
                    if !calls.is_empty() {
                        self.start_tool_calls(calls, timestamp, runtime)?;
                    }
                }
            }
            ProviderEvent::Failed {
                message_id,
                content,
                response_model,
                usage,
                timestamp,
                message,
            } => self.fail_assistant(
                message_id,
                content,
                response_model,
                usage,
                timestamp,
                message,
            )?,
        }

        Ok(self.drain_events())
    }

    /// 接收非阻塞 Tool Runtime 当前已经就绪的事件。
    pub fn handle_tool_runtime_events(
        &mut self,
        events: Vec<ToolRuntimePortEvent>,
    ) -> Result<Vec<AgentLoopEvent>, AgentLoopError> {
        for event in events {
            match event {
                ToolRuntimePortEvent::Started { tool_call } => {
                    let expected = self.pending_tool_starts.pop_front().ok_or_else(|| {
                        AgentLoopError::new("Tool Runtime 启动了未提交的工具调用。")
                    })?;
                    if expected != tool_call {
                        return Err(AgentLoopError::new("Tool Runtime 改变了工具调用顺序。"));
                    }
                    if self
                        .active_tool_calls
                        .insert(tool_call.tool_call_id.clone(), tool_call.clone())
                        .is_some()
                    {
                        return Err(AgentLoopError::new("同一工具调用不能重复启动。"));
                    }
                    self.events
                        .push(AgentLoopEvent::ToolExecutionStarted { tool_call });
                }
                ToolRuntimePortEvent::Updated {
                    tool_call_id,
                    content,
                    details,
                } => {
                    let tool_call = self.active_tool_calls.get(&tool_call_id).ok_or_else(|| {
                        AgentLoopError::new("工具开始前或结算后不能发送运行中更新。")
                    })?;
                    self.events.push(AgentLoopEvent::ToolExecutionUpdated {
                        tool_call: tool_call.clone(),
                        content,
                        details,
                    });
                }
                ToolRuntimePortEvent::Finished { result } => {
                    let tool_call = self
                        .active_tool_calls
                        .remove(&result.tool_call_id)
                        .ok_or_else(|| AgentLoopError::new("工具开始前不能产生终态。"))?;
                    if tool_call.tool_name != result.tool_name {
                        return Err(AgentLoopError::new("工具终态不属于当前调用。"));
                    }
                    if self
                        .tool_results
                        .insert(result.tool_call_id.clone(), result)
                        .is_some()
                    {
                        return Err(AgentLoopError::new("同一工具调用不能重复结算。"));
                    }
                }
            }
        }
        self.commit_stable_tool_results()?;
        Ok(self.drain_events())
    }

    /// 只提交已经形成连续前缀的工具结果。
    ///
    /// 并行 worker 可以按任意顺序结束，但 Provider continuation 的 transcript 必须与原始
    /// tool-call 顺序一致。因而后完成的前序调用会暂时阻塞后序结果的可见提交；progress 更新
    /// 仍在完成前实时转发，不受此规则影响。
    fn commit_stable_tool_results(&mut self) -> Result<(), AgentLoopError> {
        while let Some(tool_call_id) = self.tool_call_order.first().cloned() {
            let Some(result) = self.tool_results.remove(&tool_call_id) else {
                break;
            };
            self.tool_call_order.remove(0);
            self.events.push(AgentLoopEvent::ToolExecutionFinished {
                result: result.clone(),
            });
            self.commit_message(TranscriptItem::Tool(tool_result_item(&result)));
            self.finalized_tool_results.push(result);
        }
        if self.tool_call_order.is_empty() && !self.is_waiting_for_tools() {
            self.events.push(AgentLoopEvent::ToolResultsReady {
                results: std::mem::take(&mut self.finalized_tool_results),
            });
        }
        Ok(())
    }

    /// 当 Provider adapter 已确认取消生效、但没有可用的终态流事件时调用。
    pub fn settle_abort(&mut self, timestamp: u64) -> Vec<AgentLoopEvent> {
        if !self.active {
            return Vec::new();
        }

        // 取消后的工具 worker 可能仍会自然返回，但其结果不再属于当前回合。清空 Agent Loop
        // 的批次状态，确保迟到终态不能污染 transcript，也不会阻塞同一 Session 的下一 prompt。
        self.pending_tool_starts.clear();
        self.active_tool_calls.clear();
        self.tool_call_order.clear();
        self.tool_results.clear();
        self.finalized_tool_results.clear();
        let assistant = self.active_assistant.take();
        let item = TranscriptItem::Assistant(AssistantTranscriptItem::Aborted {
            id: assistant
                .as_ref()
                .map_or_else(|| format!("aborted-{timestamp}"), |value| value.id.clone()),
            role: AssistantRole::Assistant,
            content: assistant
                .as_ref()
                .map_or_else(Vec::new, ActiveAssistant::ordered_content),
            model: self.model.clone(),
            response_model: assistant
                .as_ref()
                .and_then(|value| value.response_model.clone()),
            usage: assistant.as_ref().and_then(|value| value.usage.clone()),
            timestamp,
            stop_reason: AbortedStopReason::Aborted,
            error_message: Some("Operation aborted".to_owned()),
        });
        self.finish_turn(item);
        self.drain_events()
    }

    /// 取走已按发生顺序累积的事件。
    pub fn drain_events(&mut self) -> Vec<AgentLoopEvent> {
        std::mem::take(&mut self.events)
    }

    fn start_assistant(&mut self, id: String, timestamp: u64) -> Result<(), AgentLoopError> {
        if self.active_assistant.is_some() {
            return Err(AgentLoopError::new("当前 assistant 响应尚未结束。"));
        }
        let assistant = ActiveAssistant {
            id,
            timestamp,
            content: BTreeMap::new(),
            response_model: None,
            usage: None,
            tool_call_deltas: BTreeMap::new(),
            completed_tool_call_indexes: BTreeSet::new(),
        };
        self.events.push(AgentLoopEvent::TranscriptItemStarted(
            TranscriptItem::Assistant(self.streaming_item(&assistant)),
        ));
        self.active_assistant = Some(assistant);
        Ok(())
    }

    fn append_text(&mut self, content_index: u64, text: String) -> Result<(), AgentLoopError> {
        let model = self.model.clone();
        let assistant = self
            .active_assistant
            .as_mut()
            .ok_or_else(|| AgentLoopError::new("收到文本增量前必须先收到 Provider start。"))?;
        match assistant.content.entry(content_index) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(AssistantContent::Text { text });
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => match entry.get_mut() {
                AssistantContent::Text { text: accumulated } => accumulated.push_str(&text),
                AssistantContent::Thinking { .. } | AssistantContent::ToolCall { .. } => {
                    return Err(AgentLoopError::new(
                        "同一个 content index 不能同时表示文本和其他内容。",
                    ));
                }
            },
        }
        self.events
            .push(AgentLoopEvent::TranscriptItemUpdated(streaming_item_for(
                &model, assistant,
            )));
        Ok(())
    }

    fn append_thinking(
        &mut self,
        content_index: u64,
        thinking: String,
        redacted: Option<bool>,
    ) -> Result<(), AgentLoopError> {
        let model = self.model.clone();
        let assistant = self.active_assistant.as_mut().ok_or_else(|| {
            AgentLoopError::new("收到 thinking 增量前必须先收到 Provider start。")
        })?;
        match assistant.content.entry(content_index) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(AssistantContent::Thinking { thinking, redacted });
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => match entry.get_mut() {
                AssistantContent::Thinking {
                    thinking: accumulated,
                    redacted: current_redacted,
                } => {
                    if current_redacted.is_some()
                        && redacted.is_some()
                        && *current_redacted != redacted
                    {
                        return Err(AgentLoopError::new(
                            "同一个 thinking content index 的 redacted 属性不能改变。",
                        ));
                    }
                    if current_redacted.is_none() {
                        *current_redacted = redacted;
                    }
                    accumulated.push_str(&thinking);
                }
                AssistantContent::Text { .. } | AssistantContent::ToolCall { .. } => {
                    return Err(AgentLoopError::new(
                        "同一个 content index 不能同时表示 thinking 和其他内容。",
                    ));
                }
            },
        }
        self.events
            .push(AgentLoopEvent::TranscriptItemUpdated(streaming_item_for(
                &model, assistant,
            )));
        Ok(())
    }

    fn append_tool_call_delta(
        &mut self,
        content_index: u64,
        delta: String,
    ) -> Result<(), AgentLoopError> {
        let model = self.model.clone();
        let assistant = self
            .active_assistant
            .as_mut()
            .ok_or_else(|| AgentLoopError::new("收到工具调用增量前必须先收到 Provider start。"))?;
        assistant
            .tool_call_deltas
            .entry(content_index)
            .or_default()
            .push_str(&delta);
        self.events
            .push(AgentLoopEvent::TranscriptItemUpdated(streaming_item_for(
                &model, assistant,
            )));
        Ok(())
    }

    fn finish_tool_call(
        &mut self,
        content_index: u64,
        tool_call: ToolCall,
    ) -> Result<(), AgentLoopError> {
        let model = self.model.clone();
        let assistant = self
            .active_assistant
            .as_mut()
            .ok_or_else(|| AgentLoopError::new("收到完整工具调用前必须先收到 Provider start。"))?;
        if !assistant.completed_tool_call_indexes.insert(content_index) {
            return Err(AgentLoopError::new(
                "同一个工具调用 content index 不能重复结束。",
            ));
        }
        assistant.tool_call_deltas.remove(&content_index);
        if assistant
            .content
            .insert(
                content_index,
                AssistantContent::ToolCall {
                    tool_call_id: tool_call.tool_call_id,
                    tool_name: tool_call.tool_name,
                    input: tool_call.input,
                },
            )
            .is_some()
        {
            return Err(AgentLoopError::new(
                "完整工具调用不能覆盖同一个 content index 的已有内容。",
            ));
        }
        self.events
            .push(AgentLoopEvent::TranscriptItemUpdated(streaming_item_for(
                &model, assistant,
            )));
        Ok(())
    }

    fn complete_assistant(
        &mut self,
        message_id: String,
        content: Vec<AssistantContent>,
        response_model: Option<String>,
        usage: Usage,
        timestamp: u64,
        stop_reason: AssistantStopReason,
    ) -> Result<Vec<ToolCall>, AgentLoopError> {
        self.ensure_terminal_assistant(&message_id, timestamp)?;
        let calls = tool_calls(&content);
        validate_tool_terminal(stop_reason, &calls)?;

        self.active_assistant.take();
        let item = TranscriptItem::Assistant(AssistantTranscriptItem::Complete {
            id: message_id,
            role: AssistantRole::Assistant,
            content,
            model: self.model.clone(),
            response_model,
            usage: Some(usage),
            timestamp,
            stop_reason,
        });
        self.events
            .push(AgentLoopEvent::TranscriptItemFinished(item.clone()));
        self.messages.push(item);

        if calls.is_empty() && stop_reason != AssistantStopReason::ToolUse {
            self.schedule_post_assistant_input();
        }
        Ok(calls)
    }

    fn start_tool_calls(
        &mut self,
        calls: Vec<ToolCall>,
        timestamp: u64,
        tool_runtime: &mut dyn ToolRuntimePort,
    ) -> Result<(), AgentLoopError> {
        if self.is_waiting_for_tools() {
            return Err(AgentLoopError::new("当前工具批次尚未结束。"));
        }
        self.pending_tool_starts = calls.iter().cloned().collect();
        self.tool_call_order = calls.iter().map(|call| call.tool_call_id.clone()).collect();
        self.tool_results.clear();
        self.finalized_tool_results.clear();
        let events = tool_runtime
            .start(calls, timestamp)
            .map_err(AgentLoopError::new)?;
        // 内部处理函数会 drain 完整事件队列。这里必须把结果放回队列，保证同一次 Provider
        // done 已生成的 assistant message_end 先于工具生命周期事件交给上层，且不会被静默吞掉。
        let emitted = self.handle_tool_runtime_events(events)?;
        self.events.extend(emitted);
        Ok(())
    }

    fn fail_assistant(
        &mut self,
        message_id: String,
        content: Vec<AssistantContent>,
        response_model: Option<String>,
        usage: Option<Usage>,
        timestamp: u64,
        message: String,
    ) -> Result<(), AgentLoopError> {
        self.ensure_terminal_assistant(&message_id, timestamp)?;
        self.active_assistant.take();
        let item = TranscriptItem::Assistant(AssistantTranscriptItem::Error {
            id: message_id,
            role: AssistantRole::Assistant,
            content,
            model: self.model.clone(),
            response_model,
            usage,
            timestamp,
            stop_reason: ErrorStopReason::Error,
            error_message: Some(message),
        });
        self.finish_turn(item);
        Ok(())
    }

    /// 在 assistant 正常停止后选择下一次输入。
    ///
    /// 顺序严格对应 TypeScript Agent Loop：先清空 steering（内层循环），再消费
    /// follow-up（外层循环）。每次只消费一项 follow-up，天然实现 one-at-a-time；调用方
    /// 若需要 all 模式，可在入队时合并消息，而不污染核心状态机。
    fn schedule_post_assistant_input(&mut self) {
        if !self.queued_steer.is_empty() {
            self.commit_queued_input(true);
        } else if !self.queued_follow_up.is_empty() {
            self.commit_queued_input(false);
        } else {
            self.finish_active_turn();
        }
    }

    /// 把已选择的排队用户项提交为稳定 transcript，并通知运行时发起 continuation。
    ///
    /// steering 在同一轮中必须整批插入，保证多个连续输入在下一次模型调用中保持 FIFO；
    /// follow-up 则一次一个，等待对应 assistant 回复结束后再检查下一条。
    fn commit_queued_input(&mut self, steering: bool) {
        let messages = if steering {
            std::mem::take(&mut self.queued_steer)
        } else {
            vec![self.queued_follow_up.remove(0)]
        };
        for message in messages {
            self.commit_message(TranscriptItem::User(message));
            self.events.push(if steering {
                AgentLoopEvent::SteerConsumed
            } else {
                AgentLoopEvent::FollowUpConsumed
            });
        }
        self.events
            .push(AgentLoopEvent::ProviderContinuationRequested);
    }

    /// TypeScript 在没有观察到 partial start 时，会在终态前补发 `message_start`。
    /// Provider Host 的 terminal messageId 使 Rust 可以执行同样操作，而不是丢弃合法终态。
    fn ensure_terminal_assistant(
        &mut self,
        message_id: &str,
        timestamp: u64,
    ) -> Result<(), AgentLoopError> {
        if self.active_assistant.is_none() {
            self.start_assistant(message_id.to_owned(), timestamp)?;
        }
        let active_id = &self
            .active_assistant
            .as_ref()
            .expect("缺失 assistant 已在上方补建")
            .id;
        if active_id != message_id {
            return Err(AgentLoopError::new(
                "Provider 终态 messageId 与当前 assistant 不一致。",
            ));
        }
        Ok(())
    }

    fn streaming_item(&self, assistant: &ActiveAssistant) -> AssistantTranscriptItem {
        streaming_item_for(&self.model, assistant)
    }

    fn commit_message(&mut self, item: TranscriptItem) {
        self.events
            .push(AgentLoopEvent::TranscriptItemStarted(item.clone()));
        self.events
            .push(AgentLoopEvent::TranscriptItemFinished(item.clone()));
        self.messages.push(item);
    }

    fn finish_turn(&mut self, item: TranscriptItem) {
        self.events
            .push(AgentLoopEvent::TranscriptItemFinished(item.clone()));
        self.messages.push(item.clone());
        self.finish_active_turn_with(item);
    }

    fn finish_active_turn(&mut self) {
        let message = self
            .messages
            .last()
            .cloned()
            .expect("完成回合前必须至少存在一条 assistant 消息");
        self.finish_active_turn_with(message);
    }

    fn finish_active_turn_with(&mut self, message: TranscriptItem) {
        self.events.push(AgentLoopEvent::TurnEnded { message });
        self.active = false;
        self.abort_requested = false;
        self.events.push(AgentLoopEvent::AgentEnded {
            messages: self.messages.clone(),
        });
    }
}

impl ActiveAssistant {
    fn ordered_content(&self) -> Vec<AssistantContent> {
        self.content.values().cloned().collect()
    }
}

fn streaming_item_for(model: &ModelRef, assistant: &ActiveAssistant) -> AssistantTranscriptItem {
    AssistantTranscriptItem::Streaming {
        id: assistant.id.clone(),
        role: AssistantRole::Assistant,
        content: assistant.ordered_content(),
        model: model.clone(),
        response_model: assistant.response_model.clone(),
        usage: assistant.usage.clone(),
        timestamp: assistant.timestamp,
    }
}

fn tool_calls(content: &[AssistantContent]) -> Vec<ToolCall> {
    content
        .iter()
        .filter_map(|content| match content {
            AssistantContent::ToolCall {
                tool_call_id,
                tool_name,
                input,
            } => Some(ToolCall {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                input: input.clone(),
            }),
            AssistantContent::Text { .. } | AssistantContent::Thinking { .. } => None,
        })
        .collect()
}

fn validate_tool_terminal(
    stop_reason: AssistantStopReason,
    calls: &[ToolCall],
) -> Result<(), AgentLoopError> {
    if stop_reason == AssistantStopReason::ToolUse && calls.is_empty() {
        return Err(AgentLoopError::new(
            "Provider 以 toolUse 结束，但终态快照未提供完整工具调用。",
        ));
    }
    if stop_reason != AssistantStopReason::ToolUse && !calls.is_empty() {
        return Err(AgentLoopError::new(
            "非 toolUse 的 Provider 终态不能包含工具调用。",
        ));
    }
    Ok(())
}

fn tool_result_item(result: &ToolResult) -> ToolTranscriptItem {
    let item_id = format!("tool-{}", result.tool_call_id);
    if result.is_error {
        ToolTranscriptItem::Error {
            id: item_id,
            role: ToolRole::Tool,
            tool_call_id: result.tool_call_id.clone(),
            tool_name: result.tool_name.clone(),
            input: result.input.clone(),
            content: result.content.clone(),
            details: result.details.clone(),
            usage: None,
            timestamp: result.timestamp,
            is_error: true,
        }
    } else {
        ToolTranscriptItem::Complete {
            id: item_id,
            role: ToolRole::Tool,
            tool_call_id: result.tool_call_id.clone(),
            tool_name: result.tool_name.clone(),
            input: result.input.clone(),
            content: result.content.clone(),
            details: result.details.clone(),
            usage: None,
            timestamp: result.timestamp,
            is_error: false,
        }
    }
}

/// 创建纯文本用户输入，供 Provider/Session 适配器和测试共享。
pub fn user_text_item(id: String, text: String, timestamp: u64) -> UserTranscriptItem {
    UserTranscriptItem {
        id,
        role: UserRole::User,
        content: vec![TextOrImageContent::Text { text }],
        timestamp,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use protocol::{
        AssistantContent, AssistantStopReason, AssistantTranscriptItem, ModelRef,
        ProviderStreamEvent, TextOrImageContent, ToolCall, ToolResult, TranscriptItem, Usage,
        UsageCost,
    };
    use serde::Deserialize;
    use serde_json::json;

    use super::{
        AgentLoopEngine, AgentLoopEvent, ProviderEvent, ToolRuntimePort, ToolRuntimePortEvent,
        user_text_item,
    };

    /// 与 TypeScript protocol package 共用的 Provider 流回放样本。
    ///
    /// fixture 放在协议包而非 Rust crate 内，确保字段变更必须同时通过两端验证。
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ProviderStreamFixture {
        name: String,
        events: Vec<ProviderStreamEvent>,
        expected: FixtureExpectation,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureExpectation {
        event_names: Vec<String>,
        assistant_text: Option<String>,
        assistant_thinking: Option<String>,
        assistant_status: Option<String>,
        response_model: Option<String>,
    }

    fn usage() -> Usage {
        Usage {
            input: 10,
            output: 5,
            cache_read: 2,
            cache_write: 1,
            reasoning: Some(3),
            total_tokens: 21,
            cost: UsageCost {
                input: 0.01,
                output: 0.02,
                cache_read: 0.003,
                cache_write: 0.004,
                total: 0.037,
            },
        }
    }

    fn completed(
        message_id: &str,
        content: Vec<AssistantContent>,
        timestamp: u64,
        stop_reason: AssistantStopReason,
    ) -> ProviderEvent {
        ProviderEvent::Completed {
            message_id: message_id.to_owned(),
            content,
            response_model: Some("resolved-model".to_owned()),
            usage: usage(),
            timestamp,
            stop_reason,
        }
    }

    fn failed(
        message_id: &str,
        content: Vec<AssistantContent>,
        timestamp: u64,
        message: &str,
    ) -> ProviderEvent {
        ProviderEvent::Failed {
            message_id: message_id.to_owned(),
            content,
            response_model: Some("resolved-model".to_owned()),
            usage: Some(usage()),
            timestamp,
            message: message.to_owned(),
        }
    }

    fn engine() -> AgentLoopEngine {
        AgentLoopEngine::new(ModelRef {
            provider: "test".to_owned(),
            id: "model-a".to_owned(),
        })
    }

    /// 用预先给定的端口事件验证 Agent Loop，不把具体 Tool Runtime 引入状态机测试。
    #[derive(Debug, Clone)]
    struct FakeToolRuntime {
        events: Vec<ToolRuntimePortEvent>,
        active: bool,
    }

    impl ToolRuntimePort for FakeToolRuntime {
        fn start(
            &mut self,
            _calls: Vec<ToolCall>,
            _timestamp: u64,
        ) -> Result<Vec<ToolRuntimePortEvent>, String> {
            self.active = false;
            Ok(self.events.clone())
        }

        fn poll(&mut self, _timestamp: u64) -> Vec<ToolRuntimePortEvent> {
            Vec::new()
        }

        fn cancel(&mut self) {
            self.active = false;
        }

        fn has_active_batch(&self) -> bool {
            self.active
        }
    }

    #[test]
    fn maps_shared_provider_stream_events_to_indexed_agent_events() {
        let start = ProviderEvent::try_from(ProviderStreamEvent::Start {
            message_id: "assistant-1".to_owned(),
            timestamp: 1,
        })
        .expect("start should map");
        assert_eq!(
            start,
            ProviderEvent::Started {
                message_id: "assistant-1".to_owned(),
                timestamp: 1,
            }
        );
        assert_eq!(
            ProviderEvent::try_from(ProviderStreamEvent::TextDelta {
                content_index: 0,
                delta: "hello".to_owned(),
            })
            .expect("text delta should map"),
            ProviderEvent::TextDelta {
                content_index: 0,
                text: "hello".to_owned(),
            }
        );
        assert_eq!(
            ProviderEvent::try_from(ProviderStreamEvent::ThinkingDelta {
                content_index: 1,
                delta: "reasoning".to_owned(),
                redacted: Some(false),
            })
            .expect("thinking delta should map"),
            ProviderEvent::ThinkingDelta {
                content_index: 1,
                thinking: "reasoning".to_owned(),
                redacted: Some(false),
            }
        );
    }

    #[test]
    fn replays_shared_text_thinking_and_error_fixtures() {
        for fixture_name in [
            "text-multi-delta",
            "thinking-text-interleaved",
            "provider-error",
        ] {
            let fixture = load_fixture(fixture_name);
            let mut loop_engine = engine();
            loop_engine
                .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
                .expect("prompt should start");
            loop_engine.drain_events();

            let mut events = Vec::new();
            for provider_event in fixture.events {
                let provider_event = ProviderEvent::try_from(provider_event)
                    .expect("shared fixture should use supported normalized events");
                events.extend(
                    loop_engine
                        .handle_provider_event(provider_event)
                        .expect("fixture event should be accepted"),
                );
            }

            assert_eq!(
                events.iter().map(event_name).collect::<Vec<_>>(),
                fixture.expected.event_names,
                "fixture {} emitted an unexpected event sequence",
                fixture.name
            );
            assert_eq!(
                assistant_text(&loop_engine),
                fixture.expected.assistant_text
            );
            assert_eq!(
                assistant_thinking(&loop_engine),
                fixture.expected.assistant_thinking
            );
            if let Some(status) = fixture.expected.assistant_status {
                assert_eq!(assistant_status(&loop_engine), status);
            }
            if let Some(response_model) = fixture.expected.response_model {
                assert_eq!(assistant_response_model(&loop_engine), Some(response_model));
            }
        }
    }

    #[test]
    fn replays_the_typescript_text_turn_event_order() {
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
            .expect("prompt should start");
        let mut events = loop_engine.drain_events();
        events.extend(
            loop_engine
                .handle_provider_event(ProviderEvent::Started {
                    message_id: "assistant-1".to_owned(),
                    timestamp: 2,
                })
                .expect("start should be accepted"),
        );
        events.extend(
            loop_engine
                .handle_provider_event(ProviderEvent::TextDelta {
                    content_index: 0,
                    text: "hel".to_owned(),
                })
                .expect("first delta should be accepted"),
        );
        events.extend(
            loop_engine
                .handle_provider_event(ProviderEvent::TextDelta {
                    content_index: 0,
                    text: "lo".to_owned(),
                })
                .expect("second delta should be accepted"),
        );
        events.extend(
            loop_engine
                .handle_provider_event(completed(
                    "assistant-1",
                    vec![AssistantContent::Text {
                        text: "hello".to_owned(),
                    }],
                    3,
                    AssistantStopReason::Stop,
                ))
                .expect("completion should be accepted"),
        );

        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            vec![
                "agent_start",
                "turn_start",
                "message_start",
                "message_end",
                "message_start",
                "message_update",
                "message_update",
                "message_end",
                "turn_end",
                "agent_end",
            ]
        );
        assert!(!loop_engine.is_active());
        assert_eq!(loop_engine.messages().len(), 2);
    }

    #[test]
    fn replays_shared_tool_call_fixture_without_ending_the_agent_turn() {
        let fixture = load_fixture("tool-call");
        let call = fixture
            .events
            .iter()
            .find_map(|event| match event {
                ProviderStreamEvent::ToolCallEnd { tool_call, .. } => Some(tool_call.clone()),
                _ => None,
            })
            .expect("shared fixture must include a completed tool call");
        let mut tool_runtime = FakeToolRuntime {
            events: vec![
                ToolRuntimePortEvent::Started {
                    tool_call: call.clone(),
                },
                ToolRuntimePortEvent::Finished {
                    result: ToolResult {
                        tool_call_id: call.tool_call_id,
                        tool_name: call.tool_name,
                        input: call.input,
                        content: vec![TextOrImageContent::Text {
                            text: "echo: hello".to_owned(),
                        }],
                        details: None,
                        is_error: false,
                        timestamp: 21,
                    },
                },
            ],
            active: false,
        };
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item(
                "user-1".to_owned(),
                "use echo".to_owned(),
                1,
            ))
            .unwrap();
        loop_engine.drain_events();

        let mut events = Vec::new();
        for provider_event in fixture.events {
            events.extend(
                loop_engine
                    .handle_provider_event_with_tools(
                        ProviderEvent::try_from(provider_event)
                            .expect("shared fixture must use supported events"),
                        &mut tool_runtime,
                    )
                    .expect("shared tool fixture event should be accepted"),
            );
        }

        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            fixture.expected.event_names
        );
        assert!(loop_engine.is_active());
    }

    #[test]
    fn replays_shared_parallel_tool_fixture_in_source_order_after_reverse_completion() {
        let fixture = load_fixture("parallel-tool-calls");
        let calls = fixture
            .events
            .iter()
            .filter_map(|event| match event {
                ProviderStreamEvent::ToolCallEnd { tool_call, .. } => Some(tool_call.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            calls
                .iter()
                .map(|call| call.tool_call_id.as_str())
                .collect::<Vec<_>>(),
            vec!["call-1", "call-2"],
            "共享 fixture 必须固定 Provider 的工具调用 source order"
        );
        let result_for = |call: &ToolCall| ToolResult {
            tool_call_id: call.tool_call_id.clone(),
            tool_name: call.tool_name.clone(),
            input: call.input.clone(),
            content: vec![TextOrImageContent::Text {
                text: format!("echo: {}", call.tool_call_id),
            }],
            details: None,
            is_error: false,
            timestamp: 21,
        };
        let mut tool_runtime = FakeToolRuntime {
            events: vec![
                ToolRuntimePortEvent::Started {
                    tool_call: calls[0].clone(),
                },
                ToolRuntimePortEvent::Started {
                    tool_call: calls[1].clone(),
                },
                // 模拟两个并行 worker 的实际完成顺序与 Provider source order 相反。
                ToolRuntimePortEvent::Finished {
                    result: result_for(&calls[1]),
                },
                ToolRuntimePortEvent::Finished {
                    result: result_for(&calls[0]),
                },
            ],
            active: false,
        };
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item(
                "user-1".to_owned(),
                "use echo".to_owned(),
                1,
            ))
            .unwrap();
        loop_engine.drain_events();
        let mut events = Vec::new();
        for provider_event in fixture.events {
            events.extend(
                loop_engine
                    .handle_provider_event_with_tools(
                        ProviderEvent::try_from(provider_event)
                            .expect("共享 fixture 必须使用支持的 Provider 事件"),
                        &mut tool_runtime,
                    )
                    .expect("共享并行工具 fixture 应被接受"),
            );
        }

        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            fixture.expected.event_names
        );
        assert_eq!(
            loop_engine.messages()[2..]
                .iter()
                .filter_map(|item| match item {
                    TranscriptItem::Tool(protocol::ToolTranscriptItem::Complete {
                        tool_call_id,
                        ..
                    }) => {
                        Some(tool_call_id.as_str())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec!["call-1", "call-2"]
        );
        let ready = events
            .iter()
            .find_map(|event| match event {
                AgentLoopEvent::ToolResultsReady { results } => Some(results),
                _ => None,
            })
            .expect("batch should produce one continuation trigger");
        assert_eq!(
            ready
                .iter()
                .map(|result| result.tool_call_id.as_str())
                .collect::<Vec<_>>(),
            vec!["call-1", "call-2"]
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, AgentLoopEvent::ToolResultsReady { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn executes_tool_calls_in_serial_lifecycle_order_and_continues_the_turn() {
        let call = ToolCall {
            tool_call_id: "call-1".to_owned(),
            tool_name: "echo".to_owned(),
            input: json!({ "value": "hello" }),
        };
        let result = ToolResult {
            tool_call_id: call.tool_call_id.clone(),
            tool_name: call.tool_name.clone(),
            input: call.input.clone(),
            content: vec![TextOrImageContent::Text {
                text: "echo: hello".to_owned(),
            }],
            details: None,
            is_error: false,
            timestamp: 21,
        };
        let mut tool_runtime = FakeToolRuntime {
            events: vec![
                ToolRuntimePortEvent::Started {
                    tool_call: call.clone(),
                },
                ToolRuntimePortEvent::Finished {
                    result: result.clone(),
                },
            ],
            active: false,
        };
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item(
                "user-1".to_owned(),
                "use echo".to_owned(),
                1,
            ))
            .unwrap();
        loop_engine.drain_events();

        let mut events = loop_engine
            .handle_provider_event_with_tools(
                ProviderEvent::Started {
                    message_id: "assistant-tool-1".to_owned(),
                    timestamp: 20,
                },
                &mut tool_runtime,
            )
            .unwrap();
        events.extend(
            loop_engine
                .handle_provider_event_with_tools(
                    ProviderEvent::ToolCallDelta {
                        content_index: 0,
                        delta: "{\"value\":\"hello\"}".to_owned(),
                    },
                    &mut tool_runtime,
                )
                .unwrap(),
        );
        events.extend(
            loop_engine
                .handle_provider_event_with_tools(
                    ProviderEvent::ToolCallEnded {
                        content_index: 0,
                        tool_call: call.clone(),
                    },
                    &mut tool_runtime,
                )
                .unwrap(),
        );
        events.extend(
            loop_engine
                .handle_provider_event_with_tools(
                    completed(
                        "assistant-tool-1",
                        vec![AssistantContent::ToolCall {
                            tool_call_id: call.tool_call_id,
                            tool_name: call.tool_name,
                            input: call.input,
                        }],
                        21,
                        AssistantStopReason::ToolUse,
                    ),
                    &mut tool_runtime,
                )
                .unwrap(),
        );

        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            vec![
                "message_start",
                "message_update",
                "message_update",
                "message_end",
                "tool_execution_start",
                "tool_execution_end",
                "message_start",
                "message_end",
                "tool_results_ready",
            ]
        );
        assert!(loop_engine.is_active());
        assert_eq!(loop_engine.messages().len(), 3);
        assert!(matches!(
            &loop_engine.messages()[2],
            TranscriptItem::Tool(protocol::ToolTranscriptItem::Complete { tool_call_id, .. })
                if tool_call_id == "call-1"
        ));

        let events = loop_engine
            .handle_provider_event_with_tools(
                ProviderEvent::Started {
                    message_id: "assistant-final-1".to_owned(),
                    timestamp: 22,
                },
                &mut tool_runtime,
            )
            .unwrap();
        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            vec!["message_start"]
        );
        loop_engine
            .handle_provider_event_with_tools(
                ProviderEvent::TextDelta {
                    content_index: 0,
                    text: "done".to_owned(),
                },
                &mut tool_runtime,
            )
            .unwrap();
        let events = loop_engine
            .handle_provider_event_with_tools(
                completed(
                    "assistant-final-1",
                    vec![AssistantContent::Text {
                        text: "done".to_owned(),
                    }],
                    23,
                    AssistantStopReason::Stop,
                ),
                &mut tool_runtime,
            )
            .unwrap();
        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            vec!["message_end", "turn_end", "agent_end"]
        );
        assert!(!loop_engine.is_active());
    }

    #[test]
    fn rejects_tool_use_terminal_without_authoritative_tool_call() {
        let mut tool_runtime = FakeToolRuntime {
            events: Vec::new(),
            active: false,
        };
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item(
                "user-1".to_owned(),
                "use tool".to_owned(),
                1,
            ))
            .unwrap();
        loop_engine.drain_events();
        loop_engine
            .handle_provider_event_with_tools(
                ProviderEvent::Started {
                    message_id: "assistant-tool-1".to_owned(),
                    timestamp: 2,
                },
                &mut tool_runtime,
            )
            .unwrap();
        loop_engine
            .handle_provider_event_with_tools(
                ProviderEvent::ToolCallDelta {
                    content_index: 0,
                    delta: "{\"value\":".to_owned(),
                },
                &mut tool_runtime,
            )
            .unwrap();

        let error = loop_engine
            .handle_provider_event_with_tools(
                completed(
                    "assistant-tool-1",
                    Vec::new(),
                    3,
                    AssistantStopReason::ToolUse,
                ),
                &mut tool_runtime,
            )
            .expect_err("toolUse terminal without a complete tool call must be rejected");
        assert_eq!(
            error.message(),
            "Provider 以 toolUse 结束，但终态快照未提供完整工具调用。"
        );
        assert!(loop_engine.is_active());
        assert_eq!(loop_engine.messages().len(), 1);
    }

    #[test]
    fn abort_waits_for_adapter_settlement_and_preserves_partial_text() {
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
            .unwrap();
        loop_engine.drain_events();
        loop_engine
            .handle_provider_event(ProviderEvent::Started {
                message_id: "assistant-1".to_owned(),
                timestamp: 2,
            })
            .unwrap();
        loop_engine
            .handle_provider_event(ProviderEvent::TextDelta {
                content_index: 0,
                text: "partial".to_owned(),
            })
            .unwrap();
        loop_engine.abort().unwrap();
        assert!(loop_engine.is_active());

        let events = loop_engine.settle_abort(3);
        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            vec!["message_end", "turn_end", "agent_end"]
        );
        assert!(!loop_engine.is_active());
        assert!(matches!(
            &loop_engine.messages()[1],
            TranscriptItem::Assistant(protocol::AssistantTranscriptItem::Aborted { .. })
        ));
    }

    #[test]
    fn terminal_without_start_synthesizes_message_start_like_typescript() {
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
            .expect("prompt should start");
        loop_engine.drain_events();

        let delta_error = loop_engine
            .handle_provider_event(ProviderEvent::TextDelta {
                content_index: 0,
                text: "unexpected".to_owned(),
            })
            .expect_err("text delta before start must be rejected");
        assert_eq!(
            delta_error.message(),
            "收到文本增量前必须先收到 Provider start。"
        );

        let events = loop_engine
            .handle_provider_event(completed(
                "assistant-1",
                vec![AssistantContent::Text {
                    text: "terminal only".to_owned(),
                }],
                2,
                AssistantStopReason::Stop,
            ))
            .expect("terminal snapshot without partial start should be accepted");
        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            vec!["message_start", "message_end", "turn_end", "agent_end"]
        );
        assert!(!loop_engine.is_active());
        assert_eq!(
            assistant_text(&loop_engine).as_deref(),
            Some("terminal only")
        );
    }

    #[test]
    fn rejects_terminal_id_content_kind_and_thinking_redaction_conflicts() {
        let mut id_engine = engine();
        id_engine
            .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
            .unwrap();
        id_engine.drain_events();
        id_engine
            .handle_provider_event(ProviderEvent::Started {
                message_id: "assistant-1".to_owned(),
                timestamp: 2,
            })
            .unwrap();
        let error = id_engine
            .handle_provider_event(completed(
                "assistant-2",
                vec![AssistantContent::Text {
                    text: "terminal".to_owned(),
                }],
                3,
                AssistantStopReason::Stop,
            ))
            .expect_err("terminal message id mismatch must be rejected");
        assert_eq!(
            error.message(),
            "Provider 终态 messageId 与当前 assistant 不一致。"
        );

        let mut content_engine = engine();
        content_engine
            .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
            .unwrap();
        content_engine.drain_events();
        content_engine
            .handle_provider_event(ProviderEvent::Started {
                message_id: "assistant-1".to_owned(),
                timestamp: 2,
            })
            .unwrap();
        content_engine
            .handle_provider_event(ProviderEvent::TextDelta {
                content_index: 0,
                text: "text".to_owned(),
            })
            .unwrap();
        let error = content_engine
            .handle_provider_event(ProviderEvent::ThinkingDelta {
                content_index: 0,
                thinking: "thinking".to_owned(),
                redacted: Some(false),
            })
            .expect_err("one content index cannot change its content kind");
        assert_eq!(
            error.message(),
            "同一个 content index 不能同时表示 thinking 和其他内容。"
        );

        let mut redaction_engine = engine();
        redaction_engine
            .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
            .unwrap();
        redaction_engine.drain_events();
        redaction_engine
            .handle_provider_event(ProviderEvent::Started {
                message_id: "assistant-1".to_owned(),
                timestamp: 2,
            })
            .unwrap();
        redaction_engine
            .handle_provider_event(ProviderEvent::ThinkingDelta {
                content_index: 0,
                thinking: "first".to_owned(),
                redacted: Some(false),
            })
            .unwrap();
        let error = redaction_engine
            .handle_provider_event(ProviderEvent::ThinkingDelta {
                content_index: 0,
                thinking: "second".to_owned(),
                redacted: Some(true),
            })
            .expect_err("thinking redaction cannot change within one content index");
        assert_eq!(
            error.message(),
            "同一个 thinking content index 的 redacted 属性不能改变。"
        );
    }

    #[test]
    fn provider_failure_commits_an_error_assistant_and_ends_the_turn() {
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
            .expect("prompt should start");
        loop_engine.drain_events();
        loop_engine
            .handle_provider_event(ProviderEvent::Started {
                message_id: "assistant-1".to_owned(),
                timestamp: 2,
            })
            .expect("start should be accepted");
        loop_engine
            .handle_provider_event(ProviderEvent::TextDelta {
                content_index: 0,
                text: "partial".to_owned(),
            })
            .expect("text delta should be accepted");

        let events = loop_engine
            .handle_provider_event(failed(
                "assistant-1",
                vec![AssistantContent::Text {
                    text: "stable partial".to_owned(),
                }],
                3,
                "network failed",
            ))
            .expect("failure should become a stable error transcript item");

        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            vec!["message_end", "turn_end", "agent_end"]
        );
        assert!(!loop_engine.is_active());
        assert!(matches!(
            &loop_engine.messages()[1],
            TranscriptItem::Assistant(protocol::AssistantTranscriptItem::Error {
                content,
                response_model: Some(response_model),
                usage: Some(terminal_usage),
                error_message: Some(message),
                ..
            }) if content == &vec![AssistantContent::Text { text: "stable partial".to_owned() }]
                && response_model == "resolved-model"
                && terminal_usage == &usage()
                && message == "network failed"
        ));
    }

    #[test]
    fn retry_discards_only_the_failed_working_message_then_restarts_same_turn() {
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
            .expect("prompt should start");
        loop_engine.drain_events();
        loop_engine
            .handle_provider_event(ProviderEvent::Started {
                message_id: "assistant-1".to_owned(),
                timestamp: 2,
            })
            .expect("provider start should be accepted");
        loop_engine
            .handle_provider_event(failed("assistant-1", vec![], 3, "network timeout"))
            .expect("failure should finish the first attempt");

        loop_engine
            .discard_last_error_assistant_for_retry()
            .expect("retry should remove the failed assistant from provider context");
        assert_eq!(loop_engine.messages().len(), 1);
        assert!(matches!(loop_engine.messages()[0], TranscriptItem::User(_)));
        loop_engine
            .restart_after_retry()
            .expect("retry should reactivate the existing turn without another user prompt");
        assert!(loop_engine.is_active());
        assert!(loop_engine.active_assistant_id().is_none());
        assert!(
            loop_engine
                .restart_after_retry()
                .expect_err("an active retry turn cannot restart twice")
                .message()
                .contains("运行中的")
        );
    }

    #[test]
    fn next_provider_event_settles_an_abort_without_processing_that_event() {
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
            .expect("prompt should start");
        loop_engine.drain_events();
        loop_engine
            .handle_provider_event(ProviderEvent::Started {
                message_id: "assistant-1".to_owned(),
                timestamp: 2,
            })
            .expect("start should be accepted");
        loop_engine.abort().expect("abort should be accepted");

        let events = loop_engine
            .handle_provider_event(completed(
                "assistant-1",
                vec![AssistantContent::Text {
                    text: "ignored after abort".to_owned(),
                }],
                3,
                AssistantStopReason::Stop,
            ))
            .expect("a provider event after abort should settle cancellation");

        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            vec!["message_end", "turn_end", "agent_end"]
        );
        assert!(matches!(
            &loop_engine.messages()[1],
            TranscriptItem::Assistant(protocol::AssistantTranscriptItem::Aborted { .. })
        ));
    }

    #[test]
    fn steering_is_fifo_and_does_not_interrupt_the_current_provider_stream() {
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item("user-1".to_owned(), "first".to_owned(), 1))
            .unwrap();
        loop_engine
            .steer(user_text_item("user-2".to_owned(), "second".to_owned(), 2))
            .unwrap();
        loop_engine
            .steer(user_text_item("user-3".to_owned(), "third".to_owned(), 3))
            .unwrap();

        assert_eq!(
            loop_engine
                .queued_steer()
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["user-2", "user-3"]
        );
        assert!(loop_engine.is_active());
    }

    #[test]
    fn steering_precedes_follow_up_and_follow_ups_are_consumed_one_at_a_time() {
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item("user-1".to_owned(), "prompt".to_owned(), 1))
            .unwrap();
        loop_engine.drain_events();
        loop_engine
            .steer(user_text_item("user-2".to_owned(), "steer".to_owned(), 2))
            .unwrap();
        loop_engine
            .follow_up(user_text_item(
                "user-3".to_owned(),
                "follow-up-1".to_owned(),
                3,
            ))
            .unwrap();
        loop_engine
            .follow_up(user_text_item(
                "user-4".to_owned(),
                "follow-up-2".to_owned(),
                4,
            ))
            .unwrap();

        let first = loop_engine
            .handle_provider_event(completed(
                "assistant-1",
                vec![AssistantContent::Text {
                    text: "first response".to_owned(),
                }],
                5,
                AssistantStopReason::Stop,
            ))
            .unwrap();
        assert_eq!(
            first.iter().map(event_name).collect::<Vec<_>>(),
            vec![
                "message_start",
                "message_end",
                "message_start",
                "message_end",
                "steer_consumed",
                "provider_continuation_requested",
            ]
        );
        assert_eq!(
            loop_engine
                .messages()
                .iter()
                .filter_map(|item| match item {
                    TranscriptItem::User(message) => Some(message.id.as_str()),
                    TranscriptItem::Assistant(_) | TranscriptItem::Tool(_) => None,
                })
                .collect::<Vec<_>>(),
            vec!["user-1", "user-2"]
        );

        let second = loop_engine
            .handle_provider_event(completed(
                "assistant-2",
                vec![AssistantContent::Text {
                    text: "second response".to_owned(),
                }],
                6,
                AssistantStopReason::Stop,
            ))
            .unwrap();
        assert_eq!(
            second.iter().map(event_name).collect::<Vec<_>>(),
            vec![
                "message_start",
                "message_end",
                "message_start",
                "message_end",
                "follow_up_consumed",
                "provider_continuation_requested",
            ]
        );
        assert_eq!(loop_engine.queued_follow_up().len(), 1);

        let third = loop_engine
            .handle_provider_event(completed(
                "assistant-3",
                vec![AssistantContent::Text {
                    text: "third response".to_owned(),
                }],
                7,
                AssistantStopReason::Stop,
            ))
            .unwrap();
        assert_eq!(
            third.iter().map(event_name).collect::<Vec<_>>(),
            vec![
                "message_start",
                "message_end",
                "message_start",
                "message_end",
                "follow_up_consumed",
                "provider_continuation_requested",
            ]
        );
        assert!(loop_engine.queued_follow_up().is_empty());

        let final_events = loop_engine
            .handle_provider_event(completed(
                "assistant-4",
                vec![AssistantContent::Text {
                    text: "final response".to_owned(),
                }],
                8,
                AssistantStopReason::Stop,
            ))
            .unwrap();
        assert_eq!(
            final_events.iter().map(event_name).collect::<Vec<_>>(),
            vec!["message_start", "message_end", "turn_end", "agent_end"]
        );
        assert!(!loop_engine.is_active());
    }

    #[test]
    fn replaces_only_idle_complete_working_context() {
        let mut loop_engine = engine();
        let replacement = vec![TranscriptItem::User(user_text_item(
            "summary-user".to_owned(),
            "compressed context".to_owned(),
            10,
        ))];

        loop_engine
            .replace_context(replacement.clone())
            .expect("idle loop should accept rebuilt context");
        assert_eq!(loop_engine.messages(), replacement.as_slice());

        loop_engine
            .prompt(user_text_item("user-1".to_owned(), "run".to_owned(), 11))
            .expect("prompt should activate the loop");
        assert!(
            loop_engine
                .replace_context(replacement)
                .expect_err("active loop must not lose its current state")
                .message()
                .contains("运行中")
        );
    }

    fn load_fixture(name: &str) -> ProviderStreamFixture {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../packages/protocol/test/fixtures/provider-stream")
            .join(format!("{name}.json"));
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("无法读取共享 fixture {}: {error}", path.display()));
        serde_json::from_str(&content)
            .unwrap_or_else(|error| panic!("共享 fixture {} 不符合协议: {error}", path.display()))
    }

    fn assistant(loop_engine: &AgentLoopEngine) -> Option<&AssistantTranscriptItem> {
        match loop_engine.messages().last()? {
            TranscriptItem::Assistant(assistant) => Some(assistant),
            TranscriptItem::User(_) | TranscriptItem::Tool(_) => None,
        }
    }

    fn assistant_content(loop_engine: &AgentLoopEngine) -> Option<&[AssistantContent]> {
        Some(match assistant(loop_engine)? {
            AssistantTranscriptItem::Complete { content, .. }
            | AssistantTranscriptItem::Error { content, .. }
            | AssistantTranscriptItem::Aborted { content, .. }
            | AssistantTranscriptItem::Streaming { content, .. } => content,
        })
    }

    fn assistant_text(loop_engine: &AgentLoopEngine) -> Option<String> {
        assistant_content(loop_engine)?
            .iter()
            .find_map(|item| match item {
                AssistantContent::Text { text } => Some(text.clone()),
                AssistantContent::Thinking { .. } | AssistantContent::ToolCall { .. } => None,
            })
    }

    fn assistant_thinking(loop_engine: &AgentLoopEngine) -> Option<String> {
        assistant_content(loop_engine)?
            .iter()
            .find_map(|item| match item {
                AssistantContent::Thinking { thinking, .. } => Some(thinking.clone()),
                AssistantContent::Text { .. } | AssistantContent::ToolCall { .. } => None,
            })
    }

    fn assistant_response_model(loop_engine: &AgentLoopEngine) -> Option<String> {
        match assistant(loop_engine)? {
            AssistantTranscriptItem::Complete {
                response_model,
                usage,
                ..
            }
            | AssistantTranscriptItem::Error {
                response_model,
                usage,
                ..
            }
            | AssistantTranscriptItem::Aborted {
                response_model,
                usage,
                ..
            }
            | AssistantTranscriptItem::Streaming {
                response_model,
                usage,
                ..
            } => {
                assert!(
                    usage.is_some(),
                    "带 responseModel 的 fixture 必须保留 usage"
                );
                response_model.clone()
            }
        }
    }

    fn assistant_status(loop_engine: &AgentLoopEngine) -> String {
        match loop_engine.messages().last() {
            Some(TranscriptItem::Assistant(AssistantTranscriptItem::Complete { .. })) => "complete",
            Some(TranscriptItem::Assistant(AssistantTranscriptItem::Error { .. })) => "error",
            Some(TranscriptItem::Assistant(AssistantTranscriptItem::Aborted { .. })) => "aborted",
            Some(TranscriptItem::Assistant(AssistantTranscriptItem::Streaming { .. })) => {
                "streaming"
            }
            Some(TranscriptItem::User(_)) | Some(TranscriptItem::Tool(_)) | None => "unknown",
        }
        .to_owned()
    }

    fn event_name(event: &AgentLoopEvent) -> &'static str {
        match event {
            AgentLoopEvent::AgentStarted => "agent_start",
            AgentLoopEvent::TurnStarted => "turn_start",
            AgentLoopEvent::TranscriptItemStarted(_) => "message_start",
            AgentLoopEvent::TranscriptItemUpdated(_) => "message_update",
            AgentLoopEvent::TranscriptItemFinished(_) => "message_end",
            AgentLoopEvent::ToolExecutionStarted { .. } => "tool_execution_start",
            AgentLoopEvent::ToolExecutionUpdated { .. } => "tool_execution_update",
            AgentLoopEvent::ToolExecutionFinished { .. } => "tool_execution_end",
            AgentLoopEvent::ToolResultsReady { .. } => "tool_results_ready",
            AgentLoopEvent::ProviderContinuationRequested => "provider_continuation_requested",
            AgentLoopEvent::SteerConsumed => "steer_consumed",
            AgentLoopEvent::FollowUpConsumed => "follow_up_consumed",
            AgentLoopEvent::TurnEnded { .. } => "turn_end",
            AgentLoopEvent::AgentEnded { .. } => "agent_end",
        }
    }
}
