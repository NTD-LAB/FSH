use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use super::{ClientInfo, FolderInfo, ShellType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FshMessage {
    // 握手阶段
    Connect(ConnectMessage),
    ConnectResponse(ConnectResponseMessage),

    // 认证阶段
    Authenticate(AuthenticateMessage),
    AuthResponse(AuthResponseMessage),

    // 文件夹绑定
    FolderBind(FolderBindMessage),
    FolderBound(FolderBoundMessage),

    // 会话管理
    SessionStart(SessionStartMessage),
    SessionReady(SessionReadyMessage),

    // 命令执行
    Command(CommandMessage),
    CommandOutput(CommandOutputMessage),
    CommandComplete(CommandCompleteMessage),

    // 文件操作
    FileList(FileListMessage),
    FileListResponse(FileListResponseMessage),
    FileRead(FileReadMessage),
    FileReadResponse(FileReadResponseMessage),
    FileWrite(FileWriteMessage),
    FileWriteResponse(FileWriteResponseMessage),

    // 控制消息
    Ping,
    Pong,
    Disconnect(DisconnectMessage),
    Error(ErrorMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectMessage {
    pub version: String,
    pub client_info: ClientInfo,
    pub supported_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectResponseMessage {
    pub success: bool,
    pub server_version: String,
    pub supported_features: Vec<String>,
    pub available_folders: Vec<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticateMessage {
    pub auth_type: String,
    pub credentials: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponseMessage {
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderBindMessage {
    pub target_folder: String,
    pub preferred_shell: Option<ShellType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderBoundMessage {
    pub success: bool,
    pub folder_info: Option<FolderInfo>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartMessage {
    pub session_id: String,
    pub environment_vars: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionReadyMessage {
    pub session_id: String,
    pub shell_prompt: String,
    pub working_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMessage {
    pub session_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub environment: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutputMessage {
    pub session_id: String,
    pub output_type: OutputType,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputType {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandCompleteMessage {
    pub session_id: String,
    pub exit_code: i32,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileListMessage {
    pub session_id: String,
    pub path: String,
    pub show_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileListResponseMessage {
    pub success: bool,
    pub files: Vec<FileEntry>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub size: u64,
    pub modified: chrono::DateTime<chrono::Utc>,
    pub permissions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadMessage {
    pub session_id: String,
    pub file_path: String,
    pub offset: Option<u64>,
    pub length: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadResponseMessage {
    pub success: bool,
    pub data: Vec<u8>,
    pub total_size: u64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWriteMessage {
    pub session_id: String,
    pub file_path: String,
    pub data: Vec<u8>,
    pub append: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWriteResponseMessage {
    pub success: bool,
    pub bytes_written: u64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectMessage {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    pub error_type: String,
    pub message: String,
    pub details: Option<HashMap<String, String>>,
}

impl FshMessage {
    pub fn message_type(&self) -> &'static str {
        match self {
            FshMessage::Connect(_) => "connect",
            FshMessage::ConnectResponse(_) => "connect_response",
            FshMessage::Authenticate(_) => "authenticate",
            FshMessage::AuthResponse(_) => "auth_response",
            FshMessage::FolderBind(_) => "folder_bind",
            FshMessage::FolderBound(_) => "folder_bound",
            FshMessage::SessionStart(_) => "session_start",
            FshMessage::SessionReady(_) => "session_ready",
            FshMessage::Command(_) => "command",
            FshMessage::CommandOutput(_) => "command_output",
            FshMessage::CommandComplete(_) => "command_complete",
            FshMessage::FileList(_) => "file_list",
            FshMessage::FileListResponse(_) => "file_list_response",
            FshMessage::FileRead(_) => "file_read",
            FshMessage::FileReadResponse(_) => "file_read_response",
            FshMessage::FileWrite(_) => "file_write",
            FshMessage::FileWriteResponse(_) => "file_write_response",
            FshMessage::Ping => "ping",
            FshMessage::Pong => "pong",
            FshMessage::Disconnect(_) => "disconnect",
            FshMessage::Error(_) => "error",
        }
    }
}