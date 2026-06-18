use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Matches SessionResponse from backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSession {
    pub id: String,
    pub user_id: Option<String>,
    pub title: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Matches SessionList { items, total }
#[derive(Debug, Deserialize)]
pub struct SessionList {
    pub items: Vec<ApiSession>,
    pub total: u64,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionRequest {
    pub title: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateSessionRequest {
    pub title: Option<String>,
}

pub struct SessionsApi {
    client: Client,
    base_url: String,
    token: String,
}

impl SessionsApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /sessions → SessionList { items, total }
    pub async fn list(&self) -> Result<Vec<ApiSession>> {
        let url = format!("{}/sessions", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch sessions")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        let list: SessionList = resp.json().await.context("Failed to parse sessions")?;
        Ok(list.items)
    }

    /// POST /sessions
    pub async fn create(&self, title: Option<String>) -> Result<ApiSession> {
        let url = format!("{}/sessions", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.token).json(&CreateSessionRequest { title }).send().await
            .context("Failed to create session")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await?)
    }

    /// PATCH /sessions/{id}
    pub async fn update(&self, id: &str, title: Option<String>) -> Result<ApiSession> {
        let url = format!("{}/sessions/{}", self.base_url, id);
        let resp = self.client.patch(&url).bearer_auth(&self.token).json(&UpdateSessionRequest { title }).send().await
            .context("Failed to update session")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await?)
    }

    /// DELETE /sessions/{id}
    pub async fn delete(&self, id: &str) -> Result<()> {
        let url = format!("{}/sessions/{}", self.base_url, id);
        self.client.delete(&url).bearer_auth(&self.token).send().await
            .context("Failed to delete session")?;
        Ok(())
    }
}