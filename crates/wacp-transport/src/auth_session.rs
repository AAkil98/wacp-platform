use std::collections::HashMap;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use parking_lot::RwLock;
use ring::rand::{SecureRandom, SystemRandom};
use wacp_types::{UserId, WorkspaceId};

use crate::auth::{AgentIdentity, AuthError, Authenticator};

/// Session token authenticator — short-lived tokens with expiry and renewal.
pub struct SessionTokenAuthenticator {
    rng: SystemRandom,
    sessions: RwLock<HashMap<String, SessionEntry>>,
    token_ttl: Duration,
}

struct SessionEntry {
    user_id: UserId,
    created_at: Instant,
    expires_at: Instant,
    last_activity: Instant,
}

impl SessionTokenAuthenticator {
    /// Create with a token TTL (how long tokens live).
    pub fn new(token_ttl: Duration) -> Self {
        Self {
            rng: SystemRandom::new(),
            sessions: RwLock::new(HashMap::new()),
            token_ttl,
        }
    }

    /// Create a session for a user. Returns the session token.
    pub fn create_session(&self, user_id: UserId) -> String {
        let token = self.generate_token();
        let now = Instant::now();
        self.sessions.write().insert(
            token.clone(),
            SessionEntry {
                user_id,
                created_at: now,
                expires_at: now + self.token_ttl,
                last_activity: now,
            },
        );
        token
    }

    /// Validate a session token. Updates last_activity on success.
    pub fn validate_session(&self, token: &str) -> Result<UserId, AuthError> {
        let mut sessions = self.sessions.write();
        let entry = sessions.get_mut(token).ok_or(AuthError::InvalidToken)?;

        if Instant::now() > entry.expires_at {
            // Expired — remove and reject
            sessions.remove(token);
            return Err(AuthError::InvalidToken);
        }

        entry.last_activity = Instant::now();
        Ok(entry.user_id.clone())
    }

    /// Explicitly invalidate a session (logout).
    pub fn invalidate_session(&self, token: &str) {
        self.sessions.write().remove(token);
    }

    /// Remove all expired sessions.
    pub fn cleanup_expired(&self) -> usize {
        let now = Instant::now();
        let mut sessions = self.sessions.write();
        let before = sessions.len();
        sessions.retain(|_, entry| entry.expires_at > now);
        before - sessions.len()
    }

    /// Number of active sessions.
    pub fn active_sessions(&self) -> usize {
        self.sessions.read().len()
    }

    fn generate_token(&self) -> String {
        let mut bytes = [0u8; 32];
        self.rng.fill(&mut bytes).expect("RNG failure");
        URL_SAFE_NO_PAD.encode(bytes)
    }
}

impl Authenticator for SessionTokenAuthenticator {
    fn authenticate_agent(
        &self,
        token: &str,
        _workspace_id: &WorkspaceId,
    ) -> Result<AgentIdentity, AuthError> {
        // Session tokens are for humans, not agents.
        // However, we implement the trait for composability.
        let user_id = self.validate_session(token)?;
        Ok(AgentIdentity {
            workspace_id: WorkspaceId::from("session"),
            role: format!("session:{}", user_id),
        })
    }

    fn authenticate_human(&self, token: &str) -> Result<UserId, AuthError> {
        self.validate_session(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_validate_session() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let token = auth.create_session(UserId::from("user-1"));

        let result = auth.validate_session(&token);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), UserId::from("user-1"));
    }

    #[test]
    fn invalid_token_rejected() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let result = auth.validate_session("nonexistent-token");
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn expired_session_rejected() {
        let auth = SessionTokenAuthenticator::new(Duration::from_millis(1));
        let token = auth.create_session(UserId::from("user-1"));

        // Wait for expiry
        std::thread::sleep(Duration::from_millis(10));

        let result = auth.validate_session(&token);
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn invalidated_session_rejected() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let token = auth.create_session(UserId::from("user-1"));

        auth.invalidate_session(&token);

        let result = auth.validate_session(&token);
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn cleanup_expired_removes_old() {
        let auth = SessionTokenAuthenticator::new(Duration::from_millis(1));
        auth.create_session(UserId::from("user-1"));
        auth.create_session(UserId::from("user-2"));

        std::thread::sleep(Duration::from_millis(10));

        let removed = auth.cleanup_expired();
        assert_eq!(removed, 2);
        assert_eq!(auth.active_sessions(), 0);
    }

    #[test]
    fn active_sessions_count() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        assert_eq!(auth.active_sessions(), 0);

        auth.create_session(UserId::from("user-1"));
        auth.create_session(UserId::from("user-2"));
        assert_eq!(auth.active_sessions(), 2);
    }

    #[test]
    fn authenticate_human_uses_session() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let token = auth.create_session(UserId::from("user-1"));

        let result = auth.authenticate_human(&token);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), UserId::from("user-1"));
    }
}
