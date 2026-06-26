use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::state::task_state::TaskEvent;

/// Model override sent from client to backend
#[derive(Debug, Clone, Serialize, Default)]
pub struct ModelOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
}

/// Attachment metadata returned from upload and sent with task request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInfo {
    pub attachment_id: String,
    pub original_name: String,
    pub stored_path: String,
    pub mime_type: String,
    pub size: u64,
    pub is_text: bool,
}

/// Matches TaskCreate from backend: { session_id, input_text, mode_slug?, model_override?, attachments? }
#[derive(Debug, Serialize)]
pub struct CreateTaskRequest {
    pub session_id: String,
    pub input_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_override: Option<ModelOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<AttachmentInfo>>,
}

#[derive(Debug, Deserialize)]
pub struct TaskResponse {
    pub id: String,
    pub status: String,
}

pub struct TasksApi {
    client: Client,
    base_url: String,
    token: String,
}

impl TasksApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// POST /tasks — direct SSE stream
    pub async fn stream(
        &self,
        session_id: String,
        text: String,
        mode_slug: Option<String>,
        model_override: Option<ModelOverride>,
        attachments: Option<Vec<AttachmentInfo>>,
    ) -> Result<mpsc::Receiver<Result<TaskEvent>>> {
        let url = format!("{}/tasks", self.base_url);
        let body = CreateTaskRequest {
            session_id,
            input_text: text,
            mode_slug,
            model_override,
            attachments,
        };

        let response = self.client.post(&url).bearer_auth(&self.token).json(&body).send().await
            .context("Failed to start task")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to start task: {}", response.status());
        }

        let (tx, rx) = mpsc::channel::<Result<TaskEvent>>(256);
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        tokio::spawn(async move {
            let mut current_event: Option<String> = None;
            let mut current_data = String::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        buffer.push_str(&String::from_utf8_lossy(&chunk));
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer = buffer[pos + 1..].to_string();

                            if line.is_empty() {
                                if let Some(evt) = current_event.take() {
                                    let data = std::mem::take(&mut current_data);
                                    if let Some(event) = parse_sse_line(&evt, &data) {
                                        let _ = tx.send(Ok(event)).await;
                                    }
                                }
                            } else if let Some(v) = line.strip_prefix("event:") {
                                current_event = Some(v.trim().to_string());
                            } else if let Some(v) = line.strip_prefix("data:") {
                                if !current_data.is_empty() { current_data.push('\n'); }
                                current_data.push_str(v.trim());
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("SSE error: {}", e))).await;
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }
}

/// Parse an SSE event line into a TaskEvent.
///
/// The backend sends the full AgentEvent envelope in the `data` field:
/// ```json
/// event: thinking
/// data: {"type":"thinking","data":{"input":"..."},"timestamp":1234567890.123,"step":1}
/// ```
///
/// We must first parse the outer JSON, then extract the inner `.data` payload
/// to read the actual event fields.
fn parse_sse_line(event: &str, data: &str) -> Option<TaskEvent> {
    // Parse the outer envelope JSON
    let envelope: serde_json::Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Extract the inner data payload from the AgentEvent envelope.
    // The backend serializes: {"type":"X","data":{actual_payload},"timestamp":...,"step":...}
    // We need the inner "data" field for the actual event content.
    let payload = envelope.get("data").unwrap_or(&envelope);

    match event {
        "done" => {
            let task_id = payload.get("task_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(TaskEvent::TaskCompleted { task_id })
        }
        "error" => {
            let error = payload.get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error")
                .to_string();
            Some(TaskEvent::TaskError {
                task_id: String::new(),
                error,
            })
        }
        "message" => {
            // For message events, the content is in the payload
            let content = payload.get("content").cloned()
                .unwrap_or_else(|| payload.clone());
            let is_partial = payload.get("partial")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let action = if is_partial { "updated" } else { "created" };
            Some(TaskEvent::Message {
                task_id: String::new(),
                action: action.to_string(),
                message: content,
            })
        }
        "tool_call" => {
            let tool_name = payload.get("name")
                .or_else(|| payload.get("tool_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = payload.get("arguments")
                .or_else(|| payload.get("args"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            Some(TaskEvent::ToolStart {
                tool_name,
                arguments,
            })
        }
        "tool_result" => {
            let tool_name = payload.get("name")
                .or_else(|| payload.get("tool_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let result = payload.get("result")
                .or_else(|| payload.get("output"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            Some(TaskEvent::ToolResult {
                tool_name,
                result,
            })
        }
        "thinking" => {
            let content = payload.get("content")
                .or_else(|| payload.get("input"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(TaskEvent::Thinking { content })
        }
        "mode_switch" => {
            let from_mode = payload.get("from_mode")
                .or_else(|| payload.get("previous_mode"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let to_mode = payload.get("to_mode")
                .or_else(|| payload.get("current_mode"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mode_name = payload.get("mode_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(TaskEvent::ModeSwitch { from_mode, to_mode, mode_name })
        }
        "approval_requested" => {
            let request_id = payload.get("request_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tool_name = payload.get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = payload.get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let risk_level = payload.get("risk_level")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            Some(TaskEvent::ApprovalRequested {
                request_id,
                tool_name,
                arguments,
                risk_level,
            })
        }
        "approval_responded" => {
            let request_id = payload.get("request_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let approved = payload.get("approved")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Some(TaskEvent::ApprovalResponded { request_id, approved })
        }
        "checkpoint_saved" => {
            let checkpoint_id = payload.get("checkpoint_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let label = payload.get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(TaskEvent::CheckpointSaved { checkpoint_id, label })
        }
        "checkpoint_restored" => {
            let checkpoint_id = payload.get("checkpoint_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let label = payload.get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(TaskEvent::CheckpointRestored { checkpoint_id, label })
        }
        "context_compressed" => {
            let original_tokens = payload.get("original_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let compressed_tokens = payload.get("compressed_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            Some(TaskEvent::ContextCompressed { original_tokens, compressed_tokens })
        }
        "retry_delayed" => {
            let attempt = payload.get("attempt")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let delay_seconds = payload.get("delay_seconds")
                .or_else(|| payload.get("delay"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let reason = payload.get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(TaskEvent::RetryDelayed { attempt, delay_seconds, reason })
        }
        _ => None,
    }
}