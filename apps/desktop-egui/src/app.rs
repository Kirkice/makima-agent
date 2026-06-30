use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::runtime::Runtime;
use uuid::Uuid;

use crate::api::{auth::AuthApi, health::HealthApi, sessions::SessionsApi, tasks::TasksApi};
use crate::backend_launcher::BackendProcess;
use crate::config::app_config::{AppConfig, DEFAULT_SERVER_URL, normalize_server_url};
use crate::config::secure_store::SecureStore;
use crate::state::app_state::{AppState, ViewMode};
use crate::ui;
use crate::ui::dock::{normalize_layout, AppDockState, init_app_dock};
use crate::voice::VoiceManager;
use crate::websocket_bridge::WebSocketBridge;

pub struct LoginDialogState {
    pub username: String,
    pub password: String,
    pub server_url: String,
    pub error: Option<String>,
    pub loading: bool,
}

impl Default for LoginDialogState {
    fn default() -> Self {
        Self { username: String::new(), password: String::new(), server_url: DEFAULT_SERVER_URL.to_string(), error: None, loading: false }
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
    /// WebSocket bridge for Unity avatar animation commands
    pub ws_bridge: Arc<WebSocketBridge>,
    pub layout_changed: bool,
    pub last_save_frame: u64,
    pub backend_process: BackendProcess,
    pub backend_startup_error: Option<String>,
    /// Avatar WebGL WebView (only when `avatar` feature is enabled)
    #[cfg(feature = "avatar")]
    pub avatar_webview: Option<crate::ui::panels::avatar_impl::AvatarWebView>,
    /// Port that the embedded asset server is listening on
    #[cfg(feature = "avatar")]
    pub avatar_port: Option<u16>,
    /// Previous ViewMode – used to detect transitions into/out of Avatar
    prev_view_mode: ViewMode,
}

impl Default for MakimaApp {
    fn default() -> Self {
        Self::new(BackendProcess::none())
    }
}

impl MakimaApp {
    pub fn new(backend_process: BackendProcess) -> Self {
        let backend_startup_error = backend_process.startup_error().map(str::to_owned);
        let state = Arc::new(Mutex::new(AppState::default()));
        let runtime = Runtime::new().expect("Failed to create Tokio runtime");
        let config = AppConfig::load().unwrap_or_default();
        let secure_store = SecureStore::new();
        let mut login_dialog = LoginDialogState::default();
        login_dialog.server_url = config.server_url.clone();
        login_dialog.username = std::env::var("MAKIMA_CLI_USERNAME").unwrap_or_default();
        login_dialog.password = std::env::var("MAKIMA_CLI_PASSWORD").unwrap_or_default();

        // Restore persisted layout
        {
            let mut s = state.lock().unwrap();
            s.conversations_width = config.sidebar_width;
            s.inspector_width = config.inspector_width;
            s.drawer_height = config.drawer_height;
            s.show_settings_panel = config.show_settings_panel;
            s.drawer_open = config.drawer_open;
            s.server_url = config.server_url.clone();
            normalize_layout(&mut s);
        }

        if let Some(token) = secure_store.get_token() {
            if let Ok(mut s) = state.lock() { s.auth_token = Some(token); s.is_logged_in = true; s.server_url = config.server_url.clone(); }
        }
        if let Some(err) = &backend_startup_error {
            if let Ok(mut s) = state.lock() {
                s.login_error = Some(err.clone());
                s.status_message = Some(err.clone());
            }
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .user_agent("makima-desktop/0.1.0")
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        // Initialize app_dock before moving state into Self
        let app_dock = {
            let state_guard = state.lock().unwrap();
            init_app_dock(
                ViewMode::Chat,
                state_guard.show_settings_panel,
                state_guard.conversations_width,
                state_guard.inspector_width,
                egui::vec2(config.window_width, config.window_height),
            )
        };

        // Initialize WebSocket bridge for Unity avatar
        let ws_bridge = Arc::new(WebSocketBridge::new(
            "127.0.0.1:9001".parse().expect("Invalid WebSocket address")
        ));
        // Start WebSocket server in background (use the app's runtime, not tokio::spawn which requires an active reactor)
        {
            let bridge = ws_bridge.clone();
            runtime.spawn(async move {
                if let Err(e) = bridge.start().await {
                    tracing::error!("WebSocket bridge failed to start: {}", e);
                }
            });
        }

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
            ws_bridge,
            layout_changed: false,
            last_save_frame: 0,
            backend_process,
            backend_startup_error,
            #[cfg(feature = "avatar")]
            avatar_webview: None,
            #[cfg(feature = "avatar")]
            avatar_port: None,
            prev_view_mode: ViewMode::default(),
        }
    }

