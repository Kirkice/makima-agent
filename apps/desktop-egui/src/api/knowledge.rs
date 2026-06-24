use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Matches backend DocumentResponse: {id, title, file_type, file_size, status, chunk_count, error, created_at}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDocument {
    pub id: String,
    pub title: String,
    pub file_type: String,
    pub file_size: u64,
    pub status: String,
    #[serde(default)]
    pub chunk_count: u32,
    pub error: Option<String>,
    pub created_at: String,
}

/// Backend wraps document list: {"documents": [...], "count": N}
#[derive(Debug, Deserialize)]
struct DocumentListResponse {
    pub documents: Vec<ApiDocument>,
    pub count: u64,
}

/// Backend retrieval response: {"results": [...], "count": N}
#[derive(Debug, Deserialize)]
pub struct RetrievalResult {
    pub results: Vec<RetrievalChunk>,
    pub count: u64,
}

/// Matches backend RetrievalResponse: {content, document_id, document_title, chunk_index, score}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalChunk {
    pub content: String,
    pub document_id: String,
    pub document_title: String,
    pub chunk_index: u32,
    pub score: f32,
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

    /// GET /knowledge/documents → {documents: [...], count: N}
    pub async fn list(&self) -> Result<Vec<ApiDocument>> {
        let url = format!("{}/knowledge/documents", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch documents")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        let wrapper: DocumentListResponse = resp.json().await
            .context("Failed to parse documents")?;
        Ok(wrapper.documents)
    }

    /// DELETE /knowledge/documents/{id}
    pub async fn delete(&self, id: &str) -> Result<()> {
        let url = format!("{}/knowledge/documents/{}", self.base_url, id);
        self.client.delete(&url).bearer_auth(&self.token).send().await
            .context("Failed to delete document")?;
        Ok(())
    }

    /// POST /knowledge/retrieve → {results: [...], count: N}
    pub async fn retrieve(&self, query: &str) -> Result<RetrievalResult> {
        let url = format!("{}/knowledge/retrieve", self.base_url);
        let body = serde_json::json!({ "query": query });
        let resp = self.client.post(&url).bearer_auth(&self.token).json(&body).send().await
            .context("Failed to retrieve knowledge")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse retrieval result")?)
    }
}