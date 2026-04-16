use std::collections::HashMap;
use std::time::Instant;

use parking_lot::RwLock;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use wacp_types::{UserId, WorkspaceId};

use crate::auth::{AgentIdentity, AuthError, AuthRateLimiter, Authenticator};

type TokenDigest = [u8; 32];

fn digest(token: &str) -> TokenDigest {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.finalize().into()
}

/// API key authenticator — long-lived keys from configuration.
///
/// Tokens are stored as SHA-256 digests, never as plaintext. The HashMap is
/// keyed by digest so the lookup itself runs over a hash of the token rather
/// than the token bytes — bucket-probe leakage carries no information about
/// the original token. The post-lookup workspace check uses a constant-time
/// byte comparison.
pub struct ApiKeyAuthenticator {
    agent_keys: RwLock<HashMap<TokenDigest, ApiKeyEntry>>,
    human_keys: RwLock<HashMap<TokenDigest, HumanKeyEntry>>,
    _rate_limiter: AuthRateLimiter,
}

struct ApiKeyEntry {
    identity: AgentIdentity,
    last_used: Option<Instant>,
}

struct HumanKeyEntry {
    user_id: UserId,
    last_used: Option<Instant>,
}

impl ApiKeyAuthenticator {
    /// Create with a rate limiter for failed attempts.
    pub fn new(max_failures: u32, window_seconds: u32) -> Self {
        Self {
            agent_keys: RwLock::new(HashMap::new()),
            human_keys: RwLock::new(HashMap::new()),
            _rate_limiter: AuthRateLimiter::new(max_failures, window_seconds),
        }
    }

    /// Register an agent API key.
    pub fn register_agent_key(
        &self,
        key: impl Into<String>,
        workspace_id: WorkspaceId,
        role: impl Into<String>,
    ) {
        let key = key.into();
        self.agent_keys.write().insert(
            digest(&key),
            ApiKeyEntry {
                identity: AgentIdentity {
                    workspace_id,
                    role: role.into(),
                },
                last_used: None,
            },
        );
    }

    /// Register a human API key.
    pub fn register_human_key(&self, key: impl Into<String>, user_id: UserId) {
        let key = key.into();
        self.human_keys.write().insert(
            digest(&key),
            HumanKeyEntry {
                user_id,
                last_used: None,
            },
        );
    }

    /// Revoke an agent key.
    pub fn revoke_agent_key(&self, key: &str) {
        self.agent_keys.write().remove(&digest(key));
    }

    /// Revoke a human key.
    pub fn revoke_human_key(&self, key: &str) {
        self.human_keys.write().remove(&digest(key));
    }
}

impl Authenticator for ApiKeyAuthenticator {
    fn authenticate_agent(
        &self,
        token: &str,
        workspace_id: &WorkspaceId,
    ) -> Result<AgentIdentity, AuthError> {
        let mut keys = self.agent_keys.write();
        if let Some(entry) = keys.get_mut(&digest(token)) {
            // Constant-time equality on workspace IDs: the lookup has already
            // confirmed the token is valid; this branch only protects against
            // leaking *which* workspace a known-good token is bound to via
            // timing of the rejection path.
            let bound: &str = entry.identity.workspace_id.as_ref();
            let probed: &str = workspace_id.as_ref();
            if bound.as_bytes().ct_eq(probed.as_bytes()).unwrap_u8() != 1 {
                return Err(AuthError::WorkspaceMismatch);
            }
            entry.last_used = Some(Instant::now());
            Ok(entry.identity.clone())
        } else {
            Err(AuthError::InvalidToken)
        }
    }

    fn authenticate_human(&self, token: &str) -> Result<UserId, AuthError> {
        let mut keys = self.human_keys.write();
        if let Some(entry) = keys.get_mut(&digest(token)) {
            entry.last_used = Some(Instant::now());
            Ok(entry.user_id.clone())
        } else {
            Err(AuthError::InvalidToken)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_agent_key_authenticates() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws = WorkspaceId::from("ws-1");
        auth.register_agent_key("key-abc123", ws.clone(), "worker");

        let result = auth.authenticate_agent("key-abc123", &ws);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().role, "worker");
    }

    #[test]
    fn invalid_agent_key_rejected() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws = WorkspaceId::from("ws-1");

        let result = auth.authenticate_agent("wrong-key", &ws);
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn agent_key_wrong_workspace() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws1 = WorkspaceId::from("ws-1");
        let ws2 = WorkspaceId::from("ws-2");
        auth.register_agent_key("key-abc123", ws1, "worker");

