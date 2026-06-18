use std::sync::Arc;
use std::sync::Mutex;
use tokio::runtime::Runtime;
use uuid::Uuid;

use crate::api::{auth::AuthApi, health::HealthApi, sessions::SessionsApi, tasks::TasksApi};
use crate::config::app_config::AppConfig;
use crate::config::secure_store::SecureStore;
use crate::state::app_state::AppState;
use crate::ui;

/// Holds transient login dialog state (not persisted)
pub struct LoginDialogState {
    pub username: String,
    pub password: String,
    pub server_url: String,
    pub error: Option<String>,
    pub loading: bool,
}

impl Default for LoginDialogState {
    fn default() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            server_url: "http://localhost:8000".to_string(),
            error: None,
            loading: false,
        }
    }
}

/// Actions that the UI can request from the app
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
}

impl Default for MakimaApp {
    fn default() -> Self {
        let state = Arc::new(Mutex::new(AppState::default()));
        let runtime = Runtime::new().expect("Failed to create Tokio runtime");
        let config = AppConfig::load().unwrap_or_default();
        let secure_store = SecureStore::new();
        let mut login_dialog = LoginDialogState::default();
        login_dialog.server_url = config.server_url.clone();

        if let Some(token) = secure_store.get_token() {
            if let Ok(mut s) = state.lock() {
                s.auth_token = Some(token);
                s.is_logged_in = true;
                s.server_url = config.server_url.clone();
            }
        }

        let client = reqwest::Client::builder()
            .user_agent("makima-desktop/0.1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            state,
            runtime,
            client,
            config,
            secure_store,
            initialized: false,
            login_dialog,
            pending_action: None,
        }
    }
}

impl MakimaApp {
    fn exec_login(&mut self) {
        let username = std::mem::take(&mut self.login_dialog.username);
        let password = std::mem::take(&mut self.login_dialog.password);
        let server_url = self.login_dialog.server_url.clone();
        self.login_dialog.loading = true;
        self.login_dialog.error = None;

        let state = self.state.clone();
        let client = self.client.clone();

        self.config.server_url = server_url.clone();
        let _ = self.config.save();

        self.runtime.spawn(async move {
            let auth_api = AuthApi::new(client.clone(), server_url.clone());
            match auth_api.login(&username, &password).await {
                Ok(resp) => {
                    let token = resp.access_token;
                    let secure_store = SecureStore::new();
                    let _ = secure_store.store_token(&token);

                    // Scope the lock
                    {
                        let mut s = state.lock().unwrap();
                        s.server_url = server_url.clone();
                        s.auth_token = Some(token.clone());
                        s.is_logged_in = true;
                        s.show_login = false;
                        s.set_status("Logged in".to_string());
                    }

                    // Fetch sessions (separate lock window)
                    {
                        let sessions_api = SessionsApi::new(client, server_url, token);
                        let r = Runtime::new().unwrap();
                        if let Ok(api_sessions) = r.block_on(sessions_api.list()) {
                            let mut s = state.lock().unwrap();
                            for api_s in api_sessions {
                                let title = api_s.title.unwrap_or_else(|| "Untitled".to_string());
                                s.chat.sessions.push(crate::state::chat_state::Session::new(title));
                            }
                            if !s.chat.sessions.is_empty() {
                                s.chat.active_session_id = Some(s.chat.sessions[0].id);
                            }
                            s.set_status("Sessions loaded".to_string());
                        }
                    }
                }
                Err(e) => {
                    let mut s = state.lock().unwrap();
                    s.set_status(format!("Login failed: {}", e));
                }
            }
        });
    }

