use crate::state::chat_state::ChatState;
use crate::state::settings_state::SettingsState;
use crate::state::task_state::TaskState;
use crate::state::voice_state::VoiceCallState;

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
    Avatar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivitySection {
    Sessions,
    Resources,
    Agent,
    Integrations,
}

impl Default for ActivitySection {
    fn default() -> Self {
        Self::Sessions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerTab {
    TaskTimeline,
    VoiceCall,
    Audit,
    Diagnostics,
    McpActivity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Chat,
    Avatar,
}

impl Default for ViewMode {
    fn default() -> Self {
        ViewMode::Chat
    }
}

#[derive(Debug, Clone)]
pub enum ApiCommand {
    FetchModes,
    FetchPersona,
    ReloadPersona,
    FetchMemories,
    SearchMemories(String),
    DeleteMemory(String),
    FetchDocuments,
    RetrieveKnowledge(String),
    FetchMcpServers,
    ReconnectMcp(String),
    ToggleMcp(String, bool),
    FetchAuditLog,
    FetchVoiceSettings,
    StartVoiceCall {
        room_name: String,
        livekit_url: String,
        api_key: String,
        api_secret: String,
    },
    StopVoiceCall,
    ToggleVoiceMute,
}

pub struct AppState {
    pub chat: ChatState,
    pub task: TaskState,
    pub settings: SettingsState,
    pub voice_call: VoiceCallState,
    pub auth_token: Option<String>,
    pub is_logged_in: bool,
    pub server_url: String,
    pub app_config_path: Option<String>,
    pub status_message: Option<String>,
    pub show_login: bool,
    pub show_settings: bool,
    pub show_diagnostics: bool,
    pub show_panel: Option<PanelKind>,
    pub view_mode: ViewMode,
    pub memory_search_query: String,
    pub knowledge_query: String,
    pub voice_tab_index: usize,
    pub audit_severity_filter: String,
    pub show_modal_mode_reload: bool,
    pub show_modal_mode_create: bool,
    pub show_modal_model_edit: bool,
    pub show_persona_default: bool,
    pub show_modal_persona_edit: bool,
    pub api_commands: Vec<ApiCommand>,
    pub activity_section: ActivitySection,
    pub drawer_open: bool,
    pub drawer_tab: Option<DrawerTab>,
    pub drawer_user_dismissed: bool,
    pub conversations_width: f32,
    pub inspector_width: f32,
    pub drawer_height: f32,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            chat: ChatState::default(),
            task: TaskState::default(),
            settings: SettingsState::default(),
            voice_call: VoiceCallState::default(),
            auth_token: None,
            is_logged_in: false,
            server_url: "http://localhost:8000".to_string(),
            app_config_path: None,
            status_message: None,
            show_login: false,
            show_settings: false,
            show_diagnostics: false,
            show_panel: None,
            view_mode: ViewMode::default(),
            memory_search_query: String::new(),
            knowledge_query: String::new(),
            voice_tab_index: 0,
            audit_severity_filter: "all".to_string(),
            show_modal_mode_reload: false,
            show_modal_mode_create: false,
            show_modal_model_edit: false,
            show_persona_default: false,
            show_modal_persona_edit: false,
            api_commands: Vec::new(),
            activity_section: ActivitySection::default(),
            drawer_open: false,
            drawer_tab: None,
            drawer_user_dismissed: false,
            conversations_width: 280.0,
            inspector_width: 284.0,
            drawer_height: 220.0,
        }
    }
}

impl AppState {
    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    pub fn total_token_usage(&self) -> u64 {
        self.chat
            .sessions
            .iter()
            .map(|s| s.estimated_token_count())
            .sum()
    }

    pub fn total_estimated_cost(&self) -> f64 {
        (self.total_token_usage() as f64 / 1000.0) * self.settings.token_estimate_per_1k
    }
}
