//! Pi Agent Rust CLI 的第一阶段入口。
//!
//! 该入口目前只验证 Rust Core 可以独立启动和处理安全的控制命令。
//! 在 Agent Loop 尚未迁移完成前，默认不接管完整用户流程，避免破坏现有
//! TypeScript CLI 的功能。后续应增加显式的 `--runtime rust|ts|auto` 选择。

use protocol::{Command, ModelRef};
use runtime::SessionRuntime;
use session::JsonlSessionStore;

fn main() {
    let mut runtime = SessionRuntime::new(
        "bootstrap-session",
        ".",
        ModelRef {
            provider: "unconfigured".to_owned(),
            id: "unconfigured".to_owned(),
        },
    );

    // 第一阶段只执行不会调用模型的控制命令，用于验证 Rust Core 的最小闭环。
    // 完整 prompt 仍由 TypeScript fallback 处理，直到 Provider Bridge 和 Agent
    // Loop 通过 conformance tests 验证完成。
    let command = Command::SetThinking {
        session_id: "bootstrap-session".to_owned(),
        thinking_level: "medium".to_owned(),
    };

    match runtime.execute(command) {
        Ok(snapshot) => println!(
            "Rust Core ready: session={} revision={}",
            snapshot.id, snapshot.revision
        ),
        Err(error) => {
            eprintln!("Rust Core error [{}]: {}", error.code, error.message);
            std::process::exit(1);
        }
    }

    // 仅验证 Session Store 可以独立创建、追加和重新打开；真实用户 Session
    // 仍由 TypeScript Host 管理，避免在迁移阶段改变现有 CLI 的存储路径。
    let demo_path = std::env::temp_dir().join("runtime-session-store-demo.jsonl");
    let _ = std::fs::remove_file(&demo_path);
    match verify_session_store(&demo_path) {
        Ok(sequence) => println!(
            "Session Store ready: path={} seq={sequence}",
            demo_path.display()
        ),
        Err(error) => {
            eprintln!("Session Store error: {error}");
            std::process::exit(1);
        }
    }
}

/// 以真实 v4 entry 验证 Store 的创建、追加及恢复路径。
///
/// 这是迁移阶段的自检，不访问用户目录，也不接管 TypeScript 的真实 Session。
fn verify_session_store(path: &std::path::Path) -> Result<u64, session::SessionStoreError> {
    let _ = std::fs::remove_file(path);
    let mut store = JsonlSessionStore::create(path, "bootstrap-session", ".")?;
    let mut entry = serde_json::Map::new();
    entry.insert(
        "id".into(),
        serde_json::Value::String("bootstrap-entry".into()),
    );
    entry.insert("type".into(), serde_json::Value::String("message".into()));
    entry.insert("parentId".into(), serde_json::Value::Null);
    entry.insert("timestamp".into(), serde_json::Value::from(0));
    entry.insert("lane".into(), serde_json::Value::String("main".into()));
    let mutation = store.append("entry", entry)?;
    drop(store);
    JsonlSessionStore::open(path)?;
    let _ = std::fs::remove_file(path);
    Ok(mutation.seq)
}
