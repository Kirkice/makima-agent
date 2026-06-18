use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiPersona {
    pub name: String,
    pub content: Option<String>,
    pub is_default: Option<bool>,
    pub modified: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct UpdatePersonaRequest {
    pub content: String,
}

pub struct PersonaApi {
    client: Client,
    base_url: String,
    token: String,
}

impl PersonaApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /api/persona
    pub async fn get_current(&self) -> Result<ApiPersona> {
        let url = format!("{}/api/persona", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to get persona")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse persona")?)
    }

    /// GET /api/persona/default
    pub async fn get_default(&self) -> Result<ApiPersona> {
        let url = format!("{}/api/persona/default", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to get default persona")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse persona")?)
    }

    /// PUT /api/persona (in-memory update)
    pub async fn update(&self, content: &str) -> Result<ApiPersona> {
        let url = format!("{}/api/persona", self.base_url);
        let body = UpdatePersonaRequest { content: content.to_string() };
        let resp = self.client.put(&url).bearer_auth(&self.token).json(&body).send().await
            .context("Failed to update persona")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse persona")?)
    }

    /// POST /api/persona/reload
    pub async fn reload(&self) -> Result<ApiPersona> {
        let url = format!("{}/api/persona/reload", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.token).send().await
            .context("Failed to reload persona")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse persona")?)
    }
}