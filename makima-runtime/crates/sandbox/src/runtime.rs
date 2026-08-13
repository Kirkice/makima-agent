//! Sandbox 运行时生命周期与 TypeScript `srt` 后端适配。
//!
//! 现有 TypeScript Extension 通过 `@anthropic-ai/sandbox-runtime` 的 `srt` CLI 在
//! Linux/macOS 上执行隔离命令。本模块保留同一配置格式和生命周期：初始化时写出
//! runtime 配置、命令执行时包装为 `srt --settings <file> -c <command>`，关闭时删除
//! 临时配置。Windows 与 TypeScript Extension 一样明确禁用，不回退为伪隔离。

use crate::SandboxConfig;
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 当前宿主平台的 Sandbox 支持状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxPlatform {
    Linux,
    MacOs,
    Windows,
    Other,
}

impl SandboxPlatform {
    /// 返回编译目标所属的平台，而不是运行时猜测宿主名称。
    pub const fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Other
        }
    }

    /// TypeScript Sandbox Extension 仅支持 Linux 和 macOS。
    pub const fn supports_os_isolation(self) -> bool {
        matches!(self, Self::Linux | Self::MacOs)
    }
}

impl fmt::Display for SandboxPlatform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Linux => "linux",
            Self::MacOs => "darwin",
            Self::Windows => "win32",
            Self::Other => "unsupported platform",
        })
    }
}

/// 初始化后 Sandbox 是否可用于包装命令的明确状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxRuntimeStatus {
    /// 用户通过等价于 `--no-sandbox` 的宿主选项关闭隔离。
    DisabledByFlag,
    /// 配置文件的 `enabled: false` 关闭隔离。
    DisabledByConfig,
    /// 运行平台不支持 OS 级隔离；Windows 必定进入此状态。
    UnsupportedPlatform(SandboxPlatform),
    /// 当前平台支持隔离，但找不到 `srt` runtime 后端。
    BackendUnavailable { program: PathBuf },
    /// 初始化成功，可以生成隔离命令。
    Enabled,
}

/// 后端命令规格。调用方应以 `program` 和 `args` 直接启动进程，不能重新拼接为 shell
/// 字符串，避免绕过参数边界。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrappedCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// 与 `srt` CLI 对齐的 Sandbox 生命周期管理器。
#[derive(Debug)]
pub struct SandboxRuntime {
    status: SandboxRuntimeStatus,
    backend_program: Option<PathBuf>,
    settings_path: Option<PathBuf>,
}

impl SandboxRuntime {
    /// 根据配置、禁用 flag 和当前平台初始化 Sandbox。
    ///
    /// `backend_program` 可指定已随应用分发的 `srt` 可执行文件；省略时仅在 PATH 中
    /// 查找 `srt`。这样 Rust 宿主不会隐式依赖开发工作区的 `node_modules` 路径。
    pub fn initialize(
        config: &SandboxConfig,
        no_sandbox: bool,
        backend_program: Option<PathBuf>,
    ) -> Result<Self, SandboxRuntimeError> {
        Self::initialize_for_platform(config, no_sandbox, SandboxPlatform::current(), backend_program)
    }

    /// 为测试及跨平台宿主显式指定平台。
    pub fn initialize_for_platform(
        config: &SandboxConfig,
        no_sandbox: bool,
        platform: SandboxPlatform,
        backend_program: Option<PathBuf>,
    ) -> Result<Self, SandboxRuntimeError> {
        if no_sandbox {
            return Ok(Self::disabled(SandboxRuntimeStatus::DisabledByFlag));
        }
        if !config.enabled {
            return Ok(Self::disabled(SandboxRuntimeStatus::DisabledByConfig));
        }
        if !platform.supports_os_isolation() {
            return Ok(Self::disabled(SandboxRuntimeStatus::UnsupportedPlatform(platform)));
        }

        let backend_program = backend_program.unwrap_or_else(|| PathBuf::from("srt"));
        if !program_is_available(&backend_program) {
            return Ok(Self::disabled(SandboxRuntimeStatus::BackendUnavailable { program: backend_program }));
        }

        let settings_path = write_runtime_config(config)?;
        Ok(Self { status: SandboxRuntimeStatus::Enabled, backend_program: Some(backend_program), settings_path: Some(settings_path) })
    }

    fn disabled(status: SandboxRuntimeStatus) -> Self {
        Self { status, backend_program: None, settings_path: None }
    }

    /// 返回初始化结果，供 CLI/TUI 显示与决定是否回退到普通 Bash 工具。
    pub fn status(&self) -> &SandboxRuntimeStatus {
        &self.status
    }

    /// 按 TypeScript `SandboxManager.wrapWithSandbox` 的 CLI 等价形式包装命令。
    pub fn wrap_command(&self, command: impl Into<String>) -> Result<WrappedCommand, SandboxRuntimeError> {
        let backend_program = self.backend_program.as_ref().ok_or_else(|| SandboxRuntimeError::NotEnabled(self.status.clone()))?;
        let settings_path = self.settings_path.as_ref().ok_or_else(|| SandboxRuntimeError::NotEnabled(self.status.clone()))?;
        Ok(WrappedCommand {
            program: backend_program.clone(),
            args: vec!["--settings".to_owned(), settings_path.display().to_string(), "-c".to_owned(), command.into()],
        })
    }

