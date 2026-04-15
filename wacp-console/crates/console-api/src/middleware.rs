//! Auth middleware — authentication, CSRF, and request context extraction.
//!
//! Spec: `wcon-architecture` §8.1, §8.4, `wcon-auth` §8

use axum::extract::FromRequestParts;
use axum::http::header::{COOKIE, HeaderMap};
use axum::http::request::Parts;
use std::sync::Arc;
use subtle::ConstantTimeEq;

use console_core::auth::AuthenticatedUser;
use console_core::authenticator;
use console_core::error::ConsoleError;

use crate::AppState;
use crate::error::ApiError;

/// Axum extractor that authenticates the request.
/// Wraps `AuthenticatedUser` to satisfy orphan rules.
pub struct Auth(pub AuthenticatedUser);

impl std::ops::Deref for Auth {
    type Target = AuthenticatedUser;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<Arc<AppState>> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        // Try bearer token first (from Authorization header)
        if let Some(token) = parts
            .headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
        {
            return authenticator::authenticate_bearer(&state.db, token)
                .await
                .map(Auth)
                .map_err(ApiError::from);
        }

        // Try session cookie
        if let Some(cookie_value) = extract_cookie(&parts.headers, "wcon_sid") {
            return authenticator::authenticate_cookie(&state.db, &cookie_value)
                .await
                .map(Auth)
                .map_err(ApiError::from);
        }

        Err(ApiError::from(ConsoleError::Unauthenticated))
    }
}

/// Extract request metadata for audit logging.
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub ip: String,
    pub user_agent: String,
}

impl<S: Send + Sync> FromRequestParts<S> for RequestContext {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ip = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
            .or_else(|| {
                parts
                    .extensions
                    .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                    .map(|ci| ci.0.ip().to_string())
            })
            .unwrap_or_else(|| "unknown".into());

        let user_agent = parts
            .headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        Ok(RequestContext { ip, user_agent })
    }
}

/// Validate CSRF double-submit cookie on state-changing requests.
/// Only required for cookie-authenticated requests; bearer tokens are exempt.
///
/// Checks: `X-CSRF-Token` header matches `wcon_csrf` cookie value.
pub fn validate_csrf(headers: &HeaderMap, is_bearer_auth: bool) -> Result<(), ConsoleError> {
    if is_bearer_auth {
        return Ok(()); // Bearer tokens are CSRF-exempt
    }

    let cookie_value = extract_cookie(headers, "wcon_csrf").ok_or(ConsoleError::CsrfFailed)?;

    let header_value = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .ok_or(ConsoleError::CsrfFailed)?;

    // Constant-time comparison
    if cookie_value
        .as_bytes()
        .ct_eq(header_value.as_bytes())
        .into()
    {
        Ok(())
    } else {
        Err(ConsoleError::CsrfFailed)
    }
}

/// Check if the request uses bearer auth (not cookie).
pub fn is_bearer_auth(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("Bearer "))
}

/// Extract a named cookie from the Cookie header.
fn extract_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|pair| {
            let pair = pair.trim();
            let (k, v) = pair.split_once('=')?;
            if k.trim() == name {
                Some(v.trim().to_string())
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn extract_cookie_parses_correctly() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("wcon_sid=abc123; wcon_csrf=xyz789; other=val"),
        );
        assert_eq!(extract_cookie(&headers, "wcon_sid"), Some("abc123".into()));
        assert_eq!(extract_cookie(&headers, "wcon_csrf"), Some("xyz789".into()));
        assert_eq!(extract_cookie(&headers, "missing"), None);
    }

    #[test]
    fn csrf_passes_with_matching_values() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("wcon_csrf=token123"));
        headers.insert("x-csrf-token", HeaderValue::from_static("token123"));
        validate_csrf(&headers, false).unwrap();
    }

    #[test]
    fn csrf_fails_with_mismatch() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("wcon_csrf=token123"));
        headers.insert("x-csrf-token", HeaderValue::from_static("wrong"));
        assert!(validate_csrf(&headers, false).is_err());
    }

    #[test]
    fn csrf_skipped_for_bearer() {
        let headers = HeaderMap::new(); // No CSRF cookie/header
        validate_csrf(&headers, true).unwrap();
    }

    #[test]
    fn csrf_fails_when_missing() {
        let headers = HeaderMap::new();
        assert!(validate_csrf(&headers, false).is_err());
    }

    #[test]
    fn is_bearer_auth_detects_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer wcon_t_abc"),
        );
        assert!(is_bearer_auth(&headers));

        let empty = HeaderMap::new();
        assert!(!is_bearer_auth(&empty));
    }
}
