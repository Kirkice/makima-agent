use std::sync::Arc;
use std::sync::Mutex;
use tokio::runtime::Runtime;
use uuid::Uuid;

use crate::api::{auth::AuthApi, health::HealthApi, sessions::SessionsApi, tasks::TasksApi};
use crate::config::app_config::AppConfig;
use crate::config::secure_store::SecureStore;
use crate::state::app_state::{AppState, ViewMode};
use crate::ui;
use crate::ui::dock::{normalize_layout, AppDockState, init_app_dock};
use crate::voice::VoiceManager;

pub struct LoginDialogState {
    pub username: String,
    pub password: String,
    pub server_url: String,
    pub error: Option<String>,
    pub loading: bool,
}

impl Default for LoginDialogState {
    fn default() -> Self {
        Self { username: String::new(), password: String::new(), server_url: "http://localhost:8000".to_string(), error: None, loading: false }
    }
}

pub enum UiAction {
    Login,
    SendMessage,
    Logout,
    RefreshSessions,
}

pub struct MakimaApp {
    pub state: Arc<Mutex<AppState>>,
    pub runtime: Runtime,
    pub client: reqwest::Client,
    pub config: AppConfig,
    pub secure_store: SecureStore,
    pub initialized: bool,
    pub login_dialog: LoginDialogState,
    pub pending_action: Option<UiAction>,
    pub voice_manager: VoiceManager,
    pub app_dock: AppDockState,
    pub layout_changed: bool,
    pub last_save_frame: u64,
}

impl Default for MakimaApp {
    fn default() -> Self {
        let state = Arc::new(Mutex::new(AppState::default()));
        let runtime = Runtime::new().expect("Failed to create Tokio runtime");
        let config = AppConfig::load().unwrap_or_default();
        let secure_store = SecureStore::new();
        let mut login_dialog = LoginDialogState::default();
        login_dialog.server_url = config.server_url.clone();

        // Restore persisted layout
        {
            let mut s = state.lock().unwrap();
            s.conversations_width = config.sidebar_width;
            s.inspector_width = config.inspector_width;
            s.drawer_height = config.drawer_height;
            s.show_context_panel = config.show_context_panel;
            s.drawer_open = config.drawer_open;
            s.server_url = config.server_url.clone();
            normalize_layout(&mut s);
        }

        if let Some(token) = secure_store.get_token() {
            if let Ok(mut s) = state.lock() { s.auth_token = Some(token); s.is_logged_in = true; s.server_url = config.server_url.clone(); }
        }
        let client = reqwest::Client::builder().user_agent("makima-desktop/0.1.0").build().expect("Failed to create HTTP client");

        // Initialize app_dock before moving state into Self
        let app_dock = {
            let state_guard = state.lock().unwrap();
            init_app_dock(
                ViewMode::Chat,
                state_guard.show_context_panel,
                state_guard.conversations_width,
                state_guard.inspector_width,
                egui::vec2(config.window_width, config.window_height),
            )
        };

        Self {
            state,
            runtime,
            client,
            config,
            secure_store,
            initialized: false,
            login_dialog,
            pending_action: None,
            voice_manager: VoiceManager::default(),
            app_dock,
            layout_changed: false,
            last_save_frame: 0,
        }
    }
}

