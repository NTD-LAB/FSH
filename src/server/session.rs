use crate::config::FolderConfig;
use crate::protocol::{
    FshMessage, FshCodec, FshResult, ClientInfo, FolderInfo,
    message::*,
};
use crate::sandbox::{SandboxedShell, SandboxConfig};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{RwLock, Mutex};
use tokio::time::{timeout, Duration};
use tracing::{info, warn, error, debug};

#[derive(Debug)]
pub struct Session {
    id: String,
    stream: Arc<Mutex<TcpStream>>,
    folder_info: FolderInfo,
    folder_config: FolderConfig,
    client_info: ClientInfo,
    shell: Arc<Mutex<SandboxedShell>>,
    active: Arc<RwLock<bool>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl Session {
    pub async fn new(
        id: String,
        stream: TcpStream,
        folder_info: FolderInfo,
        folder_config: FolderConfig,
        client_info: ClientInfo,
    ) -> FshResult<Self> {
        // Create sandboxed shell
        let sandbox_config = SandboxConfig::new(
            folder_config.get_path(),
            folder_info.shell_type.clone(),
        )
        .with_permissions(folder_info.permissions.clone())
        .with_allowed_commands(folder_config.allowed_commands.clone())
        .with_blocked_commands(folder_config.blocked_commands.clone());

        // Add environment variables
        let sandbox_config = folder_config.environment_vars.iter()
            .fold(sandbox_config, |config, (key, value)| {
                config.add_environment_var(key.clone(), value.clone())
            });

        let shell = SandboxedShell::new(sandbox_config)?;

        let session = Self {
            id: id.clone(),
            stream: Arc::new(Mutex::new(stream)),
            folder_info,
            folder_config,
            client_info,
            shell: Arc::new(Mutex::new(shell)),
            active: Arc::new(RwLock::new(true)),
            created_at: chrono::Utc::now(),
        };

        // Send session ready message
        session.send_session_ready().await?;

        // Start message handling loop
        session.start_message_loop().await?;

        info!("Session {} initialized successfully", id);
        Ok(session)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn folder_info(&self) -> &FolderInfo {
        &self.folder_info
    }

    pub fn client_info(&self) -> &ClientInfo {
        &self.client_info
    }

    pub fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.created_at
    }

    pub async fn is_active(&self) -> bool {
        *self.active.read().await
    }

    async fn send_session_ready(&self) -> FshResult<()> {
        let shell = self.shell.lock().await;
        let prompt = shell.get_shell_prompt();
        let working_dir = shell.working_directory().to_string_lossy().to_string();

        let message = FshMessage::SessionReady(SessionReadyMessage {
            session_id: self.id.clone(),
            shell_prompt: prompt,
            working_directory: working_dir,
        });

        let mut stream = self.stream.lock().await;
        FshCodec::write_message(&mut *stream, &message).await?;

        debug!("Session ready message sent for session {}", self.id);
        Ok(())
    }

    async fn start_message_loop(&self) -> FshResult<()> {
        let session_id = self.id.clone();
        let stream = Arc::clone(&self.stream);
        let shell = Arc::clone(&self.shell);
        let active = Arc::clone(&self.active);
        let folder_config = self.folder_config.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::message_loop(session_id, stream, shell, active, folder_config).await {
                error!("Session message loop error: {}", e);
            }
        });

