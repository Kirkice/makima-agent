//! Provider 流到 AgentSession 的运行时适配器。
//!
//! 本模块位于 [`crate::provider_ipc`] 与 `AgentSession` 之间：它只处理共享 DTO、请求 ID
//! 和 Agent Loop 生命周期，不感知 socket、TUI 或具体 Provider SDK。异步传输实现应把已经
//! 解码的响应放入 [`ProviderStreamPort`]，由 SessionManager 在安全边界轮询。

use std::collections::VecDeque;

use agent_loop::{
    AgentLoopEngine, AgentLoopEvent as RustAgentLoopEvent, ProviderEvent, ToolRuntimePort,
};
use protocol::{
    ActiveTranscriptItem, AssistantTranscriptItem, FinishedTranscriptItem, ProviderHostResponse,
    ProviderRequest, ProviderStreamEvent, ServerEvent, SessionPhase, ToolRole, ToolTranscriptItem,
    TranscriptItem, TranscriptProgress,
};
use tool_runtime::ToolRuntime;

use crate::{
    agent_session::{AgentSession, JsonlSessionPersistence, session_events_from_rust_agent_loop},
    context_transform::{
        ContextTransformationPort, IdentityContextTransformer, ProviderContextInput,
        ProviderRequestPurpose,
    },
};

/// Provider transport 的非阻塞运行时端口。
///
/// `try_receive` 不得等待网络或子进程输出；返回 `Ok(None)` 表示当前没有完整响应。生产
/// Provider Host reader 应在独立线程或异步任务中解码 framed-CBOR 后实现该端口，避免 RPC
/// connection 的读循环被模型流阻塞。
pub trait ProviderStreamPort: Send {
    /// 提交一条新的不可变 Provider 请求。
    fn request(&mut self, request: ProviderRequest) -> Result<(), String>;

    /// 转发取消请求。未知或已完成的 ID 可安全忽略。
    fn abort(&mut self, request_id: &str) -> Result<(), String>;

    /// 取出下一批已经完整解码的 Host 响应。
    fn try_receive(&mut self) -> Result<Option<Vec<ProviderHostResponse>>, String>;
}

impl<P> ProviderStreamPort for Box<P>
where
    P: ProviderStreamPort + ?Sized,
{
    fn request(&mut self, request: ProviderRequest) -> Result<(), String> {
        (**self).request(request)
    }

    fn abort(&mut self, request_id: &str) -> Result<(), String> {
        (**self).abort(request_id)
    }

    fn try_receive(&mut self) -> Result<Option<Vec<ProviderHostResponse>>, String> {
        (**self).try_receive()
    }
}

/// 仅用于测试和嵌入式回放的内存 Provider transport。
#[derive(Default)]
pub struct QueuedProviderStreamPort {
    requests: Vec<ProviderRequest>,
    aborts: Vec<String>,
    responses: VecDeque<Vec<ProviderHostResponse>>,
}

impl QueuedProviderStreamPort {
    /// 使用预置的 response 批次构造 transport。
    pub fn with_responses(responses: impl IntoIterator<Item = Vec<ProviderHostResponse>>) -> Self {
        Self {
            requests: Vec::new(),
            aborts: Vec::new(),
            responses: responses.into_iter().collect(),
        }
    }

    /// 返回已发出的请求，便于 deterministic 测试断言。
    pub fn requests(&self) -> &[ProviderRequest] {
        &self.requests
    }

    /// 返回已转发的取消 ID，便于 deterministic 测试断言。
    pub fn aborts(&self) -> &[String] {
        &self.aborts
    }
}

impl ProviderStreamPort for QueuedProviderStreamPort {
    fn request(&mut self, request: ProviderRequest) -> Result<(), String> {
        self.requests.push(request);
        Ok(())
    }

    fn abort(&mut self, request_id: &str) -> Result<(), String> {
        self.aborts.push(request_id.to_owned());
        Ok(())
    }

    fn try_receive(&mut self) -> Result<Option<Vec<ProviderHostResponse>>, String> {
        Ok(self.responses.pop_front())
    }
}

/// 单个 AgentSession 的 Provider request/stream/abort 驱动。
///
/// 一个 active request 在收到 `complete` 前保持登记。`complete` 前必须有 `done` 或 `error`；
/// 否则此适配器将它归一化为失败 assistant 项，保证 AgentSession 不会永久停在 `Turn`。
pub struct ProviderStreamDriver<P, C = IdentityContextTransformer> {
    transport: P,
    context_transformer: C,
    active_request_id: Option<String>,
    active_request_terminal: bool,
    continuation_pending: bool,
    next_request_sequence: u64,
    system_prompt: String,
}

impl<P> ProviderStreamDriver<P, IdentityContextTransformer>
where
    P: ProviderStreamPort,
{
    /// 用 session 级系统提示和保持兼容的恒等上下文转换器创建驱动器。
    pub fn new(transport: P, system_prompt: impl Into<String>) -> Self {
        Self::with_context_transformer(transport, system_prompt, IdentityContextTransformer)
    }
}

