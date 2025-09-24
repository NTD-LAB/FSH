use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::protocol::{FshError, FshResult, ShellType, Permission};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderConfig {
    pub name: String,
    pub path: String,
    pub permissions: Vec<Permission>,
    pub shell_type: ShellType,
    pub allowed_commands: Vec<String>,
    pub blocked_commands: Vec<String>,
    pub system_aware_commands: Option<Vec<String>>,
    pub description: Option<String>,
    pub readonly: bool,
    pub environment_vars: HashMap<String, String>,
}

impl FolderConfig {
    pub fn new<P: AsRef<Path>>(name: String, path: P) -> Self {
        Self {
            name,
            path: path.as_ref().to_string_lossy().to_string(),
            permissions: vec![Permission::Read, Permission::Write, Permission::Execute],
            shell_type: ShellType::default(),
            allowed_commands: Self::default_allowed_commands(),
            blocked_commands: Self::default_blocked_commands(),
            system_aware_commands: Some(Self::default_system_aware_commands()),
            description: None,
            readonly: false,
            environment_vars: HashMap::new(),
        }
    }

    pub fn with_permissions(mut self, permissions: Vec<Permission>) -> Self {
        self.permissions = permissions;
        self
    }

    pub fn with_shell_type(mut self, shell_type: ShellType) -> Self {
        self.shell_type = shell_type;
        self
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    pub fn with_readonly(mut self, readonly: bool) -> Self {
        self.readonly = readonly;
        if readonly {
            // Remove write permission if readonly
            self.permissions.retain(|p| !matches!(p, Permission::Write));
        }
        self
    }

    pub fn with_allowed_commands(mut self, commands: Vec<String>) -> Self {
        self.allowed_commands = commands;
        self
    }

    pub fn with_blocked_commands(mut self, commands: Vec<String>) -> Self {
        self.blocked_commands = commands;
        self
    }

    pub fn add_environment_var(mut self, key: String, value: String) -> Self {
        self.environment_vars.insert(key, value);
        self
    }

    pub fn get_path(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions.contains(permission)
    }

    pub fn can_read(&self) -> bool {
        self.has_permission(&Permission::Read)
    }

    pub fn can_write(&self) -> bool {
        !self.readonly && self.has_permission(&Permission::Write)
    }

    pub fn can_execute(&self) -> bool {
        self.has_permission(&Permission::Execute)
    }

    pub fn is_command_allowed(&self, command: &str) -> bool {
        // First check if it's explicitly blocked
        if self.blocked_commands.iter().any(|blocked| command.contains(blocked)) {
            return false;
        }

        // Check if it's a system-aware command
        if let Some(ref system_cmds) = self.system_aware_commands {
            if system_cmds.iter().any(|sys_cmd| command.contains(sys_cmd)) {
                return true;
            }
        }

        // Check for wildcard permission
        if self.allowed_commands.contains(&"*".to_string()) {
            return true;
        }

        // If no allowed commands specified, allow all (except blocked)
        if self.allowed_commands.is_empty() {
            return true;
        }

        // Check if command is in allowed list
        self.allowed_commands.iter().any(|allowed| {
            command.starts_with(allowed) ||
            command.contains(&format!("/{}", allowed)) ||
            command.contains(&format!("\\{}", allowed))
        })
    }

    pub fn is_system_aware_command(&self, command: &str) -> bool {
        if let Some(ref system_cmds) = self.system_aware_commands {
            system_cmds.iter().any(|sys_cmd| command.contains(sys_cmd))
        } else {
            false
        }
    }

    pub fn validate(&self) -> FshResult<()> {
        // Check if name is valid
        if self.name.is_empty() {
            return Err(FshError::ConfigError("Folder name cannot be empty".to_string()));
        }

        if self.name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
            return Err(FshError::ConfigError("Folder name contains invalid characters".to_string()));
        }

        // Check if path exists and is a directory
        let path = PathBuf::from(&self.path);
        if !path.exists() {
            return Err(FshError::FolderNotFound(self.path.clone()));
        }

        if !path.is_dir() {
            return Err(FshError::ConfigError(
                format!("Path '{}' is not a directory", self.path)
            ));
        }

        // Check if path is accessible
        if let Err(e) = std::fs::read_dir(&path) {
            return Err(FshError::PermissionDenied(
                format!("Cannot access directory '{}': {}", self.path, e)
            ));
        }

        // Validate permissions
        if self.permissions.is_empty() {
            return Err(FshError::ConfigError("At least one permission must be specified".to_string()));
        }

        // If readonly, ensure write permission is not included
        if self.readonly && self.permissions.contains(&Permission::Write) {
            return Err(FshError::ConfigError("Cannot have write permission on readonly folder".to_string()));
        }

        Ok(())
    }

