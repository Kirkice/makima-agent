use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// A single audit log entry (mirrors backend AuditLogResponse).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub user_email: Option<String>,
    #[serde(default)]
    pub user_role: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    /// Backend field: resource_type
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub resource_id: Option<String>,
    #[serde(default)]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub details: Option<serde_json::Value>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    /// Convenience field derived from resource_type for backwards-compat
    #[serde(skip)]
    pub resource: Option<String>,
    /// Convenience field derived from details/error_message for backwards-compat
    #[serde(skip)]
    pub detail: Option<String>,
}

/// Backend returns { items: [...], total: N }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogListResponse {
    pub items: Vec<AuditEntry>,
    pub total: i64,
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

    /// GET /api/audit — returns all entries (no filter).
    pub async fn list(&self) -> Result<Vec<AuditEntry>> {
        let url = format!("{}/api/audit", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch audit log")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed: {} — {}", status, body);
        }
        let list: AuditLogListResponse = resp.json().await
            .context("Failed to parse audit log response")?;
        Ok(Self::enrich(list.items))
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
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed: {} — {}", status, body);
        }
        let list: AuditLogListResponse = resp.json().await
            .context("Failed to parse audit log response")?;
        Ok(Self::enrich(list.items))
    }

    /// Derive convenience `resource` and `detail` fields from backend fields.
    fn enrich(mut items: Vec<AuditEntry>) -> Vec<AuditEntry> {
        for e in items.iter_mut() {
            e.resource = e.resource_type.clone();
            e.detail = e.error_message.clone().or_else(|| {
                e.details.as_ref().map(|v| v.to_string())
            });
        }
        items
    }
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}