impl<P, C> ProviderStreamDriver<P, C>
where
    P: ProviderStreamPort,
    C: ContextTransformationPort,
{
    /// 用显式上下文转换器创建驱动器。
    ///
    /// 转换器只投影 Provider 请求，不能修改 AgentSession 的权威 transcript；因此可安全用于
    /// 上下文裁剪、模型临时覆盖或 Provider 专属消息格式化。
    pub fn with_context_transformer(
        transport: P,
        system_prompt: impl Into<String>,
        context_transformer: C,
    ) -> Self {
        Self {
            transport,
            context_transformer,
            active_request_id: None,
            active_request_terminal: false,
            continuation_pending: false,
            next_request_sequence: 0,
            system_prompt: system_prompt.into(),
        }
    }

    /// 返回底层 transport，供进程关闭或测试检查。
    pub fn into_transport(self) -> P {
        self.transport
    }

    /// 从已由 AgentSession 接受的 prompt 创建并发送首个 ProviderRequest。
    pub fn start(
        &mut self,
        session: &mut AgentSession<AgentLoopEngine, JsonlSessionPersistence>,
        tool_runtime: &ToolRuntime,
        timestamp: u64,
    ) -> Result<Vec<ServerEvent>, String> {
        self.start_with_purpose(
            session,
            tool_runtime,
            timestamp,
            ProviderRequestPurpose::Initial,
        )
    }

    /// 在唯一入口将权威 Agent Loop transcript 投影为 Provider 请求。
    ///
    /// retry 和 continuation 都必须经过这里，确保其上下文转换规则与初始请求一致。转换
    /// 失败发生在 transport 前，既不会占用 request ID，也不会启动不可恢复的 Provider 流。
    fn start_with_purpose(
        &mut self,
        session: &mut AgentSession<AgentLoopEngine, JsonlSessionPersistence>,
        tool_runtime: &ToolRuntime,
        timestamp: u64,
        purpose: ProviderRequestPurpose,
    ) -> Result<Vec<ServerEvent>, String> {
        if self.active_request_id.is_some() {
            return Err("当前 Session 已有进行中的 Provider 请求。".to_owned());
        }

        // 仅在转换和 transport 都成功后提交序号。失败的上下文策略不能留下不可响应的
        // request ID，也不能让下一次 retry/continuation 的可回放 ID 出现空洞。
        let next_request_sequence = self.next_request_sequence.saturating_add(1);
        let snapshot = session.snapshot();
        let request_id = format!("{}-provider-{next_request_sequence}", snapshot.id);
        let tools = tool_runtime.definitions();
        let context = self
            .context_transformer
            .transform(ProviderContextInput {
                request_id: &request_id,
                purpose,
                timestamp,
                model: &snapshot.model,
                system_prompt: &self.system_prompt,
                messages: session.agent_loop().messages(),
                tools: &tools,
            })
            .map_err(|error| format!("Provider 请求上下文转换失败：{error}"))?;
        let request = ProviderRequest {
            request_id: request_id.clone(),
            model: context.model,
            system_prompt: context.system_prompt,
            messages: context.messages,
            tools: context.tools,
        };
        self.transport.request(request)?;
        self.next_request_sequence = next_request_sequence;
        self.active_request_id = Some(request_id);
        self.active_request_terminal = false;
        let loop_events = session.agent_loop_mut().drain_events();
        let mut events = Vec::new();
        self.apply_loop_events(session, loop_events, timestamp, &mut events)?;
        Ok(events)
    }

    /// 将 abort 转发给当前 Host 请求；AgentSession 的取消状态仍由调用方先更新。
    pub fn abort(&mut self) -> Result<(), String> {
        if let Some(request_id) = self.active_request_id.as_deref() {
            self.transport.abort(request_id)?;
        }
        Ok(())
    }

    /// 消费当前已就绪的 Provider Host 输出，并映射为 Session progress 与持久化事件。
    pub fn poll(
        &mut self,
        session: &mut AgentSession<AgentLoopEngine, JsonlSessionPersistence>,
        tool_runtime: &mut ToolRuntime,
        timestamp: u64,
    ) -> Result<Vec<ServerEvent>, String> {
        let mut events = Vec::new();
        // retry 的截止时间由 Session 计算，Runtime 只在没有未释放 request ID 时真正重启。
        // 这条门闩避免 Provider Host 的 `done` 与稍后的 `complete` 之间出现重叠请求。
        if self.active_request_id.is_none()
            && session
                .resume_retry_at(timestamp)
                .map_err(|error| error.message)?
        {
            events.extend(self.start_with_purpose(
                session,
                tool_runtime,
                timestamp,
                ProviderRequestPurpose::Retry,
            )?);
        }

        let tool_events = ToolRuntimePort::poll(tool_runtime, timestamp);
        if !tool_events.is_empty() {
            let loop_events = if session.agent_loop().is_abort_requested() {
                // Runtime 已为活动及排队调用生成取消终态；Agent Loop 只结算 assistant abort，
                // 不持久化取消后到达的工具结果，也绝不能再启动 continuation。
                self.continuation_pending = false;
                session.agent_loop_mut().settle_abort(timestamp)
            } else {
                session
                    .agent_loop_mut()
                    .handle_tool_runtime_events(tool_events)
                    .map_err(|error| error.message().to_owned())?
            };
            if loop_events
                .iter()
                .any(|event| matches!(event, RustAgentLoopEvent::ToolResultsReady { .. }))
            {
                self.continuation_pending = true;
            }
            self.apply_loop_events(session, loop_events, timestamp, &mut events)?;
            self.maybe_start_continuation(session, tool_runtime, timestamp, &mut events)?;
        }

        let responses = match self.transport.try_receive() {
            Ok(Some(responses)) => responses,
            Ok(None) => return Ok(events),
            Err(error) => {
                let message = format!("Provider Host request crashed before settlement: {error}");
                let Some(message_id) = self
                    .active_request_id
                    .as_deref()
                    .and_then(|_| session.agent_loop().active_assistant_id())
                    .map(str::to_owned)
                    .or_else(|| Some(format!("provider-{timestamp}")))
                else {
                    return Err(message);
                };

                // child 崩溃时不会再有可靠的 `complete`。先释放本地 request 门闩，再注入
                // 一个确定性的 Provider error，让 Agent Loop 生成唯一失败终态；绝不重放
                // 原请求。若用户已经 abort，则沿用 abort 的结算语义，避免把取消报告成崩溃。
                self.active_request_id = None;
                self.active_request_terminal = true;
                self.continuation_pending = false;
                if session.agent_loop().is_abort_requested() {
                    let loop_events = session.agent_loop_mut().settle_abort(timestamp);
                    self.apply_loop_events(session, loop_events, timestamp, &mut events)?;
                } else {
                    self.apply_provider_event(
                        session,
                        tool_runtime,
                        ProviderStreamEvent::Error {
                            message_id,
                            content: Vec::new(),
                            response_model: None,
                            usage: None,
                            timestamp,
                            message,
                        },
                        timestamp,
                        &mut events,
                    )?;
                }
                return Ok(events);
            }
        };
        for response in responses {
            match response {
                ProviderHostResponse::Event { request_id, event } => {
                    self.require_active_request(&request_id)?;
                    // 取消会由第一个后续 Provider 事件结算。Host 仍可能把已经在管道中的
                    // 终态事件送达；它们属于同一请求但不能再投递给已空闲的 Agent Loop。
                    if self.active_request_terminal {
                        continue;
                    }
                    self.apply_provider_event(
                        session,
                        tool_runtime,
                        event,
                        timestamp,
                        &mut events,
                    )?;
                    if session.snapshot().phase != SessionPhase::Turn {
                        self.active_request_terminal = true;
                    }
                }
                ProviderHostResponse::Complete { request_id } => {
                    self.require_active_request(&request_id)?;
                    if !self.active_request_terminal {
                        let message_id = session
                            .agent_loop()
                            .active_assistant_id()
                            .map_or_else(|| format!("provider-{timestamp}"), str::to_owned);
                        self.apply_provider_event(
                            session,
                            tool_runtime,
                            ProviderStreamEvent::Error {
                                message_id,
                                content: Vec::new(),
                                response_model: None,
                                usage: None,
                                timestamp,
                                message: "Provider Host 在发送终态事件前结束请求。".to_owned(),
                            },
                            timestamp,
                            &mut events,
                        )?;
                    }
                    self.active_request_id = None;
                    self.active_request_terminal = false;
                    self.maybe_start_continuation(session, tool_runtime, timestamp, &mut events)?;
                }
            }
        }
        Ok(events)
    }

    /// Provider complete 与工具批次结束可按任意顺序到达。只有两个条件同时满足时才消费
    /// pending 标记并启动一次 continuation，避免丢失续轮或重复请求。
    fn maybe_start_continuation(
        &mut self,
        session: &mut AgentSession<AgentLoopEngine, JsonlSessionPersistence>,
        tool_runtime: &ToolRuntime,
        timestamp: u64,
        output: &mut Vec<ServerEvent>,
    ) -> Result<(), String> {
        if self.continuation_pending && self.active_request_id.is_none() {
            self.continuation_pending = false;
            output.extend(self.start_with_purpose(
                session,
                tool_runtime,
                timestamp,
                ProviderRequestPurpose::Continuation,
            )?);
        }
        Ok(())
    }

    fn require_active_request(&self, request_id: &str) -> Result<(), String> {
        match self.active_request_id.as_deref() {
            Some(active) if active == request_id => Ok(()),
            Some(active) => Err(format!(
                "Provider Host 响应 request ID 不匹配：收到 {request_id}，当前为 {active}。"
            )),
            None => Err(format!(
                "Provider Host 为不存在的请求发送响应：{request_id}。"
            )),
        }
    }

    fn apply_provider_event(
        &mut self,
        session: &mut AgentSession<AgentLoopEngine, JsonlSessionPersistence>,
        tool_runtime: &mut ToolRuntime,
        stream_event: ProviderStreamEvent,
        timestamp: u64,
        output: &mut Vec<ServerEvent>,
    ) -> Result<(), String> {
        // thinking、text 和 tool call 统一经过 Agent Loop 的 contentIndex 状态机。Runtime 不再
        // 特判可选内容类型，避免真实 Host 与共享 fixture 走不同路径。
        let provider_event =
            ProviderEvent::try_from(stream_event).map_err(|error| error.message().to_owned())?;
        let retry_error = match &provider_event {
            ProviderEvent::Failed { message, .. } => Some(message.clone()),
            _ => None,
        };
        // `done`/`error` 已经完成当前 Host request，即使 Agent Loop 因 follow-up 保持 turn
        // 活跃也是如此。没有这条独立门闩，紧随其后的 Complete 会被误判为“缺少终态”，并把
        // 错误 assistant 写入下一次 continuation 的上下文。
        let provider_request_terminal = matches!(
            provider_event,
            ProviderEvent::Completed { .. } | ProviderEvent::Failed { .. }
        );
        let loop_events = session
            .agent_loop_mut()
            .handle_provider_event_with_tools(provider_event, tool_runtime)
            .map_err(|error| error.message().to_owned())?;
        if provider_request_terminal || session.agent_loop().is_waiting_for_tools() {
            // Provider 的 done/error 已到达，或 done 后开始等待工具批次。两者都代表本次
            // Host request 已经拥有明确终态；后续 Complete 只能释放 request ID。
            self.active_request_terminal = true;
        }
        if loop_events
            .iter()
            .any(|event| matches!(event, RustAgentLoopEvent::ToolResultsReady { .. }))
        {
            self.continuation_pending = true;
            self.active_request_terminal = true;
        }
        // 普通失败会让 Agent Loop 同时发出 error assistant 与 AgentEnded。自动 retry 需要先
        // 持久化 error assistant，再移除它的 Provider 工作上下文，并且不能把 Session 提前
        // settle 到 idle；因此暂存唯一的结束事件，待策略拒绝 retry 时才交回 Session。
        if let Some(error_message) = retry_error {
            let mut settlement_events = Vec::new();
            let non_settlement_events = loop_events
                .into_iter()
                .filter_map(|event| {
                    if matches!(event, RustAgentLoopEvent::AgentEnded { .. }) {
                        settlement_events.push(event);
                        None
                    } else {
                        Some(event)
                    }
                })
                .collect();
            self.apply_loop_events(session, non_settlement_events, timestamp, output)?;
            if session
                .schedule_retry_at(&error_message, timestamp)
                .map_err(|error| error.message)?
                .is_some()
            {
                return Ok(());
            }
            return self.apply_loop_events(session, settlement_events, timestamp, output);
        }

        self.apply_loop_events(session, loop_events, timestamp, output)
    }

    fn apply_loop_events(
        &mut self,
        session: &mut AgentSession<AgentLoopEngine, JsonlSessionPersistence>,
        loop_events: Vec<RustAgentLoopEvent>,
        timestamp: u64,
        output: &mut Vec<ServerEvent>,
    ) -> Result<(), String> {
        // 工具结果与用户队列都通过同一个 continuation 门闩衔接下一次请求。这里仅记录
        // 意图，不立即请求：当前 Provider request 必须先收到 Complete 并释放 request ID，
        // 否则会造成同一 Host 流上的重叠请求。
        if loop_events.iter().any(|event| {
            matches!(
                event,
                RustAgentLoopEvent::ProviderContinuationRequested
                    | RustAgentLoopEvent::ToolResultsReady { .. }
            )
        }) {
            self.continuation_pending = true;
        }

        let session_id = session.snapshot().id;
        for event in &loop_events {
            if let Some(progress) = progress_for_loop_event(event, timestamp) {
                output.push(ServerEvent::SessionProgress {
                    session_id: session_id.clone(),
                    progress,
                });
            }
        }
        for event in session_events_from_rust_agent_loop(loop_events) {
            session
                .handle_agent_loop_event_at(event, timestamp)
                .map_err(|error| error.message)?;
        }
        let _ = session.drain_events();
        Ok(())
    }
}

