use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Login request body
#[derive(Debug, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response body
#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    #[serde(rename = "token_type")]
    pub token_type: Option<String>,
}

/// API client for authentication endpoints
pub struct AuthApi {
    client: Client,
    base_url: String,
}

impl AuthApi {
    pub fn new(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    /// POST /auth/token — login with username/password
    pub async fn login(&self, username: &str, password: &str) -> Result<LoginResponse> {
        let url = format!("{}/auth/token", self.base_url);

        let form = LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        };

        let resp = self
            .client
            .post(&url)
            .json(&form)
            .send()
            .await
            .context("Failed to send login request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Login failed ({}): {}", status, body);
        }

        let login_resp: LoginResponse = resp
            .json()
            .await
            .context("Failed to parse login response")?;

        Ok(login_resp)
    }

    /// GET /auth/verify — verify the current token is valid
    pub async fn verify_token(&self, token: &str) -> Result<bool> {
        let url = format!("{}/auth/verify", self.base_url);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .context("Failed to verify token")?;

        Ok(resp.status().is_success())
    }
}