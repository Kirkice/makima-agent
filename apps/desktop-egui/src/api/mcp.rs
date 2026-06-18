use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    pub transport_type: Option<String>,
    pub status: Option<String>,
    pub enabled: Option<bool>,
    pub last_error: Option<String>,
    pub tools: Option<Vec<McpTool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

pub struct McpApi {
    client: Client,
    base_url: String,
    token: String,
}

impl McpApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /api/mcp/servers
    pub async fn list(&self) -> Result<Vec<McpServer>> {
        let url = format!("{}/api/mcp/servers", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch MCP servers")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse MCP servers")?)
    }

    /// POST /api/mcp/servers/{id}/reconnect
    pub async fn reconnect(&self, name: &str) -> Result<McpServer> {
        let url = format!("{}/api/mcp/servers/{}/reconnect", self.base_url, urlencoding(name));
        let resp = self.client.post(&url).bearer_auth(&self.token).send().await
            .context("Failed to reconnect MCP server")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse MCP server")?)
    }

    /// POST /api/mcp/servers/{id}/toggle
    pub async fn toggle(&self, name: &str, enabled: bool) -> Result<McpServer> {
        let url = format!("{}/api/mcp/servers/{}/toggle", self.base_url, urlencoding(name));
        let body = serde_json::json!({ "enabled": enabled });
        let resp = self.client.post(&url).bearer_auth(&self.token).json(&body).send().await
            .context("Failed to toggle MCP server")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse MCP server")?)
    }
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}