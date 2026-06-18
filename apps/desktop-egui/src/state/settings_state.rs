use serde::{Deserialize, Serialize};

/// Mirror of Zoo-Code's ModeConfig structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeConfig {
    pub slug: String,
    pub name: String,
    pub role_definition: String,
    pub when_to_use: Option<String>,
    pub description: Option<String>,
    pub custom_instructions: Option<String>,
    pub groups: Vec<String>,
    pub source: Option<String>, // "global" or "project"
}

/// Mirror of Zoo-Code's McpServer + McpTool structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub transport_type: McpTransportType,
    pub status: McpConnectionStatus,
    pub error: Option<String>,
    pub tools: Vec<McpToolConfig>,
    pub enabled: bool,
    pub last_health_check: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpTransportType {
    Stdio,
    Sse,
    StreamableHttp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpConnectionStatus {
    Connected,
    Connecting,
    Disconnected,
    Error,
}

/// Mirror of Zoo-Code's McpTool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolConfig {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    pub always_allow: bool,
    pub enabled: bool,
}

/// Voice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    pub tts_provider: String,
    pub active_voice_id: Option<String>,
    pub push_to_talk: bool,
    pub mic_device: Option<String>,
    pub speaker_device: Option<String>,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            tts_provider: "none".to_string(),
            active_voice_id: None,
            push_to_talk: true,
            mic_device: None,
            speaker_device: None,
        }
    }
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub max_steps: u32,
    pub timeout_seconds: u32,
    pub configured: bool,
    pub provider_configured: bool,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            base_url: String::new(),
            model: String::new(),
            temperature: 0.7,
            max_steps: 100,
            timeout_seconds: 300,
            configured: false,
            provider_configured: false,
        }
    }
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub backend: bool,
    pub auth: bool,
    pub sse_connected: bool,
    pub api_base_url: String,
}

/// Settings state for the right inspector
#[derive(Debug, Clone)]
pub struct SettingsState {
    pub modes: Vec<ModeConfig>,
    pub active_mode_slug: Option<String>,
    pub mcp_servers: Vec<McpServerConfig>,
    pub model_config: ModelConfig,
    pub voice_config: VoiceConfig,
    pub health: HealthStatus,
    pub token_estimate_per_1k: f64, // assumed cost per 1k tokens for estimation
    pub config_path: Option<String>,
    pub show_secret: bool,
    // Backend-populated data
    pub memory_items: Vec<String>,
    pub knowledge_docs: Vec<crate::api::knowledge::ApiDocument>,
    pub knowledge_results: Vec<String>,
    pub audit_entries: Vec<crate::api::audit::AuditEntry>,
    // Persona fields
    pub persona_name: String,
    pub persona_is_default: bool,
    pub persona_modified: bool,
    pub persona_default_preview: String,
    pub persona_draft: String,
    // Persona parameters
    pub persona_warmth: f64,
    pub persona_verbosity: f64,
    pub persona_strictness: f64,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            modes: Vec::new(),
            active_mode_slug: None,
            mcp_servers: Vec::new(),
            model_config: ModelConfig::default(),
            voice_config: VoiceConfig::default(),
            health: HealthStatus {
                backend: false,
                auth: false,
                sse_connected: false,
                api_base_url: "http://localhost:8000".to_string(),
            },
            token_estimate_per_1k: 0.003,
            config_path: None,
            show_secret: false,
            memory_items: Vec::new(),
            knowledge_docs: Vec::new(),
            knowledge_results: Vec::new(),
            audit_entries: Vec::new(),
            persona_name: "Default".to_string(),
            persona_is_default: true,
            persona_modified: false,
            persona_default_preview: "You are Makima, a helpful AI assistant.".to_string(),
            persona_draft: String::new(),
            persona_warmth: 0.7,
            persona_verbosity: 0.5,
            persona_strictness: 0.6,
        }
    }
}

impl SettingsState {
    pub fn active_mode(&self) -> Option<&ModeConfig> {
        self.active_mode_slug
            .as_ref()
            .and_then(|slug| self.modes.iter().find(|m| m.slug == *slug))
    }
}