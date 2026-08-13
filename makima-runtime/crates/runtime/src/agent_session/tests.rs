//! AgentSession 状态机的无网络回归测试。
//!
//! fake 端口记录边界调用，覆盖 TypeScript AgentSession 最关键的并发输入、稳定
//! 消息持久化、abort 等待 settled 与模型配置行为，而不依赖尚未迁移的 Provider。

use std::{
    collections::VecDeque,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use agent_loop::{AgentLoopEngine, ProviderEvent};
use protocol::{
    AssistantContent, AssistantRole, AssistantStopReason, AssistantTranscriptItem, Command,
    ModelRef, ProtocolErrorCode, SessionPhase, ThinkingLevel, TranscriptItem,
};

use session::JsonlSessionStore;

use super::{
    AgentLoop, AgentLoopError, AgentLoopEvent, AgentSession, AgentSessionConfig,
    JsonlSessionPersistence, PersistenceEvent, SessionPersistence, SessionPersistenceError,
    session_events_from_rust_agent_loop,
};

#[derive(Default)]
struct FakeAgentLoop {
    calls: VecDeque<String>,
}

impl AgentLoop for FakeAgentLoop {
    fn prompt(&mut self, message: protocol::UserTranscriptItem) -> Result<(), AgentLoopError> {
        self.calls
            .push_back(format!("prompt:{}", text_of(&message)));
        Ok(())
    }

    fn steer(&mut self, message: protocol::UserTranscriptItem) -> Result<(), AgentLoopError> {
        self.calls.push_back(format!("steer:{}", text_of(&message)));
        Ok(())
    }

    fn abort(&mut self) -> Result<(), AgentLoopError> {
        self.calls.push_back("abort".to_owned());
        Ok(())
    }
}

#[derive(Default)]
struct FakePersistence {
    events: Vec<PersistenceEvent>,
}

impl SessionPersistence for FakePersistence {
    fn persist(&mut self, event: PersistenceEvent) -> Result<(), SessionPersistenceError> {
        self.events.push(event);
        Ok(())
    }
}

fn text_of(message: &protocol::UserTranscriptItem) -> &str {
    match message.content.first() {
        Some(protocol::TextOrImageContent::Text { text }) => text,
        Some(protocol::TextOrImageContent::Image { .. }) | None => {
            panic!("test message must contain text")
        }
    }
}

static TEMPORARY_STORE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn temporary_store_path(name: &str) -> PathBuf {
    let sequence = TEMPORARY_STORE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "agent-session-runtime-{name}-{}-{sequence}.jsonl",
        std::process::id()
    ))
}

fn test_session() -> AgentSession<FakeAgentLoop, FakePersistence> {
    AgentSession::new(
        AgentSessionConfig {
            id: "session-1".to_owned(),
            cwd: "C:/workspace".to_owned(),
            model: ModelRef {
                provider: "test".to_owned(),
                id: "model-a".to_owned(),
            },
            thinking_level: ThinkingLevel::Medium,
            created_at: 100,
        },
        FakeAgentLoop::default(),
        FakePersistence::default(),
    )
}

#[test]
fn prompt_rejects_a_second_direct_prompt_and_steer_updates_the_queue() {
    let mut session = test_session();

    session
        .execute_at(
            Command::Prompt {
                session_id: "session-1".to_owned(),
                text: "first".to_owned(),
            },
            101,
        )
        .expect("first prompt should start a turn");

    let error = session
        .execute_at(
            Command::Prompt {
                session_id: "session-1".to_owned(),
                text: "second".to_owned(),
            },
            102,
        )
        .expect_err("second direct prompt must be rejected during a turn");
    assert_eq!(error.code, ProtocolErrorCode::Busy);

    let snapshot = session
        .execute_at(
            Command::Steer {
                session_id: "session-1".to_owned(),
                text: "steer me".to_owned(),
            },
            103,
        )
        .expect("steer should be accepted during a turn");

    assert_eq!(snapshot.phase, SessionPhase::Turn);
    assert_eq!(snapshot.queued_steer_count, 1);
    assert_eq!(
        session.agent_loop().calls,
        VecDeque::from(["prompt:first".to_owned(), "steer:steer me".to_owned()])
    );

    session
        .handle_agent_loop_event_at(AgentLoopEvent::SteerConsumed, 104)
        .expect("consumed steer should update the local projection");
    assert_eq!(session.snapshot().queued_steer_count, 0);
}

