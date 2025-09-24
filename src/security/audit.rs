use crate::config::SecurityConfig;
use crate::protocol::{FshError, FshResult};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::SystemTime;
use tokio::sync::Mutex;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    pub event_type: SecurityEventType,
    pub source_ip: IpAddr,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub resource: Option<String>,
    pub details: String,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityEventType {
    ConnectionAttempt,
    AuthenticationSuccess,
    AuthenticationFailure,
    SessionEstablished,
    SessionTerminated,
    CommandExecution,
    FileAccess,
    PermissionDenied,
    SuspiciousActivity,
    IpBlocked,
    RateLimitExceeded,
}

#[derive(Debug)]
pub struct AuditLogger {
    log_file: Option<PathBuf>,
    enabled: bool,
    file_mutex: Mutex<()>,
}

impl AuditLogger {
    pub fn new(config: &SecurityConfig) -> FshResult<Self> {
        Ok(Self {
            log_file: config.log_file.clone(),
            enabled: config.enable_logging,
            file_mutex: Mutex::new(()),
        })
    }

    pub async fn log_security_event(&self, event: SecurityEvent) -> FshResult<()> {
        if !self.enabled {
            return Ok(());
        }

        debug!("Security event: {:?}", event);

        // Log to file if configured
        if let Some(ref log_file) = self.log_file {
            self.log_to_file(log_file, &event).await?;
        }

        // Log to system logger based on severity
        match event.event_type {
            SecurityEventType::SuspiciousActivity |
            SecurityEventType::PermissionDenied |
            SecurityEventType::IpBlocked => {
                tracing::warn!(
                    event_type = ?event.event_type,
                    source_ip = %event.source_ip,
                    session_id = ?event.session_id,
                    resource = ?event.resource,
                    details = %event.details,
                    "Security event"
                );
            }
            SecurityEventType::AuthenticationFailure => {
                tracing::warn!(
                    source_ip = %event.source_ip,
                    details = %event.details,
                    "Authentication failed"
                );
            }
            _ => {
                tracing::info!(
                    event_type = ?event.event_type,
                    source_ip = %event.source_ip,
                    session_id = ?event.session_id,
                    "Security event"
                );
            }
        }

        Ok(())
    }

    async fn log_to_file(&self, log_file: &PathBuf, event: &SecurityEvent) -> FshResult<()> {
        let _guard = self.file_mutex.lock().await;

        // Create JSON log entry
        let timestamp = event.timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| FshError::ConfigError(format!("Time error: {}", e)))?
            .as_secs();

        let log_entry = serde_json::json!({
            "timestamp": timestamp,
            "event_type": event.event_type,
            "source_ip": event.source_ip.to_string(),
            "session_id": event.session_id,
            "user_id": event.user_id,
            "resource": event.resource,
            "details": event.details,
        });

        // Append to log file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)
            .map_err(|e| FshError::ConfigError(format!("Failed to open log file: {}", e)))?;

        writeln!(file, "{}", log_entry)
            .map_err(|e| FshError::ConfigError(format!("Failed to write to log file: {}", e)))?;

        file.flush()
            .map_err(|e| FshError::ConfigError(format!("Failed to flush log file: {}", e)))?;

