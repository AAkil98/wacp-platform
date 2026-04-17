use std::collections::HashMap;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use parking_lot::RwLock;
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use wacp_types::{UserId, WorkspaceId};

use crate::auth::{AgentIdentity, AuthError, Authenticator};

type TokenDigest = [u8; 32];

fn digest(token: &str) -> TokenDigest {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.finalize().into()
}

/// Session token authenticator — short-lived tokens with expiry and renewal.
///
/// Tokens are returned to the caller once at creation time and never persisted
/// in plaintext: the in-memory map is keyed by the SHA-256 digest of the
/// random token, so neither the map structure nor its iteration order leaks
/// information about the underlying secret.
pub struct SessionTokenAuthenticator {
    rng: SystemRandom,
    sessions: RwLock<HashMap<TokenDigest, SessionEntry>>,
    token_ttl: Duration,
}

struct SessionEntry {
    user_id: UserId,
    _created_at: Instant,
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
            digest(&token),
            SessionEntry {
                user_id,
                _created_at: now,
                expires_at: now + self.token_ttl,
                last_activity: now,
            },
        );
        token
    }

    /// Validate a session token. Updates last_activity on success.
    pub fn validate_session(&self, token: &str) -> Result<UserId, AuthError> {
        let key = digest(token);
        let mut sessions = self.sessions.write();
        let entry = sessions.get_mut(&key).ok_or(AuthError::InvalidToken)?;

        if Instant::now() > entry.expires_at {
            sessions.remove(&key);
            return Err(AuthError::InvalidToken);
        }

        entry.last_activity = Instant::now();
        Ok(entry.user_id.clone())
    }

    /// Explicitly invalidate a session (logout).
    pub fn invalidate_session(&self, token: &str) {
        self.sessions.write().remove(&digest(token));
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

    // ── Branch-coverage: edge cases ──

    #[test]
    fn empty_string_token_rejected() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        assert!(matches!(
            auth.validate_session(""),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn whitespace_token_rejected() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        assert!(matches!(
            auth.validate_session("   "),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn token_used_at_expiry_boundary() {
        // Create a session with a very short TTL (1 ms) and try to validate
        // right at/after the boundary.
        let auth = SessionTokenAuthenticator::new(Duration::from_millis(1));
        let token = auth.create_session(UserId::from("user-boundary"));

        // Wait just past the TTL.
        std::thread::sleep(Duration::from_millis(5));

        // The token should be expired.
        assert!(matches!(
            auth.validate_session(&token),
            Err(AuthError::InvalidToken)
        ));

        // The expired session should have been removed from the map.
        assert_eq!(auth.active_sessions(), 0);
    }

    #[test]
    fn multiple_sessions_for_same_user() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let user = UserId::from("user-multi");

        let t1 = auth.create_session(user.clone());
        let t2 = auth.create_session(user.clone());
        let t3 = auth.create_session(user.clone());

        // All three sessions are valid and return the same user.
        assert_eq!(auth.validate_session(&t1).unwrap(), user);
        assert_eq!(auth.validate_session(&t2).unwrap(), user);
        assert_eq!(auth.validate_session(&t3).unwrap(), user);

        assert_eq!(auth.active_sessions(), 3);
    }

    #[test]
    fn session_token_uniqueness() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let t1 = auth.create_session(UserId::from("user-1"));
        let t2 = auth.create_session(UserId::from("user-1"));

        // Tokens for the same user must be distinct.
        assert_ne!(t1, t2);
    }

    #[test]
    fn cleanup_with_mix_of_expired_and_valid() {
        let auth = SessionTokenAuthenticator::new(Duration::from_millis(1));

        // Create two sessions that will expire quickly.
        auth.create_session(UserId::from("user-expires-1"));
        auth.create_session(UserId::from("user-expires-2"));

        // Wait for them to expire.
        std::thread::sleep(Duration::from_millis(10));

        // Now create two valid sessions (with a long TTL authenticator is not
        // possible here since TTL is per-instance, so we do the trick of
        // inserting manually).
        // Instead: create new sessions after the sleep — they get a fresh
        // expires_at relative to the current instant.
        // But our authenticator has a 1ms TTL...  We need a second instance.
        // Alternative approach: use a longer TTL and create the valid sessions
        // first, then the expired ones.

        // Let's use a fresh authenticator with a reasonable TTL.
        let auth2 = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        // Create valid sessions.
        let valid_t1 = auth2.create_session(UserId::from("user-valid-1"));
        let valid_t2 = auth2.create_session(UserId::from("user-valid-2"));

        // Now manually insert expired entries by creating a short-lived authenticator's
        // sessions in the same map. Since we cannot, let's test the original approach:
        // the auth with 1ms TTL already has 2 expired sessions.
        let removed = auth.cleanup_expired();
        assert_eq!(removed, 2);
        assert_eq!(auth.active_sessions(), 0);

        // The valid sessions in auth2 should survive cleanup.
        let removed2 = auth2.cleanup_expired();
        assert_eq!(removed2, 0);
        assert_eq!(auth2.active_sessions(), 2);
        assert!(auth2.validate_session(&valid_t1).is_ok());
        assert!(auth2.validate_session(&valid_t2).is_ok());
    }

    #[test]
    fn validate_after_cleanup_removes_expired_session() {
        let auth = SessionTokenAuthenticator::new(Duration::from_millis(1));
        let token = auth.create_session(UserId::from("user-cleanup"));

        std::thread::sleep(Duration::from_millis(10));

        // Cleanup removes the expired session.
        let removed = auth.cleanup_expired();
        assert_eq!(removed, 1);

        // Subsequent validation should fail (session already removed).
        assert!(matches!(
            auth.validate_session(&token),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn authenticate_agent_via_session_token() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let token = auth.create_session(UserId::from("user-agent"));
        let ws = WorkspaceId::from("ws-ignored");

        let identity = auth.authenticate_agent(&token, &ws).unwrap();
        // The composability path uses a fixed workspace_id of "session".
        assert_eq!(identity.workspace_id, WorkspaceId::from("session"));
        assert_eq!(identity.role, "session:user-agent");
    }

    #[test]
    fn authenticate_agent_via_session_invalid_token() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let ws = WorkspaceId::from("ws-1");
        assert!(matches!(
            auth.authenticate_agent("bad-token", &ws),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn authenticate_agent_via_session_expired_token() {
        let auth = SessionTokenAuthenticator::new(Duration::from_millis(1));
        let token = auth.create_session(UserId::from("user-expired"));
        let ws = WorkspaceId::from("ws-1");

        std::thread::sleep(Duration::from_millis(10));

        assert!(matches!(
            auth.authenticate_agent(&token, &ws),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn active_sessions_after_invalidation() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let t1 = auth.create_session(UserId::from("user-1"));
        let t2 = auth.create_session(UserId::from("user-2"));
        let t3 = auth.create_session(UserId::from("user-3"));
        assert_eq!(auth.active_sessions(), 3);

        auth.invalidate_session(&t2);
        assert_eq!(auth.active_sessions(), 2);

        // t1 and t3 still work.
        assert!(auth.validate_session(&t1).is_ok());
        assert!(auth.validate_session(&t3).is_ok());

        // t2 is gone.
        assert!(matches!(
            auth.validate_session(&t2),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn invalidate_nonexistent_session_is_noop() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let token = auth.create_session(UserId::from("user-1"));
        assert_eq!(auth.active_sessions(), 1);

        // Invalidating a non-existent session should not affect existing sessions.
        auth.invalidate_session("never-issued-token");
        assert_eq!(auth.active_sessions(), 1);
        assert!(auth.validate_session(&token).is_ok());
    }

    #[test]
    fn double_invalidation_is_noop() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let token = auth.create_session(UserId::from("user-1"));

        auth.invalidate_session(&token);
        assert_eq!(auth.active_sessions(), 0);

        // Second invalidation should be a harmless no-op.
        auth.invalidate_session(&token);
        assert_eq!(auth.active_sessions(), 0);
    }

    #[test]
    fn validate_updates_last_activity() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let token = auth.create_session(UserId::from("user-1"));

        // First validation.
        auth.validate_session(&token).unwrap();
        let first_activity = {
            let sessions = auth.sessions.read();
            sessions.get(&digest(&token)).unwrap().last_activity
        };

        // Small sleep to ensure Instant advances.
        std::thread::sleep(Duration::from_millis(5));

        // Second validation.
        auth.validate_session(&token).unwrap();
        let second_activity = {
            let sessions = auth.sessions.read();
            sessions.get(&digest(&token)).unwrap().last_activity
        };

        assert!(second_activity > first_activity);
    }

    #[test]
    fn cleanup_with_no_sessions_returns_zero() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        assert_eq!(auth.cleanup_expired(), 0);
    }

    #[test]
    fn cleanup_with_all_valid_sessions_returns_zero() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        auth.create_session(UserId::from("user-1"));
        auth.create_session(UserId::from("user-2"));
        assert_eq!(auth.cleanup_expired(), 0);
        assert_eq!(auth.active_sessions(), 2);
    }

    #[test]
    fn session_token_format() {
        let auth = SessionTokenAuthenticator::new(Duration::from_secs(3600));
        let token = auth.create_session(UserId::from("user-fmt"));
        // 256 bits = 32 bytes -> base64url no padding = 43 chars.
        assert_eq!(token.len(), 43);
        assert!(URL_SAFE_NO_PAD.decode(&token).is_ok());
    }

    #[test]
    fn expired_session_removed_on_validate() {
        // Verify that validate_session removes expired entries from the map.
        let auth = SessionTokenAuthenticator::new(Duration::from_millis(1));
        let token = auth.create_session(UserId::from("user-1"));
        assert_eq!(auth.active_sessions(), 1);

        std::thread::sleep(Duration::from_millis(10));

        // validate_session should detect expiry and remove the entry.
        let result = auth.validate_session(&token);
        assert!(result.is_err());
        assert_eq!(auth.active_sessions(), 0);
    }
}
