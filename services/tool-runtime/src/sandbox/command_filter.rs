use thiserror::Error;

#[derive(Error, Debug)]
pub enum CommandFilterError {
    #[error("Command blocked: {0}")]
    Blocked(String),
}

pub struct CommandFilter {
    blocked_patterns: Vec<String>,
}

impl CommandFilter {
    pub fn new() -> Self {
        Self {
            blocked_patterns: vec![
                "rm -rf /".to_string(),
                "rm -rf /*".to_string(),
                "mkfs".to_string(),
                ":(){ :|:& };:".to_string(),
                "fork bomb".to_string(),
                "dd if=".to_string(),
                "> /dev/sda".to_string(),
            ],
        }
    }

    pub fn validate_command(&self, command: &str) -> Result<(), CommandFilterError> {
        let command_lower = command.to_lowercase();
        
        for pattern in &self.blocked_patterns {
            if command_lower.contains(pattern) {
                return Err(CommandFilterError::Blocked(format!(
                    "Command contains blocked pattern: {}",
                    pattern
                )));
            }
        }
        
        Ok(())
    }

    pub fn add_blocked_pattern(&mut self, pattern: String) {
        self.blocked_patterns.push(pattern);
    }
}

impl Default for CommandFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_command() {
        let filter = CommandFilter::new();
        assert!(filter.validate_command("ls -la").is_ok());
        assert!(filter.validate_command("echo hello").is_ok());
    }

    #[test]
    fn test_blocked_command() {
        let filter = CommandFilter::new();
        assert!(matches!(
            filter.validate_command("rm -rf /"),
            Err(CommandFilterError::Blocked(_))
        ));
    }
}