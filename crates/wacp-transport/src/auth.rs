use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use parking_lot::{Mutex, RwLock};
use ring::rand::{SecureRandom, SystemRandom};
use wacp_types::{UserId, WorkspaceId};

// ── Types ──────────────────────────────────────────────────────────────────

/// Result of successful agent authentication.
#[derive(Debug, Clone)]
pub struct AgentIdentity {
    pub workspace_id: WorkspaceId,
    pub role: String,
}

/// Authentication errors.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid token")]
    InvalidToken,
    #[error("token valid but not for this workspace")]
    WorkspaceMismatch,
    #[error("authentication provider unavailable")]
    ProviderUnavailable,
    #[error("rate limited")]
    RateLimited,
}

/// Pluggable authenticator. The runtime holds one `Arc<dyn Authenticator>`.
pub trait Authenticator: Send + Sync {
    fn authenticate_agent(
        &self,
        token: &str,
        workspace_id: &WorkspaceId,
    ) -> Result<AgentIdentity, AuthError>;

    fn authenticate_human(&self, token: &str) -> Result<UserId, AuthError>;
}

// ── PSK Provider ───────────────────────────────────────────────────────────

/// Pre-shared key authenticator. In-memory token table.
pub struct PskAuthenticator {
    rng: SystemRandom,
    agent_tokens: RwLock<HashMap<String, AgentIdentity>>,
    human_tokens: RwLock<HashMap<String, UserId>>,
}

impl PskAuthenticator {
    pub fn new() -> Self {
        Self {
            rng: SystemRandom::new(),
            agent_tokens: RwLock::new(HashMap::new()),
            human_tokens: RwLock::new(HashMap::new()),
        }
    }

    /// Register an agent and return its token (256-bit random, base64url, 43 chars).
    pub fn register_agent(&self, workspace_id: WorkspaceId, role: String) -> String {
        let token = self.generate_token();
        let identity = AgentIdentity {
            workspace_id,
            role,
        };
        self.agent_tokens.write().insert(token.clone(), identity);
        token
    }

    /// Register a human and return their token.
    pub fn register_human(&self, user_id: UserId) -> String {
        let token = self.generate_token();
        self.human_tokens.write().insert(token.clone(), user_id);
        token
    }

    /// Revoke all tokens for a workspace (called on terminal state).
    pub fn revoke_agent(&self, workspace_id: &WorkspaceId) {
        self.agent_tokens
            .write()
            .retain(|_, id| &id.workspace_id != workspace_id);
    }

    /// Revoke a human token.
    pub fn revoke_human(&self, token: &str) {
        self.human_tokens.write().remove(token);
    }

    fn generate_token(&self) -> String {
        let mut bytes = [0u8; 32]; // 256 bits
        self.rng
            .fill(&mut bytes)
            .expect("system random available");
        URL_SAFE_NO_PAD.encode(bytes)
    }
}

impl Default for PskAuthenticator {
    fn default() -> Self {
        Self::new()
    }
}

impl Authenticator for PskAuthenticator {
    fn authenticate_agent(
        &self,
        token: &str,
        workspace_id: &WorkspaceId,
    ) -> Result<AgentIdentity, AuthError> {
        let tokens = self.agent_tokens.read();
        match tokens.get(token) {
            Some(identity) => {
                if &identity.workspace_id == workspace_id {
                    Ok(identity.clone())
                } else {
                    Err(AuthError::WorkspaceMismatch)
                }
            }
            None => Err(AuthError::InvalidToken),
        }
    }

    fn authenticate_human(&self, token: &str) -> Result<UserId, AuthError> {
        let tokens = self.human_tokens.read();
        match tokens.get(token) {
            Some(user_id) => Ok(user_id.clone()),
            None => Err(AuthError::InvalidToken),
        }
    }
}

// ── Rate Limiter ───────────────────────────────────────────────────────────

/// Sliding-window rate limiter per source IP.
pub struct AuthRateLimiter {
    failures: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    max_failures: u32,
    window: Duration,
}

const MAX_TRACKED_IPS: usize = 10_000;

impl AuthRateLimiter {
    /// Create a rate limiter. max_failures=0 disables rate limiting.
    pub fn new(max_failures: u32, window_seconds: u32) -> Self {
        Self {
            failures: Mutex::new(HashMap::new()),
            max_failures,
            window: Duration::from_secs(window_seconds as u64),
        }
    }

