//! Pi Agent Rust CLI 的第一阶段入口。
//!
//! 该入口目前只验证 Rust Core 可以独立启动和处理安全的控制命令。
//! 在 Agent Loop 尚未迁移完成前，默认不接管完整用户流程，避免破坏现有
//! TypeScript CLI 的功能。后续应增加显式的 `--runtime rust|ts|auto` 选择。

use pi_core::SessionRuntime;
use pi_protocol::{Command, ModelRef};

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
}
