use crate::config::SecurityConfig;
use crate::protocol::{FshError, FshResult};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

#[derive(Debug)]
pub struct AuthManager {
    auth_methods: Vec<String>,
    tokens: HashMap<String, TokenInfo>,
    sessions: HashMap<String, SessionInfo>,
}

#[derive(Debug, Clone)]
struct TokenInfo {
    token_hash: String,
    created_at: SystemTime,
    expires_at: Option<SystemTime>,
    permissions: Vec<crate::protocol::Permission>,
    description: String,
}

#[derive(Debug, Clone)]
struct SessionInfo {
    user_id: String,
    created_at: SystemTime,
    last_activity: SystemTime,
    client_ip: std::net::IpAddr,
}

impl AuthManager {
    pub fn new(config: &SecurityConfig) -> FshResult<Self> {
        let mut auth_manager = Self {
            auth_methods: config.auth_methods.clone(),
            tokens: HashMap::new(),
            sessions: HashMap::new(),
        };

        // Create a default token for development/testing
        if config.auth_methods.contains(&"token".to_string()) {
            auth_manager.create_token(
                "default",
                None,
                vec![
                    crate::protocol::Permission::Read,
                    crate::protocol::Permission::Write,
                    crate::protocol::Permission::Execute,
                ],
                "Default development token".to_string(),
            )?;
        }

        Ok(auth_manager)
    }

    pub fn validate_token(&self, token: &str) -> FshResult<&TokenInfo> {
        let token_hash = Self::hash_token(token);

        for token_info in self.tokens.values() {
            if token_info.token_hash == token_hash {
                // Check if token is expired
                if let Some(expires_at) = token_info.expires_at {
                    if SystemTime::now() > expires_at {
                        return Err(FshError::AuthenticationFailed);
                    }
                }

                return Ok(token_info);
            }
        }

        Err(FshError::AuthenticationFailed)
    }

    pub fn create_token(
        &mut self,
        token: &str,
        expires_at: Option<SystemTime>,
        permissions: Vec<crate::protocol::Permission>,
        description: String,
    ) -> FshResult<String> {
        let token_hash = Self::hash_token(token);
        let token_id = Uuid::new_v4().to_string();

        let token_info = TokenInfo {
            token_hash,
            created_at: SystemTime::now(),
            expires_at,
            permissions,
            description,
        };

        self.tokens.insert(token_id.clone(), token_info);

        Ok(token_id)
    }

    pub fn revoke_token(&mut self, token_id: &str) -> FshResult<()> {
        self.tokens.remove(token_id)
            .ok_or_else(|| FshError::ConfigError("Token not found".to_string()))?;

        Ok(())
    }

    pub fn create_session(
        &mut self,
        user_id: String,
        client_ip: std::net::IpAddr,
    ) -> FshResult<String> {
        let session_id = Uuid::new_v4().to_string();

        let session_info = SessionInfo {
            user_id,
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            client_ip,
        };

        self.sessions.insert(session_id.clone(), session_info);

        Ok(session_id)
    }

    pub fn validate_session(&mut self, session_id: &str) -> FshResult<bool> {
        let session = self.sessions.get_mut(session_id)
            .ok_or_else(|| FshError::SessionNotFound(session_id.to_string()))?;

        // Check if session is expired (24 hours)
        let session_timeout = Duration::from_secs(24 * 60 * 60);
        if SystemTime::now().duration_since(session.last_activity).unwrap_or(Duration::ZERO) > session_timeout {
            self.sessions.remove(session_id);
            return Err(FshError::SessionNotFound("Session expired".to_string()));
        }

        // Update last activity
        session.last_activity = SystemTime::now();

        Ok(true)
    }

    pub fn terminate_session(&mut self, session_id: &str) -> FshResult<()> {
        self.sessions.remove(session_id)
            .ok_or_else(|| FshError::SessionNotFound(session_id.to_string()))?;

        Ok(())
    }

