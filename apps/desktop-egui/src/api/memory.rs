use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Matches backend MemoryResponse: {id, memory, score, metadata}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMemory {
    pub id: String,
    /// Backend field is "memory", not "content"
    pub memory: String,
    pub score: Option<f64>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Backend wraps list: {"memories": [...], "count": N}
#[derive(Debug, Deserialize)]
struct MemoryListResponse {
    pub memories: Vec<ApiMemory>,
    pub count: u64,
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

    /// GET /memories → {memories: [...], count: N}
    pub async fn list(&self) -> Result<Vec<ApiMemory>> {
        let url = format!("{}/memories", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch memories")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        let wrapper: MemoryListResponse = resp.json().await
            .context("Failed to parse memories")?;
        Ok(wrapper.memories)
    }

    /// POST /memories/search → {memories: [...], count: N}
    pub async fn search(&self, query: &str) -> Result<Vec<ApiMemory>> {
        let url = format!("{}/memories/search", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.token)
            .json(&serde_json::json!({ "query": query })).send().await
            .context("Failed to search memories")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        let wrapper: MemoryListResponse = resp.json().await
            .context("Failed to parse search results")?;
        Ok(wrapper.memories)
    }

    /// DELETE /memories/{id}
    pub async fn delete(&self, id: &str) -> Result<()> {
        let url = format!("{}/memories/{}", self.base_url, id);
        self.client.delete(&url).bearer_auth(&self.token).send().await
            .context("Failed to delete memory")?;
        Ok(())
    }
}