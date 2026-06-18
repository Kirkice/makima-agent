use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::state::task_state::TaskEvent;

/// Task creation request
#[derive(Debug, Serialize)]
pub struct CreateTaskRequest {
    pub session_id: Option<String>,
    pub text: Option<String>,
    pub mode: Option<String>,
}

/// Task response from backend
#[derive(Debug, Deserialize)]
pub struct TaskResponse {
    pub task_id: String,
    pub status: String,
}

/// SSE stream event parser
#[derive(Debug, Deserialize)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: Option<String>,
}

/// API client for task endpoints (including SSE streaming)
pub struct TasksApi {
    client: Client,
    base_url: String,
    token: String,
}

impl TasksApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self {
            client,
            base_url,
            token,
        }
    }

    /// POST /tasks — create a new task and return the task ID
    pub async fn create(&self, session_id: Option<String>, text: Option<String>) -> Result<TaskResponse> {
        let url = format!("{}/tasks", self.base_url);

        let body = CreateTaskRequest {
            session_id,
            text,
            mode: None,
        };

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .context("Failed to create task")?;

        if !resp.status().is_success() {
            anyhow::bail!("Failed to create task: {}", resp.status());
        }

        let task: TaskResponse = resp
            .json()
            .await
            .context("Failed to parse task response")?;

        Ok(task)
    }

    /// POST /tasks/stream — create a task with SSE streaming response
    /// Returns a receiver channel that yields TaskEvent items.
    pub async fn stream(
        &self,
        session_id: Option<String>,
        text: Option<String>,
    ) -> Result<mpsc::Receiver<Result<TaskEvent>>> {
        let url = format!("{}/tasks/stream", self.base_url);

        let body = CreateTaskRequest {
            session_id,
            text,
            mode: None,
        };

        // Start the SSE stream
        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .context("Failed to start task stream")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to start task stream: {}", response.status());
        }

        // Create a channel for sending events to the UI
        let (tx, rx) = mpsc::channel::<Result<TaskEvent>>(256);

        // Spawn a task to read the SSE stream
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        tokio::spawn(async move {
            let mut current_event: Option<String> = None;
            let mut current_data = String::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        buffer.push_str(&String::from_utf8_lossy(&chunk));

                        // Process complete lines from buffer
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim().to_string();
                            buffer = buffer[newline_pos + 1..].to_string();

                            if line.is_empty() {
                                // Empty line = end of event
                                if let Some(event_name) = current_event.take() {
                                    let data = std::mem::take(&mut current_data);
                                    if let Some(event) = parse_sse_line(&event_name, &data) {
                                        let _ = tx.send(Ok(event)).await;
                                    }
                                }
                            } else if let Some(value) = line.strip_prefix("event:") {
                                current_event = Some(value.trim().to_string());
                            } else if let Some(value) = line.strip_prefix("data:") {
                                if !current_data.is_empty() {
                                    current_data.push('\n');
                                }
                                current_data.push_str(value.trim());
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("SSE stream error: {}", e))).await;
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }

    /// GET /tasks/{task_id} — get task status
    pub async fn get_status(&self, task_id: &str) -> Result<TaskResponse> {
        let url = format!("{}/tasks/{}", self.base_url, task_id);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .context("Failed to get task status")?;

        if !resp.status().is_success() {
            anyhow::bail!("Failed to get task status: {}", resp.status());
        }

        let task: TaskResponse = resp
            .json()
            .await
            .context("Failed to parse task response")?;

        Ok(task)
    }
}

/// Parse an SSE event line into a typed TaskEvent
fn parse_sse_line(event: &str, data: &str) -> Option<TaskEvent> {
    match event {
        "task_started" => Some(TaskEvent::TaskStarted {
            task_id: data.to_string(),
        }),
        "task_completed" => Some(TaskEvent::TaskCompleted {
            task_id: data.to_string(),
        }),
        "task_error" => Some(TaskEvent::TaskError {
            task_id: String::new(),
            error: data.to_string(),
        }),
        "message" => {
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(data) {
                Some(TaskEvent::Message {
                    task_id: msg.get("task_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    action: msg.get("action").and_then(|v| v.as_str()).unwrap_or("updated").to_string(),
                    message: msg.get("message").cloned().unwrap_or(serde_json::Value::Null),
                })
            } else {
                None
            }
        }
        "tool_start" => {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                Some(TaskEvent::ToolStart {
                    tool_name: val.get("tool_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    arguments: val.get("arguments").cloned().unwrap_or(serde_json::Value::Null),
                })
            } else {
                None
            }
        }
        "tool_result" => {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                Some(TaskEvent::ToolResult {
                    tool_name: val.get("tool_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    result: val.get("result").cloned().unwrap_or(serde_json::Value::Null),
                })
            } else {
                None
            }
        }
        "tool_error" => {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                Some(TaskEvent::ToolError {
                    tool_name: val.get("tool_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    error: val.get("error").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                })
            } else {
                None
            }
        }
        "thinking" => Some(TaskEvent::Thinking {
            content: data.to_string(),
        }),
        "token_usage" => {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                Some(TaskEvent::TokenUsage {
                    tokens_in: val.get("tokens_in").and_then(|v| v.as_u64()).unwrap_or(0),
                    tokens_out: val.get("tokens_out").and_then(|v| v.as_u64()).unwrap_or(0),
                    cost: val.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}