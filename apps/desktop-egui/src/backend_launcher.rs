//! Backend process launcher.
//!
//! Automatically starts the Python FastAPI backend when the desktop app launches,
//! if no backend is already running. Cleans up the child process on exit.

use std::path::PathBuf;
use std::net::TcpListener;
use std::fs::{self, File, OpenOptions};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const HEALTH_URL: &str = "http://127.0.0.1:8000/health";
const STARTUP_TIMEOUT_SECS: u64 = 30;
const POLL_INTERVAL_MS: u64 = 500;

/// Holds the backend child process and cleans it up on drop.
pub struct BackendProcess {
    child: Option<Child>,
    startup_error: Option<String>,
}

impl BackendProcess {
    fn new(child: Child) -> Self {
        Self { child: Some(child), startup_error: None }
    }

    pub fn none() -> Self {
        Self { child: None, startup_error: None }
    }

    pub fn with_error(error: impl Into<String>) -> Self {
        Self {
            child: None,
            startup_error: Some(error.into()),
        }
    }

    pub fn startup_error(&self) -> Option<&str> {
        self.startup_error.as_deref()
    }

    pub fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            let pid = child.id();
            tracing::info!("Stopping backend process (PID: {})...", pid);

            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }

            let _ = child.kill();
            let _ = child.wait();
            tracing::info!("Backend process stopped.");
        }

        terminate_backend_running_on_port();
    }
}

impl Drop for BackendProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Verify the health endpoint returns a valid makima response.
fn verify_health(body: &str) -> bool {
    // Parse the JSON and check it has "status" and "version" fields,
    // same as launcher.py does.
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        let status_ok = json.get("status")
            .and_then(|v| v.as_str())
            .map(|s| s == "healthy" || s == "ok")
            .unwrap_or(false);
        let has_version = json.get("version").is_some();
        status_ok && has_version
    } else {
        false
    }
}

/// Check if the backend is already running by hitting the health endpoint.
fn is_backend_running() -> bool {
    let client = match reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    match client.get(HEALTH_URL).send() {
        Ok(resp) if resp.status().is_success() => {
            match resp.text() {
                Ok(body) => verify_health(&body),
                Err(_) => false,
            }
        }
        _ => false,
    }
}

fn is_port_available() -> bool {
    TcpListener::bind("127.0.0.1:8000").is_ok()
}

