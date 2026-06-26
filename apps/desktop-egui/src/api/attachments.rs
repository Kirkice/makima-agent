//! Attachment upload API client.

use std::path::Path;

use anyhow::{Context, Result};
use reqwest::multipart;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::tasks::AttachmentInfo;

/// Response from the backend upload endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResponse {
    pub attachment_id: String,
    pub original_name: String,
    pub stored_path: String,
    pub mime_type: String,
    pub size: u64,
    pub is_text: bool,
}

pub struct AttachmentsApi {
    client: Client,
    base_url: String,
    token: String,
}

impl AttachmentsApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// POST /attachments/upload — multipart file upload
    ///
    /// Returns AttachmentInfo suitable for inclusion in a task request.
    pub async fn upload(
        &self,
        session_id: &str,
        file_path: &str,
    ) -> Result<AttachmentInfo> {
        let path = Path::new(file_path);
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let file_bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("Failed to read file: {}", file_path))?;

        let _file_size = file_bytes.len() as u64;

        // Guess MIME type from extension
        let mime_type = mime_guess_from_path(&file_name);

        let part = multipart::Part::bytes(file_bytes)
            .file_name(file_name.clone())
            .mime_str(&mime_type)
            .context("Failed to create multipart part")?;

        let form = multipart::Form::new()
            .text("session_id", session_id.to_string())
            .part("file", part);

        let url = format!("{}/attachments/upload", self.base_url);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .multipart(form)
            .send()
            .await
            .context("Failed to upload attachment")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Upload failed ({}): {}", status, body);
        }

        let upload_resp: UploadResponse =
            resp.json().await.context("Failed to parse upload response")?;

        Ok(AttachmentInfo {
            attachment_id: upload_resp.attachment_id,
            original_name: upload_resp.original_name,
            stored_path: upload_resp.stored_path,
            mime_type: upload_resp.mime_type,
            size: upload_resp.size,
            is_text: upload_resp.is_text,
        })
    }
}

/// Simple MIME type guesser based on file extension.
fn mime_guess_from_path(filename: &str) -> String {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "txt" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "xml" => "application/xml",
        "csv" => "text/csv",
        "py" => "text/x-python",
        "rs" => "text/x-rust",
        "js" => "application/javascript",
        "ts" => "application/typescript",
        "tsx" | "jsx" => "application/javascript",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "cs" => "text/x-csharp",
        "java" => "text/x-java",
        "cpp" | "cc" | "cxx" => "text/x-c++",
        "c" => "text/x-c",
        "h" | "hpp" => "text/x-c",
        "sh" | "bash" | "zsh" => "application/x-sh",
        "sql" => "application/sql",
        "go" => "text/x-go",
        "rb" => "text/x-ruby",
        "php" => "text/x-php",
        "swift" => "text/x-swift",
        "kt" => "text/x-kotlin",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        _ => "application/octet-stream",
    }
    .to_string()
}