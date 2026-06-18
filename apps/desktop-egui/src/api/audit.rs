use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: Option<String>,
    pub severity: Option<String>,
    pub action: Option<String>,
    pub resource: Option<String>,
    pub detail: Option<String>,
}

pub struct AuditApi {
    client: Client,
    base_url: String,
    token: String,
}

impl AuditApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /api/audit
    pub async fn list(&self) -> Result<Vec<AuditEntry>> {
        let url = format!("{}/api/audit", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch audit log")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse audit log")?)
    }

    /// GET /api/audit?severity=...&action=...
    pub async fn query(&self, severity: Option<&str>, action: Option<&str>) -> Result<Vec<AuditEntry>> {
        let mut url = format!("{}/api/audit", self.base_url);
        let mut params = vec![];
        if let Some(s) = severity { params.push(format!("severity={}", urlencoding(s))); }
        if let Some(a) = action { params.push(format!("action={}", urlencoding(a))); }
        if !params.is_empty() { url.push_str(&format!("?{}", params.join("&"))); }
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to query audit log")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse audit log")?)
    }
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}