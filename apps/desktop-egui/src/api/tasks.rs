use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::state::task_state::TaskEvent;

/// Matches TaskCreate from backend: { session_id, input_text }
#[derive(Debug, Serialize)]
pub struct CreateTaskRequest {
    pub session_id: String,
    pub input_text: String,
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
    ) -> Result<mpsc::Receiver<Result<TaskEvent>>> {
        let url = format!("{}/tasks", self.base_url);
        let body = CreateTaskRequest { session_id, input_text: text };

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

fn parse_sse_line(event: &str, data: &str) -> Option<TaskEvent> {
    match event {
        "done" => Some(TaskEvent::TaskCompleted { task_id: String::new() }),
        "error" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                Some(TaskEvent::TaskError {
                    task_id: String::new(),
                    error: v.get("error").and_then(|x| x.as_str()).unwrap_or("unknown").to_string(),
                })
            } else { None }
        }
        "message" | "assistant" => {
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(data) {
                Some(TaskEvent::Message {
                    task_id: String::new(),
                    action: msg.get("partial").map(|_| "updated").unwrap_or("created").to_string(),
                    message: msg,
                })
            } else { None }
        }
        "tool_start" | "tool_call" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                Some(TaskEvent::ToolStart {
                    tool_name: v.get("name").or(v.get("tool_name")).and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    arguments: v.get("arguments").or(v.get("args")).cloned().unwrap_or(serde_json::Value::Null),
                })
            } else { None }
        }
        "tool_result" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                Some(TaskEvent::ToolResult {
                    tool_name: v.get("name").or(v.get("tool_name")).and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    result: v.get("result").or(v.get("output")).cloned().unwrap_or(serde_json::Value::Null),
                })
            } else { None }
        }
        "thinking" | "reasoning" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                Some(TaskEvent::Thinking {
                    content: v.get("content").and_then(|x| x.as_str()).unwrap_or(data).to_string(),
                })
            } else { None }
        }
        "token_usage" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                Some(TaskEvent::TokenUsage {
                    tokens_in: v.get("tokens_in").and_then(|x| x.as_u64()).unwrap_or(0),
                    tokens_out: v.get("tokens_out").and_then(|x| x.as_u64()).unwrap_or(0),
                    cost: v.get("cost").and_then(|x| x.as_f64()).unwrap_or(0.0),
                })
            } else { None }
        }
        _ => None,
    }
}