fn progress_for_loop_event(
    event: &RustAgentLoopEvent,
    timestamp: u64,
) -> Option<TranscriptProgress> {
    match event {
        RustAgentLoopEvent::TranscriptItemStarted(item) => {
            Some(TranscriptProgress::ItemStarted { item: item.clone() })
        }
        RustAgentLoopEvent::TranscriptItemUpdated(item) => Some(TranscriptProgress::ItemUpdated {
            item: ActiveTranscriptItem::Assistant(item.clone()),
        }),
        RustAgentLoopEvent::TranscriptItemFinished(item) => {
            finished_item(item.clone()).map(|item| TranscriptProgress::ItemFinished { item })
        }
        RustAgentLoopEvent::ToolExecutionStarted { tool_call } => {
            Some(TranscriptProgress::ItemStarted {
                item: TranscriptItem::Tool(ToolTranscriptItem::Running {
                    id: format!("tool-{}", tool_call.tool_call_id),
                    role: ToolRole::Tool,
                    tool_call_id: tool_call.tool_call_id.clone(),
                    tool_name: tool_call.tool_name.clone(),
                    input: tool_call.input.clone(),
                    content: Vec::new(),
                    details: None,
                    usage: None,
                    timestamp,
                    is_error: false,
                }),
            })
        }
        RustAgentLoopEvent::ToolExecutionUpdated {
            tool_call,
            content,
            details,
        } => Some(TranscriptProgress::ItemUpdated {
            item: ActiveTranscriptItem::Tool(ToolTranscriptItem::Running {
                id: format!("tool-{}", tool_call.tool_call_id),
                role: ToolRole::Tool,
                tool_call_id: tool_call.tool_call_id.clone(),
                tool_name: tool_call.tool_name.clone(),
                input: tool_call.input.clone(),
                content: content.clone(),
                details: details.clone(),
                usage: None,
                timestamp,
                is_error: false,
            }),
        }),
        RustAgentLoopEvent::AgentStarted
        | RustAgentLoopEvent::TurnStarted
        | RustAgentLoopEvent::ToolExecutionFinished { .. }
        | RustAgentLoopEvent::ToolResultsReady { .. }
        | RustAgentLoopEvent::ProviderContinuationRequested
        | RustAgentLoopEvent::SteerConsumed
        | RustAgentLoopEvent::FollowUpConsumed
        | RustAgentLoopEvent::TurnEnded { .. }
        | RustAgentLoopEvent::AgentEnded { .. } => None,
    }
}

