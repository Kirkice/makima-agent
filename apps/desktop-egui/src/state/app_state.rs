use crate::config::app_config::DEFAULT_SERVER_URL;
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
    UpdatePersona { draft: String },
    FetchMemories,
    SearchMemories(String),
    DeleteMemory(String),
    FetchDocuments,
    RetrieveKnowledge(String),
    FetchMcpServers,
    ReconnectMcp(String),
    ToggleMcp(String, bool),
    FetchAuditLog,
    /// Query audit log with optional severity filter
    QueryAuditLog { severity: Option<String> },
    FetchVoiceSettings,
    StartVoiceCall {
        room_name: String,
        livekit_url: String,
        api_key: String,
        api_secret: String,
    },
    StopVoiceCall,
    ToggleVoiceMute,
    /// Delete a conversation by its ID (UUID string)
    DeleteSession(String),
    /// Create a custom mode on the backend
    CreateMode {
        slug: String,
        name: String,
        role_definition: String,
        when_to_use: Option<String>,
        description: Option<String>,
        custom_instructions: Option<String>,
        tool_groups: Vec<String>,
        max_steps: i32,
        temperature: f64,
    },
    /// Delete a custom mode by slug
    DeleteMode(String),
    /// Fetch a single mode by slug
    FetchModeById(String),
    /// Reload modes from config file
    ReloadModes,
    /// Refresh backend health status
    RefreshHealth,
    /// Test connection to backend (health probe)
    TestConnection,
    /// Fetch model config from backend config store
    FetchModelConfig,
    /// Save model config to backend config store
    SaveModelConfig,
    /// Test model connection (health probe + status)
    TestModelConnection,
    /// Save voice settings to backend (PUT /api/voice/settings)
    SaveVoiceSettings,

    // Model profiles (multi-LLM configuration, like Zoo-Code)
    FetchModelProfiles,
    CreateModelProfile {
        name: String,
        profile: crate::api::model_profiles::ModelProfile,
    },
    UpdateModelProfile {
        name: String,
        profile: crate::api::model_profiles::ModelProfile,
    },
    DeleteModelProfile(String),
    ActivateModelProfile(String),
    TestModelProfileConnection {
        base_url: String,
        api_key: Option<String>,
        model: String,
    },
    FetchProviderTypes,
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
    pub login_error: Option<String>,
    pub login_in_progress: bool,
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
    /// Window flags to open orphaned panels as floating windows
    pub show_window_mcp: bool,
    pub show_window_audit: bool,
    pub show_window_persona: bool,
    pub show_window_modes: bool,
    pub show_window_memory: bool,
    pub show_window_knowledge: bool,
    pub show_window_model_config: bool,
    pub show_window_diagnostics: bool,
    pub show_window_voice: bool,
    /// Selected audit entry index for Detail view
    pub audit_detail_index: Option<usize>,
    pub api_commands: Vec<ApiCommand>,
    pub activity_section: ActivitySection,
    pub drawer_open: bool,
    pub drawer_tab: Option<DrawerTab>,
    pub drawer_user_dismissed: bool,
    pub show_context_panel: bool,
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
            server_url: DEFAULT_SERVER_URL.to_string(),
            app_config_path: None,
            status_message: None,
            login_error: None,
            login_in_progress: false,
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
            show_window_mcp: false,
            show_window_audit: false,
            show_window_persona: false,
            show_window_modes: false,
            show_window_memory: false,
            show_window_knowledge: false,
            show_window_model_config: false,
            show_window_diagnostics: false,
            show_window_voice: false,
            audit_detail_index: None,
            api_commands: Vec::new(),
            activity_section: ActivitySection::default(),
            drawer_open: false,
            drawer_tab: None,
            drawer_user_dismissed: false,
            show_context_panel: true,
            conversations_width: 300.0,
            inspector_width: 210.0,
            drawer_height: 155.0,
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
