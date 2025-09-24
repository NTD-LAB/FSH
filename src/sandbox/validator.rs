use std::path::{Path, PathBuf};
use crate::protocol::{FshError, FshResult};

#[derive(Debug, Clone)]
pub struct PathValidator {
    root_path: PathBuf,
}

impl PathValidator {
    pub fn new(root_path: PathBuf) -> FshResult<Self> {
        let canonical_root = root_path.canonicalize()
            .map_err(|e| FshError::InvalidPath(format!("Cannot canonicalize root path: {}", e)))?;

        Ok(Self {
            root_path: canonical_root,
        })
    }

    pub fn validate_path(&self, path: &str) -> FshResult<PathBuf> {
        let requested_path = Path::new(path);

        // Handle relative paths
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            self.root_path.join(requested_path)
        };

        // Canonicalize to resolve .. and . components
        let canonical_path = absolute_path.canonicalize()
            .map_err(|e| FshError::InvalidPath(format!("Cannot resolve path '{}': {}", path, e)))?;

        // Check if the canonical path is within the allowed root
        if !canonical_path.starts_with(&self.root_path) {
            return Err(FshError::PermissionDenied(
                format!("Path '{}' is outside the allowed directory", path)
            ));
        }

        Ok(canonical_path)
    }

    pub fn validate_command_path(&self, command: &str) -> FshResult<String> {
        // Check for dangerous path traversal patterns
        let dangerous_patterns = ["../", "..\\", "/../../", "\\..\\..\\"];
        for pattern in &dangerous_patterns {
            if command.contains(pattern) {
                return Err(FshError::PermissionDenied(
                    "Command contains dangerous path traversal".to_string()
                ));
            }
        }

        // Check for absolute paths that might bypass the sandbox
        if command.contains(':') && (command.contains('\\') || command.contains('/')) {
            // Windows absolute path like C:\ or network path
            if cfg!(windows) && self.is_absolute_windows_path(command) {
                return Err(FshError::PermissionDenied(
                    "Absolute paths are not allowed".to_string()
                ));
            }
        }

        if command.starts_with('/') && cfg!(unix) {
            return Err(FshError::PermissionDenied(
                "Absolute paths are not allowed".to_string()
            ));
        }

        Ok(command.to_string())
    }

    pub fn get_relative_path(&self, absolute_path: &Path) -> FshResult<PathBuf> {
        absolute_path.strip_prefix(&self.root_path)
            .map(|p| p.to_path_buf())
            .map_err(|_| FshError::InvalidPath(
                format!("Path '{}' is not within the sandbox", absolute_path.display())
            ))
    }

    pub fn get_absolute_path(&self, relative_path: &str) -> FshResult<PathBuf> {
        let relative = Path::new(relative_path);

        // Ensure it's actually relative
        if relative.is_absolute() {
            return Err(FshError::InvalidPath(
                "Expected relative path".to_string()
            ));
        }

        let absolute = self.root_path.join(relative);

        // Validate the resulting path is still within bounds
        self.validate_path(&absolute.to_string_lossy())
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    fn is_absolute_windows_path(&self, path: &str) -> bool {
        if !cfg!(windows) {
            return false;
        }

        // Check for Windows drive letters (C:, D:, etc.)
        if path.len() >= 2 && path.chars().nth(1) == Some(':') {
            let first_char = path.chars().next().unwrap();
            return first_char.is_ascii_alphabetic();
        }

        // Check for UNC paths (\\server\share)
        path.starts_with("\\\\")
    }

    pub fn sanitize_output_path(&self, output: &str) -> String {
        let root_str = self.root_path.to_string_lossy();

        // Replace absolute paths with relative ones in output
        output.replace(&root_str.to_string(), ".")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_path_validation() {
        let temp_dir = TempDir::new().unwrap();
        let validator = PathValidator::new(temp_dir.path().to_path_buf()).unwrap();

        // Valid relative path
        let valid_path = validator.validate_path("test.txt");
        assert!(valid_path.is_ok());

        // Invalid path traversal
        let invalid_path = validator.validate_path("../../../etc/passwd");
        assert!(invalid_path.is_err());
    }

    #[test]
    fn test_command_validation() {
        let temp_dir = TempDir::new().unwrap();
        let validator = PathValidator::new(temp_dir.path().to_path_buf()).unwrap();

        // Valid command
        assert!(validator.validate_command_path("ls -la").is_ok());

        // Invalid command with path traversal
        assert!(validator.validate_command_path("cat ../../../etc/passwd").is_err());

        // Invalid absolute path
        if cfg!(windows) {
            assert!(validator.validate_command_path("C:\\Windows\\System32\\cmd.exe").is_err());
        } else {
            assert!(validator.validate_command_path("/bin/bash").is_err());
        }
    }

    #[test]
    fn test_relative_path_conversion() {
        let temp_dir = TempDir::new().unwrap();
        let validator = PathValidator::new(temp_dir.path().to_path_buf()).unwrap();

        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "test").unwrap();

        let relative = validator.get_relative_path(&test_file).unwrap();
        assert_eq!(relative, Path::new("test.txt"));

        let absolute = validator.get_absolute_path("test.txt").unwrap();
        assert_eq!(absolute, test_file);
    }
}