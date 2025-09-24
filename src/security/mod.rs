pub mod audit;
pub mod auth;
pub mod rate_limit;

pub use audit::*;
pub use auth::*;
pub use rate_limit::*;

use crate::protocol::{FshError, FshResult};
use std::net::IpAddr;
use std::time::{Duration, SystemTime};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{warn, error, info};

#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub client_ip: IpAddr,
    pub session_id: Option<String>,
    pub authenticated: bool,
    pub permissions: Vec<crate::protocol::Permission>,
    pub folder_path: Option<String>,
    pub created_at: SystemTime,
}

#[derive(Debug)]
pub struct SecurityManager {
    audit_logger: AuditLogger,
    auth_manager: AuthManager,
    rate_limiter: RateLimiter,
    blocked_ips: Arc<RwLock<HashMap<IpAddr, SystemTime>>>,
    failed_attempts: Arc<RwLock<HashMap<IpAddr, Vec<SystemTime>>>>,
}

impl SecurityManager {
    pub fn new(config: &crate::config::SecurityConfig) -> FshResult<Self> {
        Ok(Self {
            audit_logger: AuditLogger::new(config)?,
            auth_manager: AuthManager::new(config)?,
            rate_limiter: RateLimiter::new(100, Duration::from_secs(60)), // 100 requests per minute
            blocked_ips: Arc::new(RwLock::new(HashMap::new())),
            failed_attempts: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn check_ip_allowed(&self, ip: IpAddr) -> FshResult<()> {
        // Check if IP is blocked
        let blocked_ips = self.blocked_ips.read().await;
        if let Some(blocked_until) = blocked_ips.get(&ip) {
            if SystemTime::now() < *blocked_until {
                warn!("Blocked IP {} attempted connection", ip);
                return Err(FshError::PermissionDenied("IP blocked".to_string()));
            }
        }

        // Check rate limiting
        if !self.rate_limiter.allow(ip.to_string()).await {
            warn!("Rate limit exceeded for IP {}", ip);
            return Err(FshError::PermissionDenied("Rate limit exceeded".to_string()));
        }

        Ok(())
    }

    pub async fn record_auth_failure(&self, ip: IpAddr) -> FshResult<()> {
        let mut failed_attempts = self.failed_attempts.write().await;
        let attempts = failed_attempts.entry(ip).or_insert_with(Vec::new);

        let now = SystemTime::now();
        attempts.push(now);

        // Keep only attempts from the last hour
        attempts.retain(|&time| now.duration_since(time).unwrap_or(Duration::ZERO) < Duration::from_secs(3600));

        // Block IP if too many failures
        let max_attempts = 5;
        if attempts.len() >= max_attempts {
            let mut blocked_ips = self.blocked_ips.write().await;
            let block_duration = Duration::from_secs(3600); // Block for 1 hour
            blocked_ips.insert(ip, now + block_duration);

            error!("IP {} blocked due to {} failed authentication attempts", ip, attempts.len());

            // Log security event
            self.audit_logger.log_security_event(SecurityEvent {
                event_type: SecurityEventType::IpBlocked,
                source_ip: ip,
                session_id: None,
                user_id: None,
                resource: None,
                details: format!("Blocked after {} failed attempts", attempts.len()),
                timestamp: now,
            }).await?;
        }

        Ok(())
    }

    pub async fn record_successful_auth(&self, ip: IpAddr) -> FshResult<()> {
        // Clear failed attempts for this IP
        let mut failed_attempts = self.failed_attempts.write().await;
        failed_attempts.remove(&ip);

        info!("Successful authentication from {}", ip);
        Ok(())
    }

    pub async fn validate_command(&self, context: &SecurityContext, command: &str) -> FshResult<()> {
        // Log command execution
        self.audit_logger.log_security_event(SecurityEvent {
            event_type: SecurityEventType::CommandExecution,
            source_ip: context.client_ip,
            session_id: context.session_id.clone(),
            user_id: None,
            resource: Some(command.to_string()),
            details: format!("Command: {}", command),
            timestamp: SystemTime::now(),
        }).await?;

        // Check for dangerous patterns
        let dangerous_patterns = [
            "rm -rf /",
            "del /f /q",
            "format",
            "fdisk",
            "dd if=",
            "mkfs",
            "shutdown",
            "reboot",
            "halt",
            "poweroff",
            "sudo su",
            "sudo -i",
            "passwd",
            "chpasswd",
            "../../../",
            "..\\..\\..\\",
        ];

        for pattern in &dangerous_patterns {
            if command.to_lowercase().contains(pattern) {
                warn!("Dangerous command pattern detected: {} from {}", pattern, context.client_ip);

                self.audit_logger.log_security_event(SecurityEvent {
                    event_type: SecurityEventType::SuspiciousActivity,
                    source_ip: context.client_ip,
                    session_id: context.session_id.clone(),
                    user_id: None,
                    resource: Some(command.to_string()),
                    details: format!("Dangerous pattern detected: {}", pattern),
                    timestamp: SystemTime::now(),
                }).await?;

                return Err(FshError::PermissionDenied(
                    format!("Command contains dangerous pattern: {}", pattern)
                ));
            }
        }

        Ok(())
    }

    pub async fn validate_file_access(&self, context: &SecurityContext, file_path: &str, operation: FileOperation) -> FshResult<()> {
        // Log file access
        self.audit_logger.log_security_event(SecurityEvent {
            event_type: SecurityEventType::FileAccess,
            source_ip: context.client_ip,
            session_id: context.session_id.clone(),
            user_id: None,
            resource: Some(file_path.to_string()),
            details: format!("Operation: {:?} on {}", operation, file_path),
            timestamp: SystemTime::now(),
        }).await?;

        // Check if file path is suspicious
        let suspicious_paths = [
            "/etc/passwd",
            "/etc/shadow",
            "/etc/sudoers",
            "C:\\Windows\\System32\\config\\SAM",
            "C:\\Windows\\System32\\config\\SYSTEM",
            "/proc/",
            "/sys/",
            "/dev/",
        ];

        for path in &suspicious_paths {
            if file_path.contains(path) {
                warn!("Suspicious file access attempt: {} from {}", file_path, context.client_ip);

                self.audit_logger.log_security_event(SecurityEvent {
                    event_type: SecurityEventType::SuspiciousActivity,
                    source_ip: context.client_ip,
                    session_id: context.session_id.clone(),
                    user_id: None,
                    resource: Some(file_path.to_string()),
                    details: format!("Suspicious file access: {}", path),
                    timestamp: SystemTime::now(),
                }).await?;

                return Err(FshError::PermissionDenied(
                    format!("Access denied to system file: {}", path)
                ));
            }
        }

        Ok(())
    }

    pub async fn clean_expired_entries(&self) -> FshResult<()> {
        let now = SystemTime::now();

        // Clean expired IP blocks
        {
            let mut blocked_ips = self.blocked_ips.write().await;
            blocked_ips.retain(|_, &mut blocked_until| now < blocked_until);
        }

        // Clean old failed attempts
        {
            let mut failed_attempts = self.failed_attempts.write().await;
            for attempts in failed_attempts.values_mut() {
                attempts.retain(|&time| now.duration_since(time).unwrap_or(Duration::ZERO) < Duration::from_secs(3600));
            }
            failed_attempts.retain(|_, attempts| !attempts.is_empty());
        }

        Ok(())
    }

    pub async fn get_security_stats(&self) -> SecurityStats {
        let blocked_ips = self.blocked_ips.read().await;
        let failed_attempts = self.failed_attempts.read().await;

        SecurityStats {
            blocked_ips_count: blocked_ips.len(),
            failed_attempts_count: failed_attempts.values().map(|v| v.len()).sum(),
            active_sessions_count: 0, // TODO: Track from session manager
        }
    }
}

#[derive(Debug, Clone)]
pub enum FileOperation {
    Read,
    Write,
    Execute,
    Delete,
    List,
}

#[derive(Debug, Clone)]
pub struct SecurityStats {
    pub blocked_ips_count: usize,
    pub failed_attempts_count: usize,
    pub active_sessions_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use crate::config::SecurityConfig;

    #[tokio::test]
    async fn test_security_manager() {
        let config = SecurityConfig {
            require_authentication: true,
            auth_methods: vec!["token".to_string()],
            max_failed_attempts: 3,
            enable_logging: true,
            log_file: None,
        };

        let security_manager = SecurityManager::new(&config).unwrap();
        let test_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        // IP should be allowed initially
        assert!(security_manager.check_ip_allowed(test_ip).await.is_ok());

        // Record some failed attempts
        for _ in 0..3 {
            security_manager.record_auth_failure(test_ip).await.unwrap();
        }

        // IP should now be blocked
        assert!(security_manager.check_ip_allowed(test_ip).await.is_err());
    }

    #[tokio::test]
    async fn test_command_validation() {
        let config = SecurityConfig {
            require_authentication: true,
            auth_methods: vec!["token".to_string()],
            max_failed_attempts: 3,
            enable_logging: false, // Disable logging for test
            log_file: None,
        };

        let security_manager = SecurityManager::new(&config).unwrap();
        let context = SecurityContext {
            client_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            session_id: Some("test-session".to_string()),
            authenticated: true,
            permissions: vec![],
            folder_path: Some("/test".to_string()),
            created_at: SystemTime::now(),
        };

        // Safe command should be allowed
        assert!(security_manager.validate_command(&context, "ls -la").await.is_ok());

        // Dangerous command should be blocked
        assert!(security_manager.validate_command(&context, "rm -rf /").await.is_err());
    }
}