use thiserror::Error;
use tokio::fs;

use crate::sandbox::PathSecurity;

#[derive(Error, Debug)]
pub enum FileError {
    #[error("Security error: {0}")]
    Security(String),
    #[error("File not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct FileOperations;

impl FileOperations {
    pub fn new() -> Self {
        Self
    }

    pub async fn read_file(&self, path: &str, base_dir: &str) -> Result<String, FileError> {
        let security = PathSecurity::new(base_dir);
        let safe_path = security
            .validate_path(path)
            .map_err(|e| FileError::Security(e.to_string()))?;

        if !safe_path.exists() {
            return Err(FileError::NotFound(path.to_string()));
        }

        let content = fs::read_to_string(&safe_path).await?;
        Ok(content)
    }

    pub async fn write_file(
        &self,
        path: &str,
        content: &str,
        base_dir: &str,
    ) -> Result<u64, FileError> {
        let security = PathSecurity::new(base_dir);
        let safe_path = security
            .validate_path(path)
            .map_err(|e| FileError::Security(e.to_string()))?;

        // Create parent directories if needed
        if let Some(parent) = safe_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(&safe_path, content).await?;
        Ok(content.len() as u64)
    }

    pub async fn list_directory(&self, path: &str, base_dir: &str) -> Result<Vec<DirEntry>, FileError> {
        let security = PathSecurity::new(base_dir);
        let safe_path = security
            .validate_path(path)
            .map_err(|e| FileError::Security(e.to_string()))?;

        if !safe_path.exists() {
            return Err(FileError::NotFound(path.to_string()));
        }

        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(&safe_path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
            });
        }

        Ok(entries)
    }
}

impl Default for FileOperations {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}