    /// 删除初始化期生成的配置文件。清理失败被作为错误返回，便于宿主记录诊断；调用方
    /// 若要复刻 TypeScript Extension 的 shutdown 行为，可以选择忽略该错误。
    pub fn reset(&mut self) -> Result<(), SandboxRuntimeError> {
        if let Some(path) = self.settings_path.take()
            && path.exists()
        {
            fs::remove_file(&path).map_err(|source| SandboxRuntimeError::Cleanup { path, source })?;
        }
        self.backend_program = None;
        if matches!(self.status, SandboxRuntimeStatus::Enabled) {
            self.status = SandboxRuntimeStatus::DisabledByConfig;
        }
        Ok(())
    }
}

impl Drop for SandboxRuntime {
    fn drop(&mut self) {
        let _ = self.reset();
    }
}

/// 生命周期或后端配置期间发生的错误。
#[derive(Debug)]
pub enum SandboxRuntimeError {
    NotEnabled(SandboxRuntimeStatus),
    SerializeConfig(serde_json::Error),
    WriteConfig { path: PathBuf, source: std::io::Error },
    Cleanup { path: PathBuf, source: std::io::Error },
}

impl fmt::Display for SandboxRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnabled(status) => write!(formatter, "Sandbox 未启用: {status:?}"),
            Self::SerializeConfig(source) => write!(formatter, "无法序列化 Sandbox 配置: {source}"),
            Self::WriteConfig { path, source } => write!(formatter, "无法写入 Sandbox 临时配置 {}: {source}", path.display()),
            Self::Cleanup { path, source } => write!(formatter, "无法删除 Sandbox 临时配置 {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for SandboxRuntimeError {}

fn write_runtime_config(config: &SandboxConfig) -> Result<PathBuf, SandboxRuntimeError> {
    let mut value = serde_json::to_value(config).map_err(SandboxRuntimeError::SerializeConfig)?;
    // `enabled` 是 Extension 的宿主字段，而 `srt` 只接收 SandboxRuntimeConfig。
    // 移除它可严格保持 TypeScript `SandboxManager.initialize({...})` 的入参形状。
    value.as_object_mut().expect("SandboxConfig always serializes as an object").remove("enabled");
    let serialized = serde_json::to_vec(&value).map_err(SandboxRuntimeError::SerializeConfig)?;

    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let path = env::temp_dir().join(format!("makima-sandbox-{}-{nonce}.json", std::process::id()));
    fs::write(&path, serialized).map_err(|source| SandboxRuntimeError::WriteConfig { path: path.clone(), source })?;
    Ok(path)
}

fn program_is_available(program: &Path) -> bool {
    if program.components().count() > 1 {
        return program.is_file();
    }

    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|directory| directory.join(program).is_file())
}

#[cfg(test)]
mod tests {
    use super::{SandboxPlatform, SandboxRuntime, SandboxRuntimeError, SandboxRuntimeStatus};
    use crate::SandboxConfig;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn windows_is_disabled_before_backend_lookup() {
        let runtime = SandboxRuntime::initialize_for_platform(&SandboxConfig::default(), false, SandboxPlatform::Windows, None).unwrap();
        assert_eq!(runtime.status(), &SandboxRuntimeStatus::UnsupportedPlatform(SandboxPlatform::Windows));
    }

    #[test]
    fn flag_and_configuration_disable_sandbox_with_distinct_statuses() {
        let disabled_by_flag = SandboxRuntime::initialize_for_platform(&SandboxConfig::default(), true, SandboxPlatform::Linux, None).unwrap();
        assert_eq!(disabled_by_flag.status(), &SandboxRuntimeStatus::DisabledByFlag);

        let config = SandboxConfig { enabled: false, ..SandboxConfig::default() };
        let disabled_by_config = SandboxRuntime::initialize_for_platform(&config, false, SandboxPlatform::MacOs, None).unwrap();
        assert_eq!(disabled_by_config.status(), &SandboxRuntimeStatus::DisabledByConfig);
    }

    #[test]
    fn unavailable_backend_returns_a_non_fatal_status() {
        let path = PathBuf::from("missing-makima-sandbox-backend");
        let runtime = SandboxRuntime::initialize_for_platform(&SandboxConfig::default(), false, SandboxPlatform::Linux, Some(path.clone())).unwrap();
        assert_eq!(runtime.status(), &SandboxRuntimeStatus::BackendUnavailable { program: path });
    }

    #[test]
    fn enabled_runtime_wraps_command_and_removes_its_config_on_reset() {
        let backend = std::env::current_exe().unwrap();
        let mut runtime = SandboxRuntime::initialize_for_platform(&SandboxConfig::default(), false, SandboxPlatform::Linux, Some(backend.clone())).unwrap();
        let wrapped = runtime.wrap_command("printf sandbox").unwrap();
        assert_eq!(wrapped.program, backend);
        assert_eq!(wrapped.args[0], "--settings");
        assert_eq!(wrapped.args[2], "-c");
        assert_eq!(wrapped.args[3], "printf sandbox");
        let settings_path = PathBuf::from(&wrapped.args[1]);
        let content = fs::read_to_string(&settings_path).unwrap();
        assert!(!content.contains("\"enabled\""));
        assert!(content.contains("\"allowedDomains\""));

        runtime.reset().unwrap();
        assert!(!settings_path.exists());
        assert!(matches!(runtime.wrap_command("echo no"), Err(SandboxRuntimeError::NotEnabled(_))));
    }
}
