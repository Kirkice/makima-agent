use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Matches backend DocumentResponse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDocument {
    pub id: String,
    pub filename: Option<String>,
    pub status: Option<String>,
    pub chunk_count: Option<u32>,
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DocumentListResponse {
    items: Vec<ApiDocument>,
}

#[derive(Debug, Deserialize)]
pub struct RetrievalResult {
    pub query: String,
    pub chunks: Vec<RetrievalChunk>,
}

#[derive(Debug, Deserialize)]
pub struct RetrievalChunk {
    pub content: String,
    pub score: Option<f32>,
    pub source: Option<String>,
}

pub struct KnowledgeApi {
    client: Client,
    base_url: String,
    token: String,
}

impl KnowledgeApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /knowledge/documents
    pub async fn list(&self) -> Result<Vec<ApiDocument>> {
        let url = format!("{}/knowledge/documents", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch documents")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json::<DocumentListResponse>().await.map(|l| l.items).unwrap_or_default())
    }

    /// DELETE /knowledge/documents/{id}
    pub async fn delete(&self, id: &str) -> Result<()> {
        let url = format!("{}/knowledge/documents/{}", self.base_url, id);
        self.client.delete(&url).bearer_auth(&self.token).send().await
            .context("Failed to delete document")?;
        Ok(())
    }

    /// POST /knowledge/retrieve
    pub async fn retrieve(&self, query: &str) -> Result<RetrievalResult> {
        let url = format!("{}/knowledge/retrieve", self.base_url);
        let body = serde_json::json!({ "query": query });
        let resp = self.client.post(&url).bearer_auth(&self.token).json(&body).send().await
            .context("Failed to retrieve knowledge")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse retrieval result")?)
    }
}