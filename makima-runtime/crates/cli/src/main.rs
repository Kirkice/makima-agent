//! Pi Agent Rust Core 的 stdio RPC 入口。
//!
//! stdin/stdout 专用于 framed-CBOR RPC。Provider Host 的凭证和诊断仍由其子进程的
//! 环境与 stderr 管理；本进程不会向 stdout 写入任何非协议文本。

use std::sync::{Arc, Mutex};

use protocol::ModelRef;
use runtime::{
    listener::serve_stdio,
    session_manager::{AgentSessionFactory, ConnectionSessionHandler, SessionManager},
};

fn main() {
    let session_root = std::env::var_os("PI_RUNTIME_SESSION_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(".pi").join("sessions"));
    let default_model = ModelRef {
        provider: std::env::var("PI_MODEL_PROVIDER").unwrap_or_else(|_| "unconfigured".to_owned()),
        id: std::env::var("PI_MODEL_ID").unwrap_or_else(|_| "unconfigured".to_owned()),
    };
    let factory = match AgentSessionFactory::new(session_root) {
        Ok(factory) => factory,
        Err(error) => {
            eprintln!(
                "Rust Core 初始化 Session Store 失败 [{}]: {}",
                error.code, error.message
            );
            std::process::exit(1);
        }
    };
    let manager = Arc::new(Mutex::new(SessionManager::new(
        "rust-core",
        ".",
        default_model,
        Vec::new(),
        factory,
    )));
    let handler = ConnectionSessionHandler::new("stdio", manager, unix_millis);

    if let Err(error) = serve_stdio("stdio", handler) {
        eprintln!("Rust Core RPC 失败: {error}");
        std::process::exit(1);
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}
