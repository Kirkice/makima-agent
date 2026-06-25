use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Backend config entry: { key, value }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub key: String,
    pub value: serde_json::Value,
}

/// Request body for PUT /admin/config
#[derive(Debug, Serialize)]
pub struct ConfigUpdateRequest {
    pub key: String,
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<i64>,
}

pub struct ConfigApi {
    client: Client,
    base_url: String,
    token: String,
}

impl ConfigApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /admin/config — list all config entries (requires admin).
    pub async fn list(&self) -> Result<Vec<ConfigEntry>> {
        let url = format!("{}/admin/config", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch config")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed: {} — {}", status, body);
        }
        Ok(resp.json().await.context("Failed to parse config")?)
    }

    /// PUT /admin/config — set a single config key.
    pub async fn set(&self, key: &str, value: serde_json::Value, ttl: Option<i64>) -> Result<()> {
        let url = format!("{}/admin/config", self.base_url);
        let body = ConfigUpdateRequest {
            key: key.to_string(),
            value,
            ttl,
        };
        let resp = self.client.put(&url).bearer_auth(&self.token).json(&body).send().await
            .context("Failed to update config")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed: {} — {}", status, body);
        }
        Ok(())
    }

    /// Convenience: fetch all config and collapse into a HashMap<String, String>.
    pub async fn fetch_all_map(&self) -> Result<HashMap<String, String>> {
        let entries = self.list().await?;
        let mut map = HashMap::new();
        for e in entries {
            let s = match &e.value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            map.insert(e.key, s);
        }
        Ok(map)
    }
}