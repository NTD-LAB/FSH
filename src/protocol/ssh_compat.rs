// SSH协议兼容性模块
// 实现SSH协议的主要特性，但限制在文件夹级别

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// SSH兼容的协议版本
pub const SSH_COMPAT_VERSION: &str = "2.0";

/// SSH兼容的连接消息格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshCompatConnect {
    pub protocol_version: String,
    pub software_version: String,
    pub supported_algorithms: SshAlgorithms,
    pub folder_path: String, // FSH特有：指定文件夹路径
}

/// SSH加密算法支持
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshAlgorithms {
    pub kex: Vec<String>,           // 密钥交换算法
    pub server_host_key: Vec<String>, // 服务器主机密钥算法
    pub encryption_client_to_server: Vec<String>, // 客户端到服务器加密
    pub encryption_server_to_client: Vec<String>, // 服务器到客户端加密
    pub mac_client_to_server: Vec<String>,        // 客户端到服务器MAC
    pub mac_server_to_client: Vec<String>,        // 服务器到客户端MAC
    pub compression_client_to_server: Vec<String>, // 客户端到服务器压缩
    pub compression_server_to_client: Vec<String>, // 服务器到客户端压缩
}

impl Default for SshAlgorithms {
    fn default() -> Self {
        Self {
            kex: vec![
                "curve25519-sha256".to_string(),
                "diffie-hellman-group14-sha256".to_string(),
            ],
            server_host_key: vec![
                "rsa-sha2-512".to_string(),
                "ssh-ed25519".to_string(),
            ],
            encryption_client_to_server: vec![
                "chacha20-poly1305@openssh.com".to_string(),
                "aes256-gcm@openssh.com".to_string(),
                "aes256-ctr".to_string(),
            ],
            encryption_server_to_client: vec![
                "chacha20-poly1305@openssh.com".to_string(),
                "aes256-gcm@openssh.com".to_string(),
                "aes256-ctr".to_string(),
            ],
            mac_client_to_server: vec![
                "umac-128-etm@openssh.com".to_string(),
                "hmac-sha2-256-etm@openssh.com".to_string(),
            ],
            mac_server_to_client: vec![
                "umac-128-etm@openssh.com".to_string(),
                "hmac-sha2-256-etm@openssh.com".to_string(),
            ],
            compression_client_to_server: vec![
                "none".to_string(),
                "zlib@openssh.com".to_string(),
            ],
            compression_server_to_client: vec![
                "none".to_string(),
                "zlib@openssh.com".to_string(),
            ],
        }
    }
}

/// SSH兼容的认证方法
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshAuthMethod {
    Password { username: String, password: String },
    PublicKey { username: String, public_key: Vec<u8>, signature: Vec<u8> },
    KeyboardInteractive { username: String, responses: Vec<String> },
    None { username: String }, // 无认证（测试用）
}

/// SSH兼容的通道类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshChannelType {
    Session,                    // 会话通道
    DirectTcpip {              // 直接TCP/IP转发
        host: String,
        port: u16,
        originator_host: String,
        originator_port: u16,
    },
    ForwardedTcpip {           // 转发TCP/IP
        host: String,
        port: u16,
        originator_host: String,
        originator_port: u16,
    },
    X11 {                      // X11转发
        originator_host: String,
        originator_port: u16,
    },
}

/// SSH兼容的请求类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshRequest {
    // PTY相关
    PtyReq {
        term: String,
        width_chars: u32,
        height_rows: u32,
        width_pixels: u32,
        height_pixels: u32,
        terminal_modes: HashMap<u8, u32>,
    },

    // Shell相关
    Shell,
    Exec { command: String },
    Subsystem { name: String },

    // 环境变量
    Env { name: String, value: String },

    // 窗口大小变化
    WindowChange {
        width_chars: u32,
        height_rows: u32,
        width_pixels: u32,
        height_pixels: u32,
    },

    // 信号
    Signal { signal: String },

    // 退出状态
    ExitStatus { status: u32 },
    ExitSignal {
        signal: String,
        core_dumped: bool,
        error_message: String,
        language_tag: String,
    },

    // 端口转发
    TcpipForward { address: String, port: u32 },
    CancelTcpipForward { address: String, port: u32 },
}

/// SSH兼容的通道数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshChannelData {
    pub channel_id: u32,
    pub data_type: SshDataType,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshDataType {
    Normal,        // 标准输出
    Extended(u32), // 扩展数据（如stderr）
}

/// SSH兼容的通道控制消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshChannelControl {
    Open {
        channel_type: SshChannelType,
        sender_channel: u32,
        initial_window_size: u32,
        maximum_packet_size: u32,
    },
    OpenConfirmation {
        recipient_channel: u32,
        sender_channel: u32,
        initial_window_size: u32,
        maximum_packet_size: u32,
    },
    OpenFailure {
        recipient_channel: u32,
        reason_code: u32,
        description: String,
        language_tag: String,
    },
    WindowAdjust {
        recipient_channel: u32,
        bytes_to_add: u32,
    },
    Data(SshChannelData),
    Eof {
        recipient_channel: u32,
    },
    Close {
        recipient_channel: u32,
    },
    Request {
        recipient_channel: u32,
        request: SshRequest,
        want_reply: bool,
    },
    Success {
        recipient_channel: u32,
    },
    Failure {
        recipient_channel: u32,
    },
}

