use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single model configuration profile (mirrors backend ModelProfile).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub name: String,
    pub provider: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_steps")]
    pub max_steps: i32,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: i32,
    #[serde(default)]
    pub max_tokens: Option<i32>,
    #[serde(default)]
    pub thinking_enabled: bool,
    #[serde(default)]
    pub thinking_budget: Option<i32>,
}

fn default_temperature() -> f64 { 0.7 }
fn default_max_steps() -> i32 { 100 }
fn default_timeout() -> i32 { 300 }

impl Default for ModelProfile {
    fn default() -> Self {
        Self {
            name: String::new(),
            provider: "openai-compatible".to_string(),
            base_url: String::new(),
            api_key: None,
            model: String::new(),
            temperature: 0.7,
            max_steps: 100,
            timeout_seconds: 300,
            max_tokens: None,
            thinking_enabled: false,
            thinking_budget: None,
        }
    }
}

/// Response from GET /api/model/profiles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigResponse {
    pub profiles: HashMap<String, ModelProfile>,
    pub active_profile: Option<String>,
}

/// Request body for POST /api/model/profiles
#[derive(Debug, Serialize)]
pub struct ModelProfileCreateRequest {
    pub name: String,
    pub profile: ModelProfile,
}

/// Request body for PUT /api/model/profiles/{name}
#[derive(Debug, Serialize)]
pub struct ModelProfileUpdateRequest {
    pub profile: ModelProfile,
}

/// Request body for POST /api/model/test-connection
#[derive(Debug, Serialize)]
pub struct TestConnectionRequest {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

/// Response from POST /api/model/test-connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConnectionResponse {
    pub ok: bool,
    pub message: String,
    pub latency_ms: Option<i32>,
}

/// Provider info from GET /api/model/providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub display_name: String,
    pub default_base_url: String,
}

pub struct ModelProfilesApi {
    client: Client,
    base_url: String,
    token: String,
}

impl ModelProfilesApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /api/model/profiles
    pub async fn list(&self) -> Result<ModelConfigResponse> {
        let url = format!("{}/api/model/profiles", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch model profiles")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed: {} - {}", status, body);
        }
        Ok(resp.json().await.context("Failed to parse model profiles")?)
    }

    /// POST /api/model/profiles
    pub async fn create(&self, name: &str, profile: &ModelProfile) -> Result<ModelProfile> {
        let url = format!("{}/api/model/profiles", self.base_url);
        let body = ModelProfileCreateRequest {
            name: name.to_string(),
            profile: profile.clone(),
        };
        let resp = self.client.post(&url).bearer_auth(&self.token).json(&body).send().await
            .context("Failed to create model profile")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed: {} - {}", status, body);
        }
        Ok(resp.json().await.context("Failed to parse created profile")?)
    }

    /// PUT /api/model/profiles/{name}
    pub async fn update(&self, name: &str, profile: &ModelProfile) -> Result<ModelProfile> {
        let url = format!("{}/api/model/profiles/{}", self.base_url, urlencoding(name));
        let body = ModelProfileUpdateRequest { profile: profile.clone() };
        let resp = self.client.put(&url).bearer_auth(&self.token).json(&body).send().await
            .context("Failed to update model profile")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed: {} - {}", status, body);
        }
        Ok(resp.json().await.context("Failed to parse updated profile")?)
    }

    /// DELETE /api/model/profiles/{name}
    pub async fn delete(&self, name: &str) -> Result<()> {
        let url = format!("{}/api/model/profiles/{}", self.base_url, urlencoding(name));
        let resp = self.client.delete(&url).bearer_auth(&self.token).send().await
            .context("Failed to delete model profile")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed: {} - {}", status, body);
        }
        Ok(())
    }

    /// POST /api/model/profiles/{name}/activate
    pub async fn activate(&self, name: &str) -> Result<ModelConfigResponse> {
        let url = format!("{}/api/model/profiles/{}/activate", self.base_url, urlencoding(name));
        let resp = self.client.post(&url).bearer_auth(&self.token).send().await
            .context("Failed to activate model profile")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed: {} - {}", status, body);
        }
        Ok(resp.json().await.context("Failed to parse activate response")?)
    }

    /// POST /api/model/test-connection
    pub async fn test_connection(&self, req: &TestConnectionRequest) -> Result<TestConnectionResponse> {
        let url = format!("{}/api/model/test-connection", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.token).json(req).send().await
            .context("Failed to test connection")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed: {} - {}", status, body);
        }
        Ok(resp.json().await.context("Failed to parse test connection response")?)
    }

    /// GET /api/model/providers
    pub async fn list_providers(&self) -> Result<Vec<ProviderInfo>> {
        let url = format!("{}/api/model/providers", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch providers")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed: {} - {}", status, body);
        }
        Ok(resp.json().await.context("Failed to parse providers")?)
    }
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}