    fn exec_send_message(&mut self) {
        let mut s = self.state.lock().unwrap();
        let text = s.chat.composer.input.trim().to_string();
        if text.is_empty() {
            return;
        }

        let session_count = s.chat.sessions.len();
        if s.chat.active_session().is_none() {
            s.chat.create_session(format!("Chat {}", session_count + 1));
        }

        let session_id = s.chat.active_session_id;
        let token = match &s.auth_token {
            Some(t) => t.clone(),
            None => {
                s.set_status("Not authenticated".to_string());
                return;
            }
        };
        let server_url = s.server_url.clone();

        // Add user message
        if let Some(session) = s.chat.active_session_mut() {
            let msg = crate::state::chat_state::ChatMessage {
                ts: chrono::Utc::now().timestamp_millis(),
                msg_type: crate::state::chat_state::MessageType::Ask,
                ask: None,
                say: None,
                text: Some(text.clone()),
                partial: false,
                reasoning: None,
                token_usage: None,
                tool_call_id: None,
                error: None,
                id: Uuid::new_v4(),
                session_id: session.id,
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
            let sid_str = session_id.map(|id| id.to_string());

            match tasks_api.stream(sid_str, Some(text)).await {
                Ok(mut rx) => {
                    while let Some(event_result) = rx.recv().await {
                        let mut s = state.lock().unwrap();
                        match event_result {
                            Ok(event) => handle_sse_event(&mut s, event),
                            Err(e) => {
                                s.set_status(format!("Stream error: {}", e));
                                s.chat.composer.is_streaming = false;
                            }
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
    use crate::state::chat_state::{ChatMessage, MessageType, SayKind, TokenUsage};
    use crate::state::task_state::{TaskEvent, TaskStatus, TimelinePhase};
    use chrono::Utc;

    let session_id = state.chat.active_session_id.unwrap_or_else(Uuid::new_v4);

    match event {
        TaskEvent::TaskStarted { task_id } => {
            let mut task = crate::state::task_state::TaskExecutionState::default();
            task.status = TaskStatus::Running;
            task.task_id = Uuid::parse_str(&task_id).ok();
            state.task.active_task = Some(task);
        }
        TaskEvent::TaskCompleted { .. } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.status = TaskStatus::Idle;
            }
        }
        TaskEvent::TaskError { error, .. } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.status = TaskStatus::Idle;
                t.error = Some(error);
            }
        }
        TaskEvent::Message { message, action, .. } => {
            let text = message
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_partial = action == "updated";

            if let Some(session) = state.chat.active_session_mut() {
                if is_partial {
                    if let Some(last) = session.messages.last_mut() {
                        if last.partial && last.msg_type == MessageType::Say {
                            last.text = Some(text);
                            return;
                        }
                    }
                }

                let msg = ChatMessage {
                    ts: Utc::now().timestamp_millis(),
                    msg_type: MessageType::Say,
                    ask: None,
                    say: Some(SayKind::Text),
                    text: Some(text),
                    partial: is_partial,
                    reasoning: None,
                    token_usage: None,
                    tool_call_id: None,
                    error: None,
                    id: Uuid::new_v4(),
                    session_id,
                };
                session.messages.push(msg);
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::ToolStart { tool_name, .. } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.add_timeline_entry(TimelinePhase::ToolDispatch, tool_name.clone(), None);
            }
            if let Some(session) = state.chat.active_session_mut() {
                session.messages.push(ChatMessage {
                    ts: Utc::now().timestamp_millis(),
                    msg_type: MessageType::Say,
                    ask: None,
                    say: Some(SayKind::Tool),
                    text: Some(format!("🔧 Tool: {}", tool_name)),
                    partial: false,
                    reasoning: None,
                    token_usage: None,
                    tool_call_id: None,
                    error: None,
                    id: Uuid::new_v4(),
                    session_id,
                });
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::ToolResult { tool_name, result } => {
            let result_text = result
                .as_str()
                .map(|s| truncate(s, 200))
                .unwrap_or_else(|| format!("{:?}", result));
            if let Some(session) = state.chat.active_session_mut() {
                session.messages.push(ChatMessage {
                    ts: Utc::now().timestamp_millis(),
                    msg_type: MessageType::Say,
                    ask: None,
                    say: Some(SayKind::Tool),
                    text: Some(format!("✅ {}: {}", tool_name, result_text)),
                    partial: false,
                    reasoning: None,
                    token_usage: None,
                    tool_call_id: None,
                    error: None,
                    id: Uuid::new_v4(),
                    session_id,
                });
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::ToolError { tool_name, error } => {
            if let Some(session) = state.chat.active_session_mut() {
                session.messages.push(ChatMessage {
                    ts: Utc::now().timestamp_millis(),
                    msg_type: MessageType::Say,
                    ask: None,
                    say: Some(SayKind::Tool),
                    text: Some(format!("❌ {}: {}", tool_name, error)),
                    partial: false,
                    reasoning: None,
                    token_usage: None,
                    tool_call_id: None,
                    error: Some(error.clone()),
                    id: Uuid::new_v4(),
                    session_id,
                });
                session.updated_at = Utc::now();
            }
        }
        TaskEvent::Thinking { content } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.add_timeline_entry(TimelinePhase::Thinking, "Thinking".to_string(), Some(content));
            }
        }
        TaskEvent::TokenUsage { tokens_in, tokens_out, cost } => {
            if let Some(t) = state.task.active_task.as_mut() {
                t.token_usage = TokenUsage {
                    total_tokens_in: tokens_in,
                    total_tokens_out: tokens_out,
                    total_cache_writes: None,
                    total_cache_reads: None,
                    total_cost: cost,
                    context_tokens: 0,
                };
            }
            state.set_status(format!("Tokens: ↑{} ↓{} ${:.5}", tokens_in, tokens_out, cost));
        }
        _ => {}
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}…", &s[..max]) }
}

impl eframe::App for MakimaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.initialized {
            self.initialized = true;
            crate::theme::apply_theme(ctx);
            self.bootstrap();
        }

        // Process pending action (set by UI)
        if let Some(action) = self.pending_action.take() {
            match action {
                UiAction::Login => self.exec_login(),
                UiAction::SendMessage => self.exec_send_message(),
                UiAction::Logout => {
                    let mut s = self.state.lock().unwrap();
                    s.is_logged_in = false;
                    s.auth_token = None;
                    s.show_login = true;
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
                                    s.chat.sessions.push(crate::state::chat_state::Session::new(title));
                                }
                                s.set_status("Sessions refreshed".to_string());
                            }
                        });
                    }
                }
            }
        }

        let mut state = self.state.lock().unwrap();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui::shell::draw(ui, &mut state, &mut self.login_dialog, &mut self.pending_action);
        });
        ctx.request_repaint();
    }
}