impl MakimaApp {
    fn exec_login(&mut self) {
        self.login_dialog.loading = true;
        self.login_dialog.error = None;

        let username = self.login_dialog.username.clone();
        let password = self.login_dialog.password.clone();
        let server_url = self.login_dialog.server_url.clone();
        let state = self.state.clone();
        let client = self.client.clone();

        self.config.server_url = server_url.clone();
        let _ = self.config.save();

        if let Ok(mut s) = self.state.lock() {
            s.login_in_progress = true;
            s.login_error = None;
        }

        self.runtime.spawn(async move {
            let auth_api = AuthApi::new(client.clone(), server_url.clone());
            match auth_api.login(&username, &password).await {
                Ok(resp) => {
                    let token = resp.access_token;
                    SecureStore::new().store_token(&token).ok();

                    {
                        let mut s = state.lock().unwrap();
                        s.server_url = server_url.clone();
                        s.auth_token = Some(token.clone());
                        s.is_logged_in = true;
                        s.login_in_progress = false;
                        s.login_error = None;
                        s.show_login = false;
                        s.set_status("Logged in".to_string());
                    }

                    // Fetch sessions
                    let sessions_api = SessionsApi::new(client, server_url, token);
                    if let Ok(list) = sessions_api.list().await {
                        let mut s = state.lock().unwrap();
                        for api_s in list {
                            let title = api_s.title.unwrap_or_else(|| "Untitled".to_string());
                            let backend_id = Some(api_s.id.clone());
                            let mut session = crate::state::chat_state::Session::new(title, backend_id);
                            // Preserve the backend session ID so we can use it for /tasks
                            session.id = Uuid::parse_str(&api_s.id).unwrap_or_else(|_| session.id);
                            s.chat.sessions.push(session);
                        }
                        if !s.chat.sessions.is_empty() { s.chat.active_session_id = Some(s.chat.sessions[0].id); }
                        s.set_status("Sessions loaded".to_string());
                    }
                }
                Err(e) => {
                    let mut s = state.lock().unwrap();
                    s.login_in_progress = false;
                    s.login_error = Some(e.to_string());
                    s.set_status(format!("Login failed: {}", e));
                }
            }
        });
    }

    fn exec_send_message(&mut self) {
        let mut s = self.state.lock().unwrap();
        let text = s.chat.composer.input.trim().to_string();
        if text.is_empty() { return; }

        if s.chat.active_session().is_none() {
            let n = s.chat.sessions.len();
            s.chat.create_session(format!("Chat {}", n + 1));
        }

        let session_id = match s.chat.active_session_id {
            Some(id) => id.to_string(),
            None => { s.set_status("No active session".to_string()); return; }
        };
        let token = match &s.auth_token {
            Some(t) => t.clone(),
            None => { s.set_status("Not authenticated".to_string()); return; }
        };
        let server_url = s.server_url.clone();

        // Add user message
        if let Some(session) = s.chat.active_session_mut() {
            let msg = crate::state::chat_state::ChatMessage {
                ts: chrono::Utc::now().timestamp_millis(),
                msg_type: crate::state::chat_state::MessageType::Ask,
                ask: None, say: None,
                text: Some(text.clone()), partial: false,
                reasoning: None, token_usage: None, tool_call_id: None, error: None,
                id: Uuid::new_v4(), session_id: session.id,
            };
            session.messages.push(msg);
            session.updated_at = chrono::Utc::now();
        }
        s.chat.composer.input.clear();
        s.chat.composer.is_streaming = true;
        s.set_status("Sending...".to_string());
        drop(s);

        let state = self.state.clone();
        let client = self.client.clone();

        self.runtime.spawn(async move {
            let tasks_api = TasksApi::new(client, server_url, token);
            match tasks_api.stream(session_id, text).await {
                Ok(mut rx) => {
                    while let Some(event_result) = rx.recv().await {
                        let mut s = state.lock().unwrap();
                        match event_result {
                            Ok(event) => handle_sse_event(&mut s, event),
                            Err(e) => { s.set_status(format!("Stream error: {}", e)); s.chat.composer.is_streaming = false; }
                        }
                    }
                    let mut s = state.lock().unwrap();
                    s.chat.composer.is_streaming = false;
                    s.set_status("Done".to_string());
                }
                Err(e) => {
                    let mut s = state.lock().unwrap();
                    s.set_status(format!("Failed: {}", e));
                    s.chat.composer.is_streaming = false;
                }
            }
        });
    }
}

