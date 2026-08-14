//! 隔离命令的同步执行器。
//!
//! 本模块执行 [`crate::SandboxRuntime::wrap_command`] 返回的 `srt` 调用，并提供与
//! TypeScript Bash 操作相同的三个可观测行为：合并转发 stdout/stderr、超时，以及由
//! 原子取消标记触发的终止。它不直接执行未包装的用户命令，因此调用方无法在启用
//! Sandbox 时意外绕过 OS 级隔离后端。

use crate::{SandboxRuntime, SandboxRuntimeError};
use std::fmt;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// 单次隔离命令的执行选项。
#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    /// 子进程工作目录；必须存在且为目录。
    pub cwd: PathBuf,
    /// 最大执行时长。`None` 表示不额外限制。
    pub timeout: Option<Duration>,
    /// 由 Agent Loop 或 Tool Runtime 共享的取消标记。
    pub cancellation: Option<Arc<AtomicBool>>,
}

/// 隔离命令终止后的稳定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    pub exit_code: Option<i32>,
}

/// 命令无法启动、被取消或超时的错误。
#[derive(Debug)]
pub enum ExecutionError {
    Runtime(SandboxRuntimeError),
    InvalidWorkingDirectory(PathBuf),
    Spawn {
        program: PathBuf,
        source: std::io::Error,
    },
    Wait(std::io::Error),
    Aborted,
    TimedOut {
        timeout: Duration,
    },
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(source) => write!(formatter, "无法包装 Sandbox 命令: {source}"),
            Self::InvalidWorkingDirectory(path) => {
                write!(formatter, "工作目录不存在或不是目录: {}", path.display())
            }
            Self::Spawn { program, source } => write!(
                formatter,
                "无法启动 Sandbox 后端 {}: {source}",
                program.display()
            ),
            Self::Wait(source) => write!(formatter, "等待 Sandbox 命令失败: {source}"),
            Self::Aborted => formatter.write_str("aborted"),
            Self::TimedOut { timeout } => write!(formatter, "timeout:{}", timeout.as_secs()),
        }
    }
}

impl std::error::Error for ExecutionError {}

/// 在 OS 级 Sandbox 内执行 Bash 命令。
///
/// `on_output` 会以行为单位接收 stdout 与 stderr；这与 TypeScript 版本将两个 stream
/// 都传给 `onData` 的语义一致，只是 Rust 同步 API 使用 `&mut` 回调避免额外 runtime
/// 依赖。超时或取消时执行器会尽力终止 `srt` 进程，然后返回稳定错误文字。
pub fn execute_sandboxed_command(
    runtime: &SandboxRuntime,
    command: impl Into<String>,
    options: &ExecutionOptions,
    on_output: &mut dyn FnMut(&str),
) -> Result<ExecutionResult, ExecutionError> {
    if !options.cwd.is_dir() {
        return Err(ExecutionError::InvalidWorkingDirectory(options.cwd.clone()));
    }
    let wrapped = runtime
        .wrap_command(command)
        .map_err(ExecutionError::Runtime)?;
    let mut child = Command::new(&wrapped.program)
        .args(&wrapped.args)
        .current_dir(&options.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| ExecutionError::Spawn {
            program: wrapped.program,
            source,
        })?;

    let (sender, receiver) = mpsc::channel();
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let stdout_reader = spawn_output_reader(stdout, sender.clone());
    let stderr_reader = spawn_output_reader(stderr, sender);
    let started_at = Instant::now();
    loop {
        while let Ok(chunk) = receiver.try_recv() {
            on_output(&chunk);
        }

        if options
            .cancellation
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            return terminate_and_report(
                &mut child,
                stdout_reader,
                stderr_reader,
                &receiver,
                on_output,
                ExecutionError::Aborted,
            );
        }
        if let Some(timeout) = options.timeout
            && started_at.elapsed() >= timeout
        {
            return terminate_and_report(
                &mut child,
                stdout_reader,
                stderr_reader,
                &receiver,
                on_output,
                ExecutionError::TimedOut { timeout },
            );
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                drain_output(&receiver, on_output);
                join_reader(stdout_reader);
                join_reader(stderr_reader);
                return Ok(ExecutionResult {
                    exit_code: status.code(),
                });
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(source) => {
                return terminate_and_report(
                    &mut child,
                    stdout_reader,
                    stderr_reader,
                    &receiver,
                    on_output,
                    ExecutionError::Wait(source),
                );
            }
        }
    }
}

fn terminate_and_report(
    child: &mut std::process::Child,
    stdout_reader: thread::JoinHandle<()>,
    stderr_reader: thread::JoinHandle<()>,
    receiver: &mpsc::Receiver<String>,
    on_output: &mut dyn FnMut(&str),
    error: ExecutionError,
) -> Result<ExecutionResult, ExecutionError> {
    let _ = child.kill();
    let _ = child.wait();
    join_reader(stdout_reader);
    join_reader(stderr_reader);
    drain_output(receiver, on_output);
    Err(error)
}

fn spawn_output_reader<R>(reader: R, sender: mpsc::Sender<String>) -> thread::JoinHandle<()>
where
    R: std::io::Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if sender.send(format!("{line}\n")).is_err() {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    })
}

fn drain_output(receiver: &mpsc::Receiver<String>, on_output: &mut dyn FnMut(&str)) {
    while let Ok(chunk) = receiver.try_recv() {
        on_output(&chunk);
    }
}

fn join_reader(reader: thread::JoinHandle<()>) {
    let _ = reader.join();
}

#[cfg(test)]
mod tests {
    use super::{ExecutionError, ExecutionOptions, execute_sandboxed_command};
    use crate::{SandboxConfig, SandboxPlatform, SandboxRuntime};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    fn enabled_runtime() -> SandboxRuntime {
        SandboxRuntime::initialize_for_platform(
            &SandboxConfig::default(),
            false,
            SandboxPlatform::Linux,
            Some(std::env::current_exe().unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn rejects_missing_working_directory_before_starting_backend() {
        let runtime = enabled_runtime();
        let options = ExecutionOptions {
            cwd: PathBuf::from("missing-makima-sandbox-directory"),
            timeout: None,
            cancellation: None,
        };
        let error =
            execute_sandboxed_command(&runtime, "echo ignored", &options, &mut |_| {}).unwrap_err();
        assert!(matches!(error, ExecutionError::InvalidWorkingDirectory(_)));
    }

    #[test]
    fn cancellation_is_reported_with_extension_compatible_message() {
        let runtime = enabled_runtime();
        let options = ExecutionOptions {
            cwd: std::env::temp_dir(),
            timeout: None,
            cancellation: Some(Arc::new(AtomicBool::new(true))),
        };
        let error =
            execute_sandboxed_command(&runtime, "echo ignored", &options, &mut |_| {}).unwrap_err();
        assert_eq!(error.to_string(), "aborted");
    }

    #[test]
    fn timeout_is_reported_with_extension_compatible_message() {
        let runtime = enabled_runtime();
        let options = ExecutionOptions {
            cwd: std::env::temp_dir(),
            timeout: Some(Duration::ZERO),
            cancellation: None,
        };
        let error =
            execute_sandboxed_command(&runtime, "echo ignored", &options, &mut |_| {}).unwrap_err();
        assert_eq!(error.to_string(), "timeout:0");
    }
}
