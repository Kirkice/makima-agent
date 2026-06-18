use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Session from the backend API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSession {
    pub id: String,
    pub title: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub mode: Option<String>,
}

/// Create session request
#[derive(Debug, Serialize)]
pub struct CreateSessionRequest {
    pub title: Option<String>,
}

/// Update session request
#[derive(Debug, Serialize)]
pub struct UpdateSessionRequest {
    pub title: Option<String>,
    pub mode: Option<String>,
}

/// API client for session endpoints
pub struct SessionsApi {
    client: Client,
    base_url: String,
    token: String,
}

impl SessionsApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self {
            client,
            base_url,
            token,
        }
    }

    /// GET /sessions — list all sessions
    pub async fn list(&self) -> Result<Vec<ApiSession>> {
        let url = format!("{}/sessions", self.base_url);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .context("Failed to fetch sessions")?;

        if !resp.status().is_success() {
            anyhow::bail!("Failed to fetch sessions: {}", resp.status());
        }

        let sessions: Vec<ApiSession> = resp
            .json()
            .await
            .context("Failed to parse sessions response")?;

        Ok(sessions)
    }

    /// POST /sessions — create a new session
    pub async fn create(&self, title: Option<String>) -> Result<ApiSession> {
        let url = format!("{}/sessions", self.base_url);

        let body = CreateSessionRequest { title };

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .context("Failed to create session")?;

        if !resp.status().is_success() {
            anyhow::bail!("Failed to create session: {}", resp.status());
        }

        let session: ApiSession = resp
            .json()
            .await
            .context("Failed to parse session response")?;

        Ok(session)
    }

    /// PATCH /sessions/{id} — update a session
    pub async fn update(&self, id: &str, title: Option<String>, mode: Option<String>) -> Result<ApiSession> {
        let url = format!("{}/sessions/{}", self.base_url, id);

        let body = UpdateSessionRequest { title, mode };

        let resp = self
            .client
            .patch(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .context("Failed to update session")?;

        if !resp.status().is_success() {
            anyhow::bail!("Failed to update session: {}", resp.status());
        }

        let session: ApiSession = resp
            .json()
            .await
            .context("Failed to parse session response")?;

        Ok(session)
    }

    /// DELETE /sessions/{id} — delete a session
    pub async fn delete(&self, id: &str) -> Result<()> {
        let url = format!("{}/sessions/{}", self.base_url, id);

        let resp = self
            .client
            .delete(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .context("Failed to delete session")?;

        if !resp.status().is_success() {
            anyhow::bail!("Failed to delete session: {}", resp.status());
        }

        Ok(())
    }

    /// PATCH /sessions/{id}/mode — set session mode
    pub async fn set_mode(&self, id: &str, mode: &str) -> Result<ApiSession> {
        self.update(id, None, Some(mode.to_string())).await
    }
}