//! Makima Agent 的 Sandbox 最小策略层。
//!
//! 本 crate 只回答“某项操作是否允许”，不直接执行文件 I/O、启动子进程或调用
//! 平台隔离能力。工具运行时以后通过 [`Sandbox`] 取得决策，再负责实际执行；
//! 这样可以让权限规则独立测试，并避免安全策略反向依赖 Tool Runtime。

mod config;
mod executor;
mod runtime;

pub use config::{
    load_config, load_config_from_paths, ConfigDiagnostic, FilesystemConfig, LoadedSandboxConfig, NetworkConfig,
    SandboxConfig, PROJECT_CONFIG_DIRECTORY_NAME, SANDBOX_CONFIG_FILE_NAME,
};
pub use executor::{execute_sandboxed_command, ExecutionError, ExecutionOptions, ExecutionResult};
pub use runtime::{
    SandboxPlatform, SandboxRuntime, SandboxRuntimeError, SandboxRuntimeStatus, WrappedCommand,
};

use std::fmt;
use std::path::{Component, Path, PathBuf};

/// 文件系统操作的权限类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAccess {
    /// 读取已有内容或列出目录。
    Read,
    /// 创建、修改、删除文件或目录。
    Write,
}

/// 网络访问的权限类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkAccess {
    /// 不允许建立网络连接。
    Deny,
    /// 允许建立网络连接；具体域名限制将在平台适配层中实现。
    Allow,
}

/// 进程启动请求。
///
/// Sandbox 在本阶段只校验命令是否被允许，并保留超时与环境变量规则；真正的进程
/// 创建、取消和资源限制将在后续平台执行器中实现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRequest<'a> {
    /// 准备执行的程序名或路径。
    pub program: &'a Path,
    /// 计划使用的工作目录；为空时由 Tool Runtime 使用当前 Session 工作目录。
    pub cwd: Option<&'a Path>,
}

/// 进程执行策略。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessPolicy {
    /// 是否允许启动子进程。
    pub allowed: bool,
    /// 允许的程序绝对路径。空集合表示不限制程序路径。
    pub allowed_programs: Vec<PathBuf>,
    /// 单个进程的最大执行时长，单位为毫秒。`None` 表示由调用方决定。
    pub timeout_ms: Option<u64>,
    /// 子进程允许继承的环境变量名称。空集合表示不额外限制。
    pub allowed_environment: Vec<String>,
}

impl Default for ProcessPolicy {
    fn default() -> Self {
        Self {
            allowed: true,
            allowed_programs: Vec::new(),
            timeout_ms: None,
            allowed_environment: Vec::new(),
        }
    }
}

/// Sandbox 的不可变配置。
///
/// `workspace_root` 是默认读写根目录。额外根目录必须在创建策略时显式声明，避免
/// Tool Runtime 根据单次请求临时扩大权限范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPolicy {
    workspace_root: PathBuf,
    readable_roots: Vec<PathBuf>,
    writable_roots: Vec<PathBuf>,
    network: NetworkAccess,
    process: ProcessPolicy,
}

impl SandboxPolicy {
    /// 创建只允许访问工作目录的默认策略。
    ///
    /// 根目录会以词法方式标准化：解析 `.` 和 `..`，但不会访问文件系统，因此创建
    /// 策略时不要求目录已存在。符号链接的真实路径校验必须由未来平台适配层完成。
    pub fn workspace_only(workspace_root: impl Into<PathBuf>) -> Result<Self, SandboxPolicyError> {
        let workspace_root = normalize_absolute_path(workspace_root.into())?;
        Ok(Self {
            readable_roots: vec![workspace_root.clone()],
            writable_roots: vec![workspace_root.clone()],
            workspace_root,
            network: NetworkAccess::Deny,
            process: ProcessPolicy::default(),
        })
    }

    /// 在保留既有根目录的前提下增加一个只读根目录。
    pub fn allow_read_root(mut self, root: impl Into<PathBuf>) -> Result<Self, SandboxPolicyError> {
        self.readable_roots.push(normalize_absolute_path(root.into())?);
        Ok(self)
    }

