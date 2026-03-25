# Task 14.5: Authentication Providers

## Scope

Add the `Authenticator` trait, PSK provider (in-memory token table, random token generation, register/revoke), and auth rate limiter (sliding window per IP). The external HTTP provider is deferred to when `reqwest` is wired (it requires an async runtime and HTTP client — a heavyweight dep). For now, the external provider returns `ProviderUnavailable`.

This task adds `auth.rs` to `wacp-transport` with the trait, types, PSK implementation, and rate limiter. The transport gRPC services are NOT wired to the authenticator yet (that requires refactoring the service impls to hold an `Arc<dyn Authenticator>` — planned for Phase 17 e2e testing). The trait and implementations are fully tested standalone.

## Dependencies

- `ring` (new workspace dep, for `SystemRandom` token generation)
- `base64` (new workspace dep, for base64url encoding)
- `parking_lot` (new workspace dep, for efficient `RwLock`/`Mutex`)

## Types

### `Authenticator` trait

```rust
pub trait Authenticator: Send + Sync {
    fn authenticate_agent(&self, token: &str, workspace_id: &WorkspaceId) -> Result<AgentIdentity, AuthError>;
    fn authenticate_human(&self, token: &str) -> Result<UserId, AuthError>;
}
```

### `AgentIdentity`

```rust
pub struct AgentIdentity {
    pub workspace_id: WorkspaceId,
    pub role: String,
}
```

### `AuthError`

```rust
pub enum AuthError {
    InvalidToken,
    WorkspaceMismatch,
    ProviderUnavailable,
    RateLimited,
}
```

### `PskAuthenticator`

In-memory token table. `register_agent` generates a 256-bit random token (base64url, 43 chars). `revoke_agent` removes all tokens for a workspace.

### `AuthRateLimiter`

Sliding window per IP. `HashMap<IpAddr, VecDeque<Instant>>`. 10k entry cap. `check()` before auth, `record_failure()` after.

## Tests

| Test | Verifies |
|------|----------|
| `psk_register_and_auth` | Register agent, authenticate with correct token → success |
| `psk_wrong_token` | Authenticate with wrong token → InvalidToken |
| `psk_wrong_workspace` | Token valid but for different workspace → WorkspaceMismatch |
| `psk_revoke` | Revoke workspace, authenticate → InvalidToken |
| `psk_human_register_and_auth` | Register human, authenticate → success |
| `psk_token_format` | Generated token is 43 chars base64url |
| `rate_limiter_blocks_after_max` | max_failures exceeded → RateLimited |
| `rate_limiter_window_expiry` | Failures outside window don't count |
| `rate_limiter_disabled` | max_failures=0 → never rate limited |

## Acceptance Criteria

- `Authenticator` trait defined with agent + human methods.
- PSK provider generates cryptographically random tokens.
- PSK register/revoke/authenticate lifecycle works.
- Rate limiter blocks IPs exceeding failure threshold.
- All tests pass, clippy clean.
