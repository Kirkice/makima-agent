use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Matches Persona from backend (packages/schemas/src/makima_schemas/persona.py)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiPersona {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default)]
    pub identity: String,
    #[serde(default)]
    pub gender: String,
    #[serde(default)]
    pub age_perception: String,
    #[serde(default)]
    pub personality: String,
    #[serde(default)]
    pub traits: Vec<String>,
    #[serde(default)]
    pub emotional_style: String,
    #[serde(default)]
    pub speaking_style: String,
    #[serde(default = "default_semi_formal")]
    pub formality: String,
    #[serde(default = "default_concise")]
    pub verbosity: String,
    #[serde(default)]
    pub humor_style: String,
    #[serde(default = "default_minimal")]
    pub emoji_usage: String,
    #[serde(default)]
    pub quirks: Vec<String>,
    #[serde(default)]
    pub problem_approach: String,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub boundaries: Vec<String>,
    #[serde(default)]
    pub relationship_style: String,
    #[serde(default)]
    pub addressing_style: String,
    #[serde(default)]
    pub when_frustrated: String,
    #[serde(default)]
    pub when_success: String,
    #[serde(default)]
    pub when_user_confused: String,
    #[serde(default)]
    pub when_user_requests_help: String,
    #[serde(default)]
    pub catchphrases: Vec<String>,
}

fn default_name() -> String { "Makima".to_string() }
fn default_semi_formal() -> String { "semi-formal".to_string() }
fn default_concise() -> String { "concise".to_string() }
fn default_minimal() -> String { "minimal".to_string() }

/// Backend wraps persona response: {"persona": {...}}
#[derive(Debug, Deserialize)]
pub struct PersonaResponse {
    pub persona: ApiPersona,
}

/// Request to update persona — backend expects {"persona": {...}}
#[derive(Debug, Serialize)]
pub struct UpdatePersonaRequest {
    pub persona: ApiPersona,
}

pub struct PersonaApi {
    client: Client,
    base_url: String,
    token: String,
}

impl PersonaApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /api/persona → PersonaResponse { persona }
    pub async fn get_current(&self) -> Result<ApiPersona> {
        let url = format!("{}/api/persona", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to get persona")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        let wrapper: PersonaResponse = resp.json().await.context("Failed to parse persona")?;
        Ok(wrapper.persona)
    }

    /// GET /api/persona/default → PersonaResponse { persona }
    pub async fn get_default(&self) -> Result<ApiPersona> {
        let url = format!("{}/api/persona/default", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to get default persona")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        let wrapper: PersonaResponse = resp.json().await.context("Failed to parse persona")?;
        Ok(wrapper.persona)
    }

    /// PUT /api/persona → PersonaResponse { persona }
    pub async fn update(&self, persona: &ApiPersona) -> Result<ApiPersona> {
        let url = format!("{}/api/persona", self.base_url);
        let body = UpdatePersonaRequest { persona: persona.clone() };
        let resp = self.client.put(&url).bearer_auth(&self.token).json(&body).send().await
            .context("Failed to update persona")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        let wrapper: PersonaResponse = resp.json().await.context("Failed to parse persona")?;
        Ok(wrapper.persona)
    }

    /// POST /api/persona/reload → PersonaResponse { persona }
    pub async fn reload(&self) -> Result<ApiPersona> {
        let url = format!("{}/api/persona/reload", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.token).send().await
            .context("Failed to reload persona")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        let wrapper: PersonaResponse = resp.json().await.context("Failed to parse persona")?;
        Ok(wrapper.persona)
    }
}