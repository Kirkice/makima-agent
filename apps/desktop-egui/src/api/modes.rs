use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMode {
    pub slug: String,
    pub name: String,
    pub role_definition: Option<String>,
    pub when_to_use: Option<String>,
    pub description: Option<String>,
    pub custom_instructions: Option<String>,
    pub groups: Option<Vec<String>>,
    pub source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateModeRequest {
    pub slug: String,
    pub name: String,
    pub role_definition: Option<String>,
    pub when_to_use: Option<String>,
    pub description: Option<String>,
    pub custom_instructions: Option<String>,
    pub groups: Option<Vec<String>>,
}

pub struct ModesApi {
    client: Client,
    base_url: String,
    token: String,
}

impl ModesApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /api/modes
    pub async fn list(&self) -> Result<Vec<ApiMode>> {
        let url = format!("{}/api/modes", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch modes")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse modes")?)
    }

    /// GET /api/modes/{slug}
    pub async fn get(&self, slug: &str) -> Result<ApiMode> {
        let url = format!("{}/api/modes/{}", self.base_url, slug);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to get mode")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse mode")?)
    }

    /// POST /api/modes
    pub async fn create(&self, req: &CreateModeRequest) -> Result<ApiMode> {
        let url = format!("{}/api/modes", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.token).json(req).send().await
            .context("Failed to create mode")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse mode")?)
    }

    /// DELETE /api/modes/{slug}
    pub async fn delete(&self, slug: &str) -> Result<()> {
        let url = format!("{}/api/modes/{}", self.base_url, slug);
        let resp = self.client.delete(&url).bearer_auth(&self.token).send().await
            .context("Failed to delete mode")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(())
    }

    /// POST /api/modes/reload
    pub async fn reload(&self) -> Result<()> {
        let url = format!("{}/api/modes/reload", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.token).send().await
            .context("Failed to reload modes")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(())
    }
}