fn finished_item(item: TranscriptItem) -> Option<FinishedTranscriptItem> {
    match item {
        TranscriptItem::Assistant(AssistantTranscriptItem::Complete {
            id,
            role,
            content,
            model,
            response_model,
            usage,
            timestamp,
            stop_reason,
        }) => Some(FinishedTranscriptItem::AssistantComplete(
            protocol::CompleteAssistantTranscriptItem::Complete {
                id,
                role,
                content,
                model,
                response_model,
                usage,
                timestamp,
                stop_reason,
            },
        )),
        TranscriptItem::Assistant(AssistantTranscriptItem::Error {
            id,
            role,
            content,
            model,
            response_model,
            usage,
            timestamp,
            stop_reason,
            error_message,
        }) => Some(FinishedTranscriptItem::AssistantError(
            protocol::ErrorAssistantTranscriptItem::Error {
                id,
                role,
                content,
                model,
                response_model,
                usage,
                timestamp,
                stop_reason,
                error_message,
            },
        )),
        TranscriptItem::Assistant(AssistantTranscriptItem::Aborted {
            id,
            role,
            content,
            model,
            response_model,
            usage,
            timestamp,
            stop_reason,
            error_message,
        }) => Some(FinishedTranscriptItem::AssistantAborted(
            protocol::AbortedAssistantTranscriptItem::Aborted {
                id,
                role,
                content,
                model,
                response_model,
                usage,
                timestamp,
                stop_reason,
                error_message,
            },
        )),
        TranscriptItem::Tool(ToolTranscriptItem::Complete {
            id,
            role,
            tool_call_id,
            tool_name,
            input,
            content,
            details,
            usage,
            timestamp,
            is_error,
        }) => Some(FinishedTranscriptItem::ToolComplete(
            protocol::CompleteToolTranscriptItem::Complete {
                id,
                role,
                tool_call_id,
                tool_name,
                input,
                content,
                details,
                usage,
                timestamp,
                is_error,
            },
        )),
        TranscriptItem::Tool(ToolTranscriptItem::Error {
            id,
            role,
            tool_call_id,
            tool_name,
            input,
            content,
            details,
            usage,
            timestamp,
            is_error,
        }) => Some(FinishedTranscriptItem::ToolError(
            protocol::ErrorToolTranscriptItem::Error {
                id,
                role,
                tool_call_id,
                tool_name,
                input,
                content,
                details,
                usage,
                timestamp,
                is_error,
            },
        )),
        TranscriptItem::User(_)
        | TranscriptItem::Assistant(AssistantTranscriptItem::Streaming { .. })
        | TranscriptItem::Tool(ToolTranscriptItem::Running { .. }) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        ops::{Deref, DerefMut},
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use protocol::{
        AssistantContent, AssistantStopReason, AssistantTranscriptItem, Command,
        FinishedTranscriptItem, ModelRef, ProviderHostResponse, ProviderStreamEvent, SessionPhase,
        ThinkingLevel, TranscriptItem, Usage, UsageCost,
    };
    use session::JsonlSessionStore;
    use tool_runtime::{Tool, ToolExecutionContext, ToolExecutionError, ToolOutput, ToolRuntime};

    use super::{
        AgentLoopEngine, AgentSession, JsonlSessionPersistence, ProviderStreamDriver,
        ProviderStreamPort, QueuedProviderStreamPort,
    };
    use crate::{
        agent_session::{AgentSessionConfig, RetryPolicy},
        context_transform::{ContextTransformationPort, ProviderContext, ProviderContextInput},
        provider_ipc::ProviderHostStreamPort,
    };

    static STORE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestSession {
        session: AgentSession<AgentLoopEngine, JsonlSessionPersistence>,
        path: PathBuf,
    }

    impl Deref for TestSession {
        type Target = AgentSession<AgentLoopEngine, JsonlSessionPersistence>;

        fn deref(&self) -> &Self::Target {
            &self.session
        }
    }

    impl DerefMut for TestSession {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.session
        }
    }

    impl Drop for TestSession {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn test_session() -> TestSession {
        test_session_with_retry_policy(RetryPolicy::default())
    }

    fn test_session_with_retry_policy(retry_policy: RetryPolicy) -> TestSession {
        let sequence = STORE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = temporary_store_path(sequence);
        let store = JsonlSessionStore::create(path.clone(), "session-1", "C:/workspace")
            .expect("test session store should be created");
        let config = AgentSessionConfig {
            id: "session-1".to_owned(),
            name: None,
            cwd: "C:/workspace".to_owned(),
            model: ModelRef {
                provider: "test".to_owned(),
                id: "model-a".to_owned(),
            },
            thinking_level: ThinkingLevel::Medium,
            created_at: 100,
            retry_policy,
        };
        TestSession {
            session: AgentSession::new(
                config.clone(),
                AgentLoopEngine::new(config.model),
                JsonlSessionPersistence::new(store),
            ),
            path,
        }
    }

    fn temporary_store_path(sequence: u64) -> PathBuf {
        std::env::temp_dir().join(format!(
            "provider-runtime-driver-{}-{sequence}.jsonl",
            std::process::id()
        ))
    }

    fn prompt(session: &mut TestSession, timestamp: u64) {
        session
            .execute_at(
                Command::Prompt {
                    session_id: "session-1".to_owned(),
                    text: "hello".to_owned(),
                },
                timestamp,
            )
            .expect("prompt should start the AgentSession turn");
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

    fn done(
        message_id: &str,
        content: Vec<AssistantContent>,
        timestamp: u64,
        stop_reason: AssistantStopReason,
    ) -> ProviderStreamEvent {
        ProviderStreamEvent::Done {
            message_id: message_id.to_owned(),
            content,
            response_model: Some("resolved-model".to_owned()),
            usage: usage(),
            timestamp,
            stop_reason,
        }
    }

    fn event(event: ProviderStreamEvent) -> ProviderHostResponse {
        event_for("session-1-provider-1", event)
    }

    fn event_for(request_id: &str, event: ProviderStreamEvent) -> ProviderHostResponse {
        ProviderHostResponse::Event {
            request_id: request_id.to_owned(),
            event,
        }
    }

    struct ControlledTool {
        release: Arc<AtomicBool>,
    }

    /// 保留权威上下文，只在系统提示中记录请求原因，用于验证三条请求路径都经过同一端口。
    #[derive(Default)]
    struct PurposeAnnotatingTransformer;

    impl ContextTransformationPort for PurposeAnnotatingTransformer {
        fn transform(
            &mut self,
            input: ProviderContextInput<'_>,
        ) -> Result<ProviderContext, String> {
            Ok(ProviderContext {
                model: input.model.clone(),
                system_prompt: format!("{}:{:?}", input.system_prompt, input.purpose),
                messages: input.messages.to_vec(),
                tools: input.tools.to_vec(),
            })
        }
    }

    struct FailingContextTransformer;

    impl ContextTransformationPort for FailingContextTransformer {
        fn transform(
            &mut self,
            _input: ProviderContextInput<'_>,
        ) -> Result<ProviderContext, String> {
            Err("拒绝发送测试上下文".to_owned())
        }
    }

    /// 用于验证运行时边界的逆序完成工具：第二个调用可先完成，首个调用必须显式放行。
    /// 这让测试可以证明 continuation 请求上下文不会受 worker 的实际完成顺序影响。
    struct ReverseCompletionTool {
        release_first: Arc<AtomicBool>,
        second_started: Arc<AtomicBool>,
    }

    impl Tool for ReverseCompletionTool {
        fn definition(&self) -> protocol::ToolDefinition {
            protocol::ToolDefinition {
                name: "reverse-completion".to_owned(),
                description: "使第二个调用先完成的测试工具".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                execution_mode: protocol::ToolExecutionMode::Parallel,
            }
        }

        fn execute(
            &self,
            call: &protocol::ToolCall,
            _context: &ToolExecutionContext,
        ) -> Result<ToolOutput, ToolExecutionError> {
            if call.tool_call_id == "call-1" {
                while !self.release_first.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(1));
                }
            } else {
                self.second_started.store(true, Ordering::Release);
            }
            Ok(ToolOutput::text(format!(
                "result for {}",
                call.tool_call_id
            )))
        }
    }

    impl Tool for ControlledTool {
        fn definition(&self) -> protocol::ToolDefinition {
            protocol::ToolDefinition {
                name: "controlled".to_owned(),
                description: "可控制完成时机的测试工具".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                execution_mode: protocol::ToolExecutionMode::Parallel,
            }
        }

        fn execute(
            &self,
            _call: &protocol::ToolCall,
            context: &ToolExecutionContext,
        ) -> Result<ToolOutput, ToolExecutionError> {
            context.report_update(ToolOutput::text("working"));
            while !self.release.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }
            context.report_update(ToolOutput::text("late update"));
            Ok(ToolOutput::text("late result"))
        }
    }

    struct CrashingProviderStreamPort {
        requests: Vec<protocol::ProviderRequest>,
        aborts: Vec<String>,
        error: String,
    }

    impl CrashingProviderStreamPort {
        fn new(error: impl Into<String>) -> Self {
            Self {
                requests: Vec::new(),
                aborts: Vec::new(),
                error: error.into(),
            }
        }
    }

    impl ProviderStreamPort for CrashingProviderStreamPort {
        fn request(&mut self, request: protocol::ProviderRequest) -> Result<(), String> {
            self.requests.push(request);
            Ok(())
        }

        fn abort(&mut self, request_id: &str) -> Result<(), String> {
            self.aborts.push(request_id.to_owned());
            Ok(())
        }

        fn try_receive(&mut self) -> Result<Option<Vec<ProviderHostResponse>>, String> {
            Err(self.error.clone())
        }
    }

    #[test]
    fn provider_host_crash_settles_active_request_without_replay_or_continuation() {
        let mut driver = ProviderStreamDriver::new(
            CrashingProviderStreamPort::new("child exited with status 1"),
            "system prompt",
        );
        let mut session = test_session();
        let mut tool_runtime = ToolRuntime::new();
        prompt(&mut session, 100);
        driver
            .start(&mut session, &tool_runtime, 100)
            .expect("request should start");

        let events = driver
            .poll(&mut session, &mut tool_runtime, 101)
            .expect("Host crash should be converted into a settled provider error");

        assert!(driver.active_request_id.is_none());
        assert!(driver.active_request_terminal);
        assert!(!driver.continuation_pending);
        assert_eq!(driver.transport.requests.len(), 1);
        assert_eq!(session.snapshot().phase, SessionPhase::Idle);
        assert!(events.iter().any(|event| matches!(
            event,
            protocol::ServerEvent::SessionProgress {
                progress: protocol::TranscriptProgress::ItemFinished {
                    item: FinishedTranscriptItem::AssistantError(_),
                },
                ..
            }
        )));
        assert!(matches!(
            session.snapshot().transcript.last(),
            Some(TranscriptItem::Assistant(AssistantTranscriptItem::Error {
                error_message: Some(message),
                ..
            })) if message.contains("Provider Host request crashed before settlement")
        ));

        // 再次轮询只能观察到空闲状态，不能把已经失败的活动请求重放成第二个请求。
        driver
            .poll(&mut session, &mut tool_runtime, 102)
            .expect("settled crash should remain stable");
        assert_eq!(driver.transport.requests.len(), 1);
    }

    #[test]
    fn provider_host_crash_after_abort_preserves_aborted_settlement() {
        let mut driver = ProviderStreamDriver::new(
            CrashingProviderStreamPort::new("child exited while aborting"),
            "system prompt",
        );
        let mut session = test_session();
        let mut tool_runtime = ToolRuntime::new();
        prompt(&mut session, 100);
        driver
            .start(&mut session, &tool_runtime, 100)
            .expect("request should start");
        session
            .execute_at(
                Command::Abort {
                    session_id: "session-1".to_owned(),
                },
                101,
            )
            .expect("abort should be accepted");
        driver.abort().expect("abort should be forwarded");

        driver
            .poll(&mut session, &mut tool_runtime, 102)
            .expect("Host crash must use abort settlement");

        assert!(driver.active_request_id.is_none());
        assert!(!driver.continuation_pending);
        assert_eq!(session.snapshot().phase, SessionPhase::Idle);
        assert!(matches!(
            session.snapshot().transcript.last(),
            Some(TranscriptItem::Assistant(AssistantTranscriptItem::Aborted { .. }))
        ));
        assert!(!session.snapshot().transcript.iter().any(|item| matches!(
            item,
            TranscriptItem::Assistant(AssistantTranscriptItem::Error { .. })
        )));
    }

    #[test]
    fn drives_text_stream_to_progress_persistence_and_settlement() {
        let transport = QueuedProviderStreamPort::with_responses([vec![
            event(ProviderStreamEvent::Start {
                message_id: "assistant-1".to_owned(),
                timestamp: 101,
            }),
            event(ProviderStreamEvent::TextDelta {
                content_index: 0,
                delta: "world".to_owned(),
            }),
            event(done(
                "assistant-1",
                vec![AssistantContent::Text {
                    text: "world".to_owned(),
                }],
                102,
                AssistantStopReason::Stop,
            )),
            ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            },
        ]]);
        let mut driver = ProviderStreamDriver::new(transport, "system prompt");
        let mut session = test_session();
        prompt(&mut session, 100);

        let initial_progress = driver
            .start(&mut session, &ToolRuntime::new(), 100)
            .expect("request should start");
        assert!(initial_progress.iter().any(|event| matches!(
            event,
            protocol::ServerEvent::SessionProgress {
                progress: protocol::TranscriptProgress::ItemStarted { .. },
                ..
            }
        )));

        let mut tool_runtime = ToolRuntime::new();
        let progress = driver
            .poll(&mut session, &mut tool_runtime, 102)
            .expect("stream should apply");
        let transport = driver.into_transport();
        assert_eq!(transport.requests().len(), 1);
        assert_eq!(transport.requests()[0].system_prompt, "system prompt");
        assert_eq!(transport.requests()[0].messages.len(), 1);
        assert!(progress.iter().any(|event| matches!(
            event,
            protocol::ServerEvent::SessionProgress {
                progress: protocol::TranscriptProgress::ItemUpdated { .. },
                ..
            }
        )));
        assert_eq!(session.snapshot().phase, SessionPhase::Idle);
        assert!(matches!(
            session.snapshot().transcript.last(),
            Some(TranscriptItem::Assistant(AssistantTranscriptItem::Complete {
                content,
                ..
            })) if matches!(content.as_slice(), [AssistantContent::Text { text }] if text == "world")
        ));
    }

    #[test]
    fn rejects_context_transformation_before_sending_or_allocating_a_request() {
        let transport = QueuedProviderStreamPort::default();
        let mut driver = ProviderStreamDriver::with_context_transformer(
            transport,
            "system prompt",
            FailingContextTransformer,
        );
        let mut session = test_session();
        prompt(&mut session, 100);

        let error = driver
            .start(&mut session, &ToolRuntime::new(), 100)
            .expect_err("failing transformer should reject the request");
        assert!(error.contains("Provider 请求上下文转换失败：拒绝发送测试上下文"));
        assert!(driver.transport.requests().is_empty());
        assert!(driver.active_request_id.is_none());
        assert_eq!(driver.next_request_sequence, 0);
    }

    #[test]
    fn retry_waits_for_complete_then_reuses_error_free_provider_context() {
        let transport = QueuedProviderStreamPort::with_responses([
            vec![event(ProviderStreamEvent::Error {
                message_id: "assistant-1".to_owned(),
                content: Vec::new(),
                response_model: None,
                usage: None,
                timestamp: 101,
                message: "Provider overloaded".to_owned(),
            })],
            vec![ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            }],
            vec![
                event_for(
                    "session-1-provider-2",
                    ProviderStreamEvent::Start {
                        message_id: "assistant-2".to_owned(),
                        timestamp: 200,
                    },
                ),
                event_for(
                    "session-1-provider-2",
                    done(
                        "assistant-2",
                        vec![AssistantContent::Text {
                            text: "recovered".to_owned(),
                        }],
                        201,
                        AssistantStopReason::Stop,
                    ),
                ),
                ProviderHostResponse::Complete {
                    request_id: "session-1-provider-2".to_owned(),
                },
            ],
        ]);
        let mut driver = ProviderStreamDriver::with_context_transformer(
            transport,
            "system prompt",
            PurposeAnnotatingTransformer,
        );
        let mut session = test_session_with_retry_policy(RetryPolicy {
            enabled: true,
            max_retries: 1,
            base_delay_ms: 50,
        });
        let mut tool_runtime = ToolRuntime::new();
        prompt(&mut session, 100);
        driver
            .start(&mut session, &tool_runtime, 100)
            .expect("initial request should start");

        driver
            .poll(&mut session, &mut tool_runtime, 101)
            .expect("transient error should be persisted and scheduled");
        assert_eq!(session.snapshot().phase, SessionPhase::Retry);
        assert_eq!(session.retry_attempt(), 1);
        assert_eq!(
            session.retry_schedule().map(|schedule| schedule.retry_at),
            Some(151)
        );
        driver
            .poll(&mut session, &mut tool_runtime, 151)
            .expect("complete must release the first request before retry starts");
        assert_eq!(driver.transport.requests().len(), 1);
        assert_eq!(session.snapshot().phase, SessionPhase::Retry);

        driver
            .poll(&mut session, &mut tool_runtime, 151)
            .expect("released request should allow retry to start");
        let transport = driver.into_transport();
        assert_eq!(transport.requests().len(), 2);
        assert_eq!(
            transport.requests()[0].system_prompt,
            "system prompt:Initial"
        );
        assert_eq!(transport.requests()[1].system_prompt, "system prompt:Retry");
        assert_eq!(transport.requests()[1].messages.len(), 1);
        assert!(matches!(
            transport.requests()[1].messages.as_slice(),
            [TranscriptItem::User(_)]
        ));
        assert_eq!(session.snapshot().phase, SessionPhase::Idle);
        assert_eq!(session.retry_attempt(), 0);
        assert!(matches!(
            session.snapshot().transcript.as_slice(),
            [
                TranscriptItem::User(_),
                TranscriptItem::Assistant(AssistantTranscriptItem::Error { .. }),
                TranscriptItem::Assistant(AssistantTranscriptItem::Complete { .. })
            ]
        ));
    }

    #[test]
    fn abort_during_retry_backoff_prevents_a_new_provider_request() {
        let transport = QueuedProviderStreamPort::with_responses([
            vec![event(ProviderStreamEvent::Error {
                message_id: "assistant-1".to_owned(),
                content: Vec::new(),
                response_model: None,
                usage: None,
                timestamp: 101,
                message: "network timeout".to_owned(),
            })],
            vec![ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            }],
        ]);
        let mut driver = ProviderStreamDriver::new(transport, "system prompt");
        let mut session = test_session_with_retry_policy(RetryPolicy {
            enabled: true,
            max_retries: 1,
            base_delay_ms: 50,
        });
        let mut tool_runtime = ToolRuntime::new();
        prompt(&mut session, 100);
        driver
            .start(&mut session, &tool_runtime, 100)
            .expect("initial request should start");
        driver
            .poll(&mut session, &mut tool_runtime, 101)
            .expect("error should schedule retry");
        session
            .execute_at(
                Command::Abort {
                    session_id: "session-1".to_owned(),
                },
                102,
            )
            .expect("abort should cancel the backoff");
        driver
            .poll(&mut session, &mut tool_runtime, 200)
            .expect("complete after abort should only release the old request");
        let transport = driver.into_transport();
        assert_eq!(transport.requests().len(), 1);
        assert_eq!(session.snapshot().phase, SessionPhase::Idle);
    }

    #[test]
    fn follow_up_starts_the_next_provider_request_only_after_the_current_complete() {
        let transport = QueuedProviderStreamPort::with_responses([vec![
            event(ProviderStreamEvent::Start {
                message_id: "assistant-1".to_owned(),
                timestamp: 101,
            }),
            event(done(
                "assistant-1",
                vec![AssistantContent::Text {
                    text: "first answer".to_owned(),
                }],
                102,
                AssistantStopReason::Stop,
            )),
            ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            },
        ]]);
        let mut driver = ProviderStreamDriver::new(transport, "system prompt");
        let mut session = test_session();
        let mut tool_runtime = ToolRuntime::new();
        prompt(&mut session, 100);
        driver
            .start(&mut session, &tool_runtime, 100)
            .expect("initial provider request should start");
        session
            .execute_at(
                Command::FollowUp {
                    session_id: "session-1".to_owned(),
                    text: "please continue".to_owned(),
                },
                101,
            )
            .expect("active turn should accept follow-up");

        driver
            .poll(&mut session, &mut tool_runtime, 102)
            .expect("completion should schedule the follow-up continuation");

        let transport = driver.into_transport();
        assert_eq!(transport.requests().len(), 2);
        assert_eq!(transport.requests()[1].request_id, "session-1-provider-2");
        assert!(matches!(
            transport.requests()[1].messages.last(),
            Some(TranscriptItem::User(user))
                if matches!(user.content.as_slice(), [protocol::TextOrImageContent::Text { text }] if text == "please continue")
        ));
        assert_eq!(session.snapshot().queued_follow_up_count, 0);
        assert_eq!(session.snapshot().phase, SessionPhase::Turn);
    }

    #[test]
    fn executes_tool_call_and_starts_a_provider_continuation_after_complete() {
        use std::fs;

        use tool_runtime::ReadTool;

        let workspace = std::env::temp_dir().join(format!(
            "provider-runtime-tool-{}",
            STORE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&workspace).unwrap();
        fs::write(workspace.join("hello.txt"), "hello from tool").unwrap();
        let transport = QueuedProviderStreamPort::with_responses([vec![
            event(ProviderStreamEvent::Start {
                message_id: "assistant-tool".to_owned(),
                timestamp: 101,
            }),
            event(ProviderStreamEvent::ToolCallDelta {
                content_index: 0,
                delta: "{\"path\":\"hello.txt\"}".to_owned(),
            }),
            event(ProviderStreamEvent::ToolCallEnd {
                content_index: 0,
                tool_call: protocol::ToolCall {
                    tool_call_id: "call-1".to_owned(),
                    tool_name: "read".to_owned(),
                    input: serde_json::json!({ "path": "hello.txt" }),
                },
            }),
            event(done(
                "assistant-tool",
                vec![AssistantContent::ToolCall {
                    tool_call_id: "call-1".to_owned(),
                    tool_name: "read".to_owned(),
                    input: serde_json::json!({ "path": "hello.txt" }),
                }],
                102,
                AssistantStopReason::ToolUse,
            )),
            ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            },
        ]]);
        let mut tool_runtime = ToolRuntime::new();
        tool_runtime
            .register(ReadTool::new(&workspace).unwrap())
            .unwrap();
        let mut driver = ProviderStreamDriver::with_context_transformer(
            transport,
            "system prompt",
            PurposeAnnotatingTransformer,
        );
        let mut session = test_session();
        prompt(&mut session, 100);
        driver.start(&mut session, &tool_runtime, 100).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while driver.transport.requests().len() < 2 {
            driver.poll(&mut session, &mut tool_runtime, 102).unwrap();
            assert!(Instant::now() < deadline, "tool continuation should start");
            thread::yield_now();
        }

        let transport = driver.into_transport();
        assert_eq!(transport.requests().len(), 2);
        assert_eq!(transport.requests()[0].tools[0].name, "read");
        assert_eq!(transport.requests()[1].request_id, "session-1-provider-2");
        assert_eq!(
            transport.requests()[0].system_prompt,
            "system prompt:Initial"
        );
        assert_eq!(
            transport.requests()[1].system_prompt,
            "system prompt:Continuation"
        );
        assert!(matches!(
            transport.requests()[1].messages.last(),
            Some(TranscriptItem::Tool(_))
        ));
        assert!(matches!(
            session.snapshot().transcript.last(),
            Some(TranscriptItem::Tool(protocol::ToolTranscriptItem::Complete { content, .. }))
                if matches!(content.as_slice(), [protocol::TextOrImageContent::Text { text }] if text == "hello from tool")
        ));
        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn continuation_context_keeps_parallel_tool_results_in_provider_source_order() {
        let transport = QueuedProviderStreamPort::with_responses([vec![
            event(ProviderStreamEvent::Start {
                message_id: "assistant-tools".to_owned(),
                timestamp: 101,
            }),
            event(ProviderStreamEvent::ToolCallEnd {
                content_index: 0,
                tool_call: protocol::ToolCall {
                    tool_call_id: "call-1".to_owned(),
                    tool_name: "reverse-completion".to_owned(),
                    input: serde_json::json!({}),
                },
            }),
            event(ProviderStreamEvent::ToolCallEnd {
                content_index: 1,
                tool_call: protocol::ToolCall {
                    tool_call_id: "call-2".to_owned(),
                    tool_name: "reverse-completion".to_owned(),
                    input: serde_json::json!({}),
                },
            }),
            event(done(
                "assistant-tools",
                vec![
                    AssistantContent::ToolCall {
                        tool_call_id: "call-1".to_owned(),
                        tool_name: "reverse-completion".to_owned(),
                        input: serde_json::json!({}),
                    },
                    AssistantContent::ToolCall {
                        tool_call_id: "call-2".to_owned(),
                        tool_name: "reverse-completion".to_owned(),
                        input: serde_json::json!({}),
                    },
                ],
                102,
                AssistantStopReason::ToolUse,
            )),
            ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            },
        ]]);
        let release_first = Arc::new(AtomicBool::new(false));
        let second_started = Arc::new(AtomicBool::new(false));
        let mut tool_runtime = ToolRuntime::new().with_timeout(None);
        tool_runtime
            .register(ReverseCompletionTool {
                release_first: Arc::clone(&release_first),
                second_started: Arc::clone(&second_started),
            })
            .expect("test tool should register");
        let mut driver = ProviderStreamDriver::new(transport, "system prompt");
        let mut session = test_session();
        prompt(&mut session, 100);
        driver
            .start(&mut session, &tool_runtime, 100)
            .expect("initial request should start");
        driver
            .poll(&mut session, &mut tool_runtime, 102)
            .expect("tool calls and provider completion should apply");

        let deadline = Instant::now() + Duration::from_secs(1);
        while !second_started.load(Ordering::Acquire) {
            driver
                .poll(&mut session, &mut tool_runtime, 103)
                .expect("parallel tools should start");
            assert!(
                Instant::now() < deadline,
                "second parallel tool should start"
            );
            thread::yield_now();
        }
        // 此时 call-2 已物理完成或可完成，但稳定排序必须等待 source-order 的 call-1。
        release_first.store(true, Ordering::Release);
        while driver.transport.requests().len() < 2 {
            driver
                .poll(&mut session, &mut tool_runtime, 104)
                .expect("tool completion should start one continuation");
            assert!(
                Instant::now() < deadline,
                "tool continuation should start after stable result commit"
            );
            thread::yield_now();
        }

        let transport = driver.into_transport();
        assert_eq!(transport.requests().len(), 2);
        assert_eq!(transport.requests()[1].request_id, "session-1-provider-2");
        let continuation_tools = transport.requests()[1]
            .messages
            .iter()
            .filter_map(|item| match item {
                TranscriptItem::Tool(tool) => match tool {
                    protocol::ToolTranscriptItem::Complete { tool_call_id, .. }
                    | protocol::ToolTranscriptItem::Error { tool_call_id, .. } => {
                        Some(tool_call_id.as_str())
                    }
                    protocol::ToolTranscriptItem::Running { .. } => None,
                },
                TranscriptItem::User(_) | TranscriptItem::Assistant(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(continuation_tools, ["call-1", "call-2"]);
        assert!(matches!(
            transport.requests()[1].messages.as_slice(),
            [
                TranscriptItem::User(_),
                TranscriptItem::Assistant(AssistantTranscriptItem::Complete { content, .. }),
                TranscriptItem::Tool(protocol::ToolTranscriptItem::Complete { .. }),
                TranscriptItem::Tool(protocol::ToolTranscriptItem::Complete { .. }),
            ] if matches!(content.as_slice(), [
                AssistantContent::ToolCall { tool_call_id: first, .. },
                AssistantContent::ToolCall { tool_call_id: second, .. },
            ] if first == "call-1" && second == "call-2")
        ));
    }

    #[test]
    fn node_provider_host_executes_a_complete_rust_tool_round_over_framed_cbor() {
        use tool_runtime::ReadTool;

        let sequence = STORE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let workspace = std::env::temp_dir().join(format!(
            "provider-host-tool-round-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&workspace).expect("temporary workspace should be created");
        fs::write(
            workspace.join("first.txt"),
            "hello from the first Rust read tool call",
        )
        .expect("first read fixture should be written");
        fs::write(
            workspace.join("second.txt"),
            "hello from the second Rust read tool call",
        )
        .expect("second read fixture should be written");

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../packages/provider-host/test/e2e-tool-round-host.ts");
        let transport = ProviderHostStreamPort::spawn(
            "node",
            ["--experimental-strip-types".as_ref(), fixture.as_os_str()],
        )
        .expect("real Node Provider Host should start");
        let mut tool_runtime = ToolRuntime::new();
        tool_runtime
            .register(ReadTool::new(&workspace).expect("read tool should initialize"))
            .expect("read tool should register");
        let mut driver = ProviderStreamDriver::new(transport, "system prompt");
        let mut session = test_session();
        prompt(&mut session, 100);
        let mut progress = driver
            .start(&mut session, &tool_runtime, 100)
            .expect("first Provider request should start");

        // transport 的 reader 在独立线程中解码 stdout。测试按生产方式非阻塞轮询，而不是
        // 直接读取子进程，确保覆盖 request、framing、Host 映射、continuation 和最终结算。
        let deadline = Instant::now() + Duration::from_secs(10);
        while session.snapshot().phase != SessionPhase::Idle {
            assert!(
                Instant::now() < deadline,
                "Node Provider Host tool round should settle before timeout"
            );
            progress.extend(
                driver
                    .poll(&mut session, &mut tool_runtime, 105)
                    .expect("Provider Host responses should apply"),
            );
            thread::sleep(Duration::from_millis(10));
        }

        let transcript = session.snapshot().transcript;
        assert_eq!(transcript.len(), 5);
        assert!(matches!(
            &transcript[1],
            TranscriptItem::Assistant(AssistantTranscriptItem::Complete { content, .. })
                if matches!(content.as_slice(), [
                    AssistantContent::ToolCall { tool_call_id: first_id, tool_name: first_name, .. },
                    AssistantContent::ToolCall { tool_call_id: second_id, tool_name: second_name, .. },
                ] if first_id == "read-call-1" && first_name == "read"
                    && second_id == "read-call-2" && second_name == "read")
        ));
        for (item, tool_call_id, text) in [
            (
                &transcript[2],
                "read-call-1",
                "hello from the first Rust read tool call",
            ),
            (
                &transcript[3],
                "read-call-2",
                "hello from the second Rust read tool call",
            ),
        ] {
            assert!(matches!(
                item,
                TranscriptItem::Tool(protocol::ToolTranscriptItem::Complete {
                    tool_call_id: actual_tool_call_id,
                    content,
                    is_error,
                    ..
                }) if actual_tool_call_id == tool_call_id
                    && !is_error
                    && matches!(content.as_slice(), [protocol::TextOrImageContent::Text { text: actual_text }]
                        if actual_text == text)
            ));
        }
        assert!(matches!(
            &transcript[4],
            TranscriptItem::Assistant(AssistantTranscriptItem::Complete {
                id,
                content,
                response_model: Some(response_model),
                usage: Some(terminal_usage),
                ..
            }) if id == "assistant-final"
                && response_model == "resolved-thinking-model"
                && terminal_usage == &usage()
                && matches!(content.as_slice(), [
                    AssistantContent::Thinking { thinking, .. },
                    AssistantContent::Text { text }
                ] if thinking == "final reasoning"
                    && text == "Rust runtime completed the tool round")
        ));
        assert!(progress.iter().any(|event| matches!(
            event,
            protocol::ServerEvent::SessionProgress {
                progress: protocol::TranscriptProgress::ItemUpdated {
                    item: protocol::ActiveTranscriptItem::Assistant(
                        AssistantTranscriptItem::Streaming { content, .. }
                    )
                },
                ..
            } if matches!(content.as_slice(), [
                AssistantContent::Thinking { thinking, .. },
                AssistantContent::Text { text }
            ] if thinking == "draft reasoning" && text == "draft answer")
        )));
        assert!(progress.iter().any(|event| matches!(
            event,
            protocol::ServerEvent::SessionProgress {
                progress: protocol::TranscriptProgress::ItemStarted {
                    item: TranscriptItem::Tool(_)
                },
                ..
            }
        )));

        fs::remove_dir_all(workspace).expect("temporary workspace should be removed");
    }

    #[test]
    fn forwards_abort_for_the_active_provider_request() {
        let mut driver = ProviderStreamDriver::new(QueuedProviderStreamPort::default(), "");
        let mut session = test_session();
        prompt(&mut session, 100);
        driver
            .start(&mut session, &ToolRuntime::new(), 100)
            .expect("request should start");
        session
            .execute_at(
                Command::Abort {
                    session_id: "session-1".to_owned(),
                },
                101,
            )
            .expect("session abort should be accepted");
        driver.abort().expect("provider abort should be forwarded");

        let transport = driver.into_transport();
        assert_eq!(transport.aborts(), ["session-1-provider-1"]);
    }

    #[test]
    fn complete_without_terminal_event_becomes_error_and_settles() {
        let transport = QueuedProviderStreamPort::with_responses([vec![
            event(ProviderStreamEvent::Start {
                message_id: "assistant-1".to_owned(),
                timestamp: 101,
            }),
            ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            },
        ]]);
        let mut driver = ProviderStreamDriver::new(transport, "");
        let mut session = test_session();
        prompt(&mut session, 100);
        driver
            .start(&mut session, &ToolRuntime::new(), 100)
            .expect("request should start");

        let mut tool_runtime = ToolRuntime::new();
        driver
            .poll(&mut session, &mut tool_runtime, 102)
            .expect("incomplete stream should normalize to error");

        assert_eq!(session.snapshot().phase, SessionPhase::Idle);
        assert!(matches!(
            session.snapshot().transcript.last(),
            Some(TranscriptItem::Assistant(AssistantTranscriptItem::Error {
                error_message: Some(error_message),
                ..
            })) if error_message.contains("终态事件前结束")
        ));
    }

    #[test]
    fn rejects_response_for_another_request_id() {
        let transport =
            QueuedProviderStreamPort::with_responses([vec![ProviderHostResponse::Complete {
                request_id: "another-request".to_owned(),
            }]]);
        let mut driver = ProviderStreamDriver::new(transport, "");
        let mut session = test_session();
        prompt(&mut session, 100);
        driver
            .start(&mut session, &ToolRuntime::new(), 100)
            .expect("request should start");

        let mut tool_runtime = ToolRuntime::new();
        let error = driver
            .poll(&mut session, &mut tool_runtime, 101)
            .expect_err("foreign response must be rejected");
        assert!(error.contains("request ID 不匹配"));
    }

    #[test]
    fn projects_thinking_progress_and_persists_only_the_authoritative_terminal() {
        let transport = QueuedProviderStreamPort::with_responses([vec![
            event(ProviderStreamEvent::Start {
                message_id: "assistant-1".to_owned(),
                timestamp: 101,
            }),
            event(ProviderStreamEvent::ThinkingDelta {
                content_index: 0,
                delta: "private reasoning".to_owned(),
                redacted: Some(false),
            }),
            event(done(
                "assistant-1",
                vec![
                    AssistantContent::Thinking {
                        thinking: "final reasoning".to_owned(),
                        redacted: Some(false),
                    },
                    AssistantContent::Text {
                        text: "answer".to_owned(),
                    },
                ],
                102,
                AssistantStopReason::Stop,
            )),
            ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            },
        ]]);
        let mut driver = ProviderStreamDriver::new(transport, "");
        let mut session = test_session();
        prompt(&mut session, 100);
        driver
            .start(&mut session, &ToolRuntime::new(), 100)
            .expect("request should start");

        let mut tool_runtime = ToolRuntime::new();
        let progress = driver
            .poll(&mut session, &mut tool_runtime, 102)
            .expect("thinking delta should flow through Agent Loop");
        assert!(progress.iter().any(|event| matches!(
            event,
            protocol::ServerEvent::SessionProgress {
                progress: protocol::TranscriptProgress::ItemUpdated {
                    item: protocol::ActiveTranscriptItem::Assistant(
                        AssistantTranscriptItem::Streaming { content, .. }
                    )
                },
                ..
            } if matches!(content.as_slice(), [AssistantContent::Thinking { thinking, .. }]
                if thinking == "private reasoning")
        )));
        assert_eq!(session.snapshot().phase, SessionPhase::Idle);
        assert!(matches!(
            session.snapshot().transcript.last(),
            Some(TranscriptItem::Assistant(AssistantTranscriptItem::Complete {
                content,
                response_model: Some(response_model),
                usage: Some(terminal_usage),
                ..
            })) if response_model == "resolved-model"
                && terminal_usage == &usage()
                && matches!(content.as_slice(), [
                    AssistantContent::Thinking { thinking, .. },
                    AssistantContent::Text { text }
                ] if thinking == "final reasoning" && text == "answer")
        ));
    }

    #[test]
    fn abort_settles_to_an_aborted_assistant_after_host_error() {
        let transport = QueuedProviderStreamPort::with_responses([vec![
            event(ProviderStreamEvent::Start {
                message_id: "assistant-1".to_owned(),
                timestamp: 101,
            }),
            event(ProviderStreamEvent::Error {
                message_id: "assistant-1".to_owned(),
                content: Vec::new(),
                response_model: None,
                usage: None,
                timestamp: 102,
                message: "cancelled".to_owned(),
            }),
            ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            },
        ]]);
        let mut driver = ProviderStreamDriver::new(transport, "");
        let mut session = test_session();
        prompt(&mut session, 100);
        driver
            .start(&mut session, &ToolRuntime::new(), 100)
            .expect("request should start");
        session
            .execute_at(
                Command::Abort {
                    session_id: "session-1".to_owned(),
                },
                101,
            )
            .expect("session abort should be accepted");
        driver.abort().expect("provider abort should be forwarded");

        let mut tool_runtime = ToolRuntime::new();
        driver
            .poll(&mut session, &mut tool_runtime, 102)
            .expect("provider error should settle the abort");

        assert_eq!(session.snapshot().phase, SessionPhase::Idle);
        assert!(matches!(
            session.snapshot().transcript.last(),
            Some(TranscriptItem::Assistant(
                AssistantTranscriptItem::Aborted { .. }
            ))
        ));
    }

    #[test]
    fn abort_during_tool_execution_drops_late_output_and_next_prompt_starts_cleanly() {
        let transport = QueuedProviderStreamPort::with_responses([vec![
            event(ProviderStreamEvent::Start {
                message_id: "assistant-tool".to_owned(),
                timestamp: 101,
            }),
            event(ProviderStreamEvent::ToolCallEnd {
                content_index: 0,
                tool_call: protocol::ToolCall {
                    tool_call_id: "controlled-1".to_owned(),
                    tool_name: "controlled".to_owned(),
                    input: serde_json::json!({}),
                },
            }),
            event(done(
                "assistant-tool",
                vec![AssistantContent::ToolCall {
                    tool_call_id: "controlled-1".to_owned(),
                    tool_name: "controlled".to_owned(),
                    input: serde_json::json!({}),
                }],
                102,
                AssistantStopReason::ToolUse,
            )),
            ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            },
        ]]);
        let release = Arc::new(AtomicBool::new(false));
        let mut tool_runtime = ToolRuntime::new().with_timeout(None);
        tool_runtime
            .register(ControlledTool {
                release: Arc::clone(&release),
            })
            .unwrap();
        let mut driver = ProviderStreamDriver::new(transport, "");
        let mut session = test_session();
        prompt(&mut session, 100);
        driver.start(&mut session, &tool_runtime, 100).unwrap();
        driver.poll(&mut session, &mut tool_runtime, 102).unwrap();

        let deadline = Instant::now() + Duration::from_secs(1);
        let progress = loop {
            let progress = driver.poll(&mut session, &mut tool_runtime, 103).unwrap();
            if progress.iter().any(|event| {
                matches!(
                    event,
                    protocol::ServerEvent::SessionProgress {
                        progress: protocol::TranscriptProgress::ItemUpdated {
                            item: protocol::ActiveTranscriptItem::Tool(_)
                        },
                        ..
                    }
                )
            }) {
                break progress;
            }
            assert!(Instant::now() < deadline, "tool progress should arrive");
            thread::yield_now();
        };
        assert!(!progress.is_empty());

        session
            .execute_at(
                Command::Abort {
                    session_id: "session-1".to_owned(),
                },
                104,
            )
            .unwrap();
        tool_runtime.cancel();
        driver.abort().unwrap();
        driver.poll(&mut session, &mut tool_runtime, 105).unwrap();
        assert_eq!(session.snapshot().phase, SessionPhase::Idle);
        assert!(matches!(
            session.snapshot().transcript.last(),
            Some(TranscriptItem::Assistant(
                AssistantTranscriptItem::Aborted { .. }
            ))
        ));
        assert!(!session.snapshot().transcript.iter().any(|item| matches!(
            item,
            TranscriptItem::Tool(protocol::ToolTranscriptItem::Complete { tool_call_id, .. })
                | TranscriptItem::Tool(protocol::ToolTranscriptItem::Error { tool_call_id, .. })
                if tool_call_id == "controlled-1"
        )));

        release.store(true, Ordering::Release);
        thread::sleep(Duration::from_millis(10));
        assert!(
            driver
                .poll(&mut session, &mut tool_runtime, 106)
                .unwrap()
                .is_empty()
        );

        prompt(&mut session, 107);
        tool_runtime.reset_cancellation();
        driver.start(&mut session, &tool_runtime, 107).unwrap();
        assert_eq!(driver.transport.requests().len(), 2);
        assert_eq!(
            driver.transport.requests()[1].request_id,
            "session-1-provider-2"
        );
    }
}