        let result = auth.authenticate_agent("key-abc123", &ws2);
        assert!(matches!(result, Err(AuthError::WorkspaceMismatch)));
    }

    #[test]
    fn agent_key_workspace_same_length_mismatch() {
        // Same-length workspace IDs exercise the constant-time path without
        // early-out shortcuts.
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws1 = WorkspaceId::from("ws-aaa");
        let ws2 = WorkspaceId::from("ws-aab");
        auth.register_agent_key("key-abc123", ws1, "worker");

        let result = auth.authenticate_agent("key-abc123", &ws2);
        assert!(matches!(result, Err(AuthError::WorkspaceMismatch)));
    }

    #[test]
    fn valid_human_key_authenticates() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        auth.register_human_key("human-key-xyz", UserId::from("user-1"));

        let result = auth.authenticate_human("human-key-xyz");
        assert!(result.is_ok());
    }

    #[test]
    fn revoked_key_rejected() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws = WorkspaceId::from("ws-1");
        auth.register_agent_key("key-to-revoke", ws.clone(), "worker");
        auth.revoke_agent_key("key-to-revoke");

        let result = auth.authenticate_agent("key-to-revoke", &ws);
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    // ── Branch-coverage: edge cases ──

    #[test]
    fn empty_string_token_rejected() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws = WorkspaceId::from("ws-1");
        assert!(matches!(
            auth.authenticate_agent("", &ws),
            Err(AuthError::InvalidToken)
        ));
        assert!(matches!(
            auth.authenticate_human(""),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn whitespace_only_token_rejected() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws = WorkspaceId::from("ws-1");
        assert!(matches!(
            auth.authenticate_agent("   ", &ws),
            Err(AuthError::InvalidToken)
        ));
        assert!(matches!(
            auth.authenticate_agent("\t\n", &ws),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn very_long_token_rejected() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws = WorkspaceId::from("ws-1");
        let long_key = "x".repeat(2048);
        // Not registered, so it should fail.
        assert!(matches!(
            auth.authenticate_agent(&long_key, &ws),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn very_long_token_registered_and_authenticated() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws = WorkspaceId::from("ws-1");
        let long_key = "k".repeat(2048);
        auth.register_agent_key(long_key.clone(), ws.clone(), "worker");
        let id = auth.authenticate_agent(&long_key, &ws).unwrap();
        assert_eq!(id.role, "worker");
    }

    #[test]
    fn register_same_key_twice_overwrites() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws1 = WorkspaceId::from("ws-1");
        let ws2 = WorkspaceId::from("ws-2");

        auth.register_agent_key("shared-key", ws1.clone(), "first-role");
        auth.register_agent_key("shared-key", ws2.clone(), "second-role");

        // The second registration should overwrite the first.
        let id = auth.authenticate_agent("shared-key", &ws2).unwrap();
        assert_eq!(id.role, "second-role");
        assert_eq!(id.workspace_id, ws2);

        // The old workspace should fail with WorkspaceMismatch (not InvalidToken).
        assert!(matches!(
            auth.authenticate_agent("shared-key", &ws1),
            Err(AuthError::WorkspaceMismatch)
        ));
    }

    #[test]
    fn revoke_nonexistent_key_is_noop() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws = WorkspaceId::from("ws-1");
        auth.register_agent_key("real-key", ws.clone(), "worker");

        // Revoking a key that was never registered should not affect existing keys.
        auth.revoke_agent_key("never-registered");
        assert!(auth.authenticate_agent("real-key", &ws).is_ok());
    }

    #[test]
    fn revoke_then_reregister_same_key() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws = WorkspaceId::from("ws-1");

        auth.register_agent_key("recycle-key", ws.clone(), "worker");
        auth.revoke_agent_key("recycle-key");
        assert!(matches!(
            auth.authenticate_agent("recycle-key", &ws),
            Err(AuthError::InvalidToken)
        ));

        // Re-register the same key with a different role.
        auth.register_agent_key("recycle-key", ws.clone(), "observer");
        let id = auth.authenticate_agent("recycle-key", &ws).unwrap();
        assert_eq!(id.role, "observer");
    }

    #[test]
    fn multiple_agents_different_workspaces() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws1 = WorkspaceId::from("ws-alpha");
        let ws2 = WorkspaceId::from("ws-beta");
        let ws3 = WorkspaceId::from("ws-gamma");

        auth.register_agent_key("key-a", ws1.clone(), "worker");
        auth.register_agent_key("key-b", ws2.clone(), "swe");
        auth.register_agent_key("key-c", ws3.clone(), "observer");

        // Each key works against its own workspace.
        assert_eq!(
            auth.authenticate_agent("key-a", &ws1).unwrap().workspace_id,
            ws1
        );
        assert_eq!(
            auth.authenticate_agent("key-b", &ws2).unwrap().workspace_id,
            ws2
        );
        assert_eq!(
            auth.authenticate_agent("key-c", &ws3).unwrap().workspace_id,
            ws3
        );

        // Cross-workspace fails.
        assert!(matches!(
            auth.authenticate_agent("key-a", &ws2),
            Err(AuthError::WorkspaceMismatch)
        ));
    }

    #[test]
    fn human_key_success_and_failure() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        auth.register_human_key("hk-valid", UserId::from("user-42"));

        let user = auth.authenticate_human("hk-valid").unwrap();
        assert_eq!(user, UserId::from("user-42"));

        // Wrong key fails.
        assert!(matches!(
            auth.authenticate_human("hk-wrong"),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn revoked_human_key_rejected() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        auth.register_human_key("hk-revoke", UserId::from("user-1"));

        // Authenticate before revocation succeeds.
        assert!(auth.authenticate_human("hk-revoke").is_ok());

        auth.revoke_human_key("hk-revoke");

        // After revocation, authentication fails.
        assert!(matches!(
            auth.authenticate_human("hk-revoke"),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn revoke_nonexistent_human_key_is_noop() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        auth.register_human_key("hk-real", UserId::from("user-1"));
        auth.revoke_human_key("hk-never-registered");
        // Existing key still works.
        assert!(auth.authenticate_human("hk-real").is_ok());
    }

    #[test]
    fn authenticate_agent_updates_last_used() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws = WorkspaceId::from("ws-1");
        auth.register_agent_key("ts-key", ws.clone(), "worker");

        // Before auth, last_used should be None.
        {
            let keys = auth.agent_keys.read();
            let entry = keys.get(&digest("ts-key")).unwrap();
            assert!(entry.last_used.is_none());
        }

        auth.authenticate_agent("ts-key", &ws).unwrap();

        // After auth, last_used should be Some.
        {
            let keys = auth.agent_keys.read();
            let entry = keys.get(&digest("ts-key")).unwrap();
            assert!(entry.last_used.is_some());
        }
    }

    #[test]
    fn authenticate_human_updates_last_used() {
        let auth = ApiKeyAuthenticator::new(5, 60);
        auth.register_human_key("hk-ts", UserId::from("user-1"));

        // Before auth, last_used should be None.
        {
            let keys = auth.human_keys.read();
            let entry = keys.get(&digest("hk-ts")).unwrap();
            assert!(entry.last_used.is_none());
        }

        auth.authenticate_human("hk-ts").unwrap();

        // After auth, last_used should be Some.
        {
            let keys = auth.human_keys.read();
            let entry = keys.get(&digest("hk-ts")).unwrap();
            assert!(entry.last_used.is_some());
        }
    }

    #[test]
    fn digest_is_deterministic() {
        // Same input always produces the same digest.
        let d1 = digest("test-token");
        let d2 = digest("test-token");
        assert_eq!(d1, d2);
    }

    #[test]
    fn digest_different_inputs_differ() {
        let d1 = digest("token-a");
        let d2 = digest("token-b");
        assert_ne!(d1, d2);
    }

    #[test]
    fn constant_time_eq_different_lengths() {
        // Verify the ct_eq path handles workspace IDs of different lengths.
        let auth = ApiKeyAuthenticator::new(5, 60);
        let ws_short = WorkspaceId::from("ws");
        let ws_long = WorkspaceId::from("ws-very-long-workspace-id");
        auth.register_agent_key("key-len", ws_short, "worker");

        let result = auth.authenticate_agent("key-len", &ws_long);
        assert!(matches!(result, Err(AuthError::WorkspaceMismatch)));
    }

    #[test]
    fn concurrent_register_and_authenticate() {
        use std::sync::Arc;

        let auth = Arc::new(ApiKeyAuthenticator::new(5, 60));
        let ws = WorkspaceId::from("ws-concurrent");

        // Pre-register a key so the auth thread can find it.
        auth.register_agent_key("concurrent-key", ws.clone(), "worker");

        let auth_clone = Arc::clone(&auth);
        let ws_clone = ws.clone();
        let handle = std::thread::spawn(move || {
            for i in 0..100 {
                auth_clone.register_agent_key(
                    format!("extra-key-{i}"),
                    ws_clone.clone(),
                    "worker",
                );
            }
        });

        // Authenticate concurrently.
        for _ in 0..100 {
            // The pre-registered key may or may not be blocked by the writer,
            // but should never panic.
            let _ = auth.authenticate_agent("concurrent-key", &ws);
        }

        handle.join().unwrap();
        // After the thread joins, the key should still be valid.
        assert!(auth.authenticate_agent("concurrent-key", &ws).is_ok());
    }
}