    pub fn cleanup_expired_sessions(&mut self) -> usize {
        let session_timeout = Duration::from_secs(24 * 60 * 60);
        let now = SystemTime::now();

        let expired_sessions: Vec<String> = self.sessions
            .iter()
            .filter_map(|(id, session)| {
                if now.duration_since(session.last_activity).unwrap_or(Duration::ZERO) > session_timeout {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        let count = expired_sessions.len();
        for session_id in expired_sessions {
            self.sessions.remove(&session_id);
        }

        count
    }

    pub fn cleanup_expired_tokens(&mut self) -> usize {
        let now = SystemTime::now();

        let expired_tokens: Vec<String> = self.tokens
            .iter()
            .filter_map(|(id, token)| {
                if let Some(expires_at) = token.expires_at {
                    if now > expires_at {
                        return Some(id.clone());
                    }
                }
                None
            })
            .collect();

        let count = expired_tokens.len();
        for token_id in expired_tokens {
            self.tokens.remove(&token_id);
        }

        count
    }

    pub fn get_active_sessions(&self) -> Vec<&SessionInfo> {
        self.sessions.values().collect()
    }

    pub fn get_token_count(&self) -> usize {
        self.tokens.len()
    }

    pub fn get_session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn supports_auth_method(&self, method: &str) -> bool {
        self.auth_methods.contains(&method.to_string())
    }

    fn hash_token(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn generate_secure_token() -> String {
        // Generate a cryptographically secure random token
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 32] = rng.gen();
        hex::encode(bytes)
    }

    pub fn validate_password(&self, _username: &str, _password: &str) -> FshResult<()> {
        // TODO: Implement proper password validation
        // This would typically involve:
        // 1. Looking up user in database/config
        // 2. Verifying password hash
        // 3. Checking account status (enabled, locked, etc.)

        Err(FshError::ProtocolError("Password authentication not implemented".to_string()))
    }

    pub fn validate_credentials(&self, auth_type: &str, credentials: &HashMap<String, String>) -> FshResult<Vec<crate::protocol::Permission>> {
        match auth_type {
            "token" => {
                if let Some(token) = credentials.get("token") {
                    let token_info = self.validate_token(token)?;
                    Ok(token_info.permissions.clone())
                } else {
                    Err(FshError::AuthenticationFailed)
                }
            }
            "password" => {
                if let (Some(username), Some(password)) = (credentials.get("username"), credentials.get("password")) {
                    self.validate_password(username, password)?;
                    // Return default permissions for password auth
                    Ok(vec![
                        crate::protocol::Permission::Read,
                        crate::protocol::Permission::Write,
                        crate::protocol::Permission::Execute,
                    ])
                } else {
                    Err(FshError::AuthenticationFailed)
                }
            }
            _ => {
                Err(FshError::ProtocolError(format!("Unsupported auth method: {}", auth_type)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn create_test_config() -> SecurityConfig {
        SecurityConfig {
            require_authentication: true,
            auth_methods: vec!["token".to_string()],
            max_failed_attempts: 3,
            enable_logging: false,
            log_file: None,
        }
    }

    #[test]
    fn test_auth_manager_creation() {
        let config = create_test_config();
        let auth_manager = AuthManager::new(&config).unwrap();

        assert!(auth_manager.supports_auth_method("token"));
        assert!(!auth_manager.supports_auth_method("password"));
        assert_eq!(auth_manager.get_token_count(), 1); // Default token
    }

    #[test]
    fn test_token_operations() {
        let config = create_test_config();
        let mut auth_manager = AuthManager::new(&config).unwrap();

        // Create a token
        let token = "test-token-123";
        let token_id = auth_manager.create_token(
            token,
            None,
            vec![crate::protocol::Permission::Read],
            "Test token".to_string(),
        ).unwrap();

        // Validate the token
        let token_info = auth_manager.validate_token(token).unwrap();
        assert!(token_info.permissions.contains(&crate::protocol::Permission::Read));

        // Revoke the token
        auth_manager.revoke_token(&token_id).unwrap();

        // Token should no longer be valid
        assert!(auth_manager.validate_token(token).is_err());
    }

    #[test]
    fn test_session_operations() {
        let config = create_test_config();
        let mut auth_manager = AuthManager::new(&config).unwrap();

        let test_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        // Create a session
        let session_id = auth_manager.create_session("test_user".to_string(), test_ip).unwrap();

        // Validate the session
        let session = auth_manager.validate_session(&session_id).unwrap();
        assert_eq!(session.user_id, "test_user");
        assert_eq!(session.client_ip, test_ip);

        // Terminate the session
        auth_manager.terminate_session(&session_id).unwrap();

        // Session should no longer be valid
        assert!(auth_manager.validate_session(&session_id).is_err());
    }

    #[test]
    fn test_token_hashing() {
        let token1 = "test-token";
        let token2 = "test-token";
        let token3 = "different-token";

        let hash1 = AuthManager::hash_token(token1);
        let hash2 = AuthManager::hash_token(token2);
        let hash3 = AuthManager::hash_token(token3);

        assert_eq!(hash1, hash2); // Same tokens should produce same hash
        assert_ne!(hash1, hash3); // Different tokens should produce different hashes
    }

    #[test]
    fn test_credentials_validation() {
        let config = create_test_config();
        let auth_manager = AuthManager::new(&config).unwrap();

        // Test token authentication
        let mut credentials = HashMap::new();
        credentials.insert("token".to_string(), "default".to_string());

        let permissions = auth_manager.validate_credentials("token", &credentials).unwrap();
        assert!(!permissions.is_empty());

        // Test invalid token
        credentials.insert("token".to_string(), "invalid-token".to_string());
        assert!(auth_manager.validate_credentials("token", &credentials).is_err());

        // Test unsupported auth method
        assert!(auth_manager.validate_credentials("unsupported", &credentials).is_err());
    }
}