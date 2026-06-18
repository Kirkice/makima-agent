use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Mirror of Zoo-Code's TaskStatus state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is actively running
    Running,
    /// Task is waiting for user interaction (approval/input)
    Interactive,
    /// Task can be resumed
    Resumable,
    /// Task is idle (completed or awaiting)
    Idle,
    /// No active task
    None,
}

impl TaskStatus {
    pub fn label(&self) -> &'static str {
        match self {
            TaskStatus::Running => "Running",
            TaskStatus::Interactive => "Awaiting Input",
            TaskStatus::Resumable => "Paused",
            TaskStatus::Idle => "Idle",
            TaskStatus::None => "Ready",
        }
    }

    pub fn color_hex(&self) -> &'static str {
        match self {
            TaskStatus::Running => "#22c55e",       // green
            TaskStatus::Interactive => "#f59e0b",    // amber
            TaskStatus::Resumable => "#3b82f6",      // blue
            TaskStatus::Idle => "#6b7280",           // gray
            TaskStatus::None => "#6b7280",           // gray
        }
    }
}

/// SSE event types from Makima backend - mirrors Zoo-Code's RooCodeEvents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum TaskEvent {
    #[serde(rename = "task_started")]
    TaskStarted { task_id: String },
    #[serde(rename = "task_completed")]
    TaskCompleted { task_id: String },
    #[serde(rename = "task_error")]
    TaskError { task_id: String, error: String },
    #[serde(rename = "message")]
    Message {
        task_id: String,
        action: String, // "created" or "updated"
        message: serde_json::Value,
    },
    #[serde(rename = "tool_start")]
    ToolStart {
        tool_name: String,
        arguments: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_name: String,
        result: serde_json::Value,
    },
    #[serde(rename = "tool_error")]
    ToolError {
        tool_name: String,
        error: String,
    },
    #[serde(rename = "thinking")]
    Thinking { content: String },
    #[serde(rename = "memory_recall")]
    MemoryRecall { memories: Vec<String> },
    #[serde(rename = "retrieval")]
    Retrieval { query: String, results: Vec<String> },
    #[serde(rename = "token_usage")]
    TokenUsage {
        tokens_in: u64,
        tokens_out: u64,
        cost: f64,
    },
}

/// A timeline entry for the task execution display
#[derive(Debug, Clone)]
pub struct TimelineEntry {
    pub id: Uuid,
    pub timestamp: i64,
    pub phase: TimelinePhase,
    pub label: String,
    pub detail: Option<String>,
    pub expanded: bool,
    pub duration_ms: Option<u64>,
}

/// Phases of task execution - for the timeline visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelinePhase {
    Thinking,
    MemoryRecall,
    Retrieval,
    ToolDispatch,
    ToolResult,
    Completion,
    Error,
}

impl TimelinePhase {
    pub fn icon(&self) -> &'static str {
        match self {
            TimelinePhase::Thinking => "🧠",
            TimelinePhase::MemoryRecall => "💾",
            TimelinePhase::Retrieval => "🔍",
            TimelinePhase::ToolDispatch => "🔧",
            TimelinePhase::ToolResult => "✅",
            TimelinePhase::Completion => "🎯",
            TimelinePhase::Error => "❌",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TimelinePhase::Thinking => "Thinking",
            TimelinePhase::MemoryRecall => "Memory Recall",
            TimelinePhase::Retrieval => "Knowledge Retrieval",
            TimelinePhase::ToolDispatch => "Tool Execution",
            TimelinePhase::ToolResult => "Tool Result",
            TimelinePhase::Completion => "Completed",
            TimelinePhase::Error => "Error",
        }
    }
}

/// The execution state of a task
#[derive(Debug, Clone)]
pub struct TaskExecutionState {
    pub task_id: Option<Uuid>,
    pub status: TaskStatus,
    pub timeline: Vec<TimelineEntry>,
    pub current_phase: Option<TimelinePhase>,
    pub elapsed_seconds: u64,
    pub error: Option<String>,
    pub token_usage: super::chat_state::TokenUsage,
}

impl Default for TaskExecutionState {
    fn default() -> Self {
        Self {
            task_id: None,
            status: TaskStatus::None,
            timeline: Vec::new(),
            current_phase: None,
            elapsed_seconds: 0,
            error: None,
            token_usage: super::chat_state::TokenUsage::default(),
        }
    }
}

impl TaskExecutionState {
    pub fn is_active(&self) -> bool {
        matches!(self.status, TaskStatus::Running | TaskStatus::Interactive)
    }

    pub fn add_timeline_entry(&mut self, phase: TimelinePhase, label: String, detail: Option<String>) {
        let entry = TimelineEntry {
            id: Uuid::new_v4(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            phase,
            label,
            detail,
            expanded: false,
            duration_ms: None,
        };
        self.current_phase = Some(phase);
        self.timeline.push(entry);
    }
}

/// Task state container
#[derive(Debug, Clone)]
pub struct TaskState {
    pub active_task: Option<TaskExecutionState>,
    pub task_history: Vec<TaskExecutionState>,
}

impl Default for TaskState {
    fn default() -> Self {
        Self {
            active_task: None,
            task_history: Vec::new(),
        }
    }
}