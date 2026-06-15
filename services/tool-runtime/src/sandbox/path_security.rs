use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PathSecurityError {
    #[error("Path traversal detected: {0}")]
    PathTraversal(String),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

pub struct PathSecurity {
    base_dir: PathBuf,
}

impl PathSecurity {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    pub fn validate_path(&self, path: impl AsRef<Path>) -> Result<PathBuf, PathSecurityError> {
        let path = path.as_ref();
        
        // Resolve to absolute path
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_dir.join(path)
        };

        // Canonicalize to resolve symlinks and ..
        let canonical = absolute.canonicalize().map_err(|e| {
            PathSecurityError::InvalidPath(format!("{}: {}", path.display(), e))
        })?;

        // Check if path is within base_dir
        if !canonical.starts_with(&self.base_dir) {
            return Err(PathSecurityError::PathTraversal(path.display().to_string()));
        }

        Ok(canonical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_valid_path() {
        let temp = std::env::temp_dir();
        let base = temp.join("test_base");
        fs::create_dir_all(&base).unwrap();
        
        let security = PathSecurity::new(&base);
        let test_file = base.join("test.txt");
        fs::write(&test_file, "test").unwrap();
        
        assert!(security.validate_path(&test_file).is_ok());
        
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn test_path_traversal() {
        let temp = std::env::temp_dir();
        let base = temp.join("test_base2");
        fs::create_dir_all(&base).unwrap();
        
        let security = PathSecurity::new(&base);
        let malicious = base.join("../etc/passwd");
        
        assert!(matches!(
            security.validate_path(&malicious),
            Err(PathSecurityError::PathTraversal(_))
        ));
        
        fs::remove_dir_all(&base).unwrap();
    }
}