use crate::config::Config;
use crate::protocol::{
    FshMessage, FshCodec, FshError, FshResult, FSH_VERSION, ClientInfo,
    message::*,
};
use crate::server::Session;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

#[derive(Debug)]
pub struct Connection {
    stream: Option<TcpStream>,
    client_addr: String,
    config: Arc<Config>,
    authenticated: bool,
    client_info: Option<ClientInfo>,
}

impl Connection {
    pub fn new(stream: TcpStream, client_addr: String, config: Arc<Config>) -> Self {
        Self {
            stream: Some(stream),
            client_addr,
            config,
            authenticated: false,
            client_info: None,
        }
    }

    pub async fn handle(mut self) -> FshResult<Session> {
        // Set connection timeout
        let timeout_duration = Duration::from_secs(self.config.server.connection_timeout_seconds);

        // Handle connection with timeout
        timeout(timeout_duration, self.handle_connection()).await
            .map_err(|_| FshError::NetworkError("Connection timeout".to_string()))?
    }

    async fn handle_connection(&mut self) -> FshResult<Session> {
        // Step 1: Handle connection handshake
        self.handle_connect().await?;

        // Step 2: Handle authentication (if required)
        if self.config.security.require_authentication {
            self.handle_authentication().await?;
        } else {
            self.authenticated = true;
            info!("Authentication skipped for {}", self.client_addr);
        }

        // Step 3: Handle folder binding
        let folder_info = self.handle_folder_binding().await?;

        // Step 4: Create session
        let session = self.create_session(folder_info).await?;

        Ok(session)
    }

    async fn handle_connect(&mut self) -> FshResult<()> {
        debug!("Waiting for connect message from {}", self.client_addr);

        // Wait for connect message
        let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
        let message = FshCodec::read_message(stream).await?;

        match message {
            FshMessage::Connect(connect_msg) => {
                info!("Connect request from {} ({})",
                      self.client_addr, connect_msg.client_info.platform);

                // Validate protocol version
                if connect_msg.version != FSH_VERSION {
                    let response = FshMessage::ConnectResponse(ConnectResponseMessage {
                        success: false,
                        server_version: FSH_VERSION.to_string(),
                        supported_features: vec!["folder_binding".to_string(), "file_operations".to_string()],
                        available_folders: vec![],
                        message: Some(format!("Unsupported protocol version: {}. Expected: {}",
                                            connect_msg.version, FSH_VERSION)),
                    });

                    let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
                FshCodec::write_message(stream, &response).await?;
                    return Err(FshError::ProtocolError("Version mismatch".to_string()));
                }

                // Store client info
                self.client_info = Some(connect_msg.client_info);

                // Send successful response
                let available_folders = self.config.folders.iter()
                    .map(|f| f.name.clone())
                    .collect();

                let response = FshMessage::ConnectResponse(ConnectResponseMessage {
                    success: true,
                    server_version: FSH_VERSION.to_string(),
                    supported_features: vec![
                        "folder_binding".to_string(),
                        "file_operations".to_string(),
                        "command_execution".to_string(),
                        "shell_session".to_string(),
                    ],
                    available_folders,
                    message: Some("Connection accepted".to_string()),
                });

                let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
                FshCodec::write_message(stream, &response).await?;
                info!("Connect handshake completed for {}", self.client_addr);
                Ok(())
            }
            _ => {
                error!("Expected Connect message, got {:?}", message.message_type());
                let error_msg = FshMessage::Error(ErrorMessage {
                    error_type: "protocol_error".to_string(),
                    message: "Expected Connect message".to_string(),
                    details: None,
                });
                let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
                FshCodec::write_message(stream, &error_msg).await?;
                Err(FshError::ProtocolError("Expected Connect message".to_string()))
            }
        }
    }

