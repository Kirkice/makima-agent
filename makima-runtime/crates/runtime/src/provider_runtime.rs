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

use crate::agent_session::{
    AgentSession, JsonlSessionPersistence, session_events_from_rust_agent_loop,
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
pub struct ProviderStreamDriver<P> {
    transport: P,
    active_request_id: Option<String>,
    active_request_terminal: bool,
    continuation_pending: bool,
    next_request_sequence: u64,
    system_prompt: String,
}

impl<P> ProviderStreamDriver<P>
where
    P: ProviderStreamPort,
{
    /// 用 session 级系统提示创建驱动器。
    pub fn new(transport: P, system_prompt: impl Into<String>) -> Self {
        Self {
            transport,
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

    /// 从已由 AgentSession 接受的 prompt 创建并发送 ProviderRequest。
    pub fn start(
        &mut self,
        session: &mut AgentSession<AgentLoopEngine, JsonlSessionPersistence>,
        tool_runtime: &ToolRuntime,
        timestamp: u64,
    ) -> Result<Vec<ServerEvent>, String> {
        if self.active_request_id.is_some() {
            return Err("当前 Session 已有进行中的 Provider 请求。".to_owned());
        }

        self.next_request_sequence += 1;
        let request_id = format!(
            "{}-provider-{}",
            session.snapshot().id,
            self.next_request_sequence
        );
        let request = ProviderRequest {
            request_id: request_id.clone(),
            model: session.snapshot().model,
            system_prompt: self.system_prompt.clone(),
            messages: session.agent_loop().messages().to_vec(),
            tools: tool_runtime.definitions(),
        };
        self.transport.request(request)?;
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

        let Some(responses) = self.transport.try_receive()? else {
            return Ok(events);
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
            output.extend(self.start(session, tool_runtime, timestamp)?);
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
        let loop_events = session
            .agent_loop_mut()
            .handle_provider_event_with_tools(provider_event, tool_runtime)
            .map_err(|error| error.message().to_owned())?;
        if session.agent_loop().is_waiting_for_tools() {
            // Provider 的 done 已到达，但 continuation 必须等待整个工具批次稳定结束。
            self.active_request_terminal = true;
        }
        if loop_events
            .iter()
            .any(|event| matches!(event, RustAgentLoopEvent::ToolResultsReady { .. }))
        {
            self.continuation_pending = true;
            self.active_request_terminal = true;
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
        AssistantContent, AssistantStopReason, AssistantTranscriptItem, Command, ModelRef,
        ProviderHostResponse, ProviderStreamEvent, SessionPhase, ThinkingLevel, TranscriptItem,
        Usage, UsageCost,
    };
    use session::JsonlSessionStore;
    use tool_runtime::{Tool, ToolExecutionContext, ToolExecutionError, ToolOutput, ToolRuntime};

    use super::{
        AgentLoopEngine, AgentSession, JsonlSessionPersistence, ProviderStreamDriver,
        QueuedProviderStreamPort,
    };
    use crate::{agent_session::AgentSessionConfig, provider_ipc::ProviderHostStreamPort};

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
        let mut driver = ProviderStreamDriver::new(transport, "system prompt");
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
    fn node_provider_host_executes_a_complete_rust_tool_round_over_framed_cbor() {
        use tool_runtime::ReadTool;

        let sequence = STORE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let workspace = std::env::temp_dir().join(format!(
            "provider-host-tool-round-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&workspace).expect("temporary workspace should be created");
        fs::write(workspace.join("hello.txt"), "hello from the Rust read tool")
            .expect("read fixture should be written");

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
        assert_eq!(transcript.len(), 4);
        assert!(matches!(
            &transcript[1],
            TranscriptItem::Assistant(AssistantTranscriptItem::Complete { content, .. })
                if matches!(content.as_slice(), [AssistantContent::ToolCall { tool_call_id, tool_name, .. }]
                    if tool_call_id == "read-call-1" && tool_name == "read")
        ));
        assert!(matches!(
            &transcript[2],
            TranscriptItem::Tool(protocol::ToolTranscriptItem::Complete { content, is_error, .. })
                if !is_error && matches!(content.as_slice(), [protocol::TextOrImageContent::Text { text }]
                    if text == "hello from the Rust read tool")
        ));
        assert!(matches!(
            &transcript[3],
            TranscriptItem::Assistant(AssistantTranscriptItem::Complete { content, .. })
                if matches!(content.as_slice(), [AssistantContent::Text { text }]
                    if text == "Rust runtime completed the tool round")
        ));
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
