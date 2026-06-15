pub struct TextChunker {
    chunk_size: usize,
    overlap: usize,
}

#[derive(Debug)]
pub struct TextChunk {
    pub index: u32,
    pub content: String,
    pub token_count: u32,
}

impl TextChunker {
    pub fn new(chunk_size: usize, overlap: usize) -> Self {
        Self { chunk_size, overlap }
    }

    pub fn chunk_text(&self, text: &str) -> Vec<TextChunk> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut chunks = Vec::new();
        let mut start = 0;
        let mut index = 0;

        while start < text.len() {
            let end = (start + self.chunk_size).min(text.len());
            let chunk_text = text[start..end].trim().to_string();

            if !chunk_text.is_empty() {
                let token_count = self.estimate_tokens(&chunk_text);
                chunks.push(TextChunk {
                    index,
                    content: chunk_text,
                    token_count,
                });
                index += 1;
            }

            if end >= text.len() {
                break;
            }

            start = end.saturating_sub(self.overlap);
        }

        chunks
    }

    fn estimate_tokens(&self, text: &str) -> u32 {
        // Rough estimate: 1 token ≈ 4 characters
        (text.len() / 4) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text() {
        let chunker = TextChunker::new(100, 20);
        let text = "a".repeat(250);
        
        let chunks = chunker.chunk_text(&text);
        
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| !c.content.is_empty()));
    }

    #[test]
    fn test_empty_text() {
        let chunker = TextChunker::new(100, 20);
        let chunks = chunker.chunk_text("");
        
        assert_eq!(chunks.len(), 0);
    }
}