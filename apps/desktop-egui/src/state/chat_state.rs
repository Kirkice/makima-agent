use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Mirror of Zoo-Code's ClineMessage ask/say dual-mode pattern.
/// Messages are either "ask" (requires user response) or "say" (informational).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Unix timestamp (milliseconds)
    pub ts: i64,
    /// Message type: "ask" requires user input, "say" is informational
    pub msg_type: MessageType,
    /// Sub-type for ask messages
    pub ask: Option<AskKind>,
    /// Sub-type for say messages
    pub say: Option<SayKind>,
    /// Message text content
    pub text: Option<String>,
    /// Whether this message is still being streamed
    pub partial: bool,
    /// Reasoning/thought content (if applicable)
    pub reasoning: Option<String>,
    /// Token usage for this message
    pub token_usage: Option<TokenUsage>,
    /// Optional tool call ID for tool-related messages
    pub tool_call_id: Option<String>,
    /// Optional error info
    pub error: Option<String>,
    /// Message ID
    pub id: Uuid,
    /// Session this message belongs to
    pub session_id: Uuid,
}

/// Dual-mode pattern: ask (needs user response) vs say (informational)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    Ask,
    Say,
}

/// Mirror of Zoo-Code's ClineAsk enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AskKind {
    Followup,
    Command,
    CommandOutput,
    CompletionResult,
    Tool,
    ApiReqFailed,
    ResumeTask,
    ResumeCompletedTask,
    MistakeLimitReached,
    UseMcpServer,
    AutoApprovalMaxReqReached,
}

/// Mirror of Zoo-Code's ClineSay enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SayKind {
    Error,
    ApiReqStarted,
    ApiReqFinished,
    ApiReqRetried,
    Text,
    Reasoning,
    CompletionResult,
    UserFeedback,
    CommandOutput,
    McpServerRequestStarted,
    McpServerResponse,
    SubtaskResult,
    CheckpointSaved,
    Tool,
}

/// Token usage tracking - mirrors Zoo-Code's TokenUsage
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
    pub total_cache_writes: Option<u64>,
    pub total_cache_reads: Option<u64>,
    pub total_cost: f64,
    pub context_tokens: u64,
}

/// A conversation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub pinned: bool,
    pub unread: bool,
    pub draft: Option<String>,
    pub current_mode: Option<String>,
    pub messages: Vec<ChatMessage>,
}

impl Session {
    pub fn new(title: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title,
            created_at: now,
            updated_at: now,
            pinned: false,
            unread: false,
            draft: None,
            current_mode: None,
            messages: Vec::new(),
        }
    }

    pub fn estimated_token_count(&self) -> u64 {
        // Local approximation: ~4 chars per token
        self.messages
            .iter()
            .map(|m| {
                let text_len = m.text.as_deref().unwrap_or("").len() as u64;
                let reasoning_len = m.reasoning.as_deref().unwrap_or("").len() as u64;
                (text_len + reasoning_len) / 4
            })
            .sum()
    }

    pub fn estimated_cost(&self, model_cost_per_1k_tokens: f64) -> f64 {
        let tokens = self.estimated_token_count();
        (tokens as f64 / 1000.0) * model_cost_per_1k_tokens
    }
}

/// Chat input composer state
#[derive(Debug, Clone)]
pub struct ComposerState {
    pub input: String,
    pub is_streaming: bool,
    pub current_task_id: Option<Uuid>,
    pub estimated_tokens: u64,
}

impl Default for ComposerState {
    fn default() -> Self {
        Self {
            input: String::new(),
            is_streaming: false,
            current_task_id: None,
            estimated_tokens: 0,
        }
    }
}

/// Active conversation state for the center panel
#[derive(Debug, Clone)]
pub struct ChatState {
    pub sessions: Vec<Session>,
    pub active_session_id: Option<Uuid>,
    pub composer: ComposerState,
    pub show_inspector: bool,
    pub search_query: String,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            sessions: Vec::new(),
            active_session_id: None,
            composer: ComposerState::default(),
            show_inspector: true,
            search_query: String::new(),
        }
    }
}

impl ChatState {
    pub fn active_session(&self) -> Option<&Session> {
        self.active_session_id
            .and_then(|id| self.sessions.iter().find(|s| s.id == id))
    }

    pub fn active_session_mut(&mut self) -> Option<&mut Session> {
        self.active_session_id
            .and_then(move |id| self.sessions.iter_mut().find(|s| s.id == id))
    }

    pub fn create_session(&mut self, title: String) -> Uuid {
        let session = Session::new(title);
        let id = session.id;
        self.sessions.push(session);
        self.active_session_id = Some(id);
        id
    }

    pub fn delete_session(&mut self, id: Uuid) {
        self.sessions.retain(|s| s.id != id);
        if self.active_session_id == Some(id) {
            self.active_session_id = self.sessions.first().map(|s| s.id);
        }
    }

    pub fn rename_session(&mut self, id: Uuid, title: String) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
            session.title = title;
            session.updated_at = Utc::now();
        }
    }

    /// Sort sessions by updated_at descending (most recent first)
    pub fn sorted_sessions(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.iter().collect();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }
}