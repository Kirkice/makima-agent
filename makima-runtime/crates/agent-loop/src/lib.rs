//! Rust Agent Loop 的可回放回合状态机。
//!
//! 本 crate 不直接调用 Provider SDK、Tool Runtime 或 TUI。它接收已经归一化的 Provider
//! 流事件，生成稳定的生命周期事件与 transcript 项；上层适配器负责把真实网络流逐项送入
//! [`AgentLoopEngine::handle_provider_event`]。这种设计使事件顺序可离线回放和单元测试，
//! 并避免核心状态机耦合 TypeScript Provider Host。

use protocol::{
    AbortedStopReason, AssistantContent, AssistantRole, AssistantStopReason,
    AssistantTranscriptItem, ErrorStopReason, ModelRef, ProviderStreamEvent, TextOrImageContent,
    TranscriptItem, UserRole, UserTranscriptItem,
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

/// Provider Host 归一化后输入 Agent Loop 的最小事件集合。
///
/// 该集合只覆盖无工具的文本 MVP。Provider 的原始 SSE 字段、认证和网络异常必须在
/// Host 侧先归一化；工具调用、thinking、重试会在后续切片增加对应的显式事件。
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderEvent {
    /// Provider 已接受请求并开始生成 assistant 项。
    Started { message_id: String, timestamp: u64 },
    /// 一个文本增量。必须在 [`ProviderEvent::Started`] 后出现。
    TextDelta { text: String },
    /// 正常结束，并提交一个完成态 assistant 项。
    Completed {
        timestamp: u64,
        stop_reason: AssistantStopReason,
    },
    /// Provider 返回可恢复或不可恢复的错误；它仍会生成稳定 error assistant 项。
    Failed { timestamp: u64, message: String },
}

/// 将跨语言 DTO 转换为当前无工具文本状态机使用的内部事件。
///
/// 适配层必须在到达状态机前处理 Provider SDK 的私有字段。thinking 和工具调用的协议
/// 已先行定义，但它们需要对应的状态机/Tool Runtime 切片，故在此阶段明确拒绝而非静默丢弃。
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
            ProviderStreamEvent::ThinkingDelta { .. }
            | ProviderStreamEvent::ToolCallDelta { .. }
            | ProviderStreamEvent::ToolCallEnd { .. } => Err(AgentLoopError::new(
                "当前 Agent Loop 尚未启用 thinking 或工具调用事件。",
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
    content: String,
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

    /// 处理一个归一化 Provider 事件并返回本次产生的生命周期事件。
    pub fn handle_provider_event(
        &mut self,
        event: ProviderEvent,
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
                ProviderEvent::TextDelta { .. } => self
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
            ProviderEvent::Completed {
                timestamp,
                stop_reason,
            } => self.complete_assistant(timestamp, stop_reason)?,
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
            content: assistant.map_or_else(Vec::new, |value| text_content(value.content)),
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
            content: String::new(),
        };
        self.events.push(AgentLoopEvent::TranscriptItemStarted(
            TranscriptItem::Assistant(self.streaming_item(&assistant)),
        ));
        self.active_assistant = Some(assistant);
        Ok(())
    }

    fn append_text(&mut self, text: String) -> Result<(), AgentLoopError> {
        let assistant = self
            .active_assistant
            .as_mut()
            .ok_or_else(|| AgentLoopError::new("收到文本增量前必须先收到 Provider start。"))?;
        assistant.content.push_str(&text);
        let item = AssistantTranscriptItem::Streaming {
            id: assistant.id.clone(),
            role: AssistantRole::Assistant,
            content: text_content(assistant.content.clone()),
            model: self.model.clone(),
            response_model: None,
            usage: None,
            timestamp: assistant.timestamp,
        };
        self.events
            .push(AgentLoopEvent::TranscriptItemUpdated(item));
        Ok(())
    }

    fn complete_assistant(
        &mut self,
        timestamp: u64,
        stop_reason: AssistantStopReason,
    ) -> Result<(), AgentLoopError> {
        let assistant = self.active_assistant.take().ok_or_else(|| {
            AgentLoopError::new("收到 Provider 完成事件前必须先收到 Provider start。")
        })?;
        let item = TranscriptItem::Assistant(AssistantTranscriptItem::Complete {
            id: assistant.id,
            role: AssistantRole::Assistant,
            content: text_content(assistant.content),
            model: self.model.clone(),
            response_model: None,
            usage: None,
            timestamp,
            stop_reason,
        });
        self.finish_turn(item);
        Ok(())
    }

    fn fail_assistant(&mut self, timestamp: u64, message: String) -> Result<(), AgentLoopError> {
        let assistant = self.active_assistant.take().ok_or_else(|| {
            AgentLoopError::new("收到 Provider 错误事件前必须先收到 Provider start。")
        })?;
        let item = TranscriptItem::Assistant(AssistantTranscriptItem::Error {
            id: assistant.id,
            role: AssistantRole::Assistant,
            content: text_content(assistant.content),
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
        AssistantTranscriptItem::Streaming {
            id: assistant.id.clone(),
            role: AssistantRole::Assistant,
            content: text_content(assistant.content.clone()),
            model: self.model.clone(),
            response_model: None,
            usage: None,
            timestamp: assistant.timestamp,
        }
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
        self.events
            .push(AgentLoopEvent::TurnEnded { message: item });
        self.active = false;
        self.abort_requested = false;
        self.events.push(AgentLoopEvent::AgentEnded {
            messages: self.messages.clone(),
        });
    }
}

fn text_content(text: String) -> Vec<AssistantContent> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![AssistantContent::Text { text }]
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
        ProviderStreamEvent, TranscriptItem,
    };
    use serde::Deserialize;

    use super::{AgentLoopEngine, AgentLoopEvent, ProviderEvent, user_text_item};

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
            AgentLoopEvent::TurnEnded { .. } => "turn_end",
            AgentLoopEvent::AgentEnded { .. } => "agent_end",
        }
    }
}