/// SSH协议错误原因代码
pub const SSH_OPEN_ADMINISTRATIVELY_PROHIBITED: u32 = 1;
pub const SSH_OPEN_CONNECT_FAILED: u32 = 2;
pub const SSH_OPEN_UNKNOWN_CHANNEL_TYPE: u32 = 3;
pub const SSH_OPEN_RESOURCE_SHORTAGE: u32 = 4;

/// SFTP兼容的消息类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SftpMessage {
    Init { version: u32 },
    Version { version: u32, extensions: HashMap<String, String> },

    // 文件操作
    Open { id: u32, filename: String, pflags: u32, attrs: SftpFileAttrs },
    Close { id: u32, handle: Vec<u8> },
    Read { id: u32, handle: Vec<u8>, offset: u64, len: u32 },
    Write { id: u32, handle: Vec<u8>, offset: u64, data: Vec<u8> },

    // 目录操作
    Opendir { id: u32, path: String },
    Readdir { id: u32, handle: Vec<u8> },

    // 文件系统操作
    Remove { id: u32, filename: String },
    Rename { id: u32, oldpath: String, newpath: String },
    Mkdir { id: u32, path: String, attrs: SftpFileAttrs },
    Rmdir { id: u32, path: String },

    // 属性操作
    Stat { id: u32, path: String },
    Lstat { id: u32, path: String },
    Fstat { id: u32, handle: Vec<u8> },
    Setstat { id: u32, path: String, attrs: SftpFileAttrs },
    Fsetstat { id: u32, handle: Vec<u8>, attrs: SftpFileAttrs },

    // 链接操作
    Readlink { id: u32, path: String },
    Symlink { id: u32, linkpath: String, targetpath: String },

    // 路径操作
    Realpath { id: u32, path: String },

    // 响应消息
    Status { id: u32, status_code: u32, error_message: String, language_tag: String },
    Handle { id: u32, handle: Vec<u8> },
    Data { id: u32, data: Vec<u8> },
    Name { id: u32, count: u32, names: Vec<SftpName> },
    Attrs { id: u32, attrs: SftpFileAttrs },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpFileAttrs {
    pub flags: u32,
    pub size: Option<u64>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub permissions: Option<u32>,
    pub atime: Option<u32>,
    pub mtime: Option<u32>,
    pub extended: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpName {
    pub filename: String,
    pub longname: String,
    pub attrs: SftpFileAttrs,
}

/// SSH终端模式
pub const TTY_OP_ISPEED: u8 = 128;   // 输入波特率
pub const TTY_OP_OSPEED: u8 = 129;   // 输出波特率
pub const TTY_OP_VEOL: u8 = 1;       // 行结束字符
pub const TTY_OP_VEOL2: u8 = 2;      // 行结束字符2
pub const TTY_OP_VERASE: u8 = 3;     // 擦除字符
pub const TTY_OP_VINTR: u8 = 4;      // 中断字符
pub const TTY_OP_VKILL: u8 = 5;      // 删除行字符
pub const TTY_OP_VQUIT: u8 = 6;      // 退出字符
pub const TTY_OP_VSTART: u8 = 7;     // 开始字符
pub const TTY_OP_VSTOP: u8 = 8;      // 停止字符

/// 创建默认的终端模式
pub fn default_terminal_modes() -> HashMap<u8, u32> {
    let mut modes = HashMap::new();
    modes.insert(TTY_OP_ISPEED, 38400);    // 38400 baud
    modes.insert(TTY_OP_OSPEED, 38400);    // 38400 baud
    modes.insert(TTY_OP_VERASE, 127);      // DEL character
    modes.insert(TTY_OP_VINTR, 3);         // Ctrl-C
    modes.insert(TTY_OP_VKILL, 21);        // Ctrl-U
    modes.insert(TTY_OP_VQUIT, 28);        // Ctrl-\
    modes.insert(TTY_OP_VSTART, 17);       // Ctrl-Q
    modes.insert(TTY_OP_VSTOP, 19);        // Ctrl-S
    modes
}

/// SSH兼容的全局请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshGlobalRequest {
    TcpipForward { bind_address: String, bind_port: u32 },
    CancelTcpipForward { bind_address: String, bind_port: u32 },
    StreamLocalForward { socket_path: String },
    CancelStreamLocalForward { socket_path: String },
}

/// FSH特有：文件夹绑定信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FshFolderBinding {
    pub folder_id: String,
    pub folder_path: String,
    pub permissions: Vec<String>,
    pub allowed_commands: Vec<String>,
    pub blocked_commands: Vec<String>,
    pub shell_type: String,
    pub environment: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_algorithms() {
        let algs = SshAlgorithms::default();
        assert!(!algs.kex.is_empty());
        assert!(!algs.encryption_client_to_server.is_empty());
    }

    #[test]
    fn test_terminal_modes() {
        let modes = default_terminal_modes();
        assert!(modes.contains_key(&TTY_OP_ISPEED));
        assert!(modes.contains_key(&TTY_OP_OSPEED));
        assert_eq!(modes[&TTY_OP_VINTR], 3); // Ctrl-C
    }

    #[test]
    fn test_sftp_message_serialization() {
        let msg = SftpMessage::Init { version: 3 };
        let serialized = bincode::serialize(&msg).unwrap();
        let deserialized: SftpMessage = bincode::deserialize(&serialized).unwrap();

        match deserialized {
            SftpMessage::Init { version } => assert_eq!(version, 3),
            _ => panic!("Wrong message type"),
        }
    }
}