#[test]
fn abort_keeps_the_turn_active_until_the_loop_settles() {
    let mut session = test_session();
    session
        .execute_at(
            Command::Prompt {
                session_id: "session-1".to_owned(),
                text: "run".to_owned(),
            },
            101,
        )
        .expect("prompt should start");

    let after_abort = session
        .execute_at(
            Command::Abort {
                session_id: "session-1".to_owned(),
            },
            102,
        )
        .expect("abort request should reach the loop");
    assert_eq!(after_abort.phase, SessionPhase::Turn);

    let settled = session
        .handle_agent_loop_event_at(AgentLoopEvent::Settled, 103)
        .expect("settled event should finish the turn");
    assert_eq!(settled.phase, SessionPhase::Idle);
    assert_eq!(
        session.agent_loop().calls,
        VecDeque::from(["prompt:run".to_owned(), "abort".to_owned()])
    );
}

#[test]
fn only_finished_transcript_items_are_persisted_in_their_arrival_order() {
    let mut session = test_session();
    let assistant = TranscriptItem::Assistant(AssistantTranscriptItem::Complete {
        id: "assistant-1".to_owned(),
        role: AssistantRole::Assistant,
        content: vec![AssistantContent::Text {
            text: "done".to_owned(),
        }],
        model: ModelRef {
            provider: "test".to_owned(),
            id: "model-a".to_owned(),
        },
        response_model: None,
        usage: None,
        timestamp: 101,
        stop_reason: AssistantStopReason::Stop,
    });

    session
        .handle_agent_loop_event_at(
            AgentLoopEvent::TranscriptItemFinished(assistant.clone()),
            102,
        )
        .expect("finished item should be stored");

    assert_eq!(session.snapshot().transcript, vec![assistant.clone()]);
    assert_eq!(
        session.persistence().events,
        vec![PersistenceEvent::TranscriptItemFinished(assistant)]
    );
}

#[test]
fn configuration_changes_persist_before_the_snapshot_is_published() {
    let mut session = test_session();
    let model = ModelRef {
        provider: "another".to_owned(),
        id: "model-b".to_owned(),
    };

    let snapshot = session
        .execute_at(
            Command::SetModel {
                session_id: "session-1".to_owned(),
                model: model.clone(),
            },
            101,
        )
        .expect("model selection should succeed");
    session
        .execute_at(
            Command::SetThinking {
                session_id: "session-1".to_owned(),
                thinking_level: ThinkingLevel::High,
            },
            102,
        )
        .expect("thinking selection should succeed");

    assert_eq!(snapshot.model, model);
    assert_eq!(session.snapshot().thinking_level, ThinkingLevel::High);
    assert_eq!(
        session.persistence().events,
        vec![
            PersistenceEvent::ModelChanged(model),
            PersistenceEvent::ThinkingLevelChanged(ThinkingLevel::High),
        ]
    );
}

