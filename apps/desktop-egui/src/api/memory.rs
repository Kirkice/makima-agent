use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Matches backend /memories response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMemory {
    pub id: Option<String>,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryListResponse {
    items: Vec<ApiMemory>,
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

    /// GET /memories
    pub async fn list(&self) -> Result<Vec<ApiMemory>> {
        let url = format!("{}/memories", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch memories")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json::<MemoryListResponse>().await.map(|l| l.items).unwrap_or_default())
    }

    /// POST /memories/search
    pub async fn search(&self, query: &str) -> Result<Vec<ApiMemory>> {
        let url = format!("{}/memories/search", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.token)
            .json(&serde_json::json!({ "query": query })).send().await
            .context("Failed to search memories")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json::<MemoryListResponse>().await.map(|l| l.items).unwrap_or_default())
    }

    /// DELETE /memories/{id}
    pub async fn delete(&self, id: &str) -> Result<()> {
        let url = format!("{}/memories/{}", self.base_url, id);
        self.client.delete(&url).bearer_auth(&self.token).send().await
            .context("Failed to delete memory")?;
        Ok(())
    }
}