    pub fn to_folder_info(&self) -> crate::protocol::FolderInfo {
        crate::protocol::FolderInfo {
            name: self.name.clone(),
            path: self.path.clone(),
            permissions: self.permissions.clone(),
            shell_type: self.shell_type.clone(),
            current_dir: self.path.clone(),
            description: self.description.clone(),
        }
    }

    pub fn get_project_type(&self) -> Option<ProjectType> {
        let path = PathBuf::from(&self.path);

        // Check for various project types based on files present
        if path.join("package.json").exists() {
            return Some(ProjectType::NodeJs);
        }

        if path.join("Cargo.toml").exists() {
            return Some(ProjectType::Rust);
        }

        if path.join("requirements.txt").exists() ||
           path.join("setup.py").exists() ||
           path.join("pyproject.toml").exists() {
            return Some(ProjectType::Python);
        }

        if path.join("pom.xml").exists() ||
           path.join("build.gradle").exists() ||
           path.join("build.gradle.kts").exists() {
            return Some(ProjectType::Java);
        }

        if path.join("go.mod").exists() {
            return Some(ProjectType::Go);
        }

        if path.join(".git").exists() {
            return Some(ProjectType::Git);
        }

        None
    }

    fn default_allowed_commands() -> Vec<String> {
        vec![
            // File operations
            "ls".to_string(), "dir".to_string(), "cat".to_string(), "type".to_string(),
            "echo".to_string(), "pwd".to_string(), "cd".to_string(),
            "mkdir".to_string(), "rmdir".to_string(), "cp".to_string(), "copy".to_string(),
            "mv".to_string(), "move".to_string(), "rm".to_string(), "del".to_string(),
            "find".to_string(), "grep".to_string(), "head".to_string(), "tail".to_string(),
            "wc".to_string(), "sort".to_string(), "uniq".to_string(),

            // Development tools
            "git".to_string(), "npm".to_string(), "yarn".to_string(), "node".to_string(),
            "python".to_string(), "python3".to_string(), "pip".to_string(), "pip3".to_string(),
            "cargo".to_string(), "rustc".to_string(), "go".to_string(),
            "java".to_string(), "javac".to_string(), "mvn".to_string(), "gradle".to_string(),
            "make".to_string(), "cmake".to_string(),

            // Editors
            "code".to_string(), "vim".to_string(), "nano".to_string(), "emacs".to_string(),

            // Utilities
            "curl".to_string(), "wget".to_string(), "tar".to_string(), "zip".to_string(),
            "unzip".to_string(), "which".to_string(), "whereis".to_string(),
        ]
    }

    fn default_blocked_commands() -> Vec<String> {
        vec![
            // System commands
            "format".to_string(), "fdisk".to_string(), "dd".to_string(), "mkfs".to_string(),
            "shutdown".to_string(), "reboot".to_string(), "halt".to_string(), "poweroff".to_string(),

            // Security commands
            "passwd".to_string(), "su".to_string(), "sudo".to_string(), "runas".to_string(),
            "chown".to_string(), "chmod".to_string(), "chgrp".to_string(),

            // Network commands (potentially dangerous)
            "netstat".to_string(), "ss".to_string(), "nmap".to_string(),

            // Process management (that could affect system)
            "kill".to_string(), "killall".to_string(), "taskkill".to_string(),

            // Package managers (system-wide)
            "apt".to_string(), "yum".to_string(), "dnf".to_string(), "pacman".to_string(),
            "brew".to_string(), "choco".to_string(),
        ]
    }