fn handle_sse_event(state: &mut AppState, event: crate::state::task_state::TaskEvent) {
    use crate::state::chat_state::{AskKind, ChatMessage, MessageType, SayKind, TokenUsage};
    use crate::state::task_state::{TaskEvent, TaskStatus, TimelinePhase};
    use chrono::Utc;

    let session_id = state.chat.active_session_id.unwrap_or_else(Uuid::new_v4);

    match event {
        TaskEvent::TaskStarted { .. } => {
            let mut task = crate::state::task_state::TaskExecutionState::default();
            task.status = TaskStatus::Running;
            state.task.active_task = Some(task);
        }
        TaskEvent::TaskCompleted { .. } => { if let Some(t) = state.task.active_task.as_mut() { t.status = TaskStatus::Idle; } }
        TaskEvent::TaskError { error, .. } => {
            if let Some(t) = state.task.active_task.as_mut() { t.status = TaskStatus::Idle; t.error = Some(error.clone()); }
            if let Some(session) = state.chat.active_session_mut() {
                let err_clone = error.clone();
                session.messages.push(ChatMessage { ts: Utc::now().timestamp_millis(), msg_type: MessageType::Say, ask: None, say: Some(SayKind::Error), text: Some(error), partial: false, reasoning: None, token_usage: None, tool_call_id: None, error: Some(err_clone), id: Uuid::new_v4(), session_id });
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::Message { message, action, .. } => {
            let text = message.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let is_partial = action == "updated";
            if let Some(session) = state.chat.active_session_mut() {
                if is_partial {
                    if let Some(last) = session.messages.last_mut() {
                        if last.partial && last.msg_type == MessageType::Say { last.text = Some(text); return; }
                    }
                }
                session.messages.push(ChatMessage { ts: Utc::now().timestamp_millis(), msg_type: MessageType::Say, ask: None, say: Some(SayKind::Text), text: Some(text), partial: is_partial, reasoning: None, token_usage: None, tool_call_id: None, error: None, id: Uuid::new_v4(), session_id });
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::ToolStart { tool_name, .. } => {
            if let Some(t) = state.task.active_task.as_mut() { t.add_timeline_entry(TimelinePhase::ToolDispatch, tool_name.clone(), None); }
        }
        TaskEvent::ToolResult { tool_name, result } => {
            let text = result.as_str().map(|s| truncate(s, 200)).unwrap_or_else(|| format!("{:?}", result));
            if let Some(session) = state.chat.active_session_mut() {
                session.messages.push(ChatMessage { ts: Utc::now().timestamp_millis(), msg_type: MessageType::Say, ask: None, say: Some(SayKind::Tool), text: Some(format!("✅ {}: {}", tool_name, text)), partial: false, reasoning: None, token_usage: None, tool_call_id: None, error: None, id: Uuid::new_v4(), session_id });
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::ToolError { tool_name, error } => {
            if let Some(session) = state.chat.active_session_mut() {
                session.messages.push(ChatMessage { ts: Utc::now().timestamp_millis(), msg_type: MessageType::Say, ask: None, say: Some(SayKind::Tool), text: Some(format!("❌ {}: {}", tool_name, error)), partial: false, reasoning: None, token_usage: None, tool_call_id: None, error: Some(error.clone()), id: Uuid::new_v4(), session_id });
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::Thinking { content } => {
            if let Some(t) = state.task.active_task.as_mut() { t.add_timeline_entry(TimelinePhase::Thinking, "Thinking".to_string(), Some(content)); }
        }
        TaskEvent::TokenUsage { tokens_in, tokens_out, cost } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.token_usage = TokenUsage { total_tokens_in: tokens_in, total_tokens_out: tokens_out, total_cache_writes: None, total_cache_reads: None, total_cost: cost, context_tokens: 0 };
            }
            state.set_status(format!("Tokens: ↑{} ↓{} ${:.5}", tokens_in, tokens_out, cost));
        }
        TaskEvent::ModeSwitch { from_mode, to_mode, mode_name } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.add_timeline_entry(TimelinePhase::ModeSwitch, format!("{} → {}", from_mode, to_mode), Some(mode_name.clone()));
            }
            if let Some(session) = state.chat.active_session_mut() {
                session.messages.push(ChatMessage { ts: Utc::now().timestamp_millis(), msg_type: MessageType::Say, ask: None, say: Some(SayKind::Text), text: Some(format!("🔄 Mode switched: {} → {} ({})", from_mode, to_mode, mode_name)), partial: false, reasoning: None, token_usage: None, tool_call_id: None, error: None, id: Uuid::new_v4(), session_id });
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::ApprovalRequested { request_id, tool_name, risk_level, .. } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.status = TaskStatus::Interactive;
                t.add_timeline_entry(TimelinePhase::ApprovalRequested, format!("Approval: {}", tool_name), Some(format!("Risk: {}, ID: {}", risk_level, request_id)));
            }
            if let Some(session) = state.chat.active_session_mut() {
                session.messages.push(ChatMessage { ts: Utc::now().timestamp_millis(), msg_type: MessageType::Ask, ask: Some(AskKind::Followup), say: None, text: Some(format!("⏳ Approval requested for `{}` (risk: {})", tool_name, risk_level)), partial: false, reasoning: None, token_usage: None, tool_call_id: None, error: None, id: Uuid::new_v4(), session_id });
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::ApprovalResponded { approved, .. } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.status = TaskStatus::Running;
            }
            if let Some(session) = state.chat.active_session_mut() {
                let status_text = if approved { "✅ Approved" } else { "❌ Rejected" };
                session.messages.push(ChatMessage { ts: Utc::now().timestamp_millis(), msg_type: MessageType::Say, ask: None, say: Some(SayKind::Text), text: Some(format!("Approval response: {}", status_text)), partial: false, reasoning: None, token_usage: None, tool_call_id: None, error: None, id: Uuid::new_v4(), session_id });
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::CheckpointSaved { checkpoint_id, label } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.add_timeline_entry(TimelinePhase::Checkpoint, format!("Checkpoint: {}", label), Some(checkpoint_id.clone()));
            }
        }
        TaskEvent::CheckpointRestored { checkpoint_id, label } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.add_timeline_entry(TimelinePhase::Checkpoint, format!("Restored: {}", label), Some(checkpoint_id.clone()));
            }
            if let Some(session) = state.chat.active_session_mut() {
                session.messages.push(ChatMessage { ts: Utc::now().timestamp_millis(), msg_type: MessageType::Say, ask: None, say: Some(SayKind::Text), text: Some(format!("📌 Restored checkpoint: {}", label)), partial: false, reasoning: None, token_usage: None, tool_call_id: None, error: None, id: Uuid::new_v4(), session_id });
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::ContextCompressed { original_tokens, compressed_tokens } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.add_timeline_entry(TimelinePhase::ContextCompressed, "Context Compressed".to_string(), Some(format!("{} → {} tokens", original_tokens, compressed_tokens)));
            }
        }
        TaskEvent::RetryDelayed { attempt, delay_seconds, reason } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.add_timeline_entry(TimelinePhase::RetryDelayed, format!("Retry #{}", attempt), Some(format!("Delay: {:.1}s, Reason: {}", delay_seconds, reason)));
            }
            state.set_status(format!("Retrying in {:.1}s (attempt {}): {}", delay_seconds, attempt, reason));
        }
        _ => {}
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}…", &s[..max]) }
}

