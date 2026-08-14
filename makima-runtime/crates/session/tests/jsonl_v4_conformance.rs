//! 可独立删除的 JSONL v4 跨语言兼容性测试。
//!
//! 固定 fixture 按 TypeScript `encodeHeader` / `encodeMutation` 的平铺 JSONL
//! 形状编写。本文件不依赖 runtime 代码，迁移完成后可整体移除。

use serde_json::{Map, Value, json};
use session::{
    BranchBounds, EntryQuery, JsonlSessionStore, LogOptions, NewEntry, NewRecord, RecordQuery,
    SessionErrorCode,
};
use std::fs;
use std::path::PathBuf;

const TYPESCRIPT_V4_FIXTURE: &str = include_str!("fixtures/typescript-v4-session.jsonl");

// 此样本采用 Rust Store 写出的 JSONL v4 平铺字段形状。TypeScript 的 codec
// 测试会逐行读取同一文件，因此它是两端都必须长期兼容的稳定格式契约。
const RUST_V4_FIXTURE: &str = include_str!("fixtures/rust-v4-session.jsonl");

fn temporary_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "session-conformance-{name}-{}.jsonl",
        std::process::id()
    ))
}

fn copy_fixture(name: &str) -> PathBuf {
    let path = temporary_path(name);
    let _ = fs::remove_file(&path);
    fs::write(&path, TYPESCRIPT_V4_FIXTURE).unwrap();
    path
}

