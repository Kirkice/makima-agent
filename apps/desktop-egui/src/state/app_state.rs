use crate::state::chat_state::ChatState;
use crate::state::settings_state::SettingsState;
use crate::state::task_state::TaskState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Modes,
    Persona,
    Memory,
    Knowledge,
    Voice,
    Mcp,
    Audit,
    ModelConfig,
    Diagnostics,
}

/// Root application state - the single source of truth for the entire UI.
/// Accessed via Arc<Mutex<AppState>> across the tokio/egui boundary.
pub struct AppState {
    pub chat: ChatState,
    pub task: TaskState,
    pub settings: SettingsState,
    pub auth_token: Option<String>,
    pub is_logged_in: bool,
    pub server_url: String,
    pub app_config_path: Option<String>,
    pub status_message: Option<String>,
    pub show_login: bool,
    pub show_settings: bool,
    pub show_diagnostics: bool,
    pub show_panel: Option<PanelKind>,
    // Panel-specific temp state
    pub memory_search_query: String,
    pub knowledge_query: String,
    pub voice_tab_index: usize,
    pub audit_severity_filter: String,
    // Modals
    pub show_modal_mode_reload: bool,
    pub show_modal_mode_create: bool,
    pub show_modal_model_edit: bool,
    pub show_persona_default: bool,
    pub show_modal_persona_edit: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            chat: ChatState::default(),
            task: TaskState::default(),
            settings: SettingsState::default(),
            auth_token: None,
            is_logged_in: false,
            server_url: "http://localhost:8000".to_string(),
            app_config_path: None,
            status_message: None,
            show_login: false,
            show_settings: false,
            show_diagnostics: false,
            show_panel: None,
            memory_search_query: String::new(),
            knowledge_query: String::new(),
            voice_tab_index: 0,
            audit_severity_filter: "all".to_string(),
            show_modal_mode_reload: false,
            show_modal_mode_create: false,
            show_modal_model_edit: false,
            show_persona_default: false,
            show_modal_persona_edit: false,
        }
    }
}

impl AppState {
    /// Update the status message (displayed in status bar)
    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
    }

    /// Clear status message after a brief duration
    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    /// Calculate total token usage across all sessions
    pub fn total_token_usage(&self) -> u64 {
        self.chat
            .sessions
            .iter()
            .map(|s| s.estimated_token_count())
            .sum()
    }

    /// Calculate estimated total cost
    pub fn total_estimated_cost(&self) -> f64 {
        let tokens = self.total_token_usage();
        (tokens as f64 / 1000.0) * self.settings.token_estimate_per_1k
    }
}