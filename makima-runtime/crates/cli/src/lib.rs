//! Rust 产品入口的可测试编排层。
//!
//! `main` 只把进程退出码交给操作系统；这里负责验证资源、建立 Session Manager，并保证
//! RPC stdout 不混入诊断。Provider Host 的具体 child 管理仍封装在 runtime crate，CLI 只传递
//! 已解析、绝对化的启动配置。

pub mod contract;

use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::Path,
    sync::{Arc, Mutex},
};

use protocol::ModelRef;
use runtime::{
    listener::serve_stdio,
    session_manager::{AgentSessionFactory, ConnectionSessionHandler, SessionManager},
};

use crate::contract::{AppMode, CliArgs, ExitCode};

/// 执行已验证的产品配置，并返回可由 `main` 直接使用的退出码。
pub fn run(config: CliArgs) -> Result<(), (ExitCode, String)> {
    validate_bootstrap_resources(&config)?;

    match config.mode {
        AppMode::Rpc => run_rpc(config),
        AppMode::Print | AppMode::Json => Err((
            ExitCode::Usage,
            "Rust print/JSON renderer requires the M3 Agent Loop output adapter; use --mode rpc or the TypeScript runtime during M2.".to_owned(),
        )),
        AppMode::Interactive => Err((
            ExitCode::Usage,
            "Rust interactive mode requires the M4 TUI Adapter; use --mode rpc or select the TypeScript runtime during M2.".to_owned(),
        )),
    }
}

fn validate_bootstrap_resources(config: &CliArgs) -> Result<(), (ExitCode, String)> {
    if !config.cwd.is_dir() {
        return Err((
            ExitCode::Bootstrap,
            format!(
                "Rust runtime cwd does not exist or is not a directory: {}",
                config.cwd.display()
            ),
        ));
    }
    if !config.provider_host_entry.is_file() {
        return Err((
            ExitCode::Bootstrap,
            format!(
                "Rust runtime Provider Host entry does not exist or is not a file: {}",
                config.provider_host_entry.display()
            ),
        ));
    }
    Ok(())
}

fn run_rpc(config: CliArgs) -> Result<(), (ExitCode, String)> {
    // Node 以 `<node> <entry>` 启动；Bun archive 则以
    // `<pi> --provider-host-child <entry>` 进入内嵌 adapter。两种执行器都保留 manifest entry
    // 作为最后一个参数，使 Rust 侧的资源校验边界与 npm sidecar 保持一致。
    let mut provider_host_args = config
        .provider_host_args
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<OsString>>();
    provider_host_args.push(config.provider_host_entry.into_os_string());
    let factory = AgentSessionFactory::new_with_isolated_launch_config(
        config.session_root,
        config.provider_host_program,
        provider_host_args,
        Some(provider_host_environment()),
        config.system_prompt,
    )
    .map_err(|error| {
        (
            ExitCode::Session,
            format!(
                "Rust Core failed to initialize Session Store [{}]: {}",
                error.code, error.message
            ),
        )
    })?;
    let manager = Arc::new(Mutex::new(SessionManager::new(
        "rust-core",
        path_to_protocol_cwd(&config.cwd),
        ModelRef {
            provider: config.model_provider,
            id: config.model_id,
        },
        Vec::new(),
        factory,
    )));
    let handler = ConnectionSessionHandler::new("stdio", manager, unix_millis);

    serve_stdio("stdio", handler).map_err(|error| {
        (
            ExitCode::ProviderHost,
            format!("Rust Core RPC failed: {error}"),
        )
    })
}

/// 将 Provider Host 需要的系统、网络与凭据变量显式传给 child。
///
/// 不继承完整父环境可避免终端注入的无关变量改变 Host 行为；以常见 Provider 前缀保留凭据，
/// 同时保留跨平台 Node 运行所需的系统目录、临时目录、证书和代理配置。新增 Provider 时应在此
/// 列表中补充前缀，而不是退回到 `Command` 的隐式环境继承。
fn provider_host_environment() -> BTreeMap<OsString, OsString> {
    const EXACT_NAMES: &[&str] = &[
        "HOME",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "APPDATA",
        "LOCALAPPDATA",
        "TEMP",
        "TMP",
        "TMPDIR",
        "SystemRoot",
        "WINDIR",
        "COMSPEC",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "NODE_EXTRA_CA_CERTS",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "no_proxy",
    ];
    const PREFIXES: &[&str] = &[
        "ANTHROPIC_",
        "OPENAI_",
        "GOOGLE_",
        "GEMINI_",
        "AWS_",
        "AZURE_",
        "BEDROCK_",
        "OLLAMA_",
        "MISTRAL_",
        "GROQ_",
        "CEREBRAS_",
        "XAI_",
        "TOGETHER_",
        "OPENROUTER_",
        "PI_",
    ];

    std::env::vars_os()
        .filter(|(name, _)| {
            let name = name.to_string_lossy();
            EXACT_NAMES
                .iter()
                .any(|allowed| name.eq_ignore_ascii_case(allowed))
                || PREFIXES.iter().any(|prefix| name.starts_with(prefix))
        })
        .collect()
}

fn path_to_protocol_cwd(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::run;
    use crate::contract::{AppMode, CliArgs, ExitCode};

    fn config(mode: AppMode) -> CliArgs {
        CliArgs {
            mode,
            cwd: std::env::current_dir().expect("workspace cwd"),
            session_root: PathBuf::from("target/test-sessions"),
            provider_host_program: "node".to_owned(),
            provider_host_args: Vec::new(),
            provider_host_entry: PathBuf::from("missing-host.js"),
            model_provider: "test".to_owned(),
            model_id: "model".to_owned(),
            system_prompt: String::new(),
        }
    }

    #[test]
    fn reports_missing_payload_as_bootstrap_error_before_starting_a_child() {
        let error = run(config(AppMode::Rpc)).expect_err("missing host payload must fail");
        assert_eq!(error.0, ExitCode::Bootstrap);
    }

    #[test]
    fn refuses_modes_whose_rendering_adapter_is_not_yet_migrated() {
        let mut interactive = config(AppMode::Interactive);
        interactive.provider_host_entry = std::env::current_exe().expect("test executable");
        let error = run(interactive).expect_err("interactive adapter is an M4 responsibility");
        assert_eq!(error.0, ExitCode::Usage);
    }
}
