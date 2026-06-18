use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMemory {
    pub id: String,
    pub content: String,
    pub category: Option<String>,
    pub created_at: Option<String>,
    pub pinned: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CreateMemoryRequest {
    pub content: String,
    pub category: Option<String>,
}

pub struct MemoryApi {
    client: Client,
    base_url: String,
    token: String,
}

impl MemoryApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /api/memory
    pub async fn list(&self) -> Result<Vec<ApiMemory>> {
        let url = format!("{}/api/memory", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch memories")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse memories")?)
    }

    /// GET /api/memory?search=...
    pub async fn search(&self, query: &str) -> Result<Vec<ApiMemory>> {
        let url = format!("{}/api/memory?search={}", self.base_url, urlencoding(query));
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to search memories")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse memories")?)
    }

    /// POST /api/memory
    pub async fn create(&self, req: &CreateMemoryRequest) -> Result<ApiMemory> {
        let url = format!("{}/api/memory", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.token).json(req).send().await
            .context("Failed to create memory")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse memory")?)
    }

    /// DELETE /api/memory/{id}
    pub async fn delete(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/memory/{}", self.base_url, id);
        let resp = self.client.delete(&url).bearer_auth(&self.token).send().await
            .context("Failed to delete memory")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(())
    }
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}