        Ok(())
    }

    async fn message_loop(
        session_id: String,
        stream: Arc<Mutex<TcpStream>>,
        shell: Arc<Mutex<SandboxedShell>>,
        active: Arc<RwLock<bool>>,
        folder_config: FolderConfig,
    ) -> FshResult<()> {
        debug!("Starting message loop for session {}", session_id);

        while *active.read().await {
            // Read message with timeout
            let message = {
                let mut stream = stream.lock().await;
                match timeout(Duration::from_secs(30), FshCodec::read_message(&mut *stream)).await {
                    Ok(Ok(msg)) => msg,
                    Ok(Err(e)) => {
                        error!("Message read error in session {}: {}", session_id, e);
                        break;
                    }
                    Err(_) => {
                        // Timeout - send ping to check if client is still alive
                        if let Err(e) = FshCodec::write_message(&mut *stream, &FshMessage::Ping).await {
                            error!("Failed to send ping in session {}: {}", session_id, e);
                            break;
                        }
                        continue;
                    }
                }
            };

            debug!("Received message in session {}: {:?}", session_id, message.message_type());

            match message {
                FshMessage::Command(cmd_msg) => {
                    if let Err(e) = Self::handle_command(
                        &session_id,
                        cmd_msg,
                        Arc::clone(&shell),
                        Arc::clone(&stream),
                        &folder_config,
                    ).await {
                        error!("Command handling error in session {}: {}", session_id, e);
                    }
                }

                FshMessage::FileList(list_msg) => {
                    if let Err(e) = Self::handle_file_list(
                        &session_id,
                        list_msg,
                        Arc::clone(&shell),
                        Arc::clone(&stream),
                    ).await {
                        error!("File list error in session {}: {}", session_id, e);
                    }
                }

                FshMessage::FileRead(read_msg) => {
                    if let Err(e) = Self::handle_file_read(
                        &session_id,
                        read_msg,
                        Arc::clone(&shell),
                        Arc::clone(&stream),
                        &folder_config,
                    ).await {
                        error!("File read error in session {}: {}", session_id, e);
                    }
                }

                FshMessage::FileWrite(write_msg) => {
                    if let Err(e) = Self::handle_file_write(
                        &session_id,
                        write_msg,
                        Arc::clone(&shell),
                        Arc::clone(&stream),
                        &folder_config,
                    ).await {
                        error!("File write error in session {}: {}", session_id, e);
                    }
                }

                FshMessage::Ping => {
                    let mut stream = stream.lock().await;
                    if let Err(e) = FshCodec::write_message(&mut *stream, &FshMessage::Pong).await {
                        error!("Failed to send pong in session {}: {}", session_id, e);
                        break;
                    }
                }

                FshMessage::Pong => {
                    debug!("Received pong from session {}", session_id);
                }

                FshMessage::Disconnect(disconnect_msg) => {
                    info!("Client requested disconnect for session {}: {}", session_id, disconnect_msg.reason);
                    break;
                }

                _ => {
                    warn!("Unexpected message type in session {}: {:?}", session_id, message.message_type());
                }
            }
        }

        // Mark session as inactive
        *active.write().await = false;
        info!("Session {} message loop ended", session_id);
        Ok(())
    }

    async fn handle_command(
        session_id: &str,
        cmd_msg: CommandMessage,
        shell: Arc<Mutex<SandboxedShell>>,
        stream: Arc<Mutex<TcpStream>>,
        folder_config: &FolderConfig,
    ) -> FshResult<()> {
        debug!("Executing command in session {}: {}", session_id, cmd_msg.command);

        // Check permissions
        if !folder_config.can_execute() {
            let error_msg = FshMessage::Error(ErrorMessage {
                error_type: "permission_denied".to_string(),
                message: "Execute permission denied".to_string(),
                details: None,
            });

            let mut stream = stream.lock().await;
            FshCodec::write_message(&mut *stream, &error_msg).await?;
            return Ok(());
        }

        let mut shell = shell.lock().await;

        // Execute command
        match shell.execute_command(&cmd_msg.command, &cmd_msg.args).await {
            Ok((mut output_rx, mut result_rx)) => {
                drop(shell); // Release the shell lock

                // Handle output streaming
                let stream_clone = Arc::clone(&stream);
                let session_id_clone = session_id.to_string();

                tokio::spawn(async move {
                    while let Some(output) = output_rx.recv().await {
                        let output_msg = FshMessage::CommandOutput(CommandOutputMessage {
                            session_id: session_id_clone.clone(),
                            output_type: match output.output_type {
                                crate::sandbox::OutputType::Stdout => OutputType::Stdout,
                                crate::sandbox::OutputType::Stderr => OutputType::Stderr,
                            },
                            data: output.data.into_bytes(),
                        });

                        let mut stream = stream_clone.lock().await;
                        if let Err(e) = FshCodec::write_message(&mut *stream, &output_msg).await {
                            error!("Failed to send command output: {}", e);
                            break;
                        }
                    }
                });

                // Wait for command completion
                if let Some(result) = result_rx.recv().await {
                    let complete_msg = FshMessage::CommandComplete(CommandCompleteMessage {
                        session_id: session_id.to_string(),
                        exit_code: result.exit_code,
                        execution_time_ms: result.execution_time_ms,
                    });

                    let mut stream = stream.lock().await;
                    FshCodec::write_message(&mut *stream, &complete_msg).await?;
                }
            }
            Err(e) => {
                error!("Command execution failed in session {}: {}", session_id, e);

                let error_msg = FshMessage::Error(ErrorMessage {
                    error_type: "command_error".to_string(),
                    message: format!("Command execution failed: {}", e),
                    details: None,
                });

                let mut stream = stream.lock().await;
                FshCodec::write_message(&mut *stream, &error_msg).await?;
            }
        }

        Ok(())
    }

    async fn handle_file_list(
        session_id: &str,
        list_msg: FileListMessage,
        shell: Arc<Mutex<SandboxedShell>>,
        stream: Arc<Mutex<TcpStream>>,
    ) -> FshResult<()> {
        debug!("Listing files in session {}: {}", session_id, list_msg.path);

        let shell = shell.lock().await;
        let path = if list_msg.path.is_empty() { None } else { Some(list_msg.path.as_str()) };

        match shell.list_files(path, list_msg.show_hidden) {
            Ok(files) => {
                let response = FshMessage::FileListResponse(FileListResponseMessage {
                    success: true,
                    files,
                    error_message: None,
                });

                let mut stream = stream.lock().await;
                FshCodec::write_message(&mut *stream, &response).await?;
            }
            Err(e) => {
                let response = FshMessage::FileListResponse(FileListResponseMessage {
                    success: false,
                    files: vec![],
                    error_message: Some(format!("Failed to list files: {}", e)),
                });

                let mut stream = stream.lock().await;
                FshCodec::write_message(&mut *stream, &response).await?;
            }
        }

        Ok(())
    }

    async fn handle_file_read(
        session_id: &str,
        read_msg: FileReadMessage,
        shell: Arc<Mutex<SandboxedShell>>,
        stream: Arc<Mutex<TcpStream>>,
        folder_config: &FolderConfig,
    ) -> FshResult<()> {
        debug!("Reading file in session {}: {}", session_id, read_msg.file_path);

        // Check read permission
        if !folder_config.can_read() {
            let response = FshMessage::FileReadResponse(FileReadResponseMessage {
                success: false,
                data: vec![],
                total_size: 0,
                error_message: Some("Read permission denied".to_string()),
            });

            let mut stream = stream.lock().await;
            FshCodec::write_message(&mut *stream, &response).await?;
            return Ok(());
        }

        // TODO: Implement file reading with offset and length support
        // For now, just read the entire file
        let _shell = shell.lock().await;

        // Use the path validator to get the safe absolute path
        // This is a simplified implementation
        let response = FshMessage::FileReadResponse(FileReadResponseMessage {
            success: false,
            data: vec![],
            total_size: 0,
            error_message: Some("File reading not yet implemented".to_string()),
        });

        let mut stream = stream.lock().await;
        FshCodec::write_message(&mut *stream, &response).await?;

        Ok(())
    }

    async fn handle_file_write(
        session_id: &str,
        write_msg: FileWriteMessage,
        _shell: Arc<Mutex<SandboxedShell>>,
        stream: Arc<Mutex<TcpStream>>,
        folder_config: &FolderConfig,
    ) -> FshResult<()> {
        debug!("Writing file in session {}: {}", session_id, write_msg.file_path);

        // Check write permission
        if !folder_config.can_write() {
            let response = FshMessage::FileWriteResponse(FileWriteResponseMessage {
                success: false,
                bytes_written: 0,
                error_message: Some("Write permission denied".to_string()),
            });

            let mut stream = stream.lock().await;
            FshCodec::write_message(&mut *stream, &response).await?;
            return Ok(());
        }

        // TODO: Implement file writing
        // For now, just return not implemented
        let response = FshMessage::FileWriteResponse(FileWriteResponseMessage {
            success: false,
            bytes_written: 0,
            error_message: Some("File writing not yet implemented".to_string()),
        });

        let mut stream = stream.lock().await;
        FshCodec::write_message(&mut *stream, &response).await?;

        Ok(())
    }

    pub async fn close(&self) -> FshResult<()> {
        info!("Closing session {}", self.id);

        // Mark session as inactive
        *self.active.write().await = false;

        // Kill any running processes
        let mut shell = self.shell.lock().await;
        shell.kill_current_process().await?;

        // Send disconnect message to client
        let disconnect_msg = FshMessage::Disconnect(DisconnectMessage {
            reason: "Session closed by server".to_string(),
        });

        let mut stream = self.stream.lock().await;
        if let Err(e) = FshCodec::write_message(&mut *stream, &disconnect_msg).await {
            warn!("Failed to send disconnect message: {}", e);
        }

        info!("Session {} closed successfully", self.id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FolderConfig;
    use crate::protocol::ShellType;
    use tempfile::TempDir;
    use tokio::net::{TcpListener, TcpStream};

    #[tokio::test]
    async fn test_session_creation() {
        let temp_dir = TempDir::new().unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (server_stream, _) = listener.accept().await.unwrap();

        let folder_config = FolderConfig::new("test".to_string(), temp_dir.path());
        let folder_info = folder_config.to_folder_info();

        let client_info = ClientInfo {
            platform: "test".to_string(),
            app_version: "1.0".to_string(),
            app_name: "test".to_string(),
        };

        let session = Session::new(
            "test-session".to_string(),
            server_stream,
            folder_info,
            folder_config,
            client_info,
        ).await;

        assert!(session.is_ok());
        let session = session.unwrap();
        assert_eq!(session.id(), "test-session");
        assert!(session.is_active().await);
    }
}