use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A parameter required for MCP server installation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpParameter {
    pub name: String,
    pub key: String,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub optional: bool,
}

/// A specific installation method for an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInstallationMethod {
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub parameters: Vec<McpParameter>,
}

/// An item in the MCP marketplace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    pub author_url: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    /// Either a string (single method) or array of methods
    pub content: MarketplaceContent,
    #[serde(default)]
    pub parameters: Vec<McpParameter>,
    /// Added by frontend - installation status
    #[serde(default)]
    pub installed: bool,
}

/// Marketplace content can be a single JSON string or an array of methods
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MarketplaceContent {
    Single(String),
    Multiple(Vec<McpInstallationMethod>),
}

impl MarketplaceContent {
    /// Get the list of installation methods
    pub fn methods(&self) -> Vec<McpInstallationMethod> {
        match self {
            MarketplaceContent::Single(content) => {
                vec![McpInstallationMethod {
                    name: "Default".to_string(),
                    content: content.clone(),
                    prerequisites: vec![],
                    parameters: vec![],
                }]
            }
            MarketplaceContent::Multiple(methods) => methods.clone(),
        }
    }

    /// Check if there are multiple installation methods
    pub fn has_multiple_methods(&self) -> bool {
        matches!(self, MarketplaceContent::Multiple(methods) if methods.len() > 1)
    }
}

/// Response from listing marketplace items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceItemListResponse {
    pub items: Vec<MarketplaceItem>,
    pub total: usize,
}

/// Request to install an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    pub item_id: String,
    pub target: String,
    pub selected_method_index: Option<usize>,
    pub parameters: HashMap<String, String>,
}

/// Response from installing an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResponse {
    pub success: bool,
    pub item_id: String,
    pub server_name: String,
    pub config_path: String,
    pub line: Option<usize>,
    pub error: Option<String>,
}

/// Request to uninstall an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallRequest {
    pub item_id: String,
    pub target: String,
}

/// Response from uninstalling an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallResponse {
    pub success: bool,
    pub item_id: String,
    pub server_name: String,
    pub config_path: String,
    pub error: Option<String>,
}

/// Information about an installed MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledItemInfo {
    pub item_id: String,
    pub server_name: String,
    pub target: String,
    pub config_path: String,
    pub enabled: bool,
}

/// Response from checking if an item is installed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledCheckResponse {
    pub installed: bool,
    pub item_id: String,
    pub target: String,
}

/// Marketplace API client
pub struct MarketplaceApi {
    client: Client,
    base_url: String,
    token: String,
}

impl MarketplaceApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /api/marketplace/items - List all marketplace items
    pub async fn list_items(
        &self,
        search: Option<&str>,
        tags: Option<&[String]>,
    ) -> Result<MarketplaceItemListResponse> {
        let mut url = format!("{}/api/marketplace/items", self.base_url);
        let mut params = vec![];

        if let Some(s) = search {
            params.push(format!("search={}", urlencoding(s)));
        }
        if let Some(t) = tags {
            for tag in t {
                params.push(format!("tags={}", urlencoding(tag)));
            }
        }
        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch marketplace items")?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to fetch marketplace items: {}", resp.status());
        }
        Ok(resp.json().await.context("Failed to parse marketplace items")?)
    }

    /// GET /api/marketplace/items/{id} - Get a single marketplace item
    pub async fn get_item(&self, item_id: &str) -> Result<MarketplaceItem> {
        let url = format!("{}/api/marketplace/items/{}", self.base_url, urlencoding(item_id));
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch marketplace item")?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to fetch marketplace item: {}", resp.status());
        }
        Ok(resp.json().await.context("Failed to parse marketplace item")?)
    }

    /// GET /api/marketplace/tags - Get all available tags
    pub async fn list_tags(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/marketplace/tags", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch marketplace tags")?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to fetch marketplace tags: {}", resp.status());
        }
        Ok(resp.json().await.context("Failed to parse marketplace tags")?)
    }

    /// POST /api/marketplace/install - Install an MCP server
    pub async fn install(&self, request: InstallRequest) -> Result<InstallResponse> {
        let url = format!("{}/api/marketplace/install", self.base_url);
        let resp = self.client.post(&url)
            .bearer_auth(&self.token)
            .json(&request)
            .send()
            .await
            .context("Failed to install marketplace item")?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to install marketplace item: {}", resp.status());
        }
        Ok(resp.json().await.context("Failed to parse install response")?)
    }

    /// POST /api/marketplace/uninstall - Uninstall an MCP server
    pub async fn uninstall(&self, request: UninstallRequest) -> Result<UninstallResponse> {
        let url = format!("{}/api/marketplace/uninstall", self.base_url);
        let resp = self.client.post(&url)
            .bearer_auth(&self.token)
            .json(&request)
            .send()
            .await
            .context("Failed to uninstall marketplace item")?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to uninstall marketplace item: {}", resp.status());
        }
        Ok(resp.json().await.context("Failed to parse uninstall response")?)
    }

    /// GET /api/marketplace/installed - Get list of installed items
    pub async fn list_installed(&self, target: &str) -> Result<Vec<InstalledItemInfo>> {
        let url = format!("{}/api/marketplace/installed?target={}", self.base_url, target);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch installed items")?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to fetch installed items: {}", resp.status());
        }
        Ok(resp.json().await.context("Failed to parse installed items")?)
    }

    /// GET /api/marketplace/items/{id}/installed - Check if an item is installed
    pub async fn is_installed(&self, item_id: &str, target: &str) -> Result<bool> {
        let url = format!(
            "{}/api/marketplace/items/{}/installed?target={}",
            self.base_url,
            urlencoding(item_id),
            target
        );
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to check installation status")?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to check installation status: {}", resp.status());
        }
        let check: InstalledCheckResponse = resp.json().await
            .context("Failed to parse installation check")?;
        Ok(check.installed)
    }
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}