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
}
