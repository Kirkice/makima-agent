//! 与 TypeScript Sandbox Extension 兼容的配置加载与合并。
//!
//! 配置来源及优先级严格保持与现有扩展一致：内置默认值、全局配置、项目配置。解析
//! 失败的单个配置文件不会阻止 Sandbox 启动，而是记录诊断并继续使用较低优先级配置。
//! 这与扩展中打印 warning 后继续执行的行为一致。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// 现有 TypeScript Extension 使用的 Sandbox 配置文件名。
pub const SANDBOX_CONFIG_FILE_NAME: &str = "sandbox.json";

/// 项目内 Sandbox 配置所在目录名。
pub const PROJECT_CONFIG_DIRECTORY_NAME: &str = ".pi";

/// TypeScript Sandbox Extension 的完整有效配置。
///
/// 字段名称保持 camelCase，确保读取、展示和未来 RPC 传输时可直接复用
/// [`serde_json`](serde_json) 的默认编码结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxConfig {
    /// 是否启用 OS 级隔离。
    pub enabled: bool,
    /// 网络访问限制。
    pub network: NetworkConfig,
    /// 文件系统访问限制。
    pub filesystem: FilesystemConfig,
    /// 由底层 runtime 忽略的违规规则。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_violations: Option<BTreeMap<String, Vec<String>>>,
    /// 是否允许嵌套 Sandbox 降级为较弱模式。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_weaker_nested_sandbox: Option<bool>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            network: NetworkConfig {
                allowed_domains: vec![
                    "npmjs.org".to_owned(),
                    "*.npmjs.org".to_owned(),
                    "registry.npmjs.org".to_owned(),
                    "registry.yarnpkg.com".to_owned(),
                    "pypi.org".to_owned(),
                    "*.pypi.org".to_owned(),
                    "github.com".to_owned(),
                    "*.github.com".to_owned(),
                    "api.github.com".to_owned(),
                    "raw.githubusercontent.com".to_owned(),
                ],
                denied_domains: Vec::new(),
                allow_unix_sockets: None,
                allow_all_unix_sockets: None,
                allow_local_binding: None,
                http_proxy_port: None,
                socks_proxy_port: None,
            },
            filesystem: FilesystemConfig {
                deny_read: vec!["~/.ssh".to_owned(), "~/.aws".to_owned(), "~/.gnupg".to_owned()],
                allow_write: vec![".".to_owned(), "/tmp".to_owned()],
                deny_write: vec![".env".to_owned(), ".env.*".to_owned(), "*.pem".to_owned(), "*.key".to_owned()],
                allow_git_config: None,
            },
            ignore_violations: None,
            enable_weaker_nested_sandbox: None,
        }
    }
}

/// 网络规则；字段与 `SandboxRuntimeConfig.network` 对齐。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConfig {
    pub allowed_domains: Vec<String>,
    pub denied_domains: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_unix_sockets: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_all_unix_sockets: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_local_binding: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_proxy_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socks_proxy_port: Option<u16>,
}

/// 文件系统规则；字段与 `SandboxRuntimeConfig.filesystem` 对齐。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemConfig {
    pub deny_read: Vec<String>,
    pub allow_write: Vec<String>,
    pub deny_write: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_git_config: Option<bool>,
}

/// 单个配置文件可提供的覆盖项。
///
/// TypeScript 代码只合并 `enabled`、`network`、`filesystem`、`ignoreViolations` 和
/// `enableWeakerNestedSandbox`。这里有意不扩展为递归通用 merge，以避免引入与现有
/// Extension 不一致的配置语义。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SandboxConfigOverrides {
    enabled: Option<bool>,
    network: Option<NetworkConfigOverrides>,
    filesystem: Option<FilesystemConfigOverrides>,
    ignore_violations: Option<BTreeMap<String, Vec<String>>>,
    enable_weaker_nested_sandbox: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkConfigOverrides {
    allowed_domains: Option<Vec<String>>,
    denied_domains: Option<Vec<String>>,
    allow_unix_sockets: Option<Vec<String>>,
    allow_all_unix_sockets: Option<bool>,
    allow_local_binding: Option<bool>,
    http_proxy_port: Option<u16>,
    socks_proxy_port: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilesystemConfigOverrides {
    deny_read: Option<Vec<String>>,
    allow_write: Option<Vec<String>>,
    deny_write: Option<Vec<String>>,
    allow_git_config: Option<bool>,
}

/// 被忽略的配置文件解析诊断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    pub path: PathBuf,
    pub message: String,
}

