use std::collections::HashMap;
use std::time::Duration;
use reqwest::{Client, Method};
use thiserror::Error;

use crate::sandbox::NetworkPolicy;

#[derive(Error, Debug)]
pub enum HttpError {
    #[error("Network policy error: {0}")]
    Blocked(String),
    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Invalid header: {0}")]
    InvalidHeader(String),
    #[error("Invalid method: {0}")]
    InvalidMethod(String),
}

pub struct HttpClient {
    client: Client,
    network_policy: NetworkPolicy,
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap(),
            network_policy: NetworkPolicy::new(),
        }
    }

    pub async fn request(
        &self,
        url: &str,
        method: &str,
        headers: HashMap<String, String>,
        body: Option<String>,
        timeout_seconds: u64,
    ) -> Result<(u16, String), HttpError> {
        // Validate URL
        self.network_policy
            .validate_url(url)
            .map_err(|e| HttpError::Blocked(e.to_string()))?;

        // Parse method
        let method = method
            .parse::<Method>()
            .map_err(|_| HttpError::InvalidMethod(method.to_string()))?;

        // Build request
        let mut request = self.client
            .request(method, url)
            .timeout(Duration::from_secs(timeout_seconds));

        // Add headers
        for (key, value) in headers {
            request = request.header(&key, &value);
        }

        // Add body
        if let Some(body) = body {
            request = request.body(body);
        }

        // Execute request
        let response = request.send().await?;
        let status = response.status().as_u16();
        let body = response.text().await?;

        Ok((status, body))
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}