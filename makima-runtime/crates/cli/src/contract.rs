//! Rust 产品 CLI 的稳定输入与退出码契约。
//!
//! 本模块不创建 Session、Provider Host 或 RPC listener。将参数解析结果与运行时副作用
//! 分离后，Node/Bun 启动器、集成测试和未来的 TUI Adapter 可以共享同一份产品边界，避免
//! 把环境变量读取、stdout 约束和业务初始化耦合在 `main` 函数中。

use std::path::PathBuf;

/// 当前 Rust 产品入口的版本。
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Rust 产品入口支持的输出模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// stdout 只能写 framed-CBOR RPC 消息。
    Rpc,
    /// 非交互的最终文本输出。完整 Agent 行为由后续 M3 接管。
    Print,
    /// 非交互 JSONL 输出。完整事件投影由后续 M3 接管。
    Json,
    /// M2 仅声明该模式需要 TypeScript TUI Adapter，不能静默降级。
    Interactive,
}

/// 进程边界使用的稳定退出码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    /// 正常结束。
    Success = 0,
    /// 参数或当前入口未支持的能力。
    Usage = 2,
    /// sidecar、manifest、cwd 或 Session Root 的启动错误。
    Bootstrap = 70,
    /// Provider Host 的启动、管道或生命周期错误。
    ProviderHost = 71,
    /// Session 持久化边界错误。
    Session = 72,
}

/// 启动 Rust Core 所需的已规范化配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliArgs {
    pub mode: AppMode,
    pub cwd: PathBuf,
    pub session_root: PathBuf,
    pub provider_host_program: String,
    /// 在 Provider Host entry 之前传给 child 的受控参数。
    ///
    /// Node sidecar 不需要该字段；Bun 编译二进制则以内部标记进入内嵌的 Provider Host 启动器。
    /// 保留 entry 的独立字段可继续校验 manifest 中声明的 JavaScript payload，即使 Bun 路径不
    /// 直接以文件解释器执行它。
    pub provider_host_args: Vec<String>,
    pub provider_host_entry: PathBuf,
    pub model_provider: String,
    pub model_id: String,
    pub system_prompt: String,
}

/// 参数解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseOutcome {
    Run(CliArgs),
    Help,
    Version,
}

/// 参数错误在真正启动任何 child 前返回，保证 native 入口不会部分执行后再降级。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliUsageError {
    pub message: String,
}

impl CliUsageError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// 解析 native sidecar 的最小产品参数。
///
/// 参数刻意只覆盖 Rust 已能兑现的行为。未知参数直接失败，防止启动器把本应由 TypeScript
/// extension 消费的参数错误地交给 Rust 后被忽略。`--provider-host-entry` 是显式资源定位
/// 边界；运行时不从仓库 cwd 或 `PATH` 猜测 Host 脚本的位置。
pub fn parse_args<I, S>(args: I, current_dir: PathBuf) -> Result<ParseOutcome, CliUsageError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut mode = AppMode::Interactive;
    let mut cwd = current_dir;
    let mut session_root = None;
    let mut provider_host_program = "node".to_owned();
    let mut provider_host_args = Vec::new();
    let mut provider_host_entry = None;
    let mut model_provider = "unconfigured".to_owned();
    let mut model_id = "unconfigured".to_owned();
    let mut system_prompt = String::new();
    let values = args.into_iter().map(Into::into).collect::<Vec<String>>();
    let mut index = 0;

    while index < values.len() {
        let argument = &values[index];
        if argument == "--help" || argument == "-h" {
            return Ok(ParseOutcome::Help);
        }
        if argument == "--version" || argument == "-v" {
            return Ok(ParseOutcome::Version);
        }

        let (name, inline_value) = argument
            .split_once('=')
            .map_or((argument.as_str(), None), |(name, value)| {
                (name, Some(value))
            });
        let mut next_value = || -> Result<String, CliUsageError> {
            if let Some(value) = inline_value {
                return Ok(value.to_owned());
            }
            index += 1;
            values
                .get(index)
                .cloned()
                .ok_or_else(|| CliUsageError::new(format!("{name} requires a value")))
        };

        match name {
            "--mode" => {
                mode = match next_value()?.as_str() {
                    "rpc" => AppMode::Rpc,
                    "text" | "print" => AppMode::Print,
                    "json" => AppMode::Json,
                    "interactive" => AppMode::Interactive,
                    value => {
                        return Err(CliUsageError::new(format!(
                            "invalid --mode value {value:?}; expected rpc, text, json, or interactive"
                        )));
                    }
                };
            }
            "--cwd" => cwd = absolute_path(next_value()?, &cwd),
            "--session-root" => session_root = Some(absolute_path(next_value()?, &cwd)),
            "--provider-host-program" => provider_host_program = next_value()?,
            "--provider-host-arg" => provider_host_args.push(next_value()?),
            "--provider-host-entry" => {
                provider_host_entry = Some(absolute_path(next_value()?, &cwd))
            }
            "--provider" => model_provider = next_value()?,
            "--model" => model_id = next_value()?,
            "--system-prompt" => system_prompt = next_value()?,
            _ => {
                return Err(CliUsageError::new(format!(
                    "unknown native runtime option: {argument}"
                )));
            }
        }
        index += 1;
    }

    let provider_host_entry = provider_host_entry.ok_or_else(|| {
        CliUsageError::new(
            "--provider-host-entry is required when starting the Rust product runtime",
        )
    })?;
    let session_root = session_root.unwrap_or_else(|| cwd.join(".pi").join("sessions"));

    Ok(ParseOutcome::Run(CliArgs {
        mode,
        cwd,
        session_root,
        provider_host_program,
        provider_host_args,
        provider_host_entry,
        model_provider,
        model_id,
        system_prompt,
    }))
}

