use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
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
        let identity = AgentIdentity { workspace_id, role };
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
        self.rng.fill(&mut bytes).expect("system random available");
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

    // ── Phase 18b.2: Auth + rate limiter coverage ──

    #[test]
    fn psk_human_wrong_token() {
        let psk = PskAuthenticator::new();
        let _token = psk.register_human(UserId::from("user-1"));
        let err = psk.authenticate_human("bogus").unwrap_err();
        assert!(matches!(err, AuthError::InvalidToken));
    }

    #[test]
    fn psk_human_revoke() {
        let psk = PskAuthenticator::new();
        let user = UserId::from("user-1");
        let token = psk.register_human(user);
        psk.revoke_human(&token);
        let err = psk.authenticate_human(&token).unwrap_err();
        assert!(matches!(err, AuthError::InvalidToken));
    }

    #[test]
    fn psk_multiple_agents_same_workspace() {
        let psk = PskAuthenticator::new();
        let ws = WorkspaceId::from("ws-1");
        let t1 = psk.register_agent(ws.clone(), "worker".into());
        let t2 = psk.register_agent(ws.clone(), "observer".into());

        let id1 = psk.authenticate_agent(&t1, &ws).unwrap();
        let id2 = psk.authenticate_agent(&t2, &ws).unwrap();
        assert_eq!(id1.role, "worker");
        assert_eq!(id2.role, "observer");
    }

    #[test]
    fn psk_revoke_one_workspace_keeps_others() {
        let psk = PskAuthenticator::new();
        let ws1 = WorkspaceId::from("ws-1");
        let ws2 = WorkspaceId::from("ws-2");
        let t1 = psk.register_agent(ws1.clone(), "worker".into());
        let t2 = psk.register_agent(ws2.clone(), "worker".into());

        psk.revoke_agent(&ws1);
        assert!(psk.authenticate_agent(&t1, &ws1).is_err());
        assert!(psk.authenticate_agent(&t2, &ws2).is_ok());
    }

    #[test]
    fn psk_tokens_are_unique() {
        let psk = PskAuthenticator::new();
        let tokens: Vec<_> = (0..10)
            .map(|i| psk.register_agent(WorkspaceId::from(format!("ws-{i}")), "worker".into()))
            .collect();
        let unique: std::collections::HashSet<_> = tokens.iter().collect();
        assert_eq!(unique.len(), tokens.len(), "all tokens must be unique");
    }

    #[test]
    fn rate_limiter_window_expiry() {
        // Use a very short window (1 second) and verify expired entries are cleared.
        let limiter = AuthRateLimiter::new(2, 1);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        limiter.record_failure(&ip);
        limiter.record_failure(&ip);
        assert!(limiter.check(&ip).is_err()); // at limit

        // Wait for window to expire.
        std::thread::sleep(Duration::from_millis(1100));

        // Should be allowed again after window.
        assert!(limiter.check(&ip).is_ok());
    }

    #[test]
    fn rate_limiter_under_limit_ok() {
        let limiter = AuthRateLimiter::new(5, 60);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        // Record fewer than limit.
        limiter.record_failure(&ip);
        limiter.record_failure(&ip);
        assert!(limiter.check(&ip).is_ok()); // 2 < 5
    }

    #[test]
    fn rate_limiter_record_disabled_noop() {
        let limiter = AuthRateLimiter::new(0, 60);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        // When disabled, record_failure doesn't track.
        for _ in 0..100 {
            limiter.record_failure(&ip);
        }
        assert!(limiter.check(&ip).is_ok());
        // Internal map should be empty since disabled.
        assert_eq!(limiter.failures.lock().len(), 0);
    }

    // ── Branch-coverage: AuthError variants ──

    #[test]
    fn auth_error_display_invalid_token() {
        let err = AuthError::InvalidToken;
        assert_eq!(format!("{err}"), "invalid token");
    }

    #[test]
    fn auth_error_display_workspace_mismatch() {
        let err = AuthError::WorkspaceMismatch;
        assert_eq!(format!("{err}"), "token valid but not for this workspace");
    }

    #[test]
    fn auth_error_display_provider_unavailable() {
        let err = AuthError::ProviderUnavailable;
        assert_eq!(format!("{err}"), "authentication provider unavailable");
    }

    #[test]
    fn auth_error_display_rate_limited() {
        let err = AuthError::RateLimited;
        assert_eq!(format!("{err}"), "rate limited");
    }

    #[test]
    fn auth_error_debug_all_variants() {
        // Ensure Debug is implemented for every variant.
        let _ = format!("{:?}", AuthError::InvalidToken);
        let _ = format!("{:?}", AuthError::WorkspaceMismatch);
        let _ = format!("{:?}", AuthError::ProviderUnavailable);
        let _ = format!("{:?}", AuthError::RateLimited);
    }

    // ── Branch-coverage: PskAuthenticator ──

    #[test]
    fn psk_default_creates_equivalent_instance() {
        let psk = PskAuthenticator::default();
        let ws = WorkspaceId::from("ws-default");
        let token = psk.register_agent(ws.clone(), "worker".into());
        let id = psk.authenticate_agent(&token, &ws).unwrap();
        assert_eq!(id.workspace_id, ws);
    }

    #[test]
    fn psk_empty_token_rejected() {
        let psk = PskAuthenticator::new();
        let ws = WorkspaceId::from("ws-1");
        let err = psk.authenticate_agent("", &ws).unwrap_err();
        assert!(matches!(err, AuthError::InvalidToken));
    }

    #[test]
    fn psk_human_empty_token_rejected() {
        let psk = PskAuthenticator::new();
        let err = psk.authenticate_human("").unwrap_err();
        assert!(matches!(err, AuthError::InvalidToken));
    }

    #[test]
    fn psk_revoke_nonexistent_workspace_is_noop() {
        let psk = PskAuthenticator::new();
        let ws = WorkspaceId::from("ws-1");
        let token = psk.register_agent(ws.clone(), "worker".into());
        // Revoke a workspace that was never registered.
        psk.revoke_agent(&WorkspaceId::from("ws-nonexistent"));
        // Original agent should still work.
        assert!(psk.authenticate_agent(&token, &ws).is_ok());
    }

    #[test]
    fn psk_revoke_human_nonexistent_token_is_noop() {
        let psk = PskAuthenticator::new();
        let user = UserId::from("user-1");
        let token = psk.register_human(user.clone());
        psk.revoke_human("never-issued-token");
        // Original human token should still work.
        assert_eq!(psk.authenticate_human(&token).unwrap(), user);
    }

    #[test]
    fn psk_multiple_agents_different_workspaces_cross_workspace_auth_fails() {
        let psk = PskAuthenticator::new();
        let ws1 = WorkspaceId::from("ws-1");
        let ws2 = WorkspaceId::from("ws-2");
        let t1 = psk.register_agent(ws1.clone(), "worker".into());
        let t2 = psk.register_agent(ws2.clone(), "observer".into());

        // Each agent succeeds against its own workspace.
        assert!(psk.authenticate_agent(&t1, &ws1).is_ok());
        assert!(psk.authenticate_agent(&t2, &ws2).is_ok());

        // Cross-workspace authentication fails with WorkspaceMismatch.
        assert!(matches!(
            psk.authenticate_agent(&t1, &ws2),
            Err(AuthError::WorkspaceMismatch)
        ));
        assert!(matches!(
            psk.authenticate_agent(&t2, &ws1),
            Err(AuthError::WorkspaceMismatch)
        ));
    }

    #[test]
    fn psk_revoke_then_reregister_same_workspace() {
        let psk = PskAuthenticator::new();
        let ws = WorkspaceId::from("ws-1");
        let t1 = psk.register_agent(ws.clone(), "worker".into());
        psk.revoke_agent(&ws);
        assert!(psk.authenticate_agent(&t1, &ws).is_err());

        // Re-register a new agent on the same workspace.
        let t2 = psk.register_agent(ws.clone(), "observer".into());
        assert_ne!(t1, t2);
        let id = psk.authenticate_agent(&t2, &ws).unwrap();
        assert_eq!(id.role, "observer");
        // Old token still fails.
        assert!(psk.authenticate_agent(&t1, &ws).is_err());
    }

    #[test]
    fn psk_agent_identity_clone() {
        let psk = PskAuthenticator::new();
        let ws = WorkspaceId::from("ws-1");
        let token = psk.register_agent(ws.clone(), "worker".into());
        let id1 = psk.authenticate_agent(&token, &ws).unwrap();
        let id2 = id1.clone();
        assert_eq!(id1.workspace_id, id2.workspace_id);
        assert_eq!(id1.role, id2.role);
    }

    // ── Branch-coverage: AuthRateLimiter ──

    #[test]
    fn rate_limiter_check_unknown_ip_always_ok() {
        let limiter = AuthRateLimiter::new(3, 60);
        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        // An IP with no failures should always pass.
        assert!(limiter.check(&ip).is_ok());
    }

    #[test]
    fn rate_limiter_exactly_at_limit_blocked() {
        let limiter = AuthRateLimiter::new(1, 60);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        limiter.record_failure(&ip);
        // Exactly 1 failure with max_failures=1 should block.
        assert!(matches!(limiter.check(&ip), Err(AuthError::RateLimited)));
    }

    #[test]
    fn rate_limiter_evicts_oldest_ip_at_capacity() {
        // Create a limiter, fill it to MAX_TRACKED_IPS, then add one more.
        // The oldest entry should be evicted.
        let limiter = AuthRateLimiter::new(100, 3600);

        // Fill with MAX_TRACKED_IPS distinct IPs.
        for i in 0..MAX_TRACKED_IPS {
            let a = ((i >> 24) & 0xFF) as u8;
            let b = ((i >> 16) & 0xFF) as u8;
            let c = ((i >> 8) & 0xFF) as u8;
            let d = (i & 0xFF) as u8;
            let ip: IpAddr = format!("{a}.{b}.{c}.{d}").parse().unwrap();
            limiter.record_failure(&ip);
        }

        assert_eq!(limiter.failures.lock().len(), MAX_TRACKED_IPS);

        // Add one more IP that exceeds the capacity.
        let extra_ip: IpAddr = "255.255.255.255".parse().unwrap();
        limiter.record_failure(&extra_ip);

        // The map should still have MAX_TRACKED_IPS entries (one evicted, one added).
        assert_eq!(limiter.failures.lock().len(), MAX_TRACKED_IPS);
        // The new IP should be present.
        assert!(limiter.failures.lock().contains_key(&extra_ip));
    }

    #[test]
    fn rate_limiter_existing_ip_not_evicted_at_capacity() {
        // When we record a failure for an IP already in the map, no eviction
        // occurs even if we are at capacity.
        let limiter = AuthRateLimiter::new(100, 3600);
        let target_ip: IpAddr = "1.2.3.4".parse().unwrap();
        limiter.record_failure(&target_ip);

        // Fill the rest.
        for i in 1..MAX_TRACKED_IPS {
            let a = ((i >> 24) & 0xFF) as u8;
            let b = ((i >> 16) & 0xFF) as u8;
            let c = ((i >> 8) & 0xFF) as u8;
            let d = (i & 0xFF) as u8;
            let ip: IpAddr = format!("{a}.{b}.{c}.{d}").parse().unwrap();
            limiter.record_failure(&ip);
        }

        assert_eq!(limiter.failures.lock().len(), MAX_TRACKED_IPS);

        // Record another failure for the existing IP.
        limiter.record_failure(&target_ip);
        // No eviction — still at capacity.
        assert_eq!(limiter.failures.lock().len(), MAX_TRACKED_IPS);
        // target_ip now has 2 failures.
        assert_eq!(limiter.failures.lock().get(&target_ip).unwrap().len(), 2);
    }

    #[test]
    fn rate_limiter_multiple_failures_then_partial_window_expiry() {
        // Record several failures, wait for a partial window, record more,
        // and verify only the in-window failures count.
        let limiter = AuthRateLimiter::new(3, 1);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        limiter.record_failure(&ip);
        limiter.record_failure(&ip);

        // Wait for the window to expire on the first two.
        std::thread::sleep(Duration::from_millis(1100));

        // Record one more failure (within a new window).
        limiter.record_failure(&ip);

        // Only 1 failure in the current window — should be allowed.
        assert!(limiter.check(&ip).is_ok());
    }

    #[test]
    fn rate_limiter_check_disabled_always_ok() {
        let limiter = AuthRateLimiter::new(0, 0);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(limiter.check(&ip).is_ok());
    }
}