impl MakimaApp {
    fn bootstrap(&self) {
        let state = self.state.clone();
        let client = self.client.clone();
        let server_url = self.config.server_url.clone();

        self.runtime.spawn(async move {
            let health_api = HealthApi::new(client.clone(), server_url.clone());
            let backend_ok = health_api.check().await.is_ok();

            let token = {
                let mut s = state.lock().unwrap();
                s.settings.health.backend = backend_ok;
                s.settings.health.api_base_url = server_url.clone();
                if !backend_ok {
                    s.show_login = true;
                    s.set_status("Backend offline".to_string());
                    return;
                }
                s.auth_token.clone()
            };

            if let Some(token) = token {
                let auth_api = AuthApi::new(client.clone(), server_url.clone());
                let is_valid = matches!(auth_api.verify_token(&token).await, Ok(true));
                let mut s = state.lock().unwrap();
                if is_valid {
                    s.is_logged_in = true;
                    s.set_status("Authenticated".to_string());
                    let sessions_api = SessionsApi::new(client, server_url, token);
                    if let Ok(list) = Runtime::new().unwrap().block_on(sessions_api.list()) {
                        for api_s in list {
                            let title = api_s.title.unwrap_or_else(|| "Untitled".to_string());
                            s.chat.sessions.push(crate::state::chat_state::Session::new(title));
                        }
                        if !s.chat.sessions.is_empty() {
                            s.chat.active_session_id = Some(s.chat.sessions[0].id);
                        }
                        s.set_status("Sessions loaded".to_string());
                    }
                } else {
                    s.is_logged_in = false;
                    s.show_login = true;
                }
            } else {
                let mut s = state.lock().unwrap();
                s.show_login = true;
            }
        });

        if let Ok(mut s) = self.state.lock() {
            s.app_config_path = Some(AppConfig::config_path_display());
            s.server_url = self.config.server_url.clone();
        }
    }
}