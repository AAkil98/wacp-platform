//! Authenticator — extracts user identity from cookie or bearer token.
//!
//! Spec: `wcon-architecture` §8.2, `wcon-auth` §3

use console_db::queries::{api_tokens, user_sessions, users};
use console_db::DbPool;
use sha2::{Digest, Sha256};

use crate::auth::{AuthenticatedUser, ConsoleRole};
use crate::error::ConsoleError;

/// Hash a token/session value with SHA-256 for database lookup.
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    result.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write;
        write!(s, "{b:02x}").unwrap();
        s
    })
}

/// Authenticate from a session cookie value (`wcon_sid`).
pub async fn authenticate_cookie(
    pool: &DbPool,
    cookie_value: &str,
) -> Result<AuthenticatedUser, ConsoleError> {
    let token_hash = hash_token(cookie_value);
    let now = chrono::Utc::now().to_rfc3339();

    let session = user_sessions::get_by_token_hash(pool, &token_hash, &now)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))?
        .ok_or(ConsoleError::Unauthenticated)?;

    let user = users::get_by_id(pool, &session.user_id)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))?
        .ok_or(ConsoleError::Unauthenticated)?;

    if user.disabled_at.is_some() {
        return Err(ConsoleError::Unauthenticated);
    }

    if user.must_change_password {
        return Err(ConsoleError::PasswordChangeRequired);
    }

    let role: ConsoleRole = user
        .console_role
        .parse()
        .map_err(|e: String| ConsoleError::Internal(e))?;

    Ok(AuthenticatedUser {
        user_id: user.id,
        username: user.username,
        console_role: role,
    })
}

/// Authenticate from a bearer token (`Authorization: Bearer wcon_t_...`).
pub async fn authenticate_bearer(
    pool: &DbPool,
    token: &str,
) -> Result<AuthenticatedUser, ConsoleError> {
    if !token.starts_with("wcon_t_") {
        return Err(ConsoleError::Unauthenticated);
    }

    let token_hash = hash_token(token);
    let now = chrono::Utc::now().to_rfc3339();

    let api_token = api_tokens::get_by_token_hash(pool, &token_hash, &now)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))?
        .ok_or(ConsoleError::Unauthenticated)?;

    // Update last_used_at (best-effort, don't fail auth on this)
    let _ = api_tokens::update_last_used(pool, &api_token.id, &now).await;

    let user = users::get_by_id(pool, &api_token.user_id)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))?
        .ok_or(ConsoleError::Unauthenticated)?;

    if user.disabled_at.is_some() {
        return Err(ConsoleError::Unauthenticated);
    }

    let role: ConsoleRole = user
        .console_role
        .parse()
        .map_err(|e: String| ConsoleError::Internal(e))?;

    Ok(AuthenticatedUser {
        user_id: user.id,
        username: user.username,
        console_role: role,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_token_is_deterministic() {
        let h1 = hash_token("test-token");
        let h2 = hash_token("test-token");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn hash_token_differs_for_different_inputs() {
        assert_ne!(hash_token("token-a"), hash_token("token-b"));
    }
}
