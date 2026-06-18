use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

/// Health check response from backend
#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: Option<String>,
}

/// API client for health/diagnostics endpoints
pub struct HealthApi {
    client: Client,
    base_url: String,
}

impl HealthApi {
    pub fn new(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    /// GET /health — check backend health
    pub async fn check(&self) -> Result<HealthResponse> {
        let url = format!("{}/health", self.base_url);

        let resp = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .context("Failed to connect to backend")?;

        if !resp.status().is_success() {
            anyhow::bail!("Health check failed: {}", resp.status());
        }

        let health: HealthResponse = resp
            .json()
            .await
            .context("Failed to parse health response")?;

        Ok(health)
    }
}