fn terminate_backend_running_on_port() {
    if !is_backend_running() {
        return;
    }

    if let Some(pid) = backend_pid_on_port() {
        tracing::info!("Stopping Makima backend on port 8000 (PID: {})...", pid);

        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        #[cfg(not(windows))]
        {
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

fn backend_pid_on_port() -> Option<u32> {
    #[cfg(windows)]
    {
        let output = Command::new("netstat")
            .args(["-ano", "-p", "tcp"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || !trimmed.contains(":8000") {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() < 5 || !parts[0].eq_ignore_ascii_case("TCP") {
                continue;
            }

            let local_addr = parts[1];
            let state = parts[3];
            let pid = parts[4];

            let matches_port = local_addr.ends_with(":8000");
            let listening = state.eq_ignore_ascii_case("LISTENING");
            if matches_port && listening {
                if let Ok(pid) = pid.parse::<u32>() {
                    return Some(pid);
                }
            }
        }

        None
    }

    #[cfg(not(windows))]
    {
        let output = Command::new("lsof")
            .args(["-ti", "tcp:8000"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().find_map(|line| line.trim().parse::<u32>().ok())
    }
}

fn backend_log_path() -> PathBuf {
    std::env::temp_dir().join("makima-backend-startup.log")
}

fn prepare_backend_log_file() -> std::io::Result<(File, File, PathBuf)> {
    let path = backend_log_path();
    let stdout = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;
    let stderr = stdout.try_clone()?;
    Ok((stdout, stderr, path))
}

fn summarize_startup_error(log_path: &PathBuf) -> Option<String> {
    let content = fs::read_to_string(log_path).ok()?;
    let mut lines: Vec<String> = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();

    if lines.is_empty() {
        return None;
    }

    if lines.len() > 10 {
        lines = lines.split_off(lines.len() - 10);
    }

    let summary = lines.join(" | ");
    let max_len = 1200;
    if summary.len() > max_len {
        Some(format!("{}...", &summary[..max_len]))
    } else {
        Some(summary)
    }
}

/// Wait for the backend to become ready by polling the health endpoint.
fn wait_for_backend(child: &mut Child, timeout: Duration) -> bool {
    let start = Instant::now();
    let client = match reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to build HTTP client for health check: {}", e);
            return false;
        }
    };

    while start.elapsed() < timeout {
        if let Ok(Some(status)) = child.try_wait() {
            tracing::error!("Backend process exited before becoming ready: {}", status);
            return false;
        }

        match client.get(HEALTH_URL).send() {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.text() {
                    if verify_health(&body) {
                        tracing::info!("Backend is ready.");
                        return true;
                    }
                }
                // Got a 200 but not our server — keep waiting
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
            _ => {
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
        }
    }

    false
}

/// Find the project root directory (where `apps/backend/` lives).
///
/// Strategy:
/// 1. Check `MAKIMA_PROJECT_ROOT` env var
/// 2. Walk up from the current executable looking for `apps/backend/src/makima/app.py`
/// 3. Walk up from cwd as a last resort
pub fn find_project_root() -> PathBuf {
    let marker = ["apps", "backend", "src", "makima", "app.py"]
        .iter()
        .collect::<PathBuf>();

    // 1. Check env var
    if let Ok(root) = std::env::var("MAKIMA_PROJECT_ROOT") {
        let p = PathBuf::from(&root);
        if p.join(&marker).exists() {
            return p;
        }
    }

    // 2. Walk up from the executable
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent();
        // Limit walk depth to avoid traversing the entire filesystem
        for _ in 0..10 {
            if let Some(d) = dir {
                if d.join(&marker).exists() {
                    return d.to_path_buf();
                }
                dir = d.parent();
            } else {
                break;
            }
        }
    }

    // 3. Walk up from cwd
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = Some(cwd.as_path());
        for _ in 0..10 {
            if let Some(d) = dir {
                if d.join(&marker).exists() {
                    return d.to_path_buf();
                }
                dir = d.parent();
            } else {
                break;
            }
        }
    }

    // Fallback — won't find backend, but at least won't panic
    tracing::warn!("Could not locate project root. Using cwd as fallback.");
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Ensure the backend is running. Returns a `BackendProcess` that will
/// kill the child process when dropped.
///
/// If the backend is already running (detected via health check), returns
/// a no-op `BackendProcess` (won't kill the existing process).
pub fn ensure_backend_running() -> BackendProcess {
    // Check if already running
    if is_backend_running() {
        tracing::info!("Backend already running at {}", HEALTH_URL);
        return BackendProcess::none();
    }

    if !is_port_available() {
        let msg =
            "Port 8000 is already in use, but the listener does not look like Makima. Stop the conflicting process and retry.";
        tracing::error!("{}", msg);
        return BackendProcess::with_error(msg);
    }

    tracing::info!("Backend not running, starting uvicorn...");

    let project_root = find_project_root();
    let backend_dir = project_root.join("apps").join("backend");

    if !backend_dir.exists() {
        let msg = format!(
            "Backend directory not found: {:?}. Set MAKIMA_PROJECT_ROOT to the makima-agent project root.",
            backend_dir
        );
        tracing::warn!("{}", msg);
        return BackendProcess::with_error(msg);
    }

    tracing::info!("Project root: {:?}", project_root);
    tracing::info!("Backend dir:  {:?}", backend_dir);

    // Determine python executable
    let python = std::env::var("PYTHON").unwrap_or_else(|_| {
        if cfg!(windows) {
            "python".to_string()
        } else {
            "python3".to_string()
        }
    });

    let (stdout_log, stderr_log, log_path) = match prepare_backend_log_file() {
        Ok(files) => files,
        Err(e) => {
            let msg = format!("Failed to prepare backend startup log: {}", e);
            tracing::error!("{}", msg);
            return BackendProcess::with_error(msg);
        }
    };

    // Spawn uvicorn from the project root so that pydantic-settings
    // can find the .env file.  Use --app-dir to point uvicorn at
    // the Python package source directory inside apps/backend/src.
    let app_dir = format!("apps{}backend{}src", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR);
    let result = Command::new(&python)
        .args([
            "-m",
            "uvicorn",
            "makima.app:app",
            "--app-dir",
            &app_dir,
            "--host",
            "127.0.0.1",
            "--port",
            "8000",
        ])
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        // Let the backend parse complex values like JSON arrays from the .env file
        // itself. Inherited values loaded by dotenvy can arrive already de-quoted
        // and break pydantic-settings parsing.
        .env_remove("MAKIMA_API_CORS_ORIGINS")
        .current_dir(&project_root)
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log))
        .spawn();

    match result {
        Ok(mut child) => {
            let pid = child.id();
            tracing::info!("Backend process started (PID: {})", pid);

            // Wait for it to become ready
            let timeout = Duration::from_secs(STARTUP_TIMEOUT_SECS);
            if wait_for_backend(&mut child, timeout) {
                tracing::info!("Backend ready at {}", HEALTH_URL);
                BackendProcess::new(child)
            } else {
                let detail = summarize_startup_error(&log_path).unwrap_or_else(|| {
                    format!(
                        "Backend did not become ready within {} seconds. Check .env configuration and Python dependencies.",
                        STARTUP_TIMEOUT_SECS
                    )
                });
                let msg = format!("Backend startup failed: {}", detail);
                tracing::error!("{}", msg);

                // Kill the failed process (including children on Windows)
                #[cfg(windows)]
                {
                    let _ = Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
                let mut child = child;
                let _ = child.kill();
                let _ = child.wait();
                BackendProcess::with_error(msg)
            }
        }
        Err(e) => {
            let msg = format!(
                "Failed to start backend process: {}. Make sure Python '{}' is installed and in PATH.",
                e, python
            );
            tracing::error!("{}", msg);
            BackendProcess::with_error(msg)
        }
    }
}
