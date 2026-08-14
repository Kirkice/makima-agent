//! Provider 流到 AgentSession 的运行时适配器。
//!
//! 本模块位于 [`crate::provider_ipc`] 与 `AgentSession` 之间：它只处理共享 DTO、请求 ID
//! 和 Agent Loop 生命周期，不感知 socket、TUI 或具体 Provider SDK。异步传输实现应把已经
//! 解码的响应放入 [`ProviderStreamPort`]，由 SessionManager 在安全边界轮询。

use std::collections::VecDeque;

use agent_loop::{AgentLoopEngine, AgentLoopEvent as RustAgentLoopEvent, ProviderEvent};
use protocol::{
    ActiveTranscriptItem, AssistantTranscriptItem, FinishedTranscriptItem, ProviderHostResponse,
    ProviderRequest, ProviderStreamEvent, ServerEvent, SessionPhase, ToolTranscriptItem,
    TranscriptItem, TranscriptProgress,
};

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
            tools: Vec::new(),
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
        timestamp: u64,
    ) -> Result<Vec<ServerEvent>, String> {
        let responses = match self.transport.try_receive()? {
            Some(responses) => responses,
            None => return Ok(Vec::new()),
        };
        let mut events = Vec::new();
        for response in responses {
            match response {
                ProviderHostResponse::Event { request_id, event } => {
                    self.require_active_request(&request_id)?;
                    // 取消会由第一个后续 Provider 事件结算。Host 仍可能把已经在管道中的
                    // 终态事件送达；它们属于同一请求但不能再投递给已空闲的 Agent Loop。
                    if self.active_request_terminal {
                        continue;
                    }
                    self.apply_provider_event(session, event, timestamp, &mut events)?;
                    if session.snapshot().phase != SessionPhase::Turn {
                        self.active_request_terminal = true;
                    }
                }
                ProviderHostResponse::Complete { request_id } => {
                    self.require_active_request(&request_id)?;
                    if !self.active_request_terminal {
                        self.apply_provider_event(
                            session,
                            ProviderStreamEvent::Error {
                                timestamp,
                                message: "Provider Host 在发送终态事件前结束请求。".to_owned(),
                            },
                            timestamp,
                            &mut events,
                        )?;
                    }
                    self.active_request_id = None;
                    self.active_request_terminal = false;
                }
            }
        }
        Ok(events)
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
        stream_event: ProviderStreamEvent,
        timestamp: u64,
        output: &mut Vec<ServerEvent>,
    ) -> Result<(), String> {
        // AgentLoopEngine 目前没有 thinking transcript 状态。忽略该事件而不是让一个可选的
        // provider 能力中断整轮请求；后续启用 thinking 状态时应在此处投影为 progress。
        if matches!(stream_event, ProviderStreamEvent::ThinkingDelta { .. }) {
            return Ok(());
        }

        let provider_event =
            ProviderEvent::try_from(stream_event).map_err(|error| error.message().to_owned())?;
        let loop_events = session
            .agent_loop_mut()
            .handle_provider_event(provider_event)
            .map_err(|error| error.message().to_owned())?;
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
            if let Some(progress) = progress_for_loop_event(event) {
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

fn progress_for_loop_event(event: &RustAgentLoopEvent) -> Option<TranscriptProgress> {
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
        RustAgentLoopEvent::AgentStarted
        | RustAgentLoopEvent::TurnStarted
        | RustAgentLoopEvent::ToolExecutionStarted { .. }
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
        sync::atomic::{AtomicU64, Ordering},
    };

    use protocol::{
        AssistantContent, AssistantStopReason, AssistantTranscriptItem, Command, ModelRef,
        ProviderHostResponse, ProviderStreamEvent, SessionPhase, ThinkingLevel, TranscriptItem,
    };
    use session::JsonlSessionStore;

    use super::{
        AgentLoopEngine, AgentSession, JsonlSessionPersistence, ProviderStreamDriver,
        QueuedProviderStreamPort,
    };
    use crate::agent_session::AgentSessionConfig;

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

    fn event(event: ProviderStreamEvent) -> ProviderHostResponse {
        ProviderHostResponse::Event {
            request_id: "session-1-provider-1".to_owned(),
            event,
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
            event(ProviderStreamEvent::Done {
                timestamp: 102,
                stop_reason: AssistantStopReason::Stop,
            }),
            ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            },
        ]]);
        let mut driver = ProviderStreamDriver::new(transport, "system prompt");
        let mut session = test_session();
        prompt(&mut session, 100);

        let initial_progress = driver
            .start(&mut session, 100)
            .expect("request should start");
        assert!(initial_progress.iter().any(|event| matches!(
            event,
            protocol::ServerEvent::SessionProgress {
                progress: protocol::TranscriptProgress::ItemStarted { .. },
                ..
            }
        )));

        let progress = driver.poll(&mut session, 102).expect("stream should apply");
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
    fn forwards_abort_for_the_active_provider_request() {
        let mut driver = ProviderStreamDriver::new(QueuedProviderStreamPort::default(), "");
        let mut session = test_session();
        prompt(&mut session, 100);
        driver
            .start(&mut session, 100)
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
            .start(&mut session, 100)
            .expect("request should start");

        driver
            .poll(&mut session, 102)
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
            .start(&mut session, 100)
            .expect("request should start");

        let error = driver
            .poll(&mut session, 101)
            .expect_err("foreign response must be rejected");
        assert!(error.contains("request ID 不匹配"));
    }

    #[test]
    fn ignores_thinking_delta_until_the_agent_loop_supports_thinking_state() {
        let transport = QueuedProviderStreamPort::with_responses([vec![
            event(ProviderStreamEvent::ThinkingDelta {
                content_index: 0,
                delta: "private reasoning".to_owned(),
                redacted: Some(false),
            }),
            event(ProviderStreamEvent::Start {
                message_id: "assistant-1".to_owned(),
                timestamp: 101,
            }),
            event(ProviderStreamEvent::Done {
                timestamp: 102,
                stop_reason: AssistantStopReason::Stop,
            }),
            ProviderHostResponse::Complete {
                request_id: "session-1-provider-1".to_owned(),
            },
        ]]);
        let mut driver = ProviderStreamDriver::new(transport, "");
        let mut session = test_session();
        prompt(&mut session, 100);
        driver
            .start(&mut session, 100)
            .expect("request should start");

        driver
            .poll(&mut session, 102)
            .expect("thinking delta should not fail a text-only session");
        assert_eq!(session.snapshot().phase, SessionPhase::Idle);
    }

    #[test]
    fn abort_settles_to_an_aborted_assistant_after_host_error() {
        let transport = QueuedProviderStreamPort::with_responses([vec![
            event(ProviderStreamEvent::Start {
                message_id: "assistant-1".to_owned(),
                timestamp: 101,
            }),
            event(ProviderStreamEvent::Error {
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
            .start(&mut session, 100)
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

        driver
            .poll(&mut session, 102)
            .expect("provider error should settle the abort");

        assert_eq!(session.snapshot().phase, SessionPhase::Idle);
        assert!(matches!(
            session.snapshot().transcript.last(),
            Some(TranscriptItem::Assistant(
                AssistantTranscriptItem::Aborted { .. }
            ))
        ));
    }
}
