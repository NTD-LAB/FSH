pub mod folder;

pub use folder::*;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::protocol::{FshError, FshResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub security: SecurityConfig,
    pub folders: Vec<FolderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub max_connections: usize,
    pub connection_timeout_seconds: u64,
    pub session_timeout_minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub require_authentication: bool,
    pub auth_methods: Vec<String>,
    pub max_failed_attempts: u32,
    pub enable_logging: bool,
    pub log_file: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 2222,
                max_connections: 10,
                connection_timeout_seconds: 30,
                session_timeout_minutes: 60,
            },
            security: SecurityConfig {
                require_authentication: true,
                auth_methods: vec!["token".to_string()],
                max_failed_attempts: 3,
                enable_logging: true,
                log_file: None,
            },
            folders: vec![],
        }
    }
}

impl Config {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> FshResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| FshError::ConfigError(format!("Failed to read config file: {}", e)))?;

        toml::from_str(&content)
            .map_err(|e| FshError::ConfigError(format!("Failed to parse config file: {}", e)))
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> FshResult<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| FshError::ConfigError(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(path, content)
            .map_err(|e| FshError::ConfigError(format!("Failed to write config file: {}", e)))?;

        Ok(())
    }

    pub fn load_or_create_default<P: AsRef<Path>>(path: P) -> FshResult<Self> {
        let path = path.as_ref();

        if path.exists() {
            Self::load_from_file(path)
        } else {
            let config = Self::default();

            // Create parent directory if it doesn't exist
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| FshError::ConfigError(format!("Failed to create config directory: {}", e)))?;
            }

            config.save_to_file(path)?;
            Ok(config)
        }
    }

    pub fn find_folder_by_name(&self, name: &str) -> Option<&FolderConfig> {
        self.folders.iter().find(|f| f.name == name)
    }

    pub fn find_folder_by_path(&self, path: &str) -> Option<&FolderConfig> {
        self.folders.iter().find(|f| f.path == path)
    }

    pub fn add_folder(&mut self, folder: FolderConfig) -> FshResult<()> {
        // Check for duplicate names
        if self.folders.iter().any(|f| f.name == folder.name) {
            return Err(FshError::ConfigError(
                format!("Folder with name '{}' already exists", folder.name)
            ));
        }

        // Check for duplicate paths
        if self.folders.iter().any(|f| f.path == folder.path) {
            return Err(FshError::ConfigError(
                format!("Folder with path '{}' already exists", folder.path)
            ));
        }

        // Validate the folder exists and is accessible
        folder.validate()?;

        self.folders.push(folder);
        Ok(())
    }

    pub fn remove_folder(&mut self, name: &str) -> FshResult<()> {
        let index = self.folders.iter().position(|f| f.name == name)
            .ok_or_else(|| FshError::ConfigError(format!("Folder '{}' not found", name)))?;

        self.folders.remove(index);
        Ok(())
    }

    pub fn update_folder(&mut self, name: &str, updated_folder: FolderConfig) -> FshResult<()> {
        let index = self.folders.iter().position(|f| f.name == name)
            .ok_or_else(|| FshError::ConfigError(format!("Folder '{}' not found", name)))?;

        // Validate the updated folder
        updated_folder.validate()?;

        self.folders[index] = updated_folder;
        Ok(())
    }

    pub fn get_default_config_path() -> FshResult<PathBuf> {
        let config_dir = if cfg!(windows) {
            // Windows: %APPDATA%\FSH
            std::env::var("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("C:\\Users\\Default\\AppData\\Roaming"))
                .join("FSH")
        } else {
            // Unix-like: ~/.config/fsh
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("/etc"))
                .join("fsh")
        };

        Ok(config_dir.join("fsh_config.toml"))
    }

    pub fn validate(&self) -> FshResult<()> {
        // Validate server config
        if self.server.port == 0 || self.server.port > 65535 {
            return Err(FshError::ConfigError("Invalid port number".to_string()));
        }

        if self.server.max_connections == 0 {
            return Err(FshError::ConfigError("max_connections must be greater than 0".to_string()));
        }

        // Validate security config
        if self.security.require_authentication && self.security.auth_methods.is_empty() {
            return Err(FshError::ConfigError("At least one auth method must be specified when authentication is required".to_string()));
        }

        // Validate all folders
        for folder in &self.folders {
            folder.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 2222);
        assert!(config.security.require_authentication);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(config.server.host, deserialized.server.host);
        assert_eq!(config.server.port, deserialized.server.port);
    }

    #[test]
    fn test_config_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");

        let config = Config::default();
        config.save_to_file(&config_path).unwrap();

        let loaded_config = Config::load_from_file(&config_path).unwrap();
        assert_eq!(config.server.host, loaded_config.server.host);
    }

    #[test]
    fn test_folder_management() {
        let mut config = Config::default();
        let temp_dir = TempDir::new().unwrap();

        let folder = FolderConfig {
            name: "test".to_string(),
            path: temp_dir.path().to_string_lossy().to_string(),
            permissions: vec![Permission::Read, Permission::Write],
            shell_type: ShellType::Bash,
            allowed_commands: vec!["ls".to_string()],
            blocked_commands: vec!["rm".to_string()],
            description: Some("Test folder".to_string()),
            readonly: false,
            environment_vars: HashMap::new(),
        };

        config.add_folder(folder.clone()).unwrap();
        assert_eq!(config.folders.len(), 1);

        let found = config.find_folder_by_name("test");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "test");

        config.remove_folder("test").unwrap();
        assert_eq!(config.folders.len(), 0);
    }
}