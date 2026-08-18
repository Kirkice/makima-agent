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
    ModelRef, ProtocolErrorCode, SessionPhase, ThinkingLevel, TranscriptItem, Usage, UsageCost,
};

use session::JsonlSessionStore;

use super::{
    AgentLoop, AgentLoopError, AgentLoopEvent, AgentSession, AgentSessionConfig, CompactionRecord,
    JsonlSessionPersistence, PersistenceEvent, RetryPolicy, SessionContextReplacement,
    SessionPersistence, SessionPersistenceError, session_events_from_rust_agent_loop,
};

#[derive(Default)]
struct FakeAgentLoop {
    calls: VecDeque<String>,
    retry_context_discarded: bool,
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

    fn follow_up(&mut self, message: protocol::UserTranscriptItem) -> Result<(), AgentLoopError> {
        self.calls
            .push_back(format!("follow-up:{}", text_of(&message)));
        Ok(())
    }

    fn abort(&mut self) -> Result<(), AgentLoopError> {
        self.calls.push_back("abort".to_owned());
        Ok(())
    }

    fn discard_last_error_assistant_for_retry(&mut self) -> Result<(), AgentLoopError> {
        self.retry_context_discarded = true;
        self.calls.push_back("discard-retry-error".to_owned());
        Ok(())
    }

    fn restart_after_retry(&mut self) -> Result<(), AgentLoopError> {
        if !self.retry_context_discarded {
            return Err(AgentLoopError::new("retry 前必须先移除失败 assistant。"));
        }
        self.retry_context_discarded = false;
        self.calls.push_back("restart-retry".to_owned());
        Ok(())
    }

    fn replace_context(
        &mut self,
        messages: Vec<protocol::TranscriptItem>,
    ) -> Result<(), AgentLoopError> {
        self.calls
            .push_back(format!("replace-context:{}", messages.len()));
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
            name: None,
            cwd: "C:/workspace".to_owned(),
            model: ModelRef {
                provider: "test".to_owned(),
                id: "model-a".to_owned(),
            },
            thinking_level: ThinkingLevel::Medium,
            created_at: 100,
            retry_policy: RetryPolicy::default(),
        },
        FakeAgentLoop::default(),
        FakePersistence::default(),
    )
}

fn test_session_with_retry_policy(
    retry_policy: RetryPolicy,
) -> AgentSession<FakeAgentLoop, FakePersistence> {
    let mut session = test_session();
    session.retry_policy = retry_policy;
    session
}

#[test]
fn retry_schedules_exponential_backoff_and_rejects_direct_prompt_until_resumed() {
    let mut session = test_session_with_retry_policy(RetryPolicy {
        enabled: true,
        max_retries: 2,
        base_delay_ms: 50,
    });
    session
        .execute_at(
            Command::Prompt {
                session_id: "session-1".to_owned(),
                text: "run".to_owned(),
            },
            100,
        )
        .expect("prompt should start a turn");

    let first = session
        .schedule_retry_at("Provider overloaded", 110)
        .expect("retry policy should be evaluated")
        .expect("transient provider error should schedule retry");
    assert_eq!(first.attempt, 1);
    assert_eq!(first.retry_at, 160);
    assert_eq!(session.snapshot().phase, SessionPhase::Retry);
    assert_eq!(
        session.agent_loop().calls.back(),
        Some(&"discard-retry-error".to_owned())
    );
    assert_eq!(
        session
            .execute_at(
                Command::Prompt {
                    session_id: "session-1".to_owned(),
                    text: "must wait".to_owned(),
                },
                111,
            )
            .expect_err("retry backoff remains an active turn")
            .code,
        ProtocolErrorCode::Busy
    );
    assert!(
        !session
            .resume_retry_at(159)
            .expect("early retry check should work")
    );
    assert!(
        session
            .resume_retry_at(160)
            .expect("deadline should restart retry")
    );
    assert_eq!(session.snapshot().phase, SessionPhase::Turn);
    assert_eq!(
        session.agent_loop().calls.back(),
        Some(&"restart-retry".to_owned())
    );

    let second = session
        .schedule_retry_at("429 rate limit", 200)
        .expect("second retry policy should be evaluated")
        .expect("second transient error should schedule retry");
    assert_eq!(second.attempt, 2);
    assert_eq!(second.retry_at, 300);
    assert!(
        session
            .schedule_retry_at("429 rate limit", 301)
            .expect("exhaustion check should work")
            .is_none()
    );
}