impl fmt::Display for ConfigDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "无法解析 Sandbox 配置 {}: {}", self.path.display(), self.message)
    }
}

/// 合并后的配置及加载期间可展示给用户的非致命诊断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSandboxConfig {
    pub config: SandboxConfig,
    pub diagnostics: Vec<ConfigDiagnostic>,
}

/// 从 TypeScript Extension 对应的全局与项目路径加载配置。
///
/// `agent_dir` 对应 TypeScript 的 `getAgentDir()` 返回值。例如全局配置路径为
/// `<agent_dir>/extensions/sandbox.json`，项目配置路径为 `<cwd>/.pi/sandbox.json`。
pub fn load_config(cwd: &Path, agent_dir: &Path) -> LoadedSandboxConfig {
    let global_path = agent_dir.join("extensions").join(SANDBOX_CONFIG_FILE_NAME);
    let project_path = cwd.join(PROJECT_CONFIG_DIRECTORY_NAME).join(SANDBOX_CONFIG_FILE_NAME);
    load_config_from_paths(&global_path, &project_path)
}

/// 从指定的全局和项目配置文件路径加载，主要供宿主和测试调用。
pub fn load_config_from_paths(global_path: &Path, project_path: &Path) -> LoadedSandboxConfig {
    let mut config = SandboxConfig::default();
    let mut diagnostics = Vec::new();

    apply_file_overrides(&mut config, global_path, &mut diagnostics);
    apply_file_overrides(&mut config, project_path, &mut diagnostics);

    LoadedSandboxConfig { config, diagnostics }
}

fn apply_file_overrides(config: &mut SandboxConfig, path: &Path, diagnostics: &mut Vec<ConfigDiagnostic>) {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            diagnostics.push(ConfigDiagnostic { path: path.to_path_buf(), message: error.to_string() });
            return;
        }
    };

    match serde_json::from_str::<SandboxConfigOverrides>(&contents) {
        Ok(overrides) => merge_config(config, overrides),
        Err(error) => diagnostics.push(ConfigDiagnostic { path: path.to_path_buf(), message: error.to_string() }),
    }
}

fn merge_config(config: &mut SandboxConfig, overrides: SandboxConfigOverrides) {
    if let Some(enabled) = overrides.enabled {
        config.enabled = enabled;
    }
    if let Some(network) = overrides.network {
        merge_network_config(&mut config.network, network);
    }
    if let Some(filesystem) = overrides.filesystem {
        merge_filesystem_config(&mut config.filesystem, filesystem);
    }
    if let Some(ignore_violations) = overrides.ignore_violations {
        config.ignore_violations = Some(ignore_violations);
    }
    if let Some(enable_weaker_nested_sandbox) = overrides.enable_weaker_nested_sandbox {
        config.enable_weaker_nested_sandbox = Some(enable_weaker_nested_sandbox);
    }
}

fn merge_network_config(config: &mut NetworkConfig, overrides: NetworkConfigOverrides) {
    if let Some(value) = overrides.allowed_domains {
        config.allowed_domains = value;
    }
    if let Some(value) = overrides.denied_domains {
        config.denied_domains = value;
    }
    if let Some(value) = overrides.allow_unix_sockets {
        config.allow_unix_sockets = Some(value);
    }
    if let Some(value) = overrides.allow_all_unix_sockets {
        config.allow_all_unix_sockets = Some(value);
    }
    if let Some(value) = overrides.allow_local_binding {
        config.allow_local_binding = Some(value);
    }
    if let Some(value) = overrides.http_proxy_port {
        config.http_proxy_port = Some(value);
    }
    if let Some(value) = overrides.socks_proxy_port {
        config.socks_proxy_port = Some(value);
    }
}

