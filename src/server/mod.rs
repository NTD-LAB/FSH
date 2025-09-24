pub mod connection;
pub mod session;

pub use connection::*;
pub use session::*;

use crate::config::Config;
use crate::protocol::{FshError, FshResult};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{info, error, warn};
use std::collections::HashMap;

#[derive(Debug)]
pub struct FshServer {
    config: Arc<Config>,
    sessions: Arc<RwLock<HashMap<String, Arc<Session>>>>,
    listener: Option<TcpListener>,
}

impl FshServer {
    pub fn new(config: Config) -> FshResult<Self> {
        config.validate()?;

        Ok(Self {
            config: Arc::new(config),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            listener: None,
        })
    }

    pub async fn start(&mut self) -> FshResult<()> {
        let bind_addr = format!("{}:{}", self.config.server.host, self.config.server.port);

        info!("Starting FSH server on {}", bind_addr);

        let listener = TcpListener::bind(&bind_addr).await
            .map_err(|e| FshError::NetworkError(format!("Failed to bind to {}: {}", bind_addr, e)))?;

        info!("FSH server listening on {}", bind_addr);
        self.listener = Some(listener);

        // Main server loop
        while let Some(ref listener) = self.listener {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("New connection from {}", addr);

                    // Check connection limit
                    let current_connections = self.sessions.read().await.len();
                    if current_connections >= self.config.server.max_connections {
                        warn!("Connection limit reached, rejecting connection from {}", addr);
                        drop(stream);
                        continue;
                    }

                    // Handle connection
                    let config = Arc::clone(&self.config);
                    let sessions = Arc::clone(&self.sessions);

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, addr.to_string(), config, sessions).await {
                            error!("Connection error from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }

        Ok(())
    }

    pub async fn stop(&mut self) -> FshResult<()> {
        info!("Stopping FSH server");

        // Drop the listener to stop accepting new connections
        self.listener = None;

        // Close all active sessions
        let mut sessions = self.sessions.write().await;
        for (session_id, session) in sessions.drain() {
            info!("Closing session {}", session_id);
            if let Err(e) = session.close().await {
                error!("Error closing session {}: {}", session_id, e);
            }
        }

        info!("FSH server stopped");
        Ok(())
    }

    async fn handle_connection(
        stream: tokio::net::TcpStream,
        client_addr: String,
        config: Arc<Config>,
        sessions: Arc<RwLock<HashMap<String, Arc<Session>>>>,
    ) -> FshResult<()> {
        let connection = Connection::new(stream, client_addr, config);

        // Handle the connection lifecycle
        match connection.handle().await {
            Ok(session) => {
                let session_id = session.id().to_string();
                info!("Session {} established", session_id);

                // Store the session
                sessions.write().await.insert(session_id.clone(), Arc::new(session));

                // Session will be removed when it's dropped or explicitly closed
            }
            Err(e) => {
                error!("Connection handling failed: {}", e);
            }
        }

        Ok(())
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        self.sessions.read().await.keys().cloned().collect()
    }

    pub async fn get_session(&self, session_id: &str) -> Option<Arc<Session>> {
        self.sessions.read().await.get(session_id).cloned()
    }

    pub async fn close_session(&self, session_id: &str) -> FshResult<()> {
        let session = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(session_id)
        };

        if let Some(session) = session {
            session.close().await?;
            info!("Session {} closed", session_id);
            Ok(())
        } else {
            Err(FshError::SessionNotFound(session_id.to_string()))
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub async fn stats(&self) -> ServerStats {
        let sessions = self.sessions.read().await;
        ServerStats {
            active_sessions: sessions.len(),
            max_connections: self.config.server.max_connections,
            uptime_seconds: 0, // TODO: Track uptime
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerStats {
    pub active_sessions: usize,
    pub max_connections: usize,
    pub uptime_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_server_creation() {
        let config = Config::default();
        let server = FshServer::new(config);
        assert!(server.is_ok());
    }

    #[tokio::test]
    async fn test_server_stats() {
        let config = Config::default();
        let server = FshServer::new(config).unwrap();
        let stats = server.stats().await;
        assert_eq!(stats.active_sessions, 0);
        assert_eq!(stats.max_connections, 10); // Default value
    }
}