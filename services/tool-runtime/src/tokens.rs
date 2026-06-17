//! Token counter service - count and truncate tokens
//!
//! Provides token counting and truncation capabilities using tiktoken
//! for OpenAI-compatible models.

use tracing::debug;

/// Token counter using tiktoken
pub struct TokenCounter;

impl TokenCounter {
    pub fn new() -> Self {
        Self
    }

    /// Count tokens in text for a given model
    /// Uses a simple approximation if tiktoken is not available
    pub fn count(&self, text: &str, model: &str) -> Result<u32, String> {
        // Try to use tiktoken for accurate counting
        match self.count_with_tiktoken(text, model) {
            Ok(count) => Ok(count),
            Err(_) => {
                // Fallback to approximation: ~4 chars per token for English
                let approx = (text.len() as f64 / 4.0).ceil() as u32;
                debug!("Using approximation for model {}: {} tokens", model, approx);
                Ok(approx)
            }
        }
    }

    /// Count tokens using tiktoken
    fn count_with_tiktoken(&self, text: &str, model: &str) -> Result<u32, String> {
        use tiktoken_rs::get_bpe_from_model;

        let bpe = get_bpe_from_model(model)
            .map_err(|e| format!("Failed to load tokenizer for model {}: {}", model, e))?;

        let tokens = bpe.encode_ordinary(text);
        Ok(tokens.len() as u32)
    }

    /// Truncate text to a maximum number of tokens
    /// If preserve_start is true, keeps the beginning; otherwise keeps the end
    pub fn truncate(
        &self,
        text: &str,
        max_tokens: u32,
        model: &str,
        preserve_start: bool,
    ) -> Result<(String, u32, u32, bool), String> {
        let original_tokens = self.count(text, model)?;

        if original_tokens <= max_tokens {
            return Ok((text.to_string(), original_tokens, original_tokens, false));
        }

        // Need to truncate
        let truncated = if preserve_start {
            self.truncate_from_end(text, max_tokens, model)?
        } else {
            self.truncate_from_start(text, max_tokens, model)?
        };

        let truncated_tokens = self.count(&truncated, model)?;

        Ok((truncated, original_tokens, truncated_tokens, true))
    }

    /// Truncate keeping the start (remove from end)
    fn truncate_from_start(
        &self,
        text: &str,
        max_tokens: u32,
        model: &str,
    ) -> Result<String, String> {
        use tiktoken_rs::get_bpe_from_model;

        match get_bpe_from_model(model) {
            Ok(bpe) => {
                let tokens = bpe.encode_ordinary(text);
                if tokens.len() <= max_tokens as usize {
                    return Ok(text.to_string());
                }
                let truncated_tokens: Vec<_> = tokens.into_iter().take(max_tokens as usize).collect();
                bpe.decode(truncated_tokens)
                    .map_err(|e| format!("Failed to decode tokens: {}", e))
            }
            Err(_) => {
                // Fallback: approximate character-based truncation
                let chars_per_token = 4.0;
                let max_chars = (max_tokens as f64 * chars_per_token) as usize;
                Ok(text.chars().take(max_chars).collect())
            }
        }
    }

    /// Truncate keeping the end (remove from start)
    fn truncate_from_end(
        &self,
        text: &str,
        max_tokens: u32,
        model: &str,
    ) -> Result<String, String> {
        use tiktoken_rs::get_bpe_from_model;

        match get_bpe_from_model(model) {
            Ok(bpe) => {
                let tokens = bpe.encode_ordinary(text);
                if tokens.len() <= max_tokens as usize {
                    return Ok(text.to_string());
                }
                let start = tokens.len() - max_tokens as usize;
                let truncated_tokens: Vec<_> = tokens.into_iter().skip(start).collect();
                bpe.decode(truncated_tokens)
                    .map_err(|e| format!("Failed to decode tokens: {}", e))
            }
            Err(_) => {
                // Fallback: approximate character-based truncation
                let chars_per_token = 4.0;
                let max_chars = (max_tokens as f64 * chars_per_token) as usize;
                let skip_chars = text.len().saturating_sub(max_chars);
                Ok(text.chars().skip(skip_chars).collect())
            }
        }
    }

    /// Count tokens for multiple texts
    pub fn batch_count(&self, texts: &[String], model: &str) -> Result<(Vec<u32>, u32), String> {
        let mut counts = Vec::with_capacity(texts.len());
        let mut total = 0u32;

        for text in texts {
            let count = self.count(text, model)?;
            total += count;
            counts.push(count);
        }

        Ok((counts, total))
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_approximation() {
        let counter = TokenCounter::new();
        let text = "Hello, world! This is a test.";
        let count = counter.count(text, "unknown-model").unwrap();
        // Approximation: ~4 chars per token
        assert!(count > 0);
    }

    #[test]
    fn test_truncate_no_truncation() {
        let counter = TokenCounter::new();
        let text = "Hello";
        let (result, original, truncated, was_truncated) =
            counter.truncate(text, 100, "gpt-4", true).unwrap();
        assert_eq!(result, text);
        assert!(!was_truncated);
    }
}