use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Tool group configuration — mirrors backend ToolGroupConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolGroupConfig {
    pub group: String,
    pub file_regex: Option<String>,
    #[serde(default = "default_true")]
    pub auto_approve: bool,
}

fn default_true() -> bool { true }

/// Matches ModeConfig from backend (packages/schemas/src/makima_schemas/modes.py)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMode {
    pub slug: String,
    pub name: String,
    pub role_definition: Option<String>,
    pub when_to_use: Option<String>,
    pub description: Option<String>,
    pub custom_instructions: Option<String>,
    #[serde(default)]
    pub tool_groups: Vec<ToolGroupConfig>,
    #[serde(default = "default_max_steps")]
    pub max_steps: i32,
    #[serde(default)]
    pub temperature: f64,
    #[serde(default = "default_source")]
    pub source: String,
    pub model: Option<String>,
    pub api_base: Option<String>,
    // api_key intentionally omitted from frontend display
}

fn default_max_steps() -> i32 { 30 }
fn default_source() -> String { "builtin".to_string() }

/// Backend wraps list response: {"modes": [...], "total": N}
#[derive(Debug, Deserialize)]
pub struct ModeListResponse {
    pub modes: Vec<ApiMode>,
    pub total: u64,
}

/// Backend wraps single mode response: {"mode": {...}}
#[derive(Debug, Deserialize)]
pub struct ModeResponse {
    pub mode: ApiMode,
}

/// Backend delete response: {"deleted": bool, "slug": str}
#[derive(Debug, Deserialize)]
pub struct ModeDeleteResponse {
    pub deleted: bool,
    pub slug: String,
}

/// Request to create a custom mode — matches backend ModeCreateRequest
#[derive(Debug, Serialize)]
pub struct CreateModeRequest {
    pub slug: String,
    pub name: String,
    pub role_definition: String,
    pub when_to_use: Option<String>,
    pub description: Option<String>,
    pub custom_instructions: Option<String>,
    #[serde(default)]
    pub tool_groups: Vec<ToolGroupConfig>,
    #[serde(default = "default_max_steps")]
    pub max_steps: i32,
    #[serde(default)]
    pub temperature: f64,
}

pub struct ModesApi {
    client: Client,
    base_url: String,
    token: String,
}

impl ModesApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /api/modes → ModeListResponse { modes, total }
    pub async fn list(&self) -> Result<ModeListResponse> {
        let url = format!("{}/api/modes", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to fetch modes")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse modes")?)
    }

    /// GET /api/modes/{slug} → ModeResponse { mode }
    pub async fn get(&self, slug: &str) -> Result<ApiMode> {
        let url = format!("{}/api/modes/{}", self.base_url, slug);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to get mode")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        let wrapper: ModeResponse = resp.json().await.context("Failed to parse mode")?;
        Ok(wrapper.mode)
    }

    /// POST /api/modes → ModeResponse { mode }
    pub async fn create(&self, req: &CreateModeRequest) -> Result<ApiMode> {
        let url = format!("{}/api/modes", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.token).json(req).send().await
            .context("Failed to create mode")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        let wrapper: ModeResponse = resp.json().await.context("Failed to parse mode")?;
        Ok(wrapper.mode)
    }

    /// DELETE /api/modes/{slug} → ModeDeleteResponse { deleted, slug }
    pub async fn delete(&self, slug: &str) -> Result<ModeDeleteResponse> {
        let url = format!("{}/api/modes/{}", self.base_url, slug);
        let resp = self.client.delete(&url).bearer_auth(&self.token).send().await
            .context("Failed to delete mode")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse delete response")?)
    }

    /// POST /api/modes/reload → ModeListResponse { modes, total }
    pub async fn reload(&self) -> Result<ModeListResponse> {
        let url = format!("{}/api/modes/reload", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.token).send().await
            .context("Failed to reload modes")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse modes")?)
    }
}