    async fn handle_authentication(&mut self) -> FshResult<()> {
        debug!("Handling authentication for {}", self.client_addr);

        let mut attempts = 0;
        let max_attempts = self.config.security.max_failed_attempts;

        while attempts < max_attempts {
            // Wait for authentication message
            let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
        let message = FshCodec::read_message(stream).await?;

            match message {
                FshMessage::Authenticate(auth_msg) => {
                    debug!("Authentication attempt {} from {}", attempts + 1, self.client_addr);

                    // Validate authentication
                    let auth_result = self.validate_authentication(&auth_msg).await;

                    match auth_result {
                        Ok(()) => {
                            let response = FshMessage::AuthResponse(AuthResponseMessage {
                                success: true,
                                message: Some("Authentication successful".to_string()),
                            });

                            let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
                FshCodec::write_message(stream, &response).await?;
                            self.authenticated = true;
                            info!("Authentication successful for {}", self.client_addr);
                            return Ok(());
                        }
                        Err(e) => {
                            attempts += 1;
                            warn!("Authentication failed for {} (attempt {}): {}",
                                  self.client_addr, attempts, e);

                            let response = FshMessage::AuthResponse(AuthResponseMessage {
                                success: false,
                                message: Some(format!("Authentication failed: {}. Attempts: {}/{}",
                                                    e, attempts, max_attempts)),
                            });

                            let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
                FshCodec::write_message(stream, &response).await?;

                            if attempts >= max_attempts {
                                error!("Maximum authentication attempts exceeded for {}", self.client_addr);
                                return Err(FshError::AuthenticationFailed);
                            }
                        }
                    }
                }
                _ => {
                    error!("Expected Authenticate message from {}, got {:?}",
                           self.client_addr, message.message_type());
                    let error_msg = FshMessage::Error(ErrorMessage {
                        error_type: "protocol_error".to_string(),
                        message: "Expected Authenticate message".to_string(),
                        details: None,
                    });
                    let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
                FshCodec::write_message(stream, &error_msg).await?;
                    return Err(FshError::ProtocolError("Expected Authenticate message".to_string()));
                }
            }
        }

        Err(FshError::AuthenticationFailed)
    }

    async fn validate_authentication(&self, auth_msg: &AuthenticateMessage) -> FshResult<()> {
        match auth_msg.auth_type.as_str() {
            "token" => {
                if let Some(token) = auth_msg.credentials.get("token") {
                    // TODO: Implement actual token validation
                    // For now, accept any non-empty token
                    if !token.is_empty() {
                        Ok(())
                    } else {
                        Err(FshError::AuthenticationFailed)
                    }
                } else {
                    Err(FshError::AuthenticationFailed)
                }
            }
            "password" => {
                // TODO: Implement password authentication
                Err(FshError::ProtocolError("Password authentication not implemented".to_string()))
            }
            _ => {
                Err(FshError::ProtocolError(format!("Unsupported auth method: {}", auth_msg.auth_type)))
            }
        }
    }

