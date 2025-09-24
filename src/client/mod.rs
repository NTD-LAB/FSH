pub mod terminal;

pub use terminal::*;

use crate::protocol::{
    FshMessage, FshCodec, FshError, FshResult, FSH_VERSION, ClientInfo,
    message::*,
};
use std::collections::HashMap;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{info, error, debug, warn};

#[derive(Debug)]
pub struct FshClient {
    stream: Option<TcpStream>,
    server_addr: String,
    client_info: ClientInfo,
    session_id: Option<String>,
    connected: bool,
}

impl FshClient {
    pub fn new(server_addr: String) -> Self {
        let client_info = ClientInfo {
            platform: std::env::consts::OS.to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            app_name: env!("CARGO_PKG_NAME").to_string(),
        };

        Self {
            stream: None,
            server_addr,
            client_info,
            session_id: None,
            connected: false,
        }
    }

    pub async fn connect(&mut self) -> FshResult<()> {
        info!("Connecting to FSH server at {}", self.server_addr);

        // Establish TCP connection
        let stream = TcpStream::connect(&self.server_addr).await
            .map_err(|e| FshError::NetworkError(format!("Failed to connect to {}: {}", self.server_addr, e)))?;

        self.stream = Some(stream);

        // Send connect message
        let connect_msg = FshMessage::Connect(ConnectMessage {
            version: FSH_VERSION.to_string(),
            client_info: self.client_info.clone(),
            supported_features: vec![
                "folder_binding".to_string(),
                "file_operations".to_string(),
                "command_execution".to_string(),
            ],
        });

        self.send_message(connect_msg).await?;

        // Wait for connect response
        let response = self.receive_message().await?;

        match response {
            FshMessage::ConnectResponse(resp) => {
                if resp.success {
                    info!("Connected to FSH server (version {})", resp.server_version);
                    debug!("Server features: {:?}", resp.supported_features);
                    debug!("Available folders: {:?}", resp.available_folders);
                    self.connected = true;
                    Ok(())
                } else {
                    let error_msg = resp.message.unwrap_or_else(|| "Connection rejected".to_string());
                    error!("Connection rejected: {}", error_msg);
                    Err(FshError::NetworkError(error_msg))
                }
            }
            _ => {
                error!("Unexpected response to connect message");
                Err(FshError::ProtocolError("Unexpected response".to_string()))
            }
        }
    }

    pub async fn authenticate(&mut self, auth_type: &str, credentials: HashMap<String, String>) -> FshResult<()> {
        if !self.connected {
            return Err(FshError::NetworkError("Not connected to server".to_string()));
        }

        info!("Authenticating with method: {}", auth_type);

        let auth_msg = FshMessage::Authenticate(AuthenticateMessage {
            auth_type: auth_type.to_string(),
            credentials,
        });

        self.send_message(auth_msg).await?;

        // Wait for auth response
        let response = self.receive_message().await?;

        match response {
            FshMessage::AuthResponse(resp) => {
                if resp.success {
                    info!("Authentication successful");
                    Ok(())
                } else {
                    let error_msg = resp.message.unwrap_or_else(|| "Authentication failed".to_string());
                    error!("Authentication failed: {}", error_msg);
                    Err(FshError::AuthenticationFailed)
                }
            }
            _ => {
                error!("Unexpected response to authentication message");
                Err(FshError::ProtocolError("Unexpected response".to_string()))
            }
        }
    }

    pub async fn bind_folder(&mut self, folder_name: &str, preferred_shell: Option<crate::protocol::ShellType>) -> FshResult<crate::protocol::FolderInfo> {
        if !self.connected {
            return Err(FshError::NetworkError("Not connected to server".to_string()));
        }

        info!("Binding to folder: {}", folder_name);

        let bind_msg = FshMessage::FolderBind(FolderBindMessage {
            target_folder: folder_name.to_string(),
            preferred_shell,
        });

        self.send_message(bind_msg).await?;

        // Wait for folder bound response
        let response = self.receive_message().await?;

        match response {
            FshMessage::FolderBound(resp) => {
                if resp.success {
                    if let Some(folder_info) = resp.folder_info {
                        info!("Successfully bound to folder: {}", folder_info.name);
                        debug!("Folder path: {}", folder_info.path);
                        debug!("Shell type: {:?}", folder_info.shell_type);
                        debug!("Permissions: {:?}", folder_info.permissions);
                        Ok(folder_info)
                    } else {
                        error!("Folder bound successfully but no folder info received");
                        Err(FshError::ProtocolError("Missing folder info".to_string()))
                    }
                } else {
                    let error_msg = resp.error_message.unwrap_or_else(|| "Folder binding failed".to_string());
                    error!("Folder binding failed: {}", error_msg);
                    Err(FshError::FolderNotFound(folder_name.to_string()))
                }
            }
            _ => {
                error!("Unexpected response to folder bind message");
                Err(FshError::ProtocolError("Unexpected response".to_string()))
            }
        }
    }