fn merge_filesystem_config(config: &mut FilesystemConfig, overrides: FilesystemConfigOverrides) {
    if let Some(value) = overrides.deny_read {
        config.deny_read = value;
    }
    if let Some(value) = overrides.allow_write {
        config.allow_write = value;
    }
    if let Some(value) = overrides.deny_write {
        config.deny_write = value;
    }
    if let Some(value) = overrides.allow_git_config {
        config.allow_git_config = Some(value);
    }
}

#[cfg(test)]
mod tests {
    use super::{load_config, load_config_from_paths, SandboxConfig};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(name: &str) -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("makima-sandbox-config-{name}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn defaults_match_the_typescript_extension() {
        let config = SandboxConfig::default();
        assert!(config.enabled);
        assert_eq!(config.network.allowed_domains.first().map(String::as_str), Some("npmjs.org"));
        assert_eq!(config.filesystem.deny_read, vec!["~/.ssh", "~/.aws", "~/.gnupg"]);
        assert_eq!(config.filesystem.allow_write, vec![".", "/tmp"]);
        assert_eq!(config.filesystem.deny_write, vec![".env", ".env.*", "*.pem", "*.key"]);
    }

    #[test]
    fn global_then_project_overrides_follow_typescript_merge_order() {
        let root = temporary_directory("merge");
        let global_path = root.join("agent/extensions/sandbox.json");
        let project_path = root.join("project/.pi/sandbox.json");
        fs::create_dir_all(global_path.parent().unwrap()).unwrap();
        fs::create_dir_all(project_path.parent().unwrap()).unwrap();
        fs::write(
            &global_path,
            r#"{"enabled":false,"network":{"allowedDomains":["global.example"],"allowLocalBinding":true},"filesystem":{"allowWrite":["/global"]}}"#,
        )
        .unwrap();
        fs::write(
            &project_path,
            r#"{"enabled":true,"network":{"deniedDomains":["blocked.example"]},"filesystem":{"denyWrite":["secret"]}}"#,
        )
        .unwrap();

        let loaded = load_config_from_paths(&global_path, &project_path);
        assert!(loaded.diagnostics.is_empty());
        assert!(loaded.config.enabled);
        assert_eq!(loaded.config.network.allowed_domains, vec!["global.example"]);
        assert_eq!(loaded.config.network.denied_domains, vec!["blocked.example"]);
        assert_eq!(loaded.config.network.allow_local_binding, Some(true));
        assert_eq!(loaded.config.filesystem.allow_write, vec!["/global"]);
        assert_eq!(loaded.config.filesystem.deny_write, vec!["secret"]);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_file_is_reported_and_does_not_replace_lower_priority_config() {
        let root = temporary_directory("invalid");
        let global_path = root.join("global.json");
        let project_path = root.join("project.json");
        fs::create_dir_all(&root).unwrap();
        fs::write(&global_path, r#"{"enabled":false}"#).unwrap();
        fs::write(&project_path, "{").unwrap();

        let loaded = load_config_from_paths(&global_path, &project_path);
        assert!(!loaded.config.enabled);
        assert_eq!(loaded.diagnostics.len(), 1);
        assert_eq!(loaded.diagnostics[0].path, project_path);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uses_extension_compatible_default_paths() {
        let root = temporary_directory("paths");
        let cwd = root.join("project");
        let agent_dir = root.join("agent");
        let global_path = agent_dir.join("extensions/sandbox.json");
        let project_path = cwd.join(".pi/sandbox.json");
        fs::create_dir_all(global_path.parent().unwrap()).unwrap();
        fs::create_dir_all(project_path.parent().unwrap()).unwrap();
        fs::write(global_path, r#"{"enabled":false}"#).unwrap();
        fs::write(project_path, r#"{"enabled":true}"#).unwrap();

        let loaded = load_config(&cwd, &agent_dir);
        assert!(loaded.config.enabled);
        assert!(loaded.diagnostics.is_empty());

        fs::remove_dir_all(root).unwrap();
    }
}