async fn load_sessions_into_state(
    state: Arc<Mutex<AppState>>,
    client: reqwest::Client,
    server_url: String,
    token: String,
) {
    let sessions_api = SessionsApi::new(client, server_url, token);
    if let Ok(list) = sessions_api.list().await {
        let mut s = state.lock().unwrap();
        s.chat.sessions.clear();
        for api_s in list {
            let title = api_s.title.unwrap_or_else(|| "Untitled".to_string());
            let backend_id = Some(api_s.id.clone());
            let mut session = crate::state::chat_state::Session::new(title, backend_id);
            session.id = Uuid::parse_str(&api_s.id).unwrap_or_else(|_| session.id);
            s.chat.sessions.push(session);
        }
        if !s.chat.sessions.is_empty() {
            s.chat.active_session_id = Some(s.chat.sessions[0].id);
        }
    }
}

async fn try_env_auto_login(
    state: Arc<Mutex<AppState>>,
    client: reqwest::Client,
    server_url: String,
) -> bool {
    let env_user = std::env::var("MAKIMA_CLI_USERNAME").unwrap_or_default();
    let env_pass = std::env::var("MAKIMA_CLI_PASSWORD").unwrap_or_default();

    if env_user.is_empty() || env_pass.is_empty() {
        return false;
    }

    let auth_api = AuthApi::new(client.clone(), server_url.clone());
    let Ok(resp) = auth_api.login(&env_user, &env_pass).await else {
        return false;
    };

    let token = resp.access_token;
    SecureStore::new().store_token(&token).ok();
    {
        let mut s = state.lock().unwrap();
        s.auth_token = Some(token.clone());
        s.is_logged_in = true;
        s.show_login = false;
        s.set_status("Auto-login via env".to_string());
    }

    load_sessions_into_state(state, client, server_url, token).await;
    true
}

