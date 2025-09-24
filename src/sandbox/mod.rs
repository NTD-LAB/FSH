pub mod shell;
pub mod validator;

pub use shell::*;
pub use validator::*;

use std::path::PathBuf;
use crate::protocol::{ShellType, Permission};

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub root_path: PathBuf,
    pub shell_type: ShellType,
    pub permissions: Vec<Permission>,
    pub allowed_commands: Vec<String>,
    pub blocked_commands: Vec<String>,
    pub environment_vars: std::collections::HashMap<String, String>,
}

impl SandboxConfig {
    pub fn new(root_path: PathBuf, shell_type: ShellType) -> Self {
        let mut environment_vars = std::collections::HashMap::new();
        environment_vars.insert("FSH_ROOT".to_string(), root_path.to_string_lossy().to_string());
        environment_vars.insert("FSH_MODE".to_string(), "restricted".to_string());

        Self {
            root_path,
            shell_type,
            permissions: vec![Permission::Read, Permission::Write, Permission::Execute],
            allowed_commands: vec![
                "ls".to_string(), "dir".to_string(), "cd".to_string(),
                "cat".to_string(), "type".to_string(), "echo".to_string(),
                "pwd".to_string(), "mkdir".to_string(), "rmdir".to_string(),
                "cp".to_string(), "copy".to_string(), "mv".to_string(),
                "move".to_string(), "rm".to_string(), "del".to_string(),
                "git".to_string(), "npm".to_string(), "node".to_string(),
                "python".to_string(), "pip".to_string(), "cargo".to_string(),
                "rustc".to_string(), "code".to_string(),
            ],
            blocked_commands: vec![
                "format".to_string(), "fdisk".to_string(), "dd".to_string(),
                "mkfs".to_string(), "shutdown".to_string(), "reboot".to_string(),
                "halt".to_string(), "poweroff".to_string(), "passwd".to_string(),
                "su".to_string(), "sudo".to_string(), "runas".to_string(),
            ],
            environment_vars,
        }
    }

    pub fn with_permissions(mut self, permissions: Vec<Permission>) -> Self {
        self.permissions = permissions;
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

    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions.contains(permission)
    }

    pub fn is_command_allowed(&self, command: &str) -> bool {
        if self.blocked_commands.iter().any(|blocked| command.contains(blocked)) {
            return false;
        }

        if self.allowed_commands.is_empty() {
            return true;
        }

        self.allowed_commands.iter().any(|allowed| {
            command.starts_with(allowed) || command.contains(&format!("/{}", allowed))
        })
    }

    pub fn is_system_aware_command(&self, command: &str) -> bool {
        // Commands that need access to system environment and paths
        let system_aware_commands = [
            "git", "npm", "node", "python", "pip", "cargo", "rustc", "code",
            "java", "javac", "mvn", "gradle", "dotnet", "go", "gcc", "g++",
            "make", "cmake", "docker", "kubectl", "terraform", "claude",
            "cursor", "yarn", "pnpm", "deno", "bun", "gh", "aws", "az",
            "gcloud", "heroku", "vercel",
        ];

        system_aware_commands.iter().any(|&cmd| command == cmd || command.starts_with(&format!("{} ", cmd)))
    }
}