use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8000";

pub fn normalize_server_url(server_url: impl Into<String>) -> String {
    let server_url = server_url.into();
    server_url
        .replace("http://localhost:", "http://127.0.0.1:")
        .replace("https://localhost:", "https://127.0.0.1:")
}

/// Application configuration stored locally as JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Backend server URL
    pub server_url: String,
    /// Window state
    pub window_width: f32,
    pub window_height: f32,
    /// Whether to auto-connect on start
    pub auto_connect: bool,
    /// Last used session ID
    pub last_session_id: Option<String>,
    /// Token cost estimate per 1k tokens (USD)
    pub token_cost_per_1k: f64,
    /// Layout — persisted dock sizes
    pub sidebar_width: f32,
    pub inspector_width: f32,
    pub drawer_height: f32,
    pub show_context_panel: bool,
    pub drawer_open: bool,
    /// Whether to auto-start the backend on app launch
    #[serde(default = "default_true")]
    pub auto_start_backend: bool,
}

fn default_true() -> bool { true }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server_url: DEFAULT_SERVER_URL.to_string(),
            window_width: 1400.0,
            window_height: 900.0,
            auto_connect: true,
            last_session_id: None,
            token_cost_per_1k: 0.003,
            sidebar_width: 300.0,
            inspector_width: 210.0,
            drawer_height: 155.0,
            show_context_panel: true,
            drawer_open: false,
            auto_start_backend: true,
        }
    }
}

impl AppConfig {
    /// Get the project directories for this application
    pub fn project_dirs() -> Option<ProjectDirs> {
        ProjectDirs::from("com", "makima", "makima-agent")
    }

    /// Get the config directory path
    pub fn config_dir() -> Result<PathBuf> {
        let dirs = Self::project_dirs()
            .context("Failed to determine project directories")?;
        Ok(dirs.config_dir().to_path_buf())
    }

    /// Get the config file path
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.json"))
    }

    /// Load config from disk
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            let config = AppConfig::default();
            config.save()?;
            return Ok(config);
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;

        let mut config: AppConfig = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {:?}", path))?;
        config.server_url = normalize_server_url(config.server_url);

        Ok(config)
    }

    /// Save config to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }

        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;

        std::fs::write(&path, &content)
            .with_context(|| format!("Failed to write config file: {:?}", path))?;

        Ok(())
    }

    /// Get a display-friendly config path string
    pub fn config_path_display() -> String {
        Self::config_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    }
}