impl eframe::App for MakimaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.initialized {
            self.initialized = true;
            crate::theme::apply_theme(ctx);
            self.bootstrap();
        }

        // Process pending actions (these may spawn async tasks)
        if let Some(action) = self.pending_action.take() {
            match action {
                UiAction::Login => self.exec_login(),
                UiAction::SendMessage => self.exec_send_message(),
                UiAction::Logout => {
                    let mut s = self.state.lock().unwrap();
                    s.is_logged_in = false; s.auth_token = None; s.show_login = true;
                    s.login_in_progress = false;
                    s.login_error = None;
                    self.login_dialog.loading = false; // Reset loading state
                    let _ = self.secure_store.delete_token();
                }
                UiAction::RefreshSessions => {
                    let s = self.state.lock().unwrap();
                    let token = s.auth_token.clone();
                    let server_url = s.server_url.clone();
                    drop(s);
                    if let Some(t) = token {
                        let state = self.state.clone();
                        let client = self.client.clone();
                        self.runtime.spawn(async move {
                            let api = SessionsApi::new(client, server_url, t);
                            if let Ok(list) = api.list().await {
                                let mut s = state.lock().unwrap();
                                for api_s in list {
                                    let title = api_s.title.unwrap_or_else(|| "Untitled".to_string());
                                    let backend_id = Some(api_s.id.clone());
                                    let mut session = crate::state::chat_state::Session::new(title, backend_id);
                                    session.id = Uuid::parse_str(&api_s.id).unwrap_or_else(|_| session.id);
                                    s.chat.sessions.push(session);
                                }
                                s.set_status("Sessions refreshed".to_string());
                            }
                        });
                    }
                }
            }
        }

        // Process API commands from panels
        let commands: Vec<_> = {
            let mut state = self.state.lock().unwrap();
            state.api_commands.drain(..).collect()
        };

        for cmd in commands {
            self.exec_api_command(cmd);
        }

        // Render UI - only hold mutex during rendering
        // Handle potential mutex poisoning from panics
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!("Mutex was poisoned, recovering...");
                poisoned.into_inner()
            }
        };

        self.login_dialog.loading = state.login_in_progress;
        self.login_dialog.error = state.login_error.clone();
        
        egui::CentralPanel::default().show(ctx, |ui| {
            ui::shell::draw(ui, &mut state, &mut self.login_dialog, &mut self.pending_action, &mut self.app_dock);
        });
        ctx.request_repaint();

        // Persist layout changes (throttled to ~1 save per second)
        self.last_save_frame += 1;
        if self.last_save_frame % 60 == 0 {
            let mut need_save = self.layout_changed;
            self.layout_changed = false;

            let sidebar_w = state.conversations_width;
            let inspector_w = state.inspector_width;
            let drawer_h = state.drawer_height;
            let show_ctx = state.show_context_panel;
            let drawer_open = state.drawer_open;

            if (self.config.sidebar_width - sidebar_w).abs() > 1.0 {
                self.config.sidebar_width = sidebar_w;
                need_save = true;
            }
            if (self.config.inspector_width - inspector_w).abs() > 1.0 {
                self.config.inspector_width = inspector_w;
                need_save = true;
            }
            if (self.config.drawer_height - drawer_h).abs() > 1.0 {
                self.config.drawer_height = drawer_h;
                need_save = true;
            }
            if self.config.show_context_panel != show_ctx {
                self.config.show_context_panel = show_ctx;
                need_save = true;
            }
            if self.config.drawer_open != drawer_open {
                self.config.drawer_open = drawer_open;
                need_save = true;
            }

            if need_save {
                let _ = self.config.save();
            }
        }
    }
}

