use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use thiserror::Error;

use crate::sandbox::CommandFilter;

#[derive(Error, Debug)]
pub enum ShellError {
    #[error("Command blocked: {0}")]
    Blocked(String),
    #[error("Command timeout after {0} seconds")]
    Timeout(u64),
    #[error("Command failed: {0}")]
    Failed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct ShellExecutor {
    command_filter: CommandFilter,
}

impl ShellExecutor {
    pub fn new() -> Self {
        Self {
            command_filter: CommandFilter::new(),
        }
    }

    pub async fn execute(
        &self,
        command: &str,
        working_dir: &str,
        timeout_seconds: u64,
    ) -> Result<(String, String, i32), ShellError> {
        // Validate command
        self.command_filter
            .validate_command(command)
            .map_err(|e| ShellError::Blocked(e.to_string()))?;

        // Execute with timeout
        let result = timeout(Duration::from_secs(timeout_seconds), async {
            let output = Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);

            Ok::<_, ShellError>((stdout, stderr, exit_code))
        })
        .await;

        match result {
            Ok(Ok((stdout, stderr, exit_code))) => Ok((stdout, stderr, exit_code)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ShellError::Timeout(timeout_seconds)),
        }
    }
}

impl Default for ShellExecutor {
    fn default() -> Self {
        Self::new()
    }
}