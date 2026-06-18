use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSettings {
    pub tts_provider: Option<String>,
    pub active_voice_id: Option<String>,
    pub push_to_talk: Option<bool>,
    pub mic_device: Option<String>,
    pub speaker_device: Option<String>,
}

pub struct VoiceApi {
    client: Client,
    base_url: String,
    token: String,
}

impl VoiceApi {
    pub fn new(client: Client, base_url: String, token: String) -> Self {
        Self { client, base_url, token }
    }

    /// GET /api/voice/settings
    pub async fn get_settings(&self) -> Result<VoiceSettings> {
        let url = format!("{}/api/voice/settings", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.token).send().await
            .context("Failed to get voice settings")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse voice settings")?)
    }

    /// PUT /api/voice/settings
    pub async fn update_settings(&self, settings: &VoiceSettings) -> Result<VoiceSettings> {
        let url = format!("{}/api/voice/settings", self.base_url);
        let resp = self.client.put(&url).bearer_auth(&self.token).json(settings).send().await
            .context("Failed to update voice settings")?;
        if !resp.status().is_success() { anyhow::bail!("Failed: {}", resp.status()); }
        Ok(resp.json().await.context("Failed to parse voice settings")?)
    }
}