impl MakimaApp {
    fn exec_api_command(&mut self, cmd: crate::state::app_state::ApiCommand) {
        use crate::api::{audit::AuditApi, knowledge::KnowledgeApi, mcp::McpApi, memory::MemoryApi, modes::ModesApi, persona::PersonaApi, voice::VoiceApi};
        use crate::state::app_state::ApiCommand;

        // Voice commands are handled directly on self.voice_manager (not spawned)
        #[cfg(feature = "voice")]
        match &cmd {
            ApiCommand::StartVoiceCall { room_name, livekit_url, api_key, api_secret } => {
                self.voice_manager.room_name = room_name.clone();
                self.voice_manager.livekit_url = livekit_url.clone();
                self.voice_manager.api_key = api_key.clone();
                self.voice_manager.api_secret = api_secret.clone();

                let state = self.state.clone();
                let rt = &self.runtime;
                rt.spawn(async move {
                    // We can't move voice_manager into the task, so we just
                    // set the status here. The actual connect happens in the
                    // next frame via the pending_action pattern.
                    // For now, we just acknowledge the request.
                    let mut s = state.lock().unwrap();
                    s.voice_call.status = "Connecting...".to_string();
                    s.set_status("Voice call: connecting...".to_string());
                });
                return;
            }
            ApiCommand::StopVoiceCall => {
                self.voice_manager.disconnect();
                let mut s = self.state.lock().unwrap();
                s.voice_call.is_connected = false;
                s.voice_call.is_connecting = false;
                s.voice_call.status = "Disconnected".to_string();
                s.voice_call.call_duration_secs = 0;
                s.set_status("Voice call ended".to_string());
                return;
            }
            ApiCommand::ToggleVoiceMute => {
                self.voice_manager.toggle_mute();
                let mut s = self.state.lock().unwrap();
                s.voice_call.muted = self.voice_manager.muted;
                return;
            }
            _ => {}
        }

        // When voice feature is disabled, silently ignore voice commands
        #[cfg(not(feature = "voice"))]
        {
            let _ = &cmd; // suppress unused warning
        }

        let s = self.state.lock().unwrap();
        let token = match &s.auth_token { Some(t) => t.clone(), None => return };
        let server_url = s.server_url.clone();
        drop(s);

        let state = self.state.clone();
        let client = self.client.clone();

        self.runtime.spawn(async move {
            match cmd {
                ApiCommand::FetchModes => {
                    let api = ModesApi::new(client, server_url, token);
                    if let Ok(resp) = api.list().await {
                        let mut s = state.lock().unwrap();
                        s.settings.modes = resp.modes.into_iter().map(|m| crate::state::settings_state::ModeConfig {
                            slug: m.slug, name: m.name,
                            role_definition: m.role_definition.unwrap_or_default(),
                            when_to_use: m.when_to_use,
                            description: m.description,
                            custom_instructions: m.custom_instructions,
                            groups: m.tool_groups.iter().map(|tg| tg.group.clone()).collect(),
                            source: Some(m.source),
                        }).collect();
                        if s.settings.active_mode_slug.is_none() {
                            s.settings.active_mode_slug = s.settings.modes.first().map(|m| m.slug.clone());
                        }
                        s.set_status("Modes loaded".to_string());
                    }
                }
                ApiCommand::FetchPersona => {
                    let api = PersonaApi::new(client, server_url, token);
                    if let Ok(p) = api.get_current().await {
                        let mut s = state.lock().unwrap();
                        s.settings.persona_name = p.name.clone();
                        s.settings.persona_is_default = true; // backend doesn't track this
                        s.settings.persona_default_preview = format!(
                            "Identity: {}\nPersonality: {}\nStyle: {}",
                            p.identity, p.personality, p.speaking_style
                        );
                        s.set_status("Persona loaded".to_string());
                    }
                }
                ApiCommand::ReloadPersona => {
                    let api = PersonaApi::new(client, server_url, token);
                    if let Ok(p) = api.reload().await {
                        let mut s = state.lock().unwrap();
                        s.settings.persona_name = p.name.clone();
                        s.settings.persona_modified = false;
                        s.settings.persona_default_preview = format!(
                            "Identity: {}\nPersonality: {}\nStyle: {}",
                            p.identity, p.personality, p.speaking_style
                        );
                        s.set_status("Persona reloaded".to_string());
                    }
                }
                ApiCommand::FetchMemories => {
                    let api = MemoryApi::new(client, server_url, token);
                    if let Ok(list) = api.list().await {
                        let mut s = state.lock().unwrap();
                        s.settings.memory_items = list.into_iter().map(|m| m.memory).collect();
                        let count = s.settings.memory_items.len();
                        s.set_status(format!("{} memories loaded", count));
                    }
                }
                ApiCommand::SearchMemories(q) => {
                    let api = MemoryApi::new(client, server_url, token);
                    if let Ok(list) = api.search(&q).await {
                        let mut s = state.lock().unwrap();
                        s.settings.memory_items = list.into_iter().map(|m| m.memory).collect();
                        let count = s.settings.memory_items.len();
                        s.set_status(format!("{} results", count));
                    }
                }
                ApiCommand::DeleteMemory(id) => {
                    let api = MemoryApi::new(client, server_url, token);
                    let _ = api.delete(&id).await;
                    let mut s = state.lock().unwrap();
                    s.set_status("Memory deleted".to_string());
                }
                ApiCommand::DeleteSession(id) => {
                    let api = SessionsApi::new(client.clone(), server_url.clone(), token.clone());
                    let _ = api.delete(&id).await;
                    let mut s = state.lock().unwrap();
                    s.chat.delete_session(uuid::Uuid::parse_str(&id).unwrap_or_default());
                    s.set_status("Conversation deleted".to_string());
                }
                ApiCommand::FetchDocuments => {
                    let api = KnowledgeApi::new(client, server_url, token);
                    if let Ok(list) = api.list().await {
                        let mut s = state.lock().unwrap();
                        s.settings.knowledge_docs = list;
                        let count = s.settings.knowledge_docs.len();
                        s.set_status(format!("{} documents loaded", count));
                    }
                }
                ApiCommand::RetrieveKnowledge(q) => {
                    let api = KnowledgeApi::new(client, server_url, token);
                    if let Ok(result) = api.retrieve(&q).await {
                        let mut s = state.lock().unwrap();
                        s.settings.knowledge_results = result.results.into_iter().map(|c| c.content).collect();
                        s.set_status(format!("{} results", result.count));
                    }
                }
                ApiCommand::FetchMcpServers => {
                    let api = McpApi::new(client, server_url, token);
                    if let Ok(list) = api.list().await {
                        let mut s = state.lock().unwrap();
                        s.settings.mcp_servers = list.into_iter().map(|srv| crate::state::settings_state::McpServerConfig {
                            name: srv.name,
                            transport_type: match srv.transport_type.as_deref() {
                                Some("stdio") => crate::state::settings_state::McpTransportType::Stdio,
                                Some("sse") => crate::state::settings_state::McpTransportType::Sse,
                                _ => crate::state::settings_state::McpTransportType::StreamableHttp,
                            },
                            status: match srv.status.as_deref() {
                                Some("connected") => crate::state::settings_state::McpConnectionStatus::Connected,
                                Some("connecting") => crate::state::settings_state::McpConnectionStatus::Connecting,
                                Some("error") => crate::state::settings_state::McpConnectionStatus::Error,
                                _ => crate::state::settings_state::McpConnectionStatus::Disconnected,
                            },
                            error: srv.last_error,
                            tools: srv.tools.unwrap_or_default().into_iter().map(|t| crate::state::settings_state::McpToolConfig {
                                name: t.name, description: t.description, input_schema: None, always_allow: false, enabled: t.enabled.unwrap_or(true),
                            }).collect(),
                            enabled: srv.enabled.unwrap_or(true),
                            last_health_check: None,
                        }).collect();
                        s.set_status("MCP servers loaded".to_string());
                    }
                }
                ApiCommand::ReconnectMcp(name) => {
                    let api = McpApi::new(client, server_url, token);
                    let _ = api.reconnect(&name).await;
                    let mut s = state.lock().unwrap();
                    s.set_status(format!("Reconnecting {}...", name));
                }
                ApiCommand::ToggleMcp(name, enabled) => {
                    let api = McpApi::new(client, server_url, token);
                    let _ = api.toggle(&name, enabled).await;
                    let mut s = state.lock().unwrap();
                    if let Some(srv) = s.settings.mcp_servers.iter_mut().find(|x| x.name == name) {
                        srv.enabled = enabled;
                    }
                }
                ApiCommand::FetchAuditLog => {
                    let api = AuditApi::new(client, server_url, token);
                    if let Ok(list) = api.list().await {
                        let mut s = state.lock().unwrap();
                        s.settings.audit_entries = list;
                        s.set_status("Audit log loaded".to_string());
                    }
                }
                ApiCommand::FetchVoiceSettings => {
                    let api = VoiceApi::new(client, server_url, token);
                    if let Ok(vs) = api.get_settings().await {
                        let mut s = state.lock().unwrap();
                        if let Some(p) = vs.tts_provider { s.settings.voice_config.tts_provider = p; }
                        if let Some(v) = vs.active_voice_id { s.settings.voice_config.active_voice_id = Some(v); }
                        if let Some(ptt) = vs.push_to_talk { s.settings.voice_config.push_to_talk = ptt; }
                        s.set_status("Voice settings loaded".to_string());
                    }
                }
                // Voice commands are handled before spawn — unreachable here
                ApiCommand::StartVoiceCall { .. }
                | ApiCommand::StopVoiceCall
                | ApiCommand::ToggleVoiceMute => {}
            }
        });
    }