#[test]
fn retry_rejects_disabled_quota_and_context_errors_and_abort_cancels_backoff() {
    let disabled = RetryPolicy {
        enabled: false,
        max_retries: 3,
        base_delay_ms: 10,
    };
    for error in [
        "Provider overloaded",
        "insufficient_quota",
        "maximum context length exceeded",
    ] {
        let policy = if error == "Provider overloaded" {
            disabled
        } else {
            RetryPolicy::default()
        };
        let mut session = test_session_with_retry_policy(policy);
        session
            .execute_at(
                Command::Prompt {
                    session_id: "session-1".to_owned(),
                    text: "run".to_owned(),
                },
                100,
            )
            .expect("prompt should start a turn");
        assert!(
            session
                .schedule_retry_at(error, 101)
                .expect("retry policy should be evaluated")
                .is_none()
        );
    }

    let mut session = test_session_with_retry_policy(RetryPolicy {
        enabled: true,
        max_retries: 1,
        base_delay_ms: 10,
    });
    session
        .execute_at(
            Command::Prompt {
                session_id: "session-1".to_owned(),
                text: "run".to_owned(),
            },
            100,
        )
        .expect("prompt should start a turn");
    session
        .schedule_retry_at("network timeout", 101)
        .expect("retry should schedule");
    session
        .execute_at(
            Command::Abort {
                session_id: "session-1".to_owned(),
            },
            102,
        )
        .expect("abort should cancel retry backoff");
    assert_eq!(session.snapshot().phase, SessionPhase::Idle);
    assert_eq!(session.retry_attempt(), 0);
    assert_eq!(session.retry_schedule(), None);
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
fn follow_up_requires_an_active_turn_and_tracks_fifo_consumption() {
    let mut session = test_session();

    let idle_error = session
        .execute_at(
            Command::FollowUp {
                session_id: "session-1".to_owned(),
                text: "idle follow-up".to_owned(),
            },
            101,
        )
        .expect_err("idle session must reject follow-up");
    assert_eq!(idle_error.code, ProtocolErrorCode::InvalidRequest);

    session
        .execute_at(
            Command::Prompt {
                session_id: "session-1".to_owned(),
                text: "first".to_owned(),
            },
            102,
        )
        .expect("prompt should start a turn");
    let first_snapshot = session
        .execute_at(
            Command::FollowUp {
                session_id: "session-1".to_owned(),
                text: "follow-up one".to_owned(),
            },
            103,
        )
        .expect("first follow-up should be accepted");
    let second_snapshot = session
        .execute_at(
            Command::FollowUp {
                session_id: "session-1".to_owned(),
                text: "follow-up two".to_owned(),
            },
            104,
        )
        .expect("second follow-up should be accepted");

    assert_eq!(first_snapshot.queued_follow_up_count, 1);
    assert_eq!(second_snapshot.queued_follow_up_count, 2);
    assert_eq!(
        second_snapshot
            .queued_follow_up
            .iter()
            .map(text_of)
            .collect::<Vec<_>>(),
        vec!["follow-up one", "follow-up two"]
    );
    assert_eq!(
        session.agent_loop().calls,
        VecDeque::from([
            "prompt:first".to_owned(),
            "follow-up:follow-up one".to_owned(),
            "follow-up:follow-up two".to_owned(),
        ])
    );

    session
        .handle_agent_loop_event_at(AgentLoopEvent::FollowUpConsumed, 105)
        .expect("first consumed follow-up should update the local projection");
    assert_eq!(
        session
            .snapshot()
            .queued_follow_up
            .iter()
            .map(text_of)
            .collect::<Vec<_>>(),
        vec!["follow-up two"]
    );

    session
        .handle_agent_loop_event_at(AgentLoopEvent::FollowUpConsumed, 106)
        .expect("second consumed follow-up should update the local projection");
    assert_eq!(session.snapshot().queued_follow_up_count, 0);
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
fn compaction_persists_the_boundary_before_replacing_idle_working_context() {
    let mut session = test_session();
    let record = CompactionRecord {
        summary: "earlier work".to_owned(),
        first_kept_entry_id: "message-2".to_owned(),
        tokens_before: 1_024,
        from_extension: false,
    };
    let replacement = SessionContextReplacement::new(vec![TranscriptItem::Assistant(
        AssistantTranscriptItem::Complete {
            id: "summary-context".to_owned(),
            role: AssistantRole::Assistant,
            content: vec![AssistantContent::Text {
                text: "Summary: earlier work".to_owned(),
            }],
            model: ModelRef {
                provider: "test".to_owned(),
                id: "model-a".to_owned(),
            },
            response_model: None,
            usage: None,
            timestamp: 100,
            stop_reason: AssistantStopReason::Stop,
        },
    )]);

    session
        .apply_compaction(record.clone(), replacement)
        .expect("idle session should accept a complete compaction result");

    assert_eq!(
        session.persistence().events,
        vec![PersistenceEvent::Compaction(record)]
    );
    assert_eq!(
        session.agent_loop().calls.back(),
        Some(&"replace-context:1".to_owned())
    );
}

#[test]
fn compaction_rejects_running_or_empty_replacement_without_side_effects() {
    let mut session = test_session();
    let record = CompactionRecord {
        summary: "summary".to_owned(),
        first_kept_entry_id: "message-1".to_owned(),
        tokens_before: 1,
        from_extension: true,
    };

    let empty_error = session
        .apply_compaction(record.clone(), SessionContextReplacement::new(Vec::new()))
        .expect_err("an empty provider context is invalid");
    assert_eq!(empty_error.code, ProtocolErrorCode::InvalidRequest);
    assert!(session.persistence().events.is_empty());
    assert!(session.agent_loop().calls.is_empty());

    session
        .execute_at(
            Command::Prompt {
                session_id: "session-1".to_owned(),
                text: "run".to_owned(),
            },
            101,
        )
        .expect("prompt should start a turn");
    let running_error = session
        .apply_compaction(
            record,
            SessionContextReplacement::new(vec![TranscriptItem::User(
                protocol::UserTranscriptItem {
                    id: "user-1".to_owned(),
                    role: protocol::UserRole::User,
                    content: vec![protocol::TextOrImageContent::Text {
                        text: "retained".to_owned(),
                    }],
                    timestamp: 101,
                },
            )]),
        )
        .expect_err("a running turn must retain its current context");
    assert_eq!(running_error.code, ProtocolErrorCode::Busy);
    assert!(session.persistence().events.is_empty());
    assert_eq!(
        session.agent_loop().calls,
        VecDeque::from(["prompt:run".to_owned()])
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
    persistence
        .persist(PersistenceEvent::Compaction(CompactionRecord {
            summary: "earlier context".to_owned(),
            first_kept_entry_id: "assistant-1".to_owned(),
            tokens_before: 2_048,
            from_extension: true,
        }))
        .expect("compaction boundary should be persisted");

    let store = persistence.into_store();
    assert_eq!(store.mutations().len(), 4);
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
    assert_eq!(store.mutations()[3].payload["type"], "compaction");
    assert_eq!(store.mutations()[3].payload["summary"], "earlier context");
    assert_eq!(
        store.mutations()[3].payload["firstKeptEntryId"],
        "assistant-1"
    );
    assert_eq!(store.mutations()[3].payload["tokensBefore"], 2_048);
    assert_eq!(store.mutations()[3].payload["fromExtension"], true);
    drop(store);

    let recovered = JsonlSessionStore::open(&path).expect("written v4 entries should reopen");
    assert_eq!(recovered.mutations().len(), 4);
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
        name: None,
        cwd: "C:/workspace".to_owned(),
        model: ModelRef {
            provider: "test".to_owned(),
            id: "model-a".to_owned(),
        },
        thinking_level: ThinkingLevel::Medium,
        created_at: 100,
        retry_policy: RetryPolicy::default(),
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
                content_index: 0,
                text: "world".to_owned(),
            })
            .expect("provider delta should be accepted"),
    );
    loop_events.extend(
        session
            .agent_loop_mut()
            .handle_provider_event(ProviderEvent::Completed {
                message_id: "assistant-1".to_owned(),
                content: vec![AssistantContent::Text {
                    text: "world".to_owned(),
                }],
                response_model: Some("resolved-model".to_owned()),
                usage: Usage {
                    input: 1,
                    output: 1,
                    cache_read: 0,
                    cache_write: 0,
                    reasoning: None,
                    total_tokens: 2,
                    cost: UsageCost {
                        input: 0.0,
                        output: 0.0,
                        cache_read: 0.0,
                        cache_write: 0.0,
                        total: 0.0,
                    },
                },
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