    /// Check if the IP is rate-limited. Returns Ok(()) if allowed, Err if blocked.
    pub fn check(&self, ip: &IpAddr) -> Result<(), AuthError> {
        if self.max_failures == 0 {
            return Ok(()); // rate limiting disabled
        }
        let mut map = self.failures.lock();
        if let Some(timestamps) = map.get_mut(ip) {
            let cutoff = Instant::now() - self.window;
            while timestamps.front().is_some_and(|t| *t < cutoff) {
                timestamps.pop_front();
            }
            if timestamps.len() >= self.max_failures as usize {
                return Err(AuthError::RateLimited);
            }
        }
        Ok(())
    }

    /// Record a failed authentication attempt from this IP.
    pub fn record_failure(&self, ip: &IpAddr) {
        if self.max_failures == 0 {
            return; // rate limiting disabled
        }
        let mut map = self.failures.lock();
        // Evict oldest entry if at capacity.
        if map.len() >= MAX_TRACKED_IPS
            && !map.contains_key(ip)
            && let Some(oldest_ip) = map
                .iter()
                .min_by_key(|(_, ts)| ts.back().copied().unwrap_or_else(Instant::now))
                .map(|(ip, _)| *ip)
        {
            map.remove(&oldest_ip);
        }
        map.entry(*ip).or_default().push_back(Instant::now());
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn psk_register_and_auth() {
        let psk = PskAuthenticator::new();
        let ws = WorkspaceId::from("ws-1");
        let token = psk.register_agent(ws.clone(), "worker".into());
        let identity = psk.authenticate_agent(&token, &ws).unwrap();
        assert_eq!(identity.workspace_id, ws);
        assert_eq!(identity.role, "worker");
    }

    #[test]
    fn psk_wrong_token() {
        let psk = PskAuthenticator::new();
        let ws = WorkspaceId::from("ws-1");
        let _token = psk.register_agent(ws.clone(), "worker".into());
        let err = psk.authenticate_agent("bogus-token", &ws).unwrap_err();
        assert!(matches!(err, AuthError::InvalidToken));
    }

    #[test]
    fn psk_wrong_workspace() {
        let psk = PskAuthenticator::new();
        let ws1 = WorkspaceId::from("ws-1");
        let ws2 = WorkspaceId::from("ws-2");
        let token = psk.register_agent(ws1, "worker".into());
        let err = psk.authenticate_agent(&token, &ws2).unwrap_err();
        assert!(matches!(err, AuthError::WorkspaceMismatch));
    }

    #[test]
    fn psk_revoke() {
        let psk = PskAuthenticator::new();
        let ws = WorkspaceId::from("ws-1");
        let token = psk.register_agent(ws.clone(), "worker".into());
        psk.revoke_agent(&ws);
        let err = psk.authenticate_agent(&token, &ws).unwrap_err();
        assert!(matches!(err, AuthError::InvalidToken));
    }

    #[test]
    fn psk_human_register_and_auth() {
        let psk = PskAuthenticator::new();
        let user = UserId::from("user-1");
        let token = psk.register_human(user.clone());
        let result = psk.authenticate_human(&token).unwrap();
        assert_eq!(result, user);
    }

    #[test]
    fn psk_token_format() {
        let psk = PskAuthenticator::new();
        let token = psk.register_agent(WorkspaceId::from("ws-1"), "worker".into());
        // 256 bits = 32 bytes → base64url no padding = 43 chars
        assert_eq!(token.len(), 43);
        // Must be valid base64url
        assert!(URL_SAFE_NO_PAD.decode(&token).is_ok());
    }

    #[test]
    fn rate_limiter_blocks_after_max() {
        let limiter = AuthRateLimiter::new(3, 60);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        assert!(limiter.check(&ip).is_ok());
        limiter.record_failure(&ip);
        limiter.record_failure(&ip);
        limiter.record_failure(&ip);
        // Now at limit
        let err = limiter.check(&ip).unwrap_err();
        assert!(matches!(err, AuthError::RateLimited));
    }

    #[test]
    fn rate_limiter_different_ips_independent() {
        let limiter = AuthRateLimiter::new(2, 60);
        let ip1: IpAddr = "10.0.0.1".parse().unwrap();
        let ip2: IpAddr = "10.0.0.2".parse().unwrap();

        limiter.record_failure(&ip1);
        limiter.record_failure(&ip1);
        // ip1 blocked, ip2 should be fine
        assert!(limiter.check(&ip1).is_err());
        assert!(limiter.check(&ip2).is_ok());
    }

    #[test]
    fn rate_limiter_disabled() {
        let limiter = AuthRateLimiter::new(0, 60);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        // Even after recording failures, never rate-limited
        limiter.record_failure(&ip);
        limiter.record_failure(&ip);
        assert!(limiter.check(&ip).is_ok());
    }
}
