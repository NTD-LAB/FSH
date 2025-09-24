pub mod message;
pub mod codec;
pub mod ssh_compat;

pub use message::*;
pub use codec::*;
pub use ssh_compat::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellType {
    Cmd,
    PowerShell,
    Bash,
    GitBash,
}

impl Default for ShellType {
    fn default() -> Self {
        if cfg!(windows) {
            ShellType::PowerShell
        } else {
            ShellType::Bash
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    Read,
    Write,
    Execute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub platform: String,
    pub app_version: String,
    pub app_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderInfo {
    pub name: String,
    pub path: String,
    pub permissions: Vec<Permission>,
    pub shell_type: ShellType,
    pub current_dir: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub folder_info: FolderInfo,
    pub client_info: ClientInfo,
    pub established_at: chrono::DateTime<chrono::Utc>,
}

pub const FSH_VERSION: &str = "1.0";
pub const FSH_MAGIC: &[u8] = b"FSH\x01";

#[derive(Debug)]
pub enum FshError {
    ProtocolError(String),
    AuthenticationFailed,
    FolderNotFound(String),
    PermissionDenied(String),
    SessionNotFound(String),
    InvalidPath(String),
    ShellError(String),
    NetworkError(String),
    ConfigError(String),
}

impl std::fmt::Display for FshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FshError::ProtocolError(msg) => write!(f, "Protocol error: {}", msg),
            FshError::AuthenticationFailed => write!(f, "Authentication failed"),
            FshError::FolderNotFound(path) => write!(f, "Folder not found: {}", path),
            FshError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            FshError::SessionNotFound(id) => write!(f, "Session not found: {}", id),
            FshError::InvalidPath(path) => write!(f, "Invalid path: {}", path),
            FshError::ShellError(msg) => write!(f, "Shell error: {}", msg),
            FshError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            FshError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

impl std::error::Error for FshError {}

pub type FshResult<T> = Result<T, FshError>;