fn fields(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn reads_typescript_v4_fixture_and_preserves_state_projection() {
    let path = copy_fixture("typescript-read");
    let store = JsonlSessionStore::open(&path).unwrap();

    assert_eq!(store.header().id, "ts-fixture-session");
    assert_eq!(
        store.header().parent_session_id.as_deref(),
        Some("parent-session")
    );
    assert_eq!(store.state().name(), Some("TypeScript fixture"));
    assert_eq!(store.state().label("reply"), Some("keep"));
    assert_eq!(
        store.state().get_log(LogOptions::default()).unwrap().len(),
        7
    );
    assert!(
        store
            .state()
            .find_open_operations("main", None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.state().stats().cached_tokens, 3);
    assert_eq!(store.state().stats().uncached_tokens, 7);
    assert_eq!(store.state().stats().total_tokens, 10);
    assert_eq!(store.state().stats().cost_total, 0.25);

    let branch = store
        .state()
        .find_entries_on_branch_with_bounds(
            "reply",
            EntryQuery {
                oldest_first: true,
                ..Default::default()
            },
            BranchBounds {
                stop_at_id: Some("root"),
                ..Default::default()
            },
        )
        .unwrap();
    // TypeScript 的 oldest-first 查询先反转完整 branch，再从 root 向 leaf
    // 扫描；因此 stopAt=root 会在第一个结果后停止，而不会继续包含 reply。
    assert_eq!(branch.len(), 1);
    assert_eq!(branch[0].get("id"), Some(&json!("root")));

    let bounded_by_type = store
        .state()
        .find_entries_on_branch_with_bounds(
            "reply",
            EntryQuery::default(),
            BranchBounds {
                stop_at_type: Some("message"),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(bounded_by_type.len(), 1);
    assert_eq!(bounded_by_type[0].get("id"), Some(&json!("reply")));

    let log = store
        .state()
        .get_log(LogOptions {
            after_sequence: Some(4),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        log.iter().map(|mutation| mutation.seq).collect::<Vec<_>>(),
        vec![5, 6, 7]
    );

    let usage = store
        .state()
        .find_records(RecordQuery {
            record_type: Some("usage"),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(usage.len(), 1);
    fs::remove_file(path).unwrap();
}

#[test]
fn reads_rust_v4_fixture_with_typescript_codec_shapes_and_query_projection() {
    let path = temporary_path("rust-fixture-read");
    let _ = fs::remove_file(&path);
    fs::write(&path, RUST_V4_FIXTURE).unwrap();

    let store = JsonlSessionStore::open(&path).unwrap();
    assert_eq!(
        store.header().legacy_parent_session_path.as_deref(),
        Some("C:/sessions/legacy-parent.jsonl")
    );
    assert_eq!(
        store
            .header()
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("source")),
        Some(&json!("rust-fixture"))
    );
    assert_eq!(store.state().lane_leaf("review"), Some(Some("root")));
    assert_eq!(store.state().name(), Some("Rust fixture"));
    assert_eq!(store.state().label("reply"), Some("keep"));

    // 默认查询按最新优先；此断言覆盖 TypeScript `findEntries()` 的默认排序。
    let entries = store.state().find_entries(EntryQuery::default()).unwrap();
    assert_eq!(entries[0].get("id"), Some(&json!("reply")));
    let records = store
        .state()
        .find_records(RecordQuery {
            record_type: Some("usage"),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(records[0].get("id"), Some(&json!("usage-1")));
    assert_eq!(store.state().stats().total_tokens, 10);

    fs::remove_file(path).unwrap();
}

#[test]
fn round_trips_typescript_record_variants_and_usage_projection() {
    let path = temporary_path("record-variants");
    let _ = fs::remove_file(&path);
    let mut store =
        JsonlSessionStore::create(&path, "record-variants", "C:/workspace/demo").unwrap();

    // record 的扩展字段由 Provider 与 Extension 定义。此处刻意保留 TypeScript
    // storage 覆盖的全部 discriminant，验证 Rust 不会在持久化或恢复时丢失未知字段。
    let records = vec![
        json!({"id":"run","lane":"main","type":"operation_started","sourceLeafId":"anchor","intent":{"kind":"run","resumeData":{"extension":{"version":1}}}}),
        json!({"id":"steer","lane":"main","type":"queue_enqueued","queue":"steer","runId":"run","target":{"type":"message","id":"steer-message"}}),
        json!({"id":"follow-up","lane":"main","type":"queue_enqueued","queue":"followUp","runId":"run","target":{"type":"message","id":"follow-up-message"}}),
        json!({"id":"assistant-attempt","lane":"main","type":"step_attempt","runId":"run","step":"assistant","attempt":1,"resultEntryId":"assistant-result"}),
        json!({"id":"tool","lane":"main","type":"tool_started","runId":"run","assistantEntryId":"assistant-result","toolIndex":0,"toolCallId":"call-1","toolName":"read","effectiveArgs":{"path":"README.md"},"resultEntryId":"tool-result","replay":"safe"}),
        json!({"id":"deferred-write","lane":"main","type":"write_deferred","runId":"run","target":{"type":"custom","id":"deferred-entry","customType":"fact","data":{"value":true}}}),
        json!({"id":"assistant-usage","lane":"main","type":"usage","cause":"assistant","runId":"run","entryId":"assistant-result","attempt":1,"stopReason":"stop","usage":{"cacheRead":1,"input":1,"cacheWrite":1,"totalTokens":3,"cost":{"total":1.0}}}),
        json!({"id":"deferred-usage","lane":"main","type":"usage","cause":"deferred_fetch","runId":"run","entryId":"deferred-result","attempt":1,"stopReason":"deferred","usage":{"cacheRead":2,"input":2,"cacheWrite":2,"totalTokens":6,"cost":{"total":2.0}}}),
        json!({"id":"tool-usage","lane":"main","type":"usage","cause":"tool","runId":"run","entryId":"tool-result","toolCallId":"call-1","usage":{"cacheRead":3,"input":3,"cacheWrite":3,"totalTokens":9,"cost":{"total":3.0}}}),
        json!({"id":"hook-usage","lane":"main","type":"usage","cause":"hook","runId":"run","entryId":"hook-result","usage":{"cacheRead":4,"input":4,"cacheWrite":4,"totalTokens":12,"cost":{"total":4.0}}}),
        json!({"id":"adjustment","lane":"main","type":"usage","cause":"adjustment","details":{"reason":"correction"},"usage":{"cacheRead":5,"input":5,"cacheWrite":5,"totalTokens":15,"cost":{"total":5.0}}}),
        json!({"id":"abort","lane":"main","type":"abort_requested","runId":"run"}),
        json!({"id":"run-finished","lane":"main","type":"operation_finished","runId":"run","outcome":"aborted"}),
        json!({"id":"next-run","lane":"main","type":"queue_enqueued","queue":"nextRun","target":{"type":"message","id":"next-message"}}),
        json!({"id":"queue-cancelled","lane":"main","type":"queue_cancelled","entryId":"next-message"}),
        json!({"id":"compaction","lane":"main","type":"operation_started","sourceLeafId":"anchor","intent":{"kind":"compaction","customInstructions":"short","resultEntryId":"compaction-result"}}),
        json!({"id":"compaction-attempt","lane":"main","type":"step_attempt","runId":"compaction","step":"compaction","attempt":1,"resultEntryId":"compaction-result","compactionReason":"manual"}),
        json!({"id":"compaction-finished","lane":"main","type":"operation_finished","runId":"compaction","outcome":"completed"}),
        json!({"id":"navigation","lane":"main","type":"operation_started","sourceLeafId":"anchor","intent":{"kind":"navigation","targetId":null,"summarize":true,"customInstructions":"summarize","label":"checkpoint","summaryEntryId":"navigation-summary"}}),
        json!({"id":"branch-attempt","lane":"main","type":"step_attempt","runId":"navigation","step":"branch_summary","attempt":1,"resultEntryId":"navigation-summary"}),
    ];
    for record in records {
        store
            .append_record(NewRecord {
                lane: "main".into(),
                fields: fields(record),
            })
            .unwrap();
    }
    drop(store);

    let restored = JsonlSessionStore::open(&path).unwrap();
    let all_records = restored
        .state()
        .find_records(RecordQuery {
            oldest_first: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(all_records.len(), 20);
    assert_eq!(all_records[0].get("id"), Some(&json!("run")));
    assert_eq!(all_records[19].get("id"), Some(&json!("branch-attempt")));
    assert_eq!(
        restored
            .state()
            .find_records(RecordQuery {
                record_type: Some("operation_started"),
                operation_kind: Some("run"),
                ..Default::default()
            })
            .unwrap()[0]
            .get("id"),
        Some(&json!("run"))
    );
    assert_eq!(
        restored
            .state()
            .find_records(RecordQuery {
                run_id: Some("compaction"),
                oldest_first: true,
                ..Default::default()
            })
            .unwrap()
            .iter()
            .map(|record| record.get("id"))
            .collect::<Vec<_>>(),
        vec![
            Some(&json!("compaction")),
            Some(&json!("compaction-attempt")),
            Some(&json!("compaction-finished")),
        ]
    );
    assert_eq!(
        restored
            .state()
            .find_open_operations("main", None)
            .unwrap()
            .iter()
            .map(|record| record.get("id"))
            .collect::<Vec<_>>(),
        vec![Some(&json!("navigation"))]
    );
    assert_eq!(restored.state().stats().cached_tokens, 15);
    assert_eq!(restored.state().stats().uncached_tokens, 30);
    assert_eq!(restored.state().stats().total_tokens, 45);
    assert_eq!(restored.state().stats().cost_total, 15.0);
    drop(restored);
    fs::remove_file(path).unwrap();
}

#[test]
fn writes_typescript_v4_flattened_mutations_with_provisioned_fields() {
    let path = temporary_path("rust-write");
    let _ = fs::remove_file(&path);
    let mut store =
        JsonlSessionStore::create(&path, "rust-fixture-session", "C:/workspace/demo").unwrap();

    let root = store
        .append_entry(NewEntry {
            lane: "main".into(),
            fields: fields(
                json!({"id":"root","type":"message","message":{"role":"user","content":"hello"}}),
            ),
        })
        .unwrap();
    assert_eq!(root.get("parentId"), Some(&Value::Null));
    assert_eq!(root.get("seq"), Some(&json!(1)));
    assert!(root.get("timestamp").and_then(Value::as_u64).is_some());

    let operation = store
        .append_record(NewRecord {
            lane: "main".into(),
            fields: fields(json!({"id":"run-1","lane":"main","type":"operation_started","intent":{"kind":"run"}})),
        })
        .unwrap();
    assert_eq!(operation.get("seq"), Some(&json!(2)));
    assert!(operation.get("timestamp").and_then(Value::as_u64).is_some());
    let concurrent_operation = store.append_record(NewRecord {
        lane: "main".into(),
        fields: fields(
            json!({"id":"run-2","lane":"main","type":"operation_started","intent":{"kind":"run"}}),
        ),
    });
    // 与 TypeScript `JsonlSessionStorage.appendRecord()` 一致：并行 operation
    // 是持久化写入边界的 storage 错误，拒绝后不得污染内存投影或 JSONL 前缀。
    assert_eq!(
        concurrent_operation.unwrap_err().code(),
        SessionErrorCode::Storage
    );
    assert_eq!(
        store
            .state()
            .get_log(LogOptions::default())
            .unwrap()
            .iter()
            .map(|mutation| mutation.seq)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );

    let finished = store
        .append_record(NewRecord {
            lane: "main".into(),
            fields: fields(json!({"id":"finished-1","lane":"main","type":"operation_finished","runId":"run-1"})),
        })
        .unwrap();
    assert_eq!(finished.get("seq"), Some(&json!(3)));
    // 前一个 operation 完成后必须可以继续启动下一次运行，证明失败没有使写入队列
    // 或 open-operation 索引进入不可恢复状态。
    let next_operation = store
        .append_record(NewRecord {
            lane: "main".into(),
            fields: fields(
                json!({"id":"run-2","lane":"main","type":"operation_started","intent":{"kind":"run"}}),
            ),
        })
        .unwrap();
    assert_eq!(next_operation.get("seq"), Some(&json!(4)));
    drop(store);

    let lines: Vec<Value> = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(lines[0].get("createdAt").and_then(Value::as_u64).is_some());
    assert_eq!(lines[1].get("kind"), Some(&json!("entry")));
    assert_eq!(lines[1].get("payload"), None);
    assert_eq!(lines[1].get("parentId"), Some(&Value::Null));
    assert_eq!(lines[1].get("seq"), Some(&json!(1)));
    assert_eq!(lines[2].get("kind"), Some(&json!("record")));
    assert_eq!(lines[2].get("payload"), None);
    assert_eq!(lines[2].get("lane"), Some(&json!("main")));
    assert_eq!(lines[2].get("seq"), Some(&json!(2)));
    // 被拒绝的 run-2 不会占用 seq；完成 run-1 后的下一次启动连续写为 seq=4。
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[3].get("seq"), Some(&json!(3)));
    assert_eq!(lines[4].get("seq"), Some(&json!(4)));
    fs::remove_file(path).unwrap();
}