    pub async fn wait_for_session_ready(&mut self) -> FshResult<(String, String)> {
        // Wait for session start message
        let response = self.receive_message().await?;

        match response {
            FshMessage::SessionStart(session_start) => {
                self.session_id = Some(session_start.session_id.clone());
                debug!("Session started: {}", session_start.session_id);

                // Wait for session ready message
                let response = self.receive_message().await?;

                match response {
                    FshMessage::SessionReady(session_ready) => {
                        info!("Session ready: {}", session_ready.session_id);
                        Ok((session_ready.shell_prompt, session_ready.working_directory))
                    }
                    _ => {
                        error!("Expected SessionReady message");
                        Err(FshError::ProtocolError("Expected SessionReady message".to_string()))
                    }
                }
            }
            _ => {
                error!("Expected SessionStart message");
                Err(FshError::ProtocolError("Expected SessionStart message".to_string()))
            }
        }
    }

    pub async fn execute_command(&mut self, command: &str, args: Vec<String>) -> FshResult<mpsc::Receiver<CommandOutput>> {
        let session_id = self.session_id.as_ref()
            .ok_or_else(|| FshError::SessionNotFound("No active session".to_string()))?;

        debug!("Executing command: {} {:?}", command, args);

        let cmd_msg = FshMessage::Command(CommandMessage {
            session_id: session_id.clone(),
            command: command.to_string(),
            args,
            environment: None,
        });

        self.send_message(cmd_msg).await?;

        let (tx, rx) = mpsc::channel(100);

        // For simplicity, we'll handle responses synchronously in the main loop
        // This is a simplified version - in production you'd want async message handling
        tokio::spawn(async move {
            // Simulate command completion for now
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            let cmd_output = CommandOutput {
                output_type: CommandOutputType::Complete,
                data: "Command executed (simplified implementation)".to_string(),
            };

            let _ = tx.send(cmd_output).await;
        });

        Ok(rx)
    }

    pub async fn list_files(&mut self, path: &str, show_hidden: bool) -> FshResult<Vec<FileEntry>> {
        let session_id = self.session_id.as_ref()
            .ok_or_else(|| FshError::SessionNotFound("No active session".to_string()))?;

        let list_msg = FshMessage::FileList(FileListMessage {
            session_id: session_id.clone(),
            path: path.to_string(),
            show_hidden,
        });

        self.send_message(list_msg).await?;

        // Wait for response
        let response = self.receive_message().await?;

        match response {
            FshMessage::FileListResponse(resp) => {
                if resp.success {
                    Ok(resp.files)
                } else {
                    let error_msg = resp.error_message.unwrap_or_else(|| "File list failed".to_string());
                    Err(FshError::ShellError(error_msg))
                }
            }
            _ => {
                Err(FshError::ProtocolError("Unexpected response to file list".to_string()))
            }
        }
    }

    pub async fn disconnect(&mut self) -> FshResult<()> {
        if !self.connected {
            return Ok(());
        }

        info!("Disconnecting from FSH server");

        let disconnect_msg = FshMessage::Disconnect(DisconnectMessage {
            reason: "Client requested disconnect".to_string(),
        });

        if let Err(e) = self.send_message(disconnect_msg).await {
            warn!("Failed to send disconnect message: {}", e);
        }

        self.stream = None;
        self.connected = false;
        self.session_id = None;

        info!("Disconnected from FSH server");
        Ok(())
    }

    async fn send_message(&mut self, message: FshMessage) -> FshResult<()> {
        if let Some(ref mut stream) = self.stream {
            FshCodec::write_message(stream, &message).await
        } else {
            Err(FshError::NetworkError("Not connected".to_string()))
        }
    }

    async fn receive_message(&mut self) -> FshResult<FshMessage> {
        if let Some(ref mut stream) = self.stream {
            FshCodec::read_message(stream).await
        } else {
            Err(FshError::NetworkError("Not connected".to_string()))
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub output_type: CommandOutputType,
    pub data: String,
}

#[derive(Debug, Clone)]
pub enum CommandOutputType {
    Stdout,
    Stderr,
    Complete,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = FshClient::new("127.0.0.1:2222".to_string());
        assert_eq!(client.server_addr, "127.0.0.1:2222");
        assert!(!client.is_connected());
        assert!(client.session_id().is_none());
    }
}