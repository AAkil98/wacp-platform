//! Authenticator — extracts user identity from cookie or bearer token.
//!
//! Spec: `wcon-architecture` §8.2, `wcon-auth` §3

use console_db::DbPool;
use console_db::queries::{api_tokens, user_sessions, users};
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
        // write! to String is infallible
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Authenticate from a session cookie value (`wcon_sid`). Rejects users with
/// `must_change_password = true` via `PasswordChangeRequired` so most routes
/// get the right behaviour by default.
pub async fn authenticate_cookie(
    pool: &DbPool,
    cookie_value: &str,
) -> Result<AuthenticatedUser, ConsoleError> {
    let user = authenticate_cookie_allow_pending_change(pool, cookie_value).await?;
    // Re-fetch the flag — `authenticate_cookie_allow_pending_change` discards
    // it intentionally, but the non-pending path must enforce it here. This
    // is the second DB round-trip of the request and is kept short by
    // `users::get_by_id` being indexed.
    let user_row = users::get_by_id(pool, &user.user_id)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))?
        .ok_or(ConsoleError::Unauthenticated)?;
    if user_row.must_change_password {
        return Err(ConsoleError::PasswordChangeRequired);
    }
    Ok(user)
}

/// Same as `authenticate_cookie` but does NOT enforce the
/// `must_change_password` flag. Intended for the `POST /api/auth/change-
/// password` handler (and nowhere else) — a bootstrapped user must be able
/// to hit that endpoint to clear the flag, so enforcing it on every route
/// is a chicken-and-egg bug.
pub async fn authenticate_cookie_allow_pending_change(
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
