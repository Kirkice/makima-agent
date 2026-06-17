//! Checkpoint service - file snapshot and rollback
//!
//! Creates snapshots of directories with SHA256 hashing for verification,
//! and supports restoring to previous checkpoints.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::{debug, error, info};

/// A single file entry in a checkpoint
#[derive(Clone, Debug)]
pub struct CheckpointFile {
    pub relative_path: String,
    pub content: Vec<u8>,
    pub sha256: String,
    pub size: u64,
}

/// A checkpoint containing a snapshot of files
#[derive(Clone, Debug)]
pub struct Checkpoint {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub base_dir: PathBuf,
    pub files: Vec<CheckpointFile>,
}

impl Checkpoint {
    pub fn file_count(&self) -> u64 {
        self.files.len() as u64
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// Protected paths that should not be checkpointed
const PROTECTED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
    "target",
    "dist",
    "build",
    ".cache",
];

/// Checkpoint manager - stores checkpoints in memory
pub struct CheckpointManager {
    checkpoints: Mutex<HashMap<String, Checkpoint>>,
    next_id: Mutex<u64>,
}

impl CheckpointManager {
    pub fn new() -> Self {
        Self {
            checkpoints: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }

    /// Generate a new checkpoint ID
    fn generate_id(&self) -> String {
        let mut id = self.next_id.lock().unwrap();
        let current = *id;
        *id += 1;
        format!("ckpt_{:06}", current)
    }

    /// Check if a directory name should be skipped
    fn should_skip_dir(name: &str) -> bool {
        PROTECTED_DIRS.contains(&name)
    }

    /// Calculate SHA256 hash of bytes
    fn sha256_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Recursively collect files from a directory
    async fn collect_files(
        dir: &Path,
        base: &Path,
        files: &mut Vec<CheckpointFile>,
    ) -> Result<(), std::io::Error> {
        let mut entries = fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            if entry.file_type().await?.is_dir() {
                if Self::should_skip_dir(&file_name) {
                    debug!("Skipping protected dir: {}", path.display());
                    continue;
                }
                Box::pin(Self::collect_files(&path, base, files)).await?;
            } else {
                let content = fs::read(&path).await?;
                let sha256 = Self::sha256_hash(&content);
                let size = content.len() as u64;
                let relative_path = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();

                files.push(CheckpointFile {
                    relative_path,
                    content,
                    sha256,
                    size,
                });
            }
        }

        Ok(())
    }

    /// Save a checkpoint of the given directory
    pub async fn save(
        &self,
        directory: &Path,
        base_dir: &Path,
        label: &str,
    ) -> Result<Checkpoint, String> {
        // Resolve the directory path
        let dir = if directory.is_absolute() {
            directory.to_path_buf()
        } else {
            base_dir.join(directory)
        };

        if !dir.exists() {
            return Err(format!("Directory does not exist: {}", dir.display()));
        }

        info!("Saving checkpoint: dir={}, label={}", dir.display(), label);

        let mut files = Vec::new();
        Self::collect_files(&dir, &dir, &mut files)
            .await
            .map_err(|e| format!("Failed to collect files: {}", e))?;

        let id = self.generate_id();
        let created_at = chrono::Utc::now().to_rfc3339();

        let checkpoint = Checkpoint {
            id: id.clone(),
            label: label.to_string(),
            created_at,
            base_dir: dir.clone(),
            files,
        };

        info!(
            "Checkpoint saved: id={}, files={}, bytes={}",
            checkpoint.id,
            checkpoint.file_count(),
            checkpoint.total_bytes()
        );

        self.checkpoints
            .lock()
            .unwrap()
            .insert(id, checkpoint.clone());

        Ok(checkpoint)
    }

    /// Restore a checkpoint
    pub async fn restore(
        &self,
        checkpoint_id: &str,
        _base_dir: &Path,
    ) -> Result<u64, String> {
        let checkpoint = {
            let checkpoints = self.checkpoints.lock().unwrap();
            checkpoints
                .get(checkpoint_id)
                .cloned()
                .ok_or_else(|| format!("Checkpoint not found: {}", checkpoint_id))?
        };

        info!(
            "Restoring checkpoint: id={}, files={}",
            checkpoint.id,
            checkpoint.file_count()
        );

        let base_dir = &checkpoint.base_dir;
        let mut restored = 0u64;

        // First, collect current files to find deleted files
        let mut current_files = Vec::new();
        if base_dir.exists() {
            let _ = Self::collect_files(base_dir, base_dir, &mut current_files).await;
        }

        // Create a set of checkpoint relative paths
        let checkpoint_paths: std::collections::HashSet<String> = checkpoint
            .files
            .iter()
            .map(|f| f.relative_path.clone())
            .collect();

        // Delete files that exist on disk but not in checkpoint
        for current_file in &current_files {
            if !checkpoint_paths.contains(&current_file.relative_path) {
                let file_path = base_dir.join(&current_file.relative_path);
                if file_path.exists() {
                    debug!("Removing file not in checkpoint: {}", file_path.display());
                    let _ = fs::remove_file(&file_path).await;
                }
            }
        }

        // Restore each file from checkpoint
        for file in &checkpoint.files {
            let file_path = base_dir.join(&file.relative_path);

            // Create parent directories
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("Failed to create dir: {}", e))?;
            }

            // Write file content
            fs::write(&file_path, &file.content)
                .await
                .map_err(|e| format!("Failed to write file {}: {}", file_path.display(), e))?;

            restored += 1;
        }

        info!("Checkpoint restored: {} files", restored);
        Ok(restored)
    }

    /// List all checkpoints for a base directory
    pub fn list(&self, _base_dir: &Path) -> Vec<Checkpoint> {
        let checkpoints = self.checkpoints.lock().unwrap();
        checkpoints.values().cloned().collect()
    }

    /// Delete a checkpoint
    pub fn delete(&self, checkpoint_id: &str, _base_dir: &Path) -> Result<(), String> {
        let mut checkpoints = self.checkpoints.lock().unwrap();
        if checkpoints.remove(checkpoint_id).is_some() {
            info!("Checkpoint deleted: {}", checkpoint_id);
            Ok(())
        } else {
            Err(format!("Checkpoint not found: {}", checkpoint_id))
        }
    }
}

impl Default for CheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}