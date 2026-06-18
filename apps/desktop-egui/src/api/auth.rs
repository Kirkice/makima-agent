use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// POST /auth/login body
#[derive(Debug, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// POST /auth/login response — matches TokenResponse
#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub user_id: Option<String>,
}

pub struct AuthApi {
    client: Client,
    base_url: String,
}

impl AuthApi {
    pub fn new(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    /// POST /auth/login
    pub async fn login(&self, username: &str, password: &str) -> Result<LoginResponse> {
        let url = format!("{}/auth/login", self.base_url);
        let form = LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        };
        let resp = self.client.post(&url).json(&form).send().await
            .context("Failed to send login request")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Login failed ({}): {}", status, body);
        }
        Ok(resp.json().await.context("Failed to parse login response")?)
    }

    /// GET /auth/me or equivalent — just verify token with a health check
    pub async fn verify_token(&self, token: &str) -> Result<bool> {
        let url = format!("{}/sessions", self.base_url);
        let resp = self.client.get(&url).bearer_auth(token).send().await
            .context("Failed to verify token")?;
        Ok(resp.status().is_success())
    }
}