fn absolute_path(value: String, base: &std::path::Path) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

/// 仅输出 Rust sidecar 可实现的参数，避免产品帮助暗示 M3/M4 能力已经迁移完成。
pub fn help_text() -> &'static str {
    "Pi Rust runtime\n\nUsage:\n  pi-runtime [options]\n\nOptions:\n  --mode <rpc|text|json|interactive>  Product mode (default: interactive)\n  --cwd <path>                         Session working directory\n  --session-root <path>                Rust JSONL session directory\n  --provider-host-program <path>       Provider Host executable\n  --provider-host-arg <value>          Controlled argument before Provider Host entry\n  --provider-host-entry <path>         Built Provider Host entry\n  --provider <name>                    Initial model provider\n  --model <id>                         Initial model ID\n  --system-prompt <text>               Provider system prompt\n  --help, -h                           Show this help\n  --version, -v                        Show Rust runtime version\n"
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{AppMode, ParseOutcome, parse_args};

    #[test]
    fn parses_rpc_config_and_absolutizes_resources_from_the_requested_cwd() {
        let parsed = parse_args(
            [
                "--mode=rpc",
                "--cwd",
                "workspace",
                "--provider-host-entry",
                "host/main.js",
                "--provider-host-arg",
                "--provider-host-child",
                "--session-root=.sessions",
            ],
            PathBuf::from("root"),
        )
        .expect("arguments should parse");

        let config = match parsed {
            ParseOutcome::Run(config) => config,
            ParseOutcome::Help | ParseOutcome::Version => panic!("expected run configuration"),
        };
        assert_eq!(config.mode, AppMode::Rpc);
        assert_eq!(config.cwd, PathBuf::from("root/workspace"));
        assert_eq!(config.provider_host_args, ["--provider-host-child"]);
        assert_eq!(
            config.provider_host_entry,
            PathBuf::from("root/workspace/host/main.js")
        );
        assert_eq!(
            config.session_root,
            PathBuf::from("root/workspace/.sessions")
        );
    }

    #[test]
    fn rejects_unknown_or_missing_product_arguments_before_bootstrap() {
        let missing = parse_args(["--mode", "rpc"], PathBuf::from("root"));
        assert!(missing.is_err());
        let unknown = parse_args(
            ["--provider-host-entry", "host.js", "--extension-flag"],
            PathBuf::from("root"),
        );
        assert!(unknown.is_err());
    }
}