        Ok(())
    }

    pub async fn log_connection_attempt(&self, source_ip: IpAddr, success: bool) -> FshResult<()> {
        let event = SecurityEvent {
            event_type: SecurityEventType::ConnectionAttempt,
            source_ip,
            session_id: None,
            user_id: None,
            resource: None,
            details: if success { "Connection accepted".to_string() } else { "Connection rejected".to_string() },
            timestamp: SystemTime::now(),
        };

        self.log_security_event(event).await
    }

    pub async fn log_authentication_attempt(&self, source_ip: IpAddr, user_id: Option<String>, success: bool, details: String) -> FshResult<()> {
        let event = SecurityEvent {
            event_type: if success { SecurityEventType::AuthenticationSuccess } else { SecurityEventType::AuthenticationFailure },
            source_ip,
            session_id: None,
            user_id,
            resource: None,
            details,
            timestamp: SystemTime::now(),
        };

        self.log_security_event(event).await
    }

    pub async fn log_session_event(&self, source_ip: IpAddr, session_id: String, established: bool) -> FshResult<()> {
        let event = SecurityEvent {
            event_type: if established { SecurityEventType::SessionEstablished } else { SecurityEventType::SessionTerminated },
            source_ip,
            session_id: Some(session_id),
            user_id: None,
            resource: None,
            details: if established { "Session established".to_string() } else { "Session terminated".to_string() },
            timestamp: SystemTime::now(),
        };

        self.log_security_event(event).await
    }

    pub async fn log_command_execution(&self, source_ip: IpAddr, session_id: String, command: String) -> FshResult<()> {
        let event = SecurityEvent {
            event_type: SecurityEventType::CommandExecution,
            source_ip,
            session_id: Some(session_id),
            user_id: None,
            resource: Some(command.clone()),
            details: format!("Executed command: {}", command),
            timestamp: SystemTime::now(),
        };

        self.log_security_event(event).await
    }

    pub async fn log_file_access(&self, source_ip: IpAddr, session_id: String, file_path: String, operation: String) -> FshResult<()> {
        let event = SecurityEvent {
            event_type: SecurityEventType::FileAccess,
            source_ip,
            session_id: Some(session_id),
            user_id: None,
            resource: Some(file_path.clone()),
            details: format!("File operation: {} on {}", operation, file_path),
            timestamp: SystemTime::now(),
        };

        self.log_security_event(event).await
    }

    pub async fn log_permission_denied(&self, source_ip: IpAddr, session_id: Option<String>, resource: String, reason: String) -> FshResult<()> {
        let event = SecurityEvent {
            event_type: SecurityEventType::PermissionDenied,
            source_ip,
            session_id,
            user_id: None,
            resource: Some(resource),
            details: reason,
            timestamp: SystemTime::now(),
        };

        self.log_security_event(event).await
    }

    pub async fn log_suspicious_activity(&self, source_ip: IpAddr, session_id: Option<String>, activity: String) -> FshResult<()> {
        let event = SecurityEvent {
            event_type: SecurityEventType::SuspiciousActivity,
            source_ip,
            session_id,
            user_id: None,
            resource: None,
            details: activity,
            timestamp: SystemTime::now(),
        };

        self.log_security_event(event).await
    }

    pub async fn log_rate_limit_exceeded(&self, source_ip: IpAddr) -> FshResult<()> {
        let event = SecurityEvent {
            event_type: SecurityEventType::RateLimitExceeded,
            source_ip,
            session_id: None,
            user_id: None,
            resource: None,
            details: "Rate limit exceeded".to_string(),
            timestamp: SystemTime::now(),
        };

        self.log_security_event(event).await
    }

    pub fn get_log_file_path(&self) -> Option<&PathBuf> {
        self.log_file.as_ref()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn test_audit_logger() {
        let temp_file = NamedTempFile::new().unwrap();
        let config = SecurityConfig {
            require_authentication: true,
            auth_methods: vec!["token".to_string()],
            max_failed_attempts: 3,
            enable_logging: true,
            log_file: Some(temp_file.path().to_path_buf()),
        };

        let logger = AuditLogger::new(&config).unwrap();
        assert!(logger.is_enabled());

        let test_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        // Test connection attempt logging
        logger.log_connection_attempt(test_ip, true).await.unwrap();

        // Test authentication logging
        logger.log_authentication_attempt(
            test_ip,
            Some("test_user".to_string()),
            false,
            "Invalid token".to_string()
        ).await.unwrap();

        // Test command execution logging
        logger.log_command_execution(
            test_ip,
            "session-123".to_string(),
            "ls -la".to_string()
        ).await.unwrap();

        // Test suspicious activity logging
        logger.log_suspicious_activity(
            test_ip,
            Some("session-123".to_string()),
            "Attempted to access /etc/passwd".to_string()
        ).await.unwrap();

        // Read the log file and verify entries were written
        let log_content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(log_content.contains("ConnectionAttempt"));
        assert!(log_content.contains("AuthenticationFailure"));
        assert!(log_content.contains("CommandExecution"));
        assert!(log_content.contains("SuspiciousActivity"));
        assert!(log_content.contains("192.168.1.100"));
    }

    #[tokio::test]
    async fn test_disabled_audit_logger() {
        let config = SecurityConfig {
            require_authentication: true,
            auth_methods: vec!["token".to_string()],
            max_failed_attempts: 3,
            enable_logging: false,
            log_file: None,
        };

        let logger = AuditLogger::new(&config).unwrap();
        assert!(!logger.is_enabled());

        let test_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        // Should not fail even when disabled
        logger.log_connection_attempt(test_ip, true).await.unwrap();
    }
}