    fn exec_login(&mut self) {
        self.login_dialog.loading = true;
        self.login_dialog.error = None;

        let username = self.login_dialog.username.clone();
        let password = self.login_dialog.password.clone();
        let server_url = normalize_server_url(self.login_dialog.server_url.clone());
        let state = self.state.clone();
        let client = self.client.clone();

        self.login_dialog.server_url = server_url.clone();
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

                    // Also fetch modes after login
                    {
                        use crate::state::app_state::ApiCommand;
                        let mut s = state.lock().unwrap();
                        s.api_commands.push(ApiCommand::FetchModes);
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

        let local_session_id = match s.chat.active_session_id {
            Some(id) => id,
            None => { s.set_status("No active session".to_string()); return; }
        };
        let backend_session_id = s
            .chat
            .active_session()
            .and_then(|session| session.backend_id.clone());
        let session_title = s
            .chat
            .active_session()
            .map(|session| session.title.clone())
            .unwrap_or_else(|| "New Chat".to_string());
        let token = match &s.auth_token {
            Some(t) => t.clone(),
            None => { s.set_status("Not authenticated".to_string()); return; }
        };
        let server_url = s.server_url.clone();

        // Extract model override from active profile
        let mode_slug = s.settings.active_mode_slug.clone();
        let model_override = {
            use crate::api::tasks::ModelOverride;
            let active_profile_name = s.settings.active_model_profile.clone();
            let profiles = &s.settings.model_profiles;
            if let Some(name) = active_profile_name {
                profiles.iter().find(|p| p.name == name).map(|p| {
                    ModelOverride {
                        model: Some(p.model.clone()),
                        api_key: p.api_key.clone(),
                        base_url: Some(p.base_url.clone()),
                        temperature: Some(p.temperature),
                    }
                })
            } else {
                None
            }
        };

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
        // Extract attachment paths before clearing
        let attachment_paths: Vec<(String, String)> = s.chat.composer.attachments.iter()
            .map(|f| (f.path.clone(), f.name.clone()))
            .collect();

        s.chat.composer.input.clear();
        // Mark all attachments as uploading
        for att in s.chat.composer.attachments.iter_mut() {
            att.status = crate::state::chat_state::AttachmentStatus::Uploading;
        }
        s.chat.composer.is_streaming = true;
        s.set_status(if attachment_paths.is_empty() { "Sending...".to_string() } else { "Uploading attachments...".to_string() });
        drop(s);

        let state = self.state.clone();
        let client = self.client.clone();

        let ws_bridge = self.ws_bridge.clone();

        self.runtime.spawn(async move {
            let resolved_session_id = if let Some(backend_id) = backend_session_id {
                backend_id
            } else {
                let sessions_api = SessionsApi::new(client.clone(), server_url.clone(), token.clone());
                match sessions_api.create(Some(session_title)).await {
                    Ok(api_session) => {
                        let backend_id = api_session.id;
                        let mut s = state.lock().unwrap();
                        if let Some(session) = s.chat.sessions.iter_mut().find(|session| session.id == local_session_id) {
                            session.backend_id = Some(backend_id.clone());
                        }
                        backend_id
                    }
                    Err(e) => {
                        let mut s = state.lock().unwrap();
                        s.set_status(format!("Failed to create session: {}", e));
                        s.chat.composer.is_streaming = false;
                        return;
                    }
                }
            };

            // Upload attachments before sending the task. If every attachment fails,
            // stop here instead of silently sending a message with no files.
            let uploaded_attachments = if !attachment_paths.is_empty() {
                let attachments_api = crate::api::attachments::AttachmentsApi::new(
                    client.clone(), server_url.clone(), token.clone(),
                );
                let mut results: Vec<crate::api::tasks::AttachmentInfo> = Vec::new();
                let mut failed_paths: Vec<String> = Vec::new();
                for (path, name) in &attachment_paths {
                    match attachments_api.upload(&resolved_session_id, path).await {
                        Ok(info) => {
                            // Update attachment status to Uploaded
                            let mut s = state.lock().unwrap();
                            if let Some(att) = s.chat.composer.attachments.iter_mut()
                                .find(|a| a.path == *path)
                            {
                                att.status = crate::state::chat_state::AttachmentStatus::Uploaded;
                                att.uploaded_info = Some(crate::state::chat_state::UploadedAttachmentInfo {
                                    attachment_id: info.attachment_id.clone(),
                                    original_name: info.original_name.clone(),
                                    stored_path: info.stored_path.clone(),
                                    mime_type: info.mime_type.clone(),
                                    size: info.size,
                                    is_text: info.is_text,
                                });
                            }
                            results.push(info);
                        }
                        Err(e) => {
                            let err_msg = e.to_string();
                            failed_paths.push(path.clone());
                            let mut s = state.lock().unwrap();
                            if let Some(att) = s.chat.composer.attachments.iter_mut()
                                .find(|a| a.path == *path)
                            {
                                att.status = crate::state::chat_state::AttachmentStatus::Error(err_msg.clone());
                            }
                            s.set_status(format!("Failed to upload {}: {}", name, err_msg));
                        }
                    }
                }

                {
                    let mut s = state.lock().unwrap();
                    s.chat.composer.attachments.retain(|att| failed_paths.contains(&att.path));
                }

                if results.is_empty() && !failed_paths.is_empty() {
                    let mut s = state.lock().unwrap();
                    s.chat.composer.is_streaming = false;
                    s.set_status(
                        "Attachment upload failed. Message was not sent; please retry after restarting the backend or fixing the upload error."
                            .to_string(),
                    );
                    return;
                }

                if !failed_paths.is_empty() {
                    let mut s = state.lock().unwrap();
                    s.set_status(format!(
                        "{} attachment(s) uploaded, {} failed. Sending only the uploaded files.",
                        results.len(),
                        failed_paths.len()
                    ));
                }

                if results.is_empty() { None } else { Some(results) }
            } else {
                None
            };

            let tasks_api = TasksApi::new(client, server_url, token);
            match tasks_api.stream(resolved_session_id, text, mode_slug, model_override, uploaded_attachments).await {
                Ok(mut rx) => {
                    while let Some(event_result) = rx.recv().await {
                        let mut s = state.lock().unwrap();
                        match event_result {
                            Ok(event) => {
                                if let Some(animation) = handle_sse_event(&mut s, event) {
                                    // Forward animation event to WebSocket bridge
                                    ws_bridge.send_animation(&animation, None);
                                }
                            }
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

fn handle_sse_event(state: &mut AppState, event: crate::state::task_state::TaskEvent) -> Option<String> {
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
            let text = if let Some(s) = message.as_str() {
                s.to_string()
            } else {
                message
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let is_partial = action == "updated";
            if let Some(session) = state.chat.active_session_mut() {
                if is_partial {
                    if let Some(last) = session.messages.last_mut() {
                        if last.partial && last.msg_type == MessageType::Say { last.text = Some(text); return None; }
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
        TaskEvent::Animation { animation } => {
            // Return animation name to be forwarded to WebSocket bridge
            return Some(animation);
        }
        _ => {}
    }
    None
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

            {
                use crate::state::app_state::ApiCommand;
                let mut s = state.lock().unwrap();
                s.api_commands.push(ApiCommand::FetchModes);
            }
            load_sessions_into_state(state, client, server_url, token).await;
            true
}

async fn wait_for_backend_ready(
    client: reqwest::Client,
    server_url: String,
    attempts: usize,
    delay: Duration,
) -> bool {
    for attempt in 0..attempts {
        let health_api = HealthApi::new(client.clone(), server_url.clone());
        if health_api.check().await.is_ok() {
            return true;
        }

        if attempt + 1 < attempts {
            tokio::time::sleep(delay).await;
        }
    }

    false
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

        // ── Avatar WebGL WebView lifecycle ─────────────────────────
        // Extract the view mode before dropping the MutexGuard so we
        // can call `&mut self` methods afterwards.
        #[cfg(feature = "avatar")]
        {
            let view_mode = state.view_mode;
            let avatar_panel_rect = state.avatar_panel_rect;
            drop(state);
            self.update_avatar_webview(ctx, _frame, view_mode, avatar_panel_rect);
            // Re-acquire state for the persistence block below
            state = match self.state.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
        }

        // Persist layout changes (throttled to ~1 save per second)
        self.last_save_frame += 1;
        if self.last_save_frame % 60 == 0 {
            let mut need_save = self.layout_changed;
            self.layout_changed = false;

            let sidebar_w = state.conversations_width;
            let inspector_w = state.inspector_width;
            let drawer_h = state.drawer_height;
            let show_ctx = state.show_settings_panel;
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
            if self.config.show_settings_panel != show_ctx {
                self.config.show_settings_panel = show_ctx;
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

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.backend_process.terminate();
    }
}

impl MakimaApp {
    fn exec_api_command(&mut self, cmd: crate::state::app_state::ApiCommand) {
        use crate::api::{audit::AuditApi, config::ConfigApi, knowledge::KnowledgeApi, mcp::McpApi, memory::MemoryApi, modes::ModesApi, model_profiles::ModelProfilesApi, persona::PersonaApi, voice::VoiceApi};
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
                        s.settings.modes = resp.modes.into_iter().map(|m| {
                            crate::state::settings_state::ModeConfig {
                                slug: m.slug,
                                name: m.name,
                                role_definition: m.role_definition.unwrap_or_default(),
                                when_to_use: m.when_to_use,
                                description: m.description,
                                custom_instructions: m.custom_instructions,
                                tool_groups: m.tool_groups.into_iter().map(|tg| {
                                    crate::state::settings_state::ToolGroupConfig {
                                        group: tg.group,
                                        file_regex: tg.file_regex,
                                        auto_approve: tg.auto_approve,
                                    }
                                }).collect(),
                                max_steps: m.max_steps,
                                temperature: m.temperature,
                                source: Some(m.source),
                                model: m.model,
                                api_base: m.api_base,
                            }
                        }).collect();
                        if s.settings.active_mode_slug.is_none() {
                            s.settings.active_mode_slug = s.settings.modes.first().map(|m| m.slug.clone());
                        }
                        s.set_status("Modes loaded".to_string());
                    }
                }
                ApiCommand::FetchModeById(slug) => {
                    let api = ModesApi::new(client.clone(), server_url.clone(), token.clone());
                    if let Ok(m) = api.get(&slug).await {
                        let mut s = state.lock().unwrap();
                        // Update the specific mode in the list
                        if let Some(idx) = s.settings.modes.iter().position(|mode| mode.slug == slug) {
                            s.settings.modes[idx] = crate::state::settings_state::ModeConfig {
                                slug: m.slug, name: m.name,
                                role_definition: m.role_definition.unwrap_or_default(),
                                when_to_use: m.when_to_use,
                                description: m.description,
                                custom_instructions: m.custom_instructions,
                                tool_groups: m.tool_groups.into_iter().map(|tg| {
                                    crate::state::settings_state::ToolGroupConfig {
                                        group: tg.group,
                                        file_regex: tg.file_regex,
                                        auto_approve: tg.auto_approve,
                                    }
                                }).collect(),
                                max_steps: m.max_steps,
                                temperature: m.temperature,
                                source: Some(m.source),
                                model: m.model,
                                api_base: m.api_base,
                            };
                        }
                        s.set_status(format!("Mode '{}' refreshed", slug));
                    }
                }
                ApiCommand::CreateMode { slug, name, role_definition, when_to_use, description, custom_instructions, tool_groups, max_steps, temperature } => {
                    let api = ModesApi::new(client.clone(), server_url.clone(), token.clone());
                    let req = crate::api::modes::CreateModeRequest {
                        slug: slug.clone(),
                        name: name.clone(),
                        role_definition: role_definition.clone(),
                        when_to_use,
                        description,
                        custom_instructions,
                        tool_groups: tool_groups.into_iter().map(|g| {
                            crate::api::modes::ToolGroupConfig { group: g, file_regex: None, auto_approve: true }
                        }).collect(),
                        max_steps,
                        temperature,
                    };
                    if let Ok(_mode) = api.create(&req).await {
                        let mut s = state.lock().unwrap();
                        s.api_commands.push(ApiCommand::FetchModes);
                        s.set_status(format!("Mode '{}' created", slug));
                    }
                }
                ApiCommand::DeleteMode(slug) => {
                    let api = ModesApi::new(client.clone(), server_url.clone(), token.clone());
                    if let Ok(_resp) = api.delete(&slug).await {
                        let mut s = state.lock().unwrap();
                        s.settings.modes.retain(|m| m.slug != slug);
                        if s.settings.active_mode_slug.as_deref() == Some(&slug) {
                            s.settings.active_mode_slug = s.settings.modes.first().map(|m| m.slug.clone());
                        }
                        s.set_status(format!("Mode '{}' deleted", slug));
                    }
                }
                ApiCommand::ReloadModes => {
                    let api = ModesApi::new(client, server_url, token);
                    if let Ok(_resp) = api.reload().await {
                        let mut s = state.lock().unwrap();
                        s.api_commands.push(ApiCommand::FetchModes);
                        s.set_status("Modes reloaded from config".to_string());
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
                ApiCommand::SaveVoiceSettings => {
                    let vc = {
                        let s = state.lock().unwrap();
                        s.settings.voice_config.clone()
                    };
                    let api = VoiceApi::new(client, server_url, token);
                    let req = crate::api::voice::VoiceSettings {
                        tts_provider: Some(vc.tts_provider.clone()),
                        active_voice_id: vc.active_voice_id.clone(),
                        push_to_talk: Some(vc.push_to_talk),
                        mic_device: vc.mic_device.clone(),
                        speaker_device: vc.speaker_device.clone(),
                    };
                    match api.update_settings(&req).await {
                        Ok(_updated) => {
                            let mut s = state.lock().unwrap();
                            s.set_status("Voice settings saved".to_string());
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Voice settings save failed: {}", e));
                        }
                    }
                }
                ApiCommand::UpdatePersona { draft } => {
                    let api = PersonaApi::new(client.clone(), server_url.clone(), token.clone());
                    if let Ok(mut p) = api.get_current().await {
                        if !draft.is_empty() {
                            p.identity = draft.clone();
                        }
                        if let Ok(updated) = api.update(&p).await {
                            let mut s = state.lock().unwrap();
                            s.settings.persona_name = updated.name.clone();
                            s.settings.persona_modified = false;
                            s.settings.persona_default_preview = format!(
                                "Identity: {}\nPersonality: {}\nStyle: {}",
                                updated.identity, updated.personality, updated.speaking_style
                            );
                            s.set_status("Persona saved to backend".to_string());
                        }
                    } else {
                        let mut s = state.lock().unwrap();
                        s.set_status("Failed to load persona for update".to_string());
                    }
                }
                ApiCommand::QueryAuditLog { severity } => {
                    let api = AuditApi::new(client, server_url, token);
                    let sev_ref = severity.as_deref();
                    match api.query(sev_ref, None).await {
                        Ok(list) => {
                            let mut s = state.lock().unwrap();
                            s.settings.audit_entries = list.clone();
                            let count = s.settings.audit_entries.len();
                            s.set_status(format!("{} audit entries loaded", count));
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Audit query failed: {}", e));
                            if e.to_string().contains("403") {
                                s.settings.audit_entries.clear();
                            }
                        }
                    }
                }
                ApiCommand::RefreshHealth => {
                    let health_api = HealthApi::new(client.clone(), server_url.clone());
                    let backend_ok = health_api.check().await.is_ok();
                    let mut s = state.lock().unwrap();
                    s.settings.health.backend = backend_ok;
                    s.settings.health.api_base_url = server_url.clone();
                    if backend_ok {
                        s.set_status("Backend healthy".to_string());
                    } else {
                        s.set_status("Backend unreachable".to_string());
                    }
                }
                ApiCommand::TestConnection => {
                    let health_api = HealthApi::new(client.clone(), server_url.clone());
                    let backend_ok = health_api.check().await.is_ok();
                    let mut s = state.lock().unwrap();
                    s.settings.health.backend = backend_ok;
                    if backend_ok {
                        s.set_status("Connection OK".to_string());
                    } else {
                        s.set_status("Connection failed".to_string());
                    }
                }
                ApiCommand::FetchModelConfig => {
                    let api = ConfigApi::new(client, server_url, token);
                    match api.fetch_all_map().await {
                        Ok(map) => {
                            let mut s = state.lock().unwrap();
                            if let Some(v) = map.get("model.provider") {
                                s.settings.model_config.provider = v.clone();
                            }
                            if let Some(v) = map.get("model.base_url") {
                                s.settings.model_config.base_url = v.clone();
                            }
                            if let Some(v) = map.get("model.model") {
                                s.settings.model_config.model = v.clone();
                            }
                            if let Some(v) = map.get("model.temperature") {
                                if let Ok(t) = v.parse::<f64>() { s.settings.model_config.temperature = t; }
                            }
                            if let Some(v) = map.get("model.max_steps") {
                                if let Ok(t) = v.parse::<u32>() { s.settings.model_config.max_steps = t; }
                            }
                            if let Some(v) = map.get("model.timeout_seconds") {
                                if let Ok(t) = v.parse::<u32>() { s.settings.model_config.timeout_seconds = t; }
                            }
                            s.settings.model_config.configured = !s.settings.model_config.model.is_empty();
                            s.set_status("Model config loaded".to_string());
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Model config fetch failed: {}", e));
                        }
                    }
                }
                ApiCommand::SaveModelConfig => {
                    let mc = {
                        let s = state.lock().unwrap();
                        s.settings.model_config.clone()
                    };
                    let api = ConfigApi::new(client, server_url, token);
                    let _ = api.set("model.provider", serde_json::json!(mc.provider), None).await;
                    let _ = api.set("model.base_url", serde_json::json!(mc.base_url), None).await;
                    let _ = api.set("model.model", serde_json::json!(mc.model), None).await;
                    let _ = api.set("model.temperature", serde_json::json!(mc.temperature), None).await;
                    let _ = api.set("model.max_steps", serde_json::json!(mc.max_steps), None).await;
                    let _ = api.set("model.timeout_seconds", serde_json::json!(mc.timeout_seconds), None).await;
                    let mut s = state.lock().unwrap();
                    s.settings.model_config.configured = !mc.model.is_empty();
                    s.settings.model_config.provider_configured = !mc.provider.is_empty();
                    s.set_status("Model config saved to backend".to_string());
                }
                ApiCommand::TestModelConnection => {
                    let health_api = HealthApi::new(client.clone(), server_url.clone());
                    let backend_ok = health_api.check().await.is_ok();
                    let mut s = state.lock().unwrap();
                    s.settings.health.backend = backend_ok;
                    if backend_ok {
                        s.set_status("Model connection OK (backend reachable)".to_string());
                    } else {
                        s.set_status("Model connection failed (backend unreachable)".to_string());
                    }
                }
                // ── Model Profiles ──────────────────────────────────
                ApiCommand::FetchModelProfiles => {
                    let api = ModelProfilesApi::new(client, server_url, token);
                    match api.list().await {
                        Ok(resp) => {
                            let mut s = state.lock().unwrap();
                            let mut profiles: Vec<_> = resp.profiles.into_values().collect();
                            profiles.sort_by(|a, b| a.name.cmp(&b.name));
                            s.settings.model_profiles = profiles;
                            s.settings.active_model_profile = resp.active_profile;
                            // Sync active profile into legacy model_config
                            // Clone the active profile first to avoid borrow conflict
                            let active_clone = s.settings.active_model_profile.as_ref().and_then(|active_name| {
                                s.settings.model_profiles.iter().find(|p| &p.name == active_name).cloned()
                            });
                            if let Some(p) = active_clone {
                                s.settings.model_config.provider = p.provider;
                                s.settings.model_config.base_url = p.base_url;
                                s.settings.model_config.model = p.model.clone();
                                s.settings.model_config.temperature = p.temperature;
                                s.settings.model_config.max_steps = p.max_steps as u32;
                                s.settings.model_config.timeout_seconds = p.timeout_seconds as u32;
                                s.settings.model_config.configured = !p.model.is_empty();
                            }
                            let count = s.settings.model_profiles.len();
                            s.set_status(format!("{} model profile(s) loaded", count));
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Model profiles fetch failed: {}", e));
                        }
                    }
                }
                ApiCommand::CreateModelProfile { name, profile } => {
                    let api = ModelProfilesApi::new(client, server_url, token);
                    match api.create(&name, &profile).await {
                        Ok(_) => {
                            let mut s = state.lock().unwrap();
                            s.api_commands.push(ApiCommand::FetchModelProfiles);
                            s.set_status(format!("Profile '{}' created", name));
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Create profile failed: {}", e));
                        }
                    }
                }
                ApiCommand::UpdateModelProfile { name, profile } => {
                    let api = ModelProfilesApi::new(client, server_url, token);
                    match api.update(&name, &profile).await {
                        Ok(_) => {
                            let mut s = state.lock().unwrap();
                            s.api_commands.push(ApiCommand::FetchModelProfiles);
                            s.set_status(format!("Profile '{}' updated", name));
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Update profile failed: {}", e));
                        }
                    }
                }
                ApiCommand::DeleteModelProfile(name) => {
                    let api = ModelProfilesApi::new(client, server_url, token);
                    match api.delete(&name).await {
                        Ok(()) => {
                            let mut s = state.lock().unwrap();
                            s.api_commands.push(ApiCommand::FetchModelProfiles);
                            s.set_status(format!("Profile '{}' deleted", name));
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Delete profile failed: {}", e));
                        }
                    }
                }
                ApiCommand::ActivateModelProfile(name) => {
                    let api = ModelProfilesApi::new(client, server_url, token);
                    match api.activate(&name).await {
                        Ok(resp) => {
                            let mut s = state.lock().unwrap();
                            s.settings.active_model_profile = resp.active_profile;
                            let mut profiles: Vec<_> = resp.profiles.into_values().collect();
                            profiles.sort_by(|a, b| a.name.cmp(&b.name));
                            s.settings.model_profiles = profiles;
                            // Sync active profile into legacy model_config
                            let active_clone = s.settings.active_model_profile.as_ref().and_then(|active_name| {
                                s.settings.model_profiles.iter().find(|p| &p.name == active_name).cloned()
                            });
                            if let Some(p) = active_clone {
                                s.settings.model_config.provider = p.provider;
                                s.settings.model_config.base_url = p.base_url;
                                s.settings.model_config.model = p.model.clone();
                                s.settings.model_config.temperature = p.temperature;
                                s.settings.model_config.max_steps = p.max_steps as u32;
                                s.settings.model_config.timeout_seconds = p.timeout_seconds as u32;
                                s.settings.model_config.configured = !p.model.is_empty();
                            }
                            s.set_status(format!("Profile '{}' activated", name));
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Activate profile failed: {}", e));
                        }
                    }
                }
                ApiCommand::TestModelProfileConnection { base_url, api_key, model } => {
                    let api = ModelProfilesApi::new(client, server_url, token);
                    let req = crate::api::model_profiles::TestConnectionRequest {
                        base_url: base_url.clone(),
                        api_key,
                        model,
                    };
                    match api.test_connection(&req).await {
                        Ok(resp) => {
                            let mut s = state.lock().unwrap();
                            if resp.ok {
                                s.set_status(format!("Connection OK ({} ms)", resp.latency_ms.unwrap_or(0)));
                            } else {
                                s.set_status(format!("Connection failed: {}", resp.message));
                            }
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Test connection error: {}", e));
                        }
                    }
                }
                ApiCommand::FetchProviderTypes => {
                    let api = ModelProfilesApi::new(client, server_url, token);
                    match api.list_providers().await {
                        Ok(providers) => {
                            let mut s = state.lock().unwrap();
                            s.settings.provider_types = providers;
                            s.set_status("Provider types loaded".to_string());
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Fetch providers failed: {}", e));
                        }
                    }
                }
                // Marketplace commands
                ApiCommand::FetchMarketplaceItems { search, tags } => {
                    let api = crate::api::marketplace::MarketplaceApi::new(client, server_url, token);
                    match api.list_items(search.as_deref(), tags.as_deref()).await {
                        Ok(resp) => {
                            let mut s = state.lock().unwrap();
                            s.settings.marketplace_items = resp.items;
                            s.settings.marketplace_loading = false;
                            let count = s.settings.marketplace_items.len();
                            s.set_status(format!("Loaded {} marketplace items", count));
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.settings.marketplace_loading = false;
                            s.set_status(format!("Failed to load marketplace: {}", e));
                        }
                    }
                }
                ApiCommand::FetchMarketplaceTags => {
                    let api = crate::api::marketplace::MarketplaceApi::new(client, server_url, token);
                    match api.list_tags().await {
                        Ok(tags) => {
                            let mut s = state.lock().unwrap();
                            s.settings.marketplace_tags = tags;
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Failed to load tags: {}", e));
                        }
                    }
                }
                ApiCommand::InstallMarketplaceItem { item_id, target, selected_method_index, parameters } => {
                    let api = crate::api::marketplace::MarketplaceApi::new(client, server_url, token);
                    let req = crate::api::marketplace::InstallRequest {
                        item_id: item_id.clone(),
                        target,
                        selected_method_index,
                        parameters,
                    };
                    match api.install(req).await {
                        Ok(resp) => {
                            let mut s = state.lock().unwrap();
                            if resp.success {
                                s.set_status(format!("Installed {} successfully", resp.server_name));
                            } else {
                                s.set_status(format!("Install failed: {}", resp.error.unwrap_or_default()));
                            }
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Install error: {}", e));
                        }
                    }
                }
                ApiCommand::UninstallMarketplaceItem { item_id, target } => {
                    let api = crate::api::marketplace::MarketplaceApi::new(client, server_url, token);
                    let req = crate::api::marketplace::UninstallRequest {
                        item_id: item_id.clone(),
                        target,
                    };
                    match api.uninstall(req).await {
                        Ok(resp) => {
                            let mut s = state.lock().unwrap();
                            if resp.success {
                                s.set_status(format!("Uninstalled {} successfully", resp.server_name));
                            } else {
                                s.set_status(format!("Uninstall failed: {}", resp.error.unwrap_or_default()));
                            }
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Uninstall error: {}", e));
                        }
                    }
                }
                ApiCommand::FetchInstalledMarketplaceItems { target } => {
                    let api = crate::api::marketplace::MarketplaceApi::new(client, server_url, token);
                    match api.list_installed(&target).await {
                        Ok(items) => {
                            let mut s = state.lock().unwrap();
                            let installed_ids: std::collections::HashSet<String> = items.iter().map(|i| i.item_id.clone()).collect();
                            for item in &mut s.settings.marketplace_items {
                                item.installed = installed_ids.contains(&item.id);
                            }
                        }
                        Err(e) => {
                            let mut s = state.lock().unwrap();
                            s.set_status(format!("Failed to load installed items: {}", e));
                        }
                    }
                }
                // Voice commands are handled before spawn — unreachable here
                ApiCommand::StartVoiceCall { .. }
                | ApiCommand::StopVoiceCall
                | ApiCommand::ToggleVoiceMute => {}
            }
        });
    }

    /// Manage the Avatar WebView lifecycle: start server, create WebView,
    /// sync bounds, and show/hide on ViewMode transitions.
    #[cfg(feature = "avatar")]
    fn update_avatar_webview(
        &mut self,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
        view_mode: ViewMode,
        avatar_panel_rect: Option<egui::Rect>,
    ) {
        use crate::ui::panels::avatar_impl;

        // 1. Ensure the embedded HTTP server is running (once)
        avatar_impl::ensure_server(&mut self.avatar_port);

        let in_avatar_mode = matches!(view_mode, ViewMode::Avatar);
        let entered_avatar = in_avatar_mode && self.prev_view_mode != ViewMode::Avatar;
        let left_avatar = !in_avatar_mode && self.prev_view_mode == ViewMode::Avatar;
        self.prev_view_mode = view_mode;

        // 2. Create WebView when entering Avatar mode
        if entered_avatar {
            if let Some(port) = self.avatar_port {
                match avatar_impl::AvatarWebView::new(port, frame) {
                    Ok(mut wv) => {
                        let avatar_rect =
                            avatar_panel_rect.unwrap_or_else(|| ctx.content_rect().shrink(12.0));
                        wv.sync_bounds(avatar_rect);
                        wv.set_visible(true);
                        self.avatar_webview = Some(wv);
                        tracing::info!("Avatar WebView created");
                    }
                    Err(e) => {
                        tracing::error!("Failed to create avatar WebView: {e}");
                    }
                }
            }
        }

        // 3. Show/hide on mode transitions
        if left_avatar {
            if let Some(ref mut wv) = self.avatar_webview {
                wv.set_visible(false);
            }
        } else if entered_avatar {
            if let Some(ref mut wv) = self.avatar_webview {
                wv.set_visible(true);
            }
        }

        // 4. Sync bounds every frame when in Avatar mode
        if in_avatar_mode {
            if let Some(ref mut wv) = self.avatar_webview {
                let avatar_rect =
                    avatar_panel_rect.unwrap_or_else(|| ctx.content_rect().shrink(12.0));
                wv.sync_bounds(avatar_rect);
            }
        }
    }

    fn bootstrap(&self) {
        let state = self.state.clone();
        let client = self.client.clone();
        let server_url = self.config.server_url.clone();
        let auto_connect = self.config.auto_connect;
        let secure_store = SecureStore::new();
        let backend_startup_error = self.backend_startup_error.clone();

        self.runtime.spawn(async move {
            let backend_ok = if auto_connect {
                {
                    let mut s = state.lock().unwrap();
                    s.login_in_progress = true;
                    s.set_status("Waiting for backend...".to_string());
                }
                wait_for_backend_ready(client.clone(), server_url.clone(), 6, Duration::from_secs(1)).await
            } else {
                let health_api = HealthApi::new(client.clone(), server_url.clone());
                health_api.check().await.is_ok()
            };
            let token = {
                let mut s = state.lock().unwrap();
                s.settings.health.backend = backend_ok;
                s.settings.health.api_base_url = server_url.clone();
                if !backend_ok {
                    s.login_in_progress = false;
                    s.show_login = true;
                    if let Some(err) = backend_startup_error.clone() {
                        s.login_error = Some(err.clone());
                        s.set_status(err);
                    } else {
                        s.login_error = Some("Backend offline".to_string());
                        s.set_status("Backend offline".to_string());
                    }
                    return;
                }
                s.auth_token.clone()
            };

            if let Some(token) = token {
                let auth_api = AuthApi::new(client.clone(), server_url.clone());
                let is_valid = matches!(auth_api.verify_token(&token).await, Ok(true));
                if is_valid {
                    {
                        let mut s = state.lock().unwrap();
                        s.is_logged_in = true;
                        s.login_in_progress = false;
                        s.show_login = false;
                        s.set_status("Authenticated".to_string());
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
                    if auto_connect && try_env_auto_login(state.clone(), client.clone(), server_url.clone()).await {
                        return;
                    }
                    {
                        let mut s = state.lock().unwrap();
                        s.show_login = true;
                    }
                }
            } else {
                if auto_connect && try_env_auto_login(state.clone(), client.clone(), server_url.clone()).await {
                    return;
                }
                {
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