    async fn handle_folder_binding(&mut self) -> FshResult<crate::protocol::FolderInfo> {
        debug!("Handling folder binding for {}", self.client_addr);

        // Wait for folder bind message
        let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
        let message = FshCodec::read_message(stream).await?;

        match message {
            FshMessage::FolderBind(bind_msg) => {
                info!("Folder bind request for '{}' from {}",
                      bind_msg.target_folder, self.client_addr);

                // Find the requested folder in config
                let folder_config = self.config.find_folder_by_name(&bind_msg.target_folder)
                    .or_else(|| self.config.find_folder_by_path(&bind_msg.target_folder));

                match folder_config {
                    Some(folder) => {
                        // Validate folder access
                        if let Err(e) = folder.validate() {
                            warn!("Folder validation failed for '{}': {}", bind_msg.target_folder, e);
                            let response = FshMessage::FolderBound(FolderBoundMessage {
                                success: false,
                                folder_info: None,
                                error_message: Some(format!("Folder access error: {}", e)),
                            });
                            let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
                FshCodec::write_message(stream, &response).await?;
                            return Err(e);
                        }

                        // Create folder info
                        let mut folder_info = folder.to_folder_info();

                        // Override shell type if requested
                        if let Some(preferred_shell) = bind_msg.preferred_shell {
                            folder_info.shell_type = preferred_shell;
                        }

                        // Send successful response
                        let response = FshMessage::FolderBound(FolderBoundMessage {
                            success: true,
                            folder_info: Some(folder_info.clone()),
                            error_message: None,
                        });

                        let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
                FshCodec::write_message(stream, &response).await?;
                        info!("Folder '{}' bound successfully for {}", bind_msg.target_folder, self.client_addr);
                        Ok(folder_info)
                    }
                    None => {
                        warn!("Folder '{}' not found for {}", bind_msg.target_folder, self.client_addr);
                        let response = FshMessage::FolderBound(FolderBoundMessage {
                            success: false,
                            folder_info: None,
                            error_message: Some(format!("Folder '{}' not found or not accessible", bind_msg.target_folder)),
                        });
                        let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
                FshCodec::write_message(stream, &response).await?;
                        Err(FshError::FolderNotFound(bind_msg.target_folder))
                    }
                }
            }
            _ => {
                error!("Expected FolderBind message from {}, got {:?}",
                       self.client_addr, message.message_type());
                let error_msg = FshMessage::Error(ErrorMessage {
                    error_type: "protocol_error".to_string(),
                    message: "Expected FolderBind message".to_string(),
                    details: None,
                });
                let stream = self.stream.as_mut().ok_or_else(|| FshError::NetworkError("Stream not available".to_string()))?;
                FshCodec::write_message(stream, &error_msg).await?;
                Err(FshError::ProtocolError("Expected FolderBind message".to_string()))
            }
        }
    }

    async fn create_session(&mut self, folder_info: crate::protocol::FolderInfo) -> FshResult<Session> {
        let session_id = Uuid::new_v4().to_string();

        debug!("Creating session {} for {}", session_id, self.client_addr);

        // Find the folder config
        let folder_config = self.config.find_folder_by_name(&folder_info.name)
            .ok_or_else(|| FshError::ConfigError("Folder config not found".to_string()))?;

        // Take ownership of the stream for the session
        let stream = self.stream.take().ok_or_else(|| FshError::NetworkError("Stream already taken".to_string()))?;

        // Create session
        let session = Session::new(
            session_id.clone(),
            stream,
            folder_info.clone(),
            folder_config.clone(),
            self.client_info.clone().unwrap_or_else(|| ClientInfo {
                platform: "unknown".to_string(),
                app_version: "unknown".to_string(),
                app_name: "unknown".to_string(),
            }),
        ).await?;

        // Note: Session will handle sending session start message internally

        info!("Session {} created for {} on folder '{}'",
              session_id, self.client_addr, folder_config.name);

        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FolderConfig;
    use crate::protocol::ShellType;
    use tempfile::TempDir;
    use tokio::net::{TcpListener, TcpStream};

    async fn create_test_connection() -> (Connection, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (server_stream, _) = listener.accept().await.unwrap();

        let mut config = Config::default();
        config.security.require_authentication = false;

        let temp_dir = TempDir::new().unwrap();
        let folder = FolderConfig::new("test".to_string(), temp_dir.path());
        config.folders.push(folder);

        let connection = Connection::new(server_stream, "127.0.0.1:12345".to_string(), Arc::new(config));

        (connection, client_stream)
    }

    #[tokio::test]
    async fn test_connection_creation() {
        let config = Config::default();
        let stream = TcpStream::connect("127.0.0.1:1234").await;
        // This will fail, but we're just testing the constructor
        if stream.is_err() {
            // Create a dummy stream for testing
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let client = TcpStream::connect(addr).await.unwrap();
            let (server, _) = listener.accept().await.unwrap();

            let connection = Connection::new(server, "127.0.0.1:12345".to_string(), Arc::new(config));
            assert_eq!(connection.client_addr, "127.0.0.1:12345");
            assert!(!connection.authenticated);
        }
    }
}