    /// 在保留既有根目录的前提下增加一个可写根目录。
    ///
    /// 可写目录同时自动获得读取权限，保证读改写工具无需维护两套不一致的配置。
    pub fn allow_write_root(mut self, root: impl Into<PathBuf>) -> Result<Self, SandboxPolicyError> {
        let root = normalize_absolute_path(root.into())?;
        if !self.readable_roots.contains(&root) {
            self.readable_roots.push(root.clone());
        }
        self.writable_roots.push(root);
        Ok(self)
    }

    /// 配置网络访问策略。
    pub fn with_network_access(mut self, network: NetworkAccess) -> Self {
        self.network = network;
        self
    }

    /// 配置进程执行策略。
    pub fn with_process_policy(mut self, process: ProcessPolicy) -> Self {
        self.process = process;
        self
    }

    /// 返回策略绑定的工作目录。
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
}

/// 操作被拒绝的稳定原因码，供未来 RPC 和 Tool Runtime 转换为跨语言错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialReason {
    /// 目标路径为空、包含无法安全表示的根组件，或不是绝对路径。
    InvalidPath,
    /// 目标路径不属于允许根目录。
    PathOutsideAllowedRoots,
    /// 策略禁止该类网络访问。
    NetworkDisabled,
    /// 策略禁止启动子进程。
    ProcessDisabled,
    /// 子进程工作目录不属于可读根目录。
    ProcessWorkingDirectoryDenied,
    /// 程序不在显式允许清单中。
    ProgramNotAllowed,
}

impl fmt::Display for DenialReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidPath => "路径必须是可规范化的绝对路径",
            Self::PathOutsideAllowedRoots => "路径不在 Sandbox 允许的根目录内",
            Self::NetworkDisabled => "Sandbox 策略禁止网络访问",
            Self::ProcessDisabled => "Sandbox 策略禁止启动子进程",
            Self::ProcessWorkingDirectoryDenied => "进程工作目录不在 Sandbox 允许的根目录内",
            Self::ProgramNotAllowed => "程序不在 Sandbox 允许清单中",
        };
        formatter.write_str(message)
    }
}

/// Sandbox 的决策结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxDecision {
    /// 操作可由上层 Tool Runtime 执行。
    Allow,
    /// 操作被策略拒绝，并携带可序列化的稳定原因码。
    Deny(DenialReason),
}

impl SandboxDecision {
    /// 将决策转换为调用方容易判断的布尔值。
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// Sandbox 策略配置错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxPolicyError {
    /// 路径为空、相对路径，或包含 Windows 前缀等无法安全词法处理的组件。
    InvalidRoot(PathBuf),
}

impl fmt::Display for SandboxPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoot(path) => write!(formatter, "Sandbox 根目录无效: {}", path.display()),
        }
    }
}

impl std::error::Error for SandboxPolicyError {}

/// 为 Tool Runtime 提供的最小权限决策端口。
///
/// 该 trait 刻意没有文件读写和子进程创建方法；这样测试替身、远程 Sandbox 或未来
/// Windows/Linux/macOS 平台实现都可以替换，而不改变 Tool Runtime 的调用模型。
pub trait Sandbox {
    /// 判断文件操作是否可以访问目标路径。
    fn check_file_access(&self, path: &Path, access: FileAccess) -> SandboxDecision;

    /// 判断网络访问是否允许。
    fn check_network_access(&self) -> SandboxDecision;

    /// 判断子进程是否允许启动。
    fn check_process(&self, request: &ProcessRequest<'_>) -> SandboxDecision;

    /// 返回执行器应采用的进程限制。
    fn process_policy(&self) -> &ProcessPolicy;
}

/// 以 [`SandboxPolicy`] 做同步、本地决策的默认实现。
#[derive(Debug, Clone)]
pub struct PolicySandbox {
    policy: SandboxPolicy,
}

impl PolicySandbox {
    /// 用不可变策略创建 Sandbox。
    pub fn new(policy: SandboxPolicy) -> Self {
        Self { policy }
    }

