//! Rust Agent Loop 的可回放回合状态机。
//!
//! 本 crate 不直接调用 Provider SDK、Tool Runtime 或 TUI。它接收已经归一化的 Provider
//! 流事件，生成稳定的生命周期事件与 transcript 项；上层适配器负责把真实网络流逐项送入
//! [`AgentLoopEngine::handle_provider_event`]。这种设计使事件顺序可离线回放和单元测试，
//! 并避免核心状态机耦合 TypeScript Provider Host。

use std::collections::{BTreeMap, BTreeSet};

use protocol::{
    AbortedStopReason, AssistantContent, AssistantRole, AssistantStopReason,
    AssistantTranscriptItem, ErrorStopReason, ModelRef, ProviderStreamEvent, TextOrImageContent,
    ToolCall, ToolResult, ToolRole, ToolTranscriptItem, TranscriptItem, UserRole,
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
/// Provider 的原始 SSE 字段、认证和网络异常必须在 Host 侧先归一化。工具参数增量仅用于
/// 维持流式生命周期；实际执行只能使用已经完成解析的 [`ProviderEvent::ToolCallEnded`]。
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderEvent {
    /// Provider 已接受请求并开始生成 assistant 项。
    Started { message_id: String, timestamp: u64 },
    /// 一个文本增量。必须在 [`ProviderEvent::Started`] 后出现。
    TextDelta { text: String },
    /// 一个尚未完成的工具参数增量。
    ToolCallDelta { content_index: u64, delta: String },
    /// 一个已完成解析、允许执行的工具调用。
    ToolCallEnded {
        content_index: u64,
        tool_call: ToolCall,
    },
    /// 正常结束，并提交一个完成态 assistant 项。
    Completed {
        timestamp: u64,
        stop_reason: AssistantStopReason,
    },
    /// Provider 返回可恢复或不可恢复的错误；它仍会生成稳定 error assistant 项。
    Failed { timestamp: u64, message: String },
}

/// Tool Runtime 反馈给 Agent Loop 的稳定生命周期事件。
///
/// 该枚举属于端口契约而不是具体 Tool Runtime，避免 Agent Loop 依赖工具注册、Sandbox 或
/// 扩展宿主实现。事件顺序必须保持单个调用的开始、结束相邻，才能与 TypeScript 的串行工具
/// 调用生命周期一致。
#[derive(Debug, Clone, PartialEq)]
pub enum ToolRuntimePortEvent {
    /// 已开始处理一个完整工具调用。
    Started { tool_call: ToolCall },
    /// 工具已产生稳定结果；失败也以结果表示，不能中断同一批后续调用。
    Finished { result: ToolResult },
}

/// Tool Runtime 的窄端口。
///
/// Agent Loop 只负责工具调用的生命周期、结果转录和下一次 Provider 请求的上下文准备；具体
/// 工具注册、Sandbox、扩展宿主和执行策略均留在端口实现侧。当前切片固定按源顺序串行执行。
pub trait ToolRuntimePort {
    /// 执行一批完整工具调用，并按实际生命周期顺序返回端口事件。
    fn execute_serial(&self, calls: Vec<ToolCall>, timestamp: u64) -> Vec<ToolRuntimePortEvent>;
}

/// 将跨语言 DTO 转换为内部状态机事件。
///
/// 适配层必须在到达状态机前处理 Provider SDK 的私有字段。thinking 尚未具备 transcript
/// 状态机，仍会显式拒绝；工具调用已转入独立的 Tool Runtime Port。
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
            ProviderStreamEvent::TextDelta { delta, .. } => Ok(Self::TextDelta { text: delta }),
            ProviderStreamEvent::Done {
                timestamp,
                stop_reason,
            } => Ok(Self::Completed {
                timestamp,
                stop_reason,
            }),
            ProviderStreamEvent::Error { timestamp, message } => {
                Ok(Self::Failed { timestamp, message })
            }
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
            ProviderStreamEvent::ThinkingDelta { .. } => Err(AgentLoopError::new(
                "当前 Agent Loop 尚未启用 thinking 事件。",
            )),
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
    content: Vec<AssistantContent>,
    /// 仅记录尚未收到 terminal event 的参数片段，避免执行截断调用。
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
    queued_steer: Vec<UserTranscriptItem>,
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
            queued_steer: Vec::new(),
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

    /// 返回尚未注入下一次 Provider 请求的 steering 消息。
    pub fn queued_steer(&self) -> &[UserTranscriptItem] {
        &self.queued_steer
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
    /// 本基础切片保留队列和消费顺序；Provider/多回合调度就绪后再调用
    /// [`AgentLoopEngine::drain_steer`] 取得下一请求的输入。
    pub fn steer(&mut self, message: UserTranscriptItem) -> Result<(), AgentLoopError> {
        if !self.active {
            return Err(AgentLoopError::new("Agent Loop 空闲时不能接收 steer。"));
        }
        self.queued_steer.push(message);
        Ok(())
    }

    /// 取走 steering 队列，保持 FIFO 顺序。
    pub fn drain_steer(&mut self) -> Vec<UserTranscriptItem> {
        std::mem::take(&mut self.queued_steer)
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
        tool_runtime: &impl ToolRuntimePort,
    ) -> Result<Vec<AgentLoopEvent>, AgentLoopError> {
        self.handle_provider_event_inner(event, Some(tool_runtime))
    }

    fn handle_provider_event_inner(
        &mut self,
        event: ProviderEvent,
        tool_runtime: Option<&dyn ToolRuntimePort>,
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
            ProviderEvent::TextDelta { text } => self.append_text(text)?,
            ProviderEvent::ToolCallDelta {
                content_index,
                delta,
            } => self.append_tool_call_delta(content_index, delta)?,
            ProviderEvent::ToolCallEnded {
                content_index,
                tool_call,
            } => self.finish_tool_call(content_index, tool_call)?,
            ProviderEvent::Completed {
                timestamp,
                stop_reason,
            } => {
                if stop_reason == AssistantStopReason::ToolUse && tool_runtime.is_none() {
                    return Err(AgentLoopError::new(
                        "收到 toolUse 终态时必须提供 Tool Runtime Port。",
                    ));
                }
                let calls = self.complete_assistant(timestamp, stop_reason)?;
                if let Some(runtime) = tool_runtime {
                    if !calls.is_empty() {
                        self.execute_tool_calls(calls, timestamp, runtime);
                    }
                }
            }
            ProviderEvent::Failed { timestamp, message } => {
                self.fail_assistant(timestamp, message)?
            }
        }

        Ok(self.drain_events())
    }

    /// 当 Provider adapter 已确认取消生效、但没有可用的终态流事件时调用。
    pub fn settle_abort(&mut self, timestamp: u64) -> Vec<AgentLoopEvent> {
        if !self.active {
            return Vec::new();
        }

        let assistant = self.active_assistant.take();
        let item = TranscriptItem::Assistant(AssistantTranscriptItem::Aborted {
            id: assistant
                .as_ref()
                .map_or_else(|| format!("aborted-{timestamp}"), |value| value.id.clone()),
            role: AssistantRole::Assistant,
            content: assistant.map_or_else(Vec::new, |value| value.content),
            model: self.model.clone(),
            response_model: None,
            usage: None,
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
            content: Vec::new(),
            tool_call_deltas: BTreeMap::new(),
            completed_tool_call_indexes: BTreeSet::new(),
        };
        self.events.push(AgentLoopEvent::TranscriptItemStarted(
            TranscriptItem::Assistant(self.streaming_item(&assistant)),
        ));
        self.active_assistant = Some(assistant);
        Ok(())
    }

    fn append_text(&mut self, text: String) -> Result<(), AgentLoopError> {
        let model = self.model.clone();
        let assistant = self
            .active_assistant
            .as_mut()
            .ok_or_else(|| AgentLoopError::new("收到文本增量前必须先收到 Provider start。"))?;
        match assistant.content.last_mut() {
            Some(AssistantContent::Text { text: accumulated }) => accumulated.push_str(&text),
            _ => assistant.content.push(AssistantContent::Text { text }),
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
        assistant.content.push(AssistantContent::ToolCall {
            tool_call_id: tool_call.tool_call_id,
            tool_name: tool_call.tool_name,
            input: tool_call.input,
        });
        self.events
            .push(AgentLoopEvent::TranscriptItemUpdated(streaming_item_for(
                &model, assistant,
            )));
        Ok(())
    }

    fn complete_assistant(
        &mut self,
        timestamp: u64,
        stop_reason: AssistantStopReason,
    ) -> Result<Vec<ToolCall>, AgentLoopError> {
        let assistant = self.active_assistant.as_ref().ok_or_else(|| {
            AgentLoopError::new("收到 Provider 完成事件前必须先收到 Provider start。")
        })?;
        if !assistant.tool_call_deltas.is_empty() {
            return Err(AgentLoopError::new(
                "Provider 在存在未完成工具调用增量时结束，拒绝执行不完整参数。",
            ));
        }
        let calls = assistant
            .content
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
            .collect::<Vec<_>>();
        if stop_reason == AssistantStopReason::ToolUse && calls.is_empty() {
            return Err(AgentLoopError::new(
                "Provider 以 toolUse 结束，但未提供完整工具调用。",
            ));
        }
        if stop_reason != AssistantStopReason::ToolUse && !calls.is_empty() {
            return Err(AgentLoopError::new(
                "非 toolUse 的 Provider 终态不能包含工具调用。",
            ));
        }

        let assistant = self
            .active_assistant
            .take()
            .expect("已校验 Provider 完成事件存在执行中的 assistant");
        let item = TranscriptItem::Assistant(AssistantTranscriptItem::Complete {
            id: assistant.id,
            role: AssistantRole::Assistant,
            content: assistant.content,
            model: self.model.clone(),
            response_model: None,
            usage: None,
            timestamp,
            stop_reason,
        });
        self.events
            .push(AgentLoopEvent::TranscriptItemFinished(item.clone()));
        self.messages.push(item);

        if calls.is_empty() && stop_reason != AssistantStopReason::ToolUse {
            self.finish_active_turn();
        }
        Ok(calls)
    }

    fn execute_tool_calls(
        &mut self,
        calls: Vec<ToolCall>,
        timestamp: u64,
        tool_runtime: &dyn ToolRuntimePort,
    ) {
        let mut results = Vec::new();
        for event in tool_runtime.execute_serial(calls, timestamp) {
            match event {
                ToolRuntimePortEvent::Started { tool_call } => {
                    self.events
                        .push(AgentLoopEvent::ToolExecutionStarted { tool_call });
                }
                ToolRuntimePortEvent::Finished { result } => {
                    self.events.push(AgentLoopEvent::ToolExecutionFinished {
                        result: result.clone(),
                    });
                    self.commit_message(TranscriptItem::Tool(tool_result_item(&result)));
                    results.push(result);
                }
            }
        }
        self.events
            .push(AgentLoopEvent::ToolResultsReady { results });
    }

    fn fail_assistant(&mut self, timestamp: u64, message: String) -> Result<(), AgentLoopError> {
        let assistant = self.active_assistant.take().ok_or_else(|| {
            AgentLoopError::new("收到 Provider 错误事件前必须先收到 Provider start。")
        })?;
        let item = TranscriptItem::Assistant(AssistantTranscriptItem::Error {
            id: assistant.id,
            role: AssistantRole::Assistant,
            content: assistant.content,
            model: self.model.clone(),
            response_model: None,
            usage: None,
            timestamp,
            stop_reason: ErrorStopReason::Error,
            error_message: Some(message),
        });
        self.finish_turn(item);
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

fn streaming_item_for(model: &ModelRef, assistant: &ActiveAssistant) -> AssistantTranscriptItem {
    AssistantTranscriptItem::Streaming {
        id: assistant.id.clone(),
        role: AssistantRole::Assistant,
        content: assistant.content.clone(),
        model: model.clone(),
        response_model: None,
        usage: None,
        timestamp: assistant.timestamp,
    }
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
        ProviderStreamEvent, TextOrImageContent, ToolCall, ToolResult, TranscriptItem,
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
        assistant_status: Option<String>,
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
    }

    impl ToolRuntimePort for FakeToolRuntime {
        fn execute_serial(
            &self,
            _calls: Vec<ToolCall>,
            _timestamp: u64,
        ) -> Vec<ToolRuntimePortEvent> {
            self.events.clone()
        }
    }

    #[test]
    fn maps_shared_provider_stream_events_to_the_text_mvp() {
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
                text: "hello".to_owned(),
            }
        );
        assert!(
            ProviderEvent::try_from(ProviderStreamEvent::ThinkingDelta {
                content_index: 0,
                delta: "reasoning".to_owned(),
                redacted: None,
            })
            .is_err()
        );
    }

    #[test]
    fn replays_shared_text_and_error_fixtures() {
        for fixture_name in ["text-multi-delta", "provider-error"] {
            let fixture = load_fixture(fixture_name);
            let mut loop_engine = engine();
            loop_engine
                .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
                .expect("prompt should start");
            loop_engine.drain_events();

            let mut events = Vec::new();
            for provider_event in fixture.events {
                let provider_event = ProviderEvent::try_from(provider_event)
                    .expect("text and error fixtures should use the text MVP events");
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
            if let Some(status) = fixture.expected.assistant_status {
                assert_eq!(assistant_status(&loop_engine), status);
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
                    text: "hel".to_owned(),
                })
                .expect("first delta should be accepted"),
        );
        events.extend(
            loop_engine
                .handle_provider_event(ProviderEvent::TextDelta {
                    text: "lo".to_owned(),
                })
                .expect("second delta should be accepted"),
        );
        events.extend(
            loop_engine
                .handle_provider_event(ProviderEvent::Completed {
                    timestamp: 3,
                    stop_reason: AssistantStopReason::Stop,
                })
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
        let tool_runtime = FakeToolRuntime {
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
                        &tool_runtime,
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
        let tool_runtime = FakeToolRuntime {
            events: vec![
                ToolRuntimePortEvent::Started {
                    tool_call: call.clone(),
                },
                ToolRuntimePortEvent::Finished {
                    result: result.clone(),
                },
            ],
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
                &tool_runtime,
            )
            .unwrap();
        events.extend(
            loop_engine
                .handle_provider_event_with_tools(
                    ProviderEvent::ToolCallDelta {
                        content_index: 0,
                        delta: "{\"value\":\"hello\"}".to_owned(),
                    },
                    &tool_runtime,
                )
                .unwrap(),
        );
        events.extend(
            loop_engine
                .handle_provider_event_with_tools(
                    ProviderEvent::ToolCallEnded {
                        content_index: 0,
                        tool_call: call,
                    },
                    &tool_runtime,
                )
                .unwrap(),
        );
        events.extend(
            loop_engine
                .handle_provider_event_with_tools(
                    ProviderEvent::Completed {
                        timestamp: 21,
                        stop_reason: AssistantStopReason::ToolUse,
                    },
                    &tool_runtime,
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
                &tool_runtime,
            )
            .unwrap();
        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            vec!["message_start"]
        );
        loop_engine
            .handle_provider_event_with_tools(
                ProviderEvent::TextDelta {
                    text: "done".to_owned(),
                },
                &tool_runtime,
            )
            .unwrap();
        let events = loop_engine
            .handle_provider_event_with_tools(
                ProviderEvent::Completed {
                    timestamp: 23,
                    stop_reason: AssistantStopReason::Stop,
                },
                &tool_runtime,
            )
            .unwrap();
        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            vec!["message_end", "turn_end", "agent_end"]
        );
        assert!(!loop_engine.is_active());
    }

    #[test]
    fn rejects_incomplete_or_invalid_tool_terminal_states_without_committing() {
        let tool_runtime = FakeToolRuntime { events: Vec::new() };
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
                &tool_runtime,
            )
            .unwrap();
        loop_engine
            .handle_provider_event_with_tools(
                ProviderEvent::ToolCallDelta {
                    content_index: 0,
                    delta: "{\"value\":".to_owned(),
                },
                &tool_runtime,
            )
            .unwrap();

        let error = loop_engine
            .handle_provider_event_with_tools(
                ProviderEvent::Completed {
                    timestamp: 3,
                    stop_reason: AssistantStopReason::ToolUse,
                },
                &tool_runtime,
            )
            .expect_err("truncated tool arguments must not be committed");
        assert_eq!(
            error.message(),
            "Provider 在存在未完成工具调用增量时结束，拒绝执行不完整参数。"
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
        let TranscriptItem::Assistant(assistant) = &loop_engine.messages()[1] else {
            panic!("second item must be assistant")
        };
        assert!(matches!(
            assistant,
            protocol::AssistantTranscriptItem::Aborted { .. }
        ));
    }

    #[test]
    fn rejects_provider_terminal_events_that_do_not_follow_a_start() {
        let mut loop_engine = engine();
        loop_engine
            .prompt(user_text_item("user-1".to_owned(), "hello".to_owned(), 1))
            .expect("prompt should start");
        loop_engine.drain_events();

        let delta_error = loop_engine
            .handle_provider_event(ProviderEvent::TextDelta {
                text: "unexpected".to_owned(),
            })
            .expect_err("text delta before start must be rejected");
        assert_eq!(
            delta_error.message(),
            "收到文本增量前必须先收到 Provider start。"
        );

        let completion_error = loop_engine
            .handle_provider_event(ProviderEvent::Completed {
                timestamp: 2,
                stop_reason: AssistantStopReason::Stop,
            })
            .expect_err("completion before start must be rejected");
        assert_eq!(
            completion_error.message(),
            "收到 Provider 完成事件前必须先收到 Provider start。"
        );
        assert!(loop_engine.is_active());
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
                text: "partial".to_owned(),
            })
            .expect("text delta should be accepted");

        let events = loop_engine
            .handle_provider_event(ProviderEvent::Failed {
                timestamp: 3,
                message: "network failed".to_owned(),
            })
            .expect("failure should become a stable error transcript item");

        assert_eq!(
            events.iter().map(event_name).collect::<Vec<_>>(),
            vec!["message_end", "turn_end", "agent_end"]
        );
        assert!(!loop_engine.is_active());
        assert!(matches!(
            &loop_engine.messages()[1],
            TranscriptItem::Assistant(protocol::AssistantTranscriptItem::Error {
                error_message: Some(message),
                ..
            }) if message == "network failed"
        ));
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
            .handle_provider_event(ProviderEvent::Completed {
                timestamp: 3,
                stop_reason: AssistantStopReason::Stop,
            })
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
                .drain_steer()
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["user-2", "user-3"]
        );
        assert!(loop_engine.is_active());
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

    fn assistant_text(loop_engine: &AgentLoopEngine) -> Option<String> {
        let TranscriptItem::Assistant(assistant) = loop_engine.messages().last()? else {
            return None;
        };
        let content = match assistant {
            AssistantTranscriptItem::Complete { content, .. }
            | AssistantTranscriptItem::Error { content, .. }
            | AssistantTranscriptItem::Aborted { content, .. }
            | AssistantTranscriptItem::Streaming { content, .. } => content,
        };
        content.iter().find_map(|item| match item {
            AssistantContent::Text { text } => Some(text.clone()),
            AssistantContent::Thinking { .. } | AssistantContent::ToolCall { .. } => None,
        })
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
            AgentLoopEvent::ToolExecutionFinished { .. } => "tool_execution_end",
            AgentLoopEvent::ToolResultsReady { .. } => "tool_results_ready",
            AgentLoopEvent::TurnEnded { .. } => "turn_end",
            AgentLoopEvent::AgentEnded { .. } => "agent_end",
        }
    }
}
