use anyhow::{Context, Result};

/// Service name for the OS keyring
const KEYRING_SERVICE: &str = "makima-agent";
/// Key for storing the auth token
const KEYRING_KEY_TOKEN: &str = "auth_token";
/// Key for storing the API key
const KEYRING_KEY_API_KEY: &str = "api_key";

/// Secure credential storage using the OS keyring (keyring crate).
pub struct SecureStore {
    use_keyring: bool,
}

impl Default for SecureStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecureStore {
    pub fn new() -> Self {
        let use_keyring = Self::check_keyring_available();
        Self { use_keyring }
    }

    /// Check if the OS keyring is available
    fn check_keyring_available() -> bool {
        match keyring::Entry::new(KEYRING_SERVICE, "_test") {
            Ok(entry) => {
                let _ = entry.set_password("test");
                // Cleaup by setting empty password
                let _ = entry.set_password("");
                true
            }
            Err(_) => false,
        }
    }

    /// Store the auth token
    pub fn store_token(&self, token: &str) -> Result<()> {
        if self.use_keyring {
            let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_KEY_TOKEN)
                .context("Failed to create keyring entry for token")?;
            entry
                .set_password(token)
                .context("Failed to store token in keyring")?;
        }
        Ok(())
    }

    /// Retrieve the auth token
    pub fn get_token(&self) -> Option<String> {
        if self.use_keyring {
            if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_KEY_TOKEN) {
                if let Ok(password) = entry.get_password() {
                    if !password.is_empty() {
                        return Some(password);
                    }
                }
            }
        }
        None
    }

    /// Delete the stored auth token (by overwriting with empty)
    pub fn delete_token(&self) -> Result<()> {
        if self.use_keyring {
            let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_KEY_TOKEN)
                .context("Failed to create keyring entry for token")?;
            entry
                .set_password("")
                .context("Failed to clear token from keyring")?;
        }
        Ok(())
    }

    /// Store an API key
    pub fn store_api_key(&self, key: &str) -> Result<()> {
        if self.use_keyring {
            let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_KEY_API_KEY)
                .context("Failed to create keyring entry for API key")?;
            entry
                .set_password(key)
                .context("Failed to store API key in keyring")?;
        }
        Ok(())
    }

    /// Retrieve the API key
    pub fn get_api_key(&self) -> Option<String> {
        if self.use_keyring {
            if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_KEY_API_KEY) {
                if let Ok(password) = entry.get_password() {
                    if !password.is_empty() {
                        return Some(password);
                    }
                }
            }
        }
        None
    }

    /// Delete the stored API key (by overwriting with empty)
    pub fn delete_api_key(&self) -> Result<()> {
        if self.use_keyring {
            let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_KEY_API_KEY)
                .context("Failed to create keyring entry for API key")?;
            entry
                .set_password("")
                .context("Failed to clear API key from keyring")?;
        }
        Ok(())
    }

    pub fn has_token(&self) -> bool {
        self.get_token().is_some()
    }

    pub fn has_api_key(&self) -> bool {
        self.get_api_key().is_some()
    }

    pub fn is_using_keyring(&self) -> bool {
        self.use_keyring
    }
}