    /// 返回当前策略，只读暴露给启动器或诊断模块。
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }
}

impl Sandbox for PolicySandbox {
    fn check_file_access(&self, path: &Path, access: FileAccess) -> SandboxDecision {
        let path = match normalize_absolute_path(path.to_path_buf()) {
            Ok(path) => path,
            Err(_) => return SandboxDecision::Deny(DenialReason::InvalidPath),
        };
        let allowed_roots = match access {
            FileAccess::Read => &self.policy.readable_roots,
            FileAccess::Write => &self.policy.writable_roots,
        };
        if allowed_roots.iter().any(|root| is_within_root(&path, root)) {
            SandboxDecision::Allow
        } else {
            SandboxDecision::Deny(DenialReason::PathOutsideAllowedRoots)
        }
    }

    fn check_network_access(&self) -> SandboxDecision {
        match self.policy.network {
            NetworkAccess::Allow => SandboxDecision::Allow,
            NetworkAccess::Deny => SandboxDecision::Deny(DenialReason::NetworkDisabled),
        }
    }

    fn check_process(&self, request: &ProcessRequest<'_>) -> SandboxDecision {
        if !self.policy.process.allowed {
            return SandboxDecision::Deny(DenialReason::ProcessDisabled);
        }
        if let Some(cwd) = request.cwd
            && !self.check_file_access(cwd, FileAccess::Read).is_allowed()
        {
            return SandboxDecision::Deny(DenialReason::ProcessWorkingDirectoryDenied);
        }
        if self.policy.process.allowed_programs.is_empty() {
            return SandboxDecision::Allow;
        }
        let program = match normalize_absolute_path(request.program.to_path_buf()) {
            Ok(program) => program,
            Err(_) => return SandboxDecision::Deny(DenialReason::ProgramNotAllowed),
        };
        if self.policy.process.allowed_programs.iter().any(|allowed| allowed == &program) {
            SandboxDecision::Allow
        } else {
            SandboxDecision::Deny(DenialReason::ProgramNotAllowed)
        }
    }

    fn process_policy(&self) -> &ProcessPolicy {
        &self.policy.process
    }
}

/// 词法标准化绝对路径。
///
/// 此函数不访问磁盘，以便安全策略能够在文件尚未创建时先做写入授权。它不能解析
/// 符号链接，因此调用实际系统 API 前仍需要平台执行器进行最终真实路径检查。
fn normalize_absolute_path(path: PathBuf) -> Result<PathBuf, SandboxPolicyError> {
    if !path.is_absolute() {
        return Err(SandboxPolicyError::InvalidRoot(path));
    }

    let original = path.clone();
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(SandboxPolicyError::InvalidRoot(original));
                }
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        Err(SandboxPolicyError::InvalidRoot(original))
    } else {
        Ok(normalized)
    }
}