#[test]
fn jsonl_persistence_writes_reopenable_v4_entries_for_session_events() {
    let path = temporary_store_path("persistence");
    let _ = fs::remove_file(&path);
    let store = JsonlSessionStore::create(&path, "session-1", "C:/workspace")
        .expect("test store should be created");
    let mut persistence = JsonlSessionPersistence::new(store);
    let model = ModelRef {
        provider: "test".to_owned(),
        id: "model-a".to_owned(),
    };
    let assistant = TranscriptItem::Assistant(AssistantTranscriptItem::Complete {
        id: "assistant-1".to_owned(),
        role: AssistantRole::Assistant,
        content: vec![AssistantContent::Text {
            text: "saved response".to_owned(),
        }],
        model: model.clone(),
        response_model: None,
        usage: None,
        timestamp: 101,
        stop_reason: AssistantStopReason::Stop,
    });

    persistence
        .persist(PersistenceEvent::ModelChanged(model.clone()))
        .expect("model change should be persisted");
    persistence
        .persist(PersistenceEvent::ThinkingLevelChanged(ThinkingLevel::High))
        .expect("thinking level should be persisted");
    persistence
        .persist(PersistenceEvent::TranscriptItemFinished(assistant.clone()))
        .expect("finished transcript item should be persisted");

    let store = persistence.into_store();
    assert_eq!(store.mutations().len(), 3);
    assert_eq!(store.mutations()[0].payload["type"], "model_change");
    assert_eq!(store.mutations()[0].payload["provider"], model.provider);
    assert_eq!(store.mutations()[0].payload["modelId"], model.id);
    assert_eq!(
        store.mutations()[1].payload["thinkingLevel"],
        "high",
        "thinking level must use the shared protocol's camelCase JSON field"
    );
    assert_eq!(store.mutations()[2].payload["type"], "message");
    assert_eq!(store.mutations()[2].payload["message"]["id"], "assistant-1");
    drop(store);

    let recovered = JsonlSessionStore::open(&path).expect("written v4 entries should reopen");
    assert_eq!(recovered.mutations().len(), 3);
    assert_eq!(
        recovered
            .state()
            .find_entries(session::EntryQuery {
                entry_type: Some("message"),
                ..Default::default()
            })
            .expect("message query should be valid")
            .len(),
        1
    );
    fs::remove_file(path).expect("temporary store should be removed");
}

#[test]
fn rust_agent_loop_terminal_events_drive_session_persistence_and_settlement() {
    let config = AgentSessionConfig {
        id: "session-1".to_owned(),
        cwd: "C:/workspace".to_owned(),
        model: ModelRef {
            provider: "test".to_owned(),
            id: "model-a".to_owned(),
        },
        thinking_level: ThinkingLevel::Medium,
        created_at: 100,
    };
    let mut session = AgentSession::new(
        config.clone(),
        AgentLoopEngine::new(config.model),
        FakePersistence::default(),
    );

    session
        .execute_at(
            Command::Prompt {
                session_id: "session-1".to_owned(),
                text: "hello".to_owned(),
            },
            101,
        )
        .expect("prompt should reach the Rust Agent Loop");

    let mut loop_events = session.agent_loop_mut().drain_events();
    loop_events.extend(
        session
            .agent_loop_mut()
            .handle_provider_event(ProviderEvent::Started {
                message_id: "assistant-1".to_owned(),
                timestamp: 102,
            })
            .expect("provider start should be accepted"),
    );
    loop_events.extend(
        session
            .agent_loop_mut()
            .handle_provider_event(ProviderEvent::TextDelta {
                text: "world".to_owned(),
            })
            .expect("provider delta should be accepted"),
    );
    loop_events.extend(
        session
            .agent_loop_mut()
            .handle_provider_event(ProviderEvent::Completed {
                timestamp: 103,
                stop_reason: AssistantStopReason::Stop,
            })
            .expect("provider completion should be accepted"),
    );

    for event in session_events_from_rust_agent_loop(loop_events) {
        session
            .handle_agent_loop_event_at(event, 104)
            .expect("terminal Loop event should update the Session");
    }

    assert_eq!(session.snapshot().phase, SessionPhase::Idle);
    assert_eq!(session.snapshot().transcript.len(), 2);
    assert_eq!(session.persistence().events.len(), 2);
    assert!(matches!(
        &session.persistence().events[1],
        PersistenceEvent::TranscriptItemFinished(TranscriptItem::Assistant(
            AssistantTranscriptItem::Complete { .. }
        ))
    ));
}

#[test]
fn session_scoped_commands_cannot_target_another_session() {
    let mut session = test_session();
    let error = session
        .execute_at(
            Command::SetThinking {
                session_id: "other-session".to_owned(),
                thinking_level: ThinkingLevel::High,
            },
            101,
        )
        .expect_err("another session ID must not mutate this session");

    assert_eq!(error.code, ProtocolErrorCode::NotFound);
    assert_eq!(session.snapshot().revision, 0);
}
