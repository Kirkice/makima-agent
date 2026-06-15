use thiserror::Error;

#[derive(Error, Debug)]
pub enum NetworkPolicyError {
    #[error("URL blocked: {0}")]
    Blocked(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

pub struct NetworkPolicy {
    blocked_prefixes: Vec<String>,
}

impl NetworkPolicy {
    pub fn new() -> Self {
        Self {
            blocked_prefixes: vec![
                "http://localhost".to_string(),
                "http://127.0.0.1".to_string(),
                "http://10.".to_string(),
                "http://192.168.".to_string(),
                "http://172.16.".to_string(),
                "http://172.17.".to_string(),
                "http://172.18.".to_string(),
                "http://172.19.".to_string(),
                "http://172.20.".to_string(),
                "http://172.21.".to_string(),
                "http://172.22.".to_string(),
                "http://172.23.".to_string(),
                "http://172.24.".to_string(),
                "http://172.25.".to_string(),
                "http://172.26.".to_string(),
                "http://172.27.".to_string(),
                "http://172.28.".to_string(),
                "http://172.29.".to_string(),
                "http://172.30.".to_string(),
                "http://172.31.".to_string(),
            ],
        }
    }

    pub fn validate_url(&self, url: &str) -> Result<(), NetworkPolicyError> {
        let url_lower = url.to_lowercase();
        
        for prefix in &self.blocked_prefixes {
            if url_lower.starts_with(prefix) {
                return Err(NetworkPolicyError::Blocked(format!(
                    "URL matches blocked prefix: {}",
                    prefix
                )));
            }
        }
        
        Ok(())
    }

    pub fn add_blocked_prefix(&mut self, prefix: String) {
        self.blocked_prefixes.push(prefix);
    }
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_url() {
        let policy = NetworkPolicy::new();
        assert!(policy.validate_url("https://api.example.com").is_ok());
        assert!(policy.validate_url("https://google.com").is_ok());
    }

    #[test]
    fn test_blocked_url() {
        let policy = NetworkPolicy::new();
        assert!(matches!(
            policy.validate_url("http://localhost:8080"),
            Err(NetworkPolicyError::Blocked(_))
        ));
    }
}