/// 判断候选路径是否是根目录自身或其后代。
///
/// 使用 `Path::starts_with` 而不是字符串前缀匹配，避免 `/work/app2` 被误判为
/// `/work/app` 的子目录。
fn is_within_root(candidate: &Path, root: &Path) -> bool {
    candidate.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::{
        DenialReason, FileAccess, NetworkAccess, PolicySandbox, ProcessPolicy, ProcessRequest, Sandbox,
        SandboxDecision, SandboxPolicy,
    };
    use std::path::PathBuf;

    /// 使用宿主平台原生绝对路径，确保 Windows 与 Unix 都验证相同的策略语义。
    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join("makima-sandbox-tests").join(name)
    }

    #[test]
    fn workspace_only_allows_reads_and_writes_below_workspace() {
        let workspace = test_root("project");
        let sandbox = PolicySandbox::new(SandboxPolicy::workspace_only(&workspace).unwrap());

        assert_eq!(sandbox.check_file_access(&workspace.join("src/main.rs"), FileAccess::Read), SandboxDecision::Allow);
        assert_eq!(sandbox.check_file_access(&workspace.join("output.txt"), FileAccess::Write), SandboxDecision::Allow);
    }

    #[test]
    fn rejects_path_prefixes_that_are_not_real_descendants() {
        let workspace = test_root("app");
        let sandbox = PolicySandbox::new(SandboxPolicy::workspace_only(&workspace).unwrap());

        assert_eq!(
            sandbox.check_file_access(&test_root("app-other/file.txt"), FileAccess::Read),
            SandboxDecision::Deny(DenialReason::PathOutsideAllowedRoots)
        );
    }

    #[test]
    fn normalizes_parent_components_before_authorization() {
        let workspace = test_root("project");
        let sandbox = PolicySandbox::new(SandboxPolicy::workspace_only(&workspace).unwrap());

        assert_eq!(
            sandbox.check_file_access(&workspace.join("src/../../secret.txt"), FileAccess::Read),
            SandboxDecision::Deny(DenialReason::PathOutsideAllowedRoots)
        );
    }

    #[test]
    fn write_root_also_grants_read_access() {
        let workspace = test_root("project");
        let cache = test_root("cache");
        let policy = SandboxPolicy::workspace_only(workspace).unwrap().allow_write_root(&cache).unwrap();
        let sandbox = PolicySandbox::new(policy);

        assert!(sandbox.check_file_access(&cache.join("result.json"), FileAccess::Read).is_allowed());
        assert!(sandbox.check_file_access(&cache.join("result.json"), FileAccess::Write).is_allowed());
    }

    #[test]
    fn network_is_denied_by_default_and_can_be_enabled() {
        let workspace = test_root("project");
        let denied = PolicySandbox::new(SandboxPolicy::workspace_only(&workspace).unwrap());
        assert_eq!(denied.check_network_access(), SandboxDecision::Deny(DenialReason::NetworkDisabled));

        let allowed = PolicySandbox::new(SandboxPolicy::workspace_only(workspace).unwrap().with_network_access(NetworkAccess::Allow));
        assert_eq!(allowed.check_network_access(), SandboxDecision::Allow);
    }

    #[test]
    fn process_policy_checks_enablement_program_and_working_directory() {
        let workspace = test_root("project");
        let git = test_root("bin/git");
        let shell = test_root("bin/sh");
        let disabled = PolicySandbox::new(
            SandboxPolicy::workspace_only(&workspace).unwrap().with_process_policy(ProcessPolicy {
                allowed: false,
                ..ProcessPolicy::default()
            }),
        );
        assert_eq!(
            disabled.check_process(&ProcessRequest { program: &git, cwd: Some(&workspace) }),
            SandboxDecision::Deny(DenialReason::ProcessDisabled)
        );

        let restricted = PolicySandbox::new(
            SandboxPolicy::workspace_only(&workspace).unwrap().with_process_policy(ProcessPolicy {
                allowed: true,
                allowed_programs: vec![git.clone()],
                timeout_ms: Some(5_000),
                allowed_environment: vec!["PATH".to_owned()],
            }),
        );
        assert_eq!(restricted.check_process(&ProcessRequest { program: &git, cwd: Some(&workspace) }), SandboxDecision::Allow);
        assert_eq!(
            restricted.check_process(&ProcessRequest { program: &shell, cwd: Some(&workspace) }),
            SandboxDecision::Deny(DenialReason::ProgramNotAllowed)
        );
        assert_eq!(
            restricted.check_process(&ProcessRequest { program: &git, cwd: Some(&test_root("outside")) }),
            SandboxDecision::Deny(DenialReason::ProcessWorkingDirectoryDenied)
        );
    }

    #[test]
    fn rejects_relative_policy_roots() {
        assert!(SandboxPolicy::workspace_only("relative/project").is_err());
    }
}