    fn bootstrap(&self) {
        let state = self.state.clone();
        let client = self.client.clone();
        let server_url = self.config.server_url.clone();
        let secure_store = SecureStore::new();

        self.runtime.spawn(async move {
            let health_api = HealthApi::new(client.clone(), server_url.clone());
            let backend_ok = health_api.check().await.is_ok();
            let token = {
                let mut s = state.lock().unwrap();
                s.settings.health.backend = backend_ok;
                s.settings.health.api_base_url = server_url.clone();
                if !backend_ok { s.show_login = true; s.set_status("Backend offline".to_string()); return; }
                s.auth_token.clone()
            };

            if let Some(token) = token {
                let auth_api = AuthApi::new(client.clone(), server_url.clone());
                let is_valid = matches!(auth_api.verify_token(&token).await, Ok(true));
                if is_valid {
                    {
                        let mut s = state.lock().unwrap();
                        s.is_logged_in = true; s.set_status("Authenticated".to_string());
                    }
                    load_sessions_into_state(state, client, server_url, token).await;
                } else {
                    let _ = secure_store.delete_token();
                    {
                        let mut s = state.lock().unwrap();
                        s.auth_token = None;
                        s.is_logged_in = false;
                        s.login_in_progress = false;
                    }
                    if !try_env_auto_login(state.clone(), client.clone(), server_url.clone()).await {
                        let mut s = state.lock().unwrap();
                        s.show_login = true;
                    }
                }
            } else {
                if !try_env_auto_login(state.clone(), client.clone(), server_url.clone()).await {
                    let mut s = state.lock().unwrap();
                    s.login_in_progress = false;
                    s.show_login = true;
                }
            }
        });

        if let Ok(mut s) = self.state.lock() {
            s.app_config_path = Some(AppConfig::config_path_display());
            s.server_url = self.config.server_url.clone();
        }
    }
}
