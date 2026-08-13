//! 可独立删除的 JSONL v4 跨语言兼容性测试。
//!
//! 固定 fixture 按 TypeScript `encodeHeader` / `encodeMutation` 的平铺 JSONL
//! 形状编写。本文件不依赖 runtime 代码，迁移完成后可整体移除。

use serde_json::{Map, Value, json};
use session::{
    BranchBounds, EntryQuery, JsonlSessionStore, LogOptions, NewEntry, NewRecord, RecordQuery,
};
use std::fs;
use std::path::PathBuf;

const TYPESCRIPT_V4_FIXTURE: &str = include_str!("fixtures/typescript-v4-session.jsonl");

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
    assert_eq!(branch.len(), 2);
    assert_eq!(branch[0].get("id"), Some(&json!("root")));
    assert_eq!(branch[1].get("id"), Some(&json!("reply")));

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
    assert!(concurrent_operation.is_err());
    store
        .append_record(NewRecord {
            lane: "main".into(),
            fields: fields(json!({"id":"finished-1","lane":"main","type":"operation_finished","runId":"run-1"})),
        })
        .unwrap();
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
    fs::remove_file(path).unwrap();
}