    fn default_system_aware_commands() -> Vec<String> {
        vec![
            // CLI tools that need system access
            "claude".to_string(),
            "code".to_string(),
            "cursor".to_string(),
            "npm".to_string(),
            "yarn".to_string(),
            "pnpm".to_string(),
            "node".to_string(),
            "python".to_string(),
            "pip".to_string(),
            "cargo".to_string(),
            "rustc".to_string(),
            "go".to_string(),
            "docker".to_string(),
            "git".to_string(),
            "gh".to_string(),
            "aws".to_string(),
            "az".to_string(),
            "gcloud".to_string(),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    NodeJs,
    Rust,
    Python,
    Java,
    Go,
    Git,
    Generic,
}

impl ProjectType {
    pub fn get_recommended_commands(&self) -> Vec<String> {
        match self {
            ProjectType::NodeJs => vec![
                "npm install".to_string(),
                "npm start".to_string(),
                "npm test".to_string(),
                "npm run build".to_string(),
                "yarn install".to_string(),
                "yarn start".to_string(),
            ],
            ProjectType::Rust => vec![
                "cargo build".to_string(),
                "cargo run".to_string(),
                "cargo test".to_string(),
                "cargo check".to_string(),
                "cargo clippy".to_string(),
            ],
            ProjectType::Python => vec![
                "python -m pip install -r requirements.txt".to_string(),
                "python main.py".to_string(),
                "pytest".to_string(),
                "python -m venv venv".to_string(),
            ],
            ProjectType::Java => vec![
                "mvn compile".to_string(),
                "mvn test".to_string(),
                "mvn package".to_string(),
                "gradle build".to_string(),
                "gradle test".to_string(),
            ],
            ProjectType::Go => vec![
                "go build".to_string(),
                "go run main.go".to_string(),
                "go test".to_string(),
                "go mod tidy".to_string(),
            ],
            ProjectType::Git => vec![
                "git status".to_string(),
                "git add .".to_string(),
                "git commit -m".to_string(),
                "git push".to_string(),
                "git pull".to_string(),
            ],
            ProjectType::Generic => vec![],
        }
    }

    pub fn get_typical_shell(&self) -> ShellType {
        match self {
            ProjectType::NodeJs | ProjectType::Python => {
                if cfg!(windows) {
                    ShellType::PowerShell
                } else {
                    ShellType::Bash
                }
            },
            ProjectType::Rust | ProjectType::Go => ShellType::Bash,
            ProjectType::Java => {
                if cfg!(windows) {
                    ShellType::Cmd
                } else {
                    ShellType::Bash
                }
            },
            ProjectType::Git => ShellType::GitBash,
            ProjectType::Generic => ShellType::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_folder_config_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = FolderConfig::new("test".to_string(), temp_dir.path());

        assert_eq!(config.name, "test");
        assert_eq!(config.path, temp_dir.path().to_string_lossy());
        assert!(config.can_read());
        assert!(config.can_write());
        assert!(config.can_execute());
    }

    #[test]
    fn test_readonly_folder() {
        let temp_dir = TempDir::new().unwrap();
        let config = FolderConfig::new("test".to_string(), temp_dir.path())
            .with_readonly(true);

        assert!(config.can_read());
        assert!(!config.can_write());
        assert!(config.readonly);
    }

    #[test]
    fn test_command_filtering() {
        let config = FolderConfig::new("test".to_string(), "/tmp")
            .with_allowed_commands(vec!["ls".to_string(), "cat".to_string()])
            .with_blocked_commands(vec!["rm".to_string()]);

        assert!(config.is_command_allowed("ls -la"));
        assert!(config.is_command_allowed("cat file.txt"));
        assert!(!config.is_command_allowed("rm file.txt"));
        assert!(!config.is_command_allowed("chmod 777 file"));
    }

    #[test]
    fn test_project_type_detection() {
        let temp_dir = TempDir::new().unwrap();

        // Create package.json for Node.js project
        std::fs::write(temp_dir.path().join("package.json"), "{}").unwrap();
        let config = FolderConfig::new("test".to_string(), temp_dir.path());
        assert_eq!(config.get_project_type(), Some(ProjectType::NodeJs));
    }

    #[test]
    fn test_folder_validation() {
        let temp_dir = TempDir::new().unwrap();
        let config = FolderConfig::new("test".to_string(), temp_dir.path());
        assert!(config.validate().is_ok());

        // Test invalid path
        let invalid_config = FolderConfig::new("test".to_string(), "/nonexistent/path");
        assert!(invalid_config.validate().is_err());

        // Test invalid name
        let invalid_name_config = FolderConfig::new("test*".to_string(), temp_dir.path());
        assert!(invalid_name_config.validate().is_err());
    }
}