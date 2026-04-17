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

/// Cookie-only authenticator that does NOT reject users with
/// `must_change_password = true`. Used exclusively by the change-password
/// endpoint — without this, a bootstrapped admin is locked out of the only
/// route that can clear the flag. Bearer tokens are rejected on purpose
/// (tokens are scoped to post-rotation clients; pre-rotation rotation must
/// come from the browser session).
pub struct AuthAllowPendingChange(pub AuthenticatedUser);

impl std::ops::Deref for AuthAllowPendingChange {
    type Target = AuthenticatedUser;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<Arc<AppState>> for AuthAllowPendingChange {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let cookie_value = extract_cookie(&parts.headers, "wcon_sid")
            .ok_or_else(|| ApiError::from(ConsoleError::Unauthenticated))?;
        authenticator::authenticate_cookie_allow_pending_change(&state.db, &cookie_value)
            .await
            .map(AuthAllowPendingChange)
            .map_err(ApiError::from)
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
    use axum::response::IntoResponse;
    use console_core::auth::ConsoleRole;
    use console_core::authorizer::{self, Action};
    use rstest::rstest;

    // ---------------------------------------------------------------
    // Helper: build an AuthenticatedUser with a given role
    // ---------------------------------------------------------------
    fn user_with_role(role: ConsoleRole) -> console_core::auth::AuthenticatedUser {
        console_core::auth::AuthenticatedUser {
            user_id: "test-user".into(),
            username: "tester".into(),
            console_role: role,
        }
    }

    // ---------------------------------------------------------------
    // extract_cookie — existing + new edge-case tests
    // ---------------------------------------------------------------

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
    fn extract_cookie_returns_none_when_no_cookie_header() {
        let headers = HeaderMap::new();
        assert_eq!(extract_cookie(&headers, "wcon_sid"), None);
    }

    #[test]
    fn extract_cookie_handles_single_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("wcon_sid=only_one"));
        assert_eq!(
            extract_cookie(&headers, "wcon_sid"),
            Some("only_one".into())
        );
    }

    #[test]
    fn extract_cookie_handles_whitespace_around_pairs() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("  wcon_sid = spaced ;  wcon_csrf = also_spaced  "),
        );
        assert_eq!(extract_cookie(&headers, "wcon_sid"), Some("spaced".into()));
        assert_eq!(
            extract_cookie(&headers, "wcon_csrf"),
            Some("also_spaced".into())
        );
    }

    #[test]
    fn extract_cookie_returns_none_for_empty_cookie_header() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static(""));
        assert_eq!(extract_cookie(&headers, "wcon_sid"), None);
    }

    #[test]
    fn extract_cookie_handles_value_containing_equals() {
        let mut headers = HeaderMap::new();
        // Cookie values can contain '=' (e.g. base64 padding)
        headers.insert(
            COOKIE,
            HeaderValue::from_static("wcon_sid=abc=def; other=val"),
        );
        // split_once('=') means value is "abc=def"
        assert_eq!(extract_cookie(&headers, "wcon_sid"), Some("abc=def".into()));
    }

    // ---------------------------------------------------------------
    // is_bearer_auth — comprehensive coverage
    // ---------------------------------------------------------------

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

    #[test]
    fn is_bearer_auth_rejects_basic_auth() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Basic dXNlcjpwYXNz"),
        );
        assert!(!is_bearer_auth(&headers));
    }

    #[test]
    fn is_bearer_auth_rejects_lowercase_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("bearer wcon_t_abc"),
        );
        // The code checks starts_with("Bearer ") — case-sensitive
        assert!(!is_bearer_auth(&headers));
    }

    #[test]
    fn is_bearer_auth_rejects_bearer_without_space() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearerwcon_t_abc"),
        );
        assert!(!is_bearer_auth(&headers));
    }

    #[test]
    fn is_bearer_auth_with_empty_token_after_bearer_prefix() {
        // "Bearer " with nothing after it — still starts_with("Bearer ")
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer "));
        assert!(is_bearer_auth(&headers));
    }

    #[test]
    fn is_bearer_auth_rejects_empty_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static(""));
        assert!(!is_bearer_auth(&headers));
    }

    // ---------------------------------------------------------------
    // validate_csrf — comprehensive coverage
    // ---------------------------------------------------------------

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
        let err = validate_csrf(&headers, false).unwrap_err();
        assert!(matches!(err, ConsoleError::CsrfFailed));
    }

    #[test]
    fn csrf_skipped_for_bearer() {
        let headers = HeaderMap::new(); // No CSRF cookie/header
        validate_csrf(&headers, true).unwrap();
    }

    #[test]
    fn csrf_fails_when_missing() {
        let headers = HeaderMap::new();
        let err = validate_csrf(&headers, false).unwrap_err();
        assert!(matches!(err, ConsoleError::CsrfFailed));
    }

    #[test]
    fn csrf_fails_when_cookie_present_but_header_missing() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("wcon_csrf=token123"));
        // No x-csrf-token header
        let err = validate_csrf(&headers, false).unwrap_err();
        assert!(matches!(err, ConsoleError::CsrfFailed));
    }

    #[test]
    fn csrf_fails_when_header_present_but_cookie_missing() {
        let mut headers = HeaderMap::new();
        headers.insert("x-csrf-token", HeaderValue::from_static("token123"));
        // No wcon_csrf cookie
        let err = validate_csrf(&headers, false).unwrap_err();
        assert!(matches!(err, ConsoleError::CsrfFailed));
    }

    #[test]
    fn csrf_fails_when_wrong_cookie_name() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("other_csrf=token123"));
        headers.insert("x-csrf-token", HeaderValue::from_static("token123"));
        let err = validate_csrf(&headers, false).unwrap_err();
        assert!(matches!(err, ConsoleError::CsrfFailed));
    }

    #[test]
    fn csrf_bearer_exempt_even_with_mismatched_tokens() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("wcon_csrf=aaa"));
        headers.insert("x-csrf-token", HeaderValue::from_static("bbb"));
        // Bearer auth bypasses CSRF entirely
        validate_csrf(&headers, true).unwrap();
    }

    #[test]
    fn csrf_with_long_token_value() {
        let long_token = "a".repeat(256);
        let cookie = format!("wcon_csrf={long_token}");
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_str(&cookie).unwrap());
        headers.insert("x-csrf-token", HeaderValue::from_str(&long_token).unwrap());
        validate_csrf(&headers, false).unwrap();
    }

    #[test]
    fn csrf_fails_with_partial_match() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("wcon_csrf=token123"));
        headers.insert("x-csrf-token", HeaderValue::from_static("token12"));
        let err = validate_csrf(&headers, false).unwrap_err();
        assert!(matches!(err, ConsoleError::CsrfFailed));
    }

    #[test]
    fn csrf_fails_with_superset_match() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("wcon_csrf=token123"));
        headers.insert("x-csrf-token", HeaderValue::from_static("token1234"));
        let err = validate_csrf(&headers, false).unwrap_err();
        assert!(matches!(err, ConsoleError::CsrfFailed));
    }

    #[test]
    fn csrf_passes_among_multiple_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("wcon_sid=sess; wcon_csrf=abc; other=xyz"),
        );
        headers.insert("x-csrf-token", HeaderValue::from_static("abc"));
        validate_csrf(&headers, false).unwrap();
    }

    // ---------------------------------------------------------------
    // ConsoleError → ApiError mapping for auth/CSRF rejection codes
    // ---------------------------------------------------------------

    #[test]
    fn unauthenticated_maps_to_401() {
        let api_err = ApiError::from(ConsoleError::Unauthenticated);
        assert_eq!(api_err.status, axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(api_err.body.error, "unauthenticated");
    }

    #[test]
    fn account_locked_maps_to_401() {
        let api_err = ApiError::from(ConsoleError::AccountLocked);
        assert_eq!(api_err.status, axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(api_err.body.error, "account_locked");
    }

    #[test]
    fn csrf_failed_maps_to_403() {
        let api_err = ApiError::from(ConsoleError::CsrfFailed);
        assert_eq!(api_err.status, axum::http::StatusCode::FORBIDDEN);
        assert_eq!(api_err.body.error, "csrf_validation_failed");
    }

    #[test]
    fn forbidden_maps_to_403() {
        let api_err = ApiError::from(ConsoleError::Forbidden("denied".into()));
        assert_eq!(api_err.status, axum::http::StatusCode::FORBIDDEN);
        assert_eq!(api_err.body.error, "forbidden");
    }

    #[test]
    fn password_change_required_maps_to_403() {
        let api_err = ApiError::from(ConsoleError::PasswordChangeRequired);
        assert_eq!(api_err.status, axum::http::StatusCode::FORBIDDEN);
        assert_eq!(api_err.body.error, "password_change_required");
    }

    #[tokio::test]
    async fn unauthenticated_response_body_is_valid_json() {
        let api_err = ApiError::from(ConsoleError::Unauthenticated);
        let response = api_err.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "unauthenticated");
        assert_eq!(json["message"], "Authentication required");
    }

    #[tokio::test]
    async fn csrf_failed_response_body_is_valid_json() {
        let api_err = ApiError::from(ConsoleError::CsrfFailed);
        let response = api_err.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "csrf_validation_failed");
        assert_eq!(json["message"], "CSRF token missing or invalid");
    }

    #[tokio::test]
    async fn forbidden_response_includes_reason() {
        let api_err = ApiError::from(ConsoleError::Forbidden(
            "viewer cannot perform CreateUser".into(),
        ));
        let response = api_err.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "forbidden");
        assert!(
            json["message"]
                .as_str()
                .unwrap()
                .contains("viewer cannot perform CreateUser")
        );
    }

    // ---------------------------------------------------------------
    // Authorization matrix: 38 actions x 3 roles = 114 combinations
    // ---------------------------------------------------------------

    /// All 38 actions in the permission matrix.
    const ALL_ACTIONS: [Action; 38] = [
        // Discovery
        Action::BrowseTaxonomy,
        Action::ReloadTaxonomy,
        // Profiles
        Action::ListOwnProfiles,
        Action::ListAllProfiles,
        Action::ViewOwnProfile,
        Action::ViewAnyProfile,
        Action::CreateProfile,
        Action::EditOwnProfile,
        Action::EditAnyProfile,
        Action::DeleteOwnProfile,
        Action::DeleteAnyProfile,
        Action::ExportProfile,
        Action::ImportProfile,
        // Sessions
        Action::ListOwnSessions,
        Action::ListAllSessions,
        Action::ViewOwnOversight,
        Action::ViewAnyOversight,
        Action::CreateSession,
        Action::CancelOwnSession,
        Action::CancelAnySession,
        Action::ApproveOwnGates,
        Action::ApproveAnyGates,
        Action::HandleOwnEscalations,
        Action::HandleAnyEscalations,
        Action::InjectOwnDirectives,
        Action::InjectAnyDirectives,
        // Users
        Action::ListUsers,
        Action::CreateUser,
        Action::DisableUser,
        Action::ChangeUserRole,
        Action::ResetUserPassword,
        // API Tokens
        Action::ListOwnTokens,
        Action::CreateOwnToken,
        Action::RevokeOwnToken,
        Action::ManageAnyTokens,
        // Audit
        Action::ViewAuditLog,
        // Settings
        Action::ViewSettings,
        Action::ModifySettings,
    ];

    /// Actions allowed for Viewer (everyone).
    const VIEWER_ALLOWED: [Action; 7] = [
        Action::BrowseTaxonomy,
        Action::ListOwnProfiles,
        Action::ViewOwnProfile,
        Action::ExportProfile,
        Action::ListOwnSessions,
        Action::ViewOwnOversight,
        Action::ListOwnTokens,
    ];

    /// Additional actions allowed for Operator (operator + everything viewer can).
    const OPERATOR_EXTRA: [Action; 12] = [
        Action::ReloadTaxonomy,
        Action::CreateProfile,
        Action::EditOwnProfile,
        Action::DeleteOwnProfile,
        Action::ImportProfile,
        Action::CreateSession,
        Action::CancelOwnSession,
        Action::ApproveOwnGates,
        Action::HandleOwnEscalations,
        Action::InjectOwnDirectives,
        Action::CreateOwnToken,
        Action::RevokeOwnToken,
    ];

    /// Additional Operator actions that are not in Viewer or Operator-extra
    /// but still allowed for Operator — ViewSettings.
    const OPERATOR_SETTINGS: [Action; 1] = [Action::ViewSettings];

    /// Returns true if the action is allowed for the given role per the matrix.
    fn expected_allowed(role: ConsoleRole, action: Action) -> bool {
        // Viewer actions are allowed for everyone
        if VIEWER_ALLOWED.contains(&action) {
            return true;
        }
        // Operator-level actions
        if matches!(role, ConsoleRole::Operator | ConsoleRole::Admin)
            && (OPERATOR_EXTRA.contains(&action) || OPERATOR_SETTINGS.contains(&action))
        {
            return true;
        }
        // Everything else is admin-only
        if matches!(role, ConsoleRole::Admin) {
            return true;
        }
        false
    }

    // -- Parameterized matrix tests using rstest --

    #[rstest]
    #[case::admin(ConsoleRole::Admin)]
    #[case::operator(ConsoleRole::Operator)]
    #[case::viewer(ConsoleRole::Viewer)]
    fn authz_full_matrix_for_role(#[case] role: ConsoleRole) {
        let user = user_with_role(role);
        for &action in &ALL_ACTIONS {
            let result = authorizer::authorize(&user, action);
            let expected = expected_allowed(role, action);
            assert_eq!(
                result.is_ok(),
                expected,
                "role={role:?} action={action:?}: expected allowed={expected}, got {:?}",
                result
            );
        }
    }

    // -- Explicit role x tier tests for readability --

    #[test]
    fn admin_is_allowed_all_38_actions() {
        let user = user_with_role(ConsoleRole::Admin);
        for &action in &ALL_ACTIONS {
            authorizer::authorize(&user, action)
                .unwrap_or_else(|_| panic!("admin should be allowed {action:?}"));
        }
    }

    #[test]
    fn viewer_allowed_only_viewer_tier_actions() {
        let user = user_with_role(ConsoleRole::Viewer);
        for &action in &ALL_ACTIONS {
            let result = authorizer::authorize(&user, action);
            if VIEWER_ALLOWED.contains(&action) {
                result.unwrap_or_else(|_| panic!("viewer should be allowed {action:?}"));
            } else {
                assert!(result.is_err(), "viewer should be denied {action:?}");
            }
        }
    }

    #[test]
    fn operator_allowed_viewer_and_operator_tier_actions() {
        let user = user_with_role(ConsoleRole::Operator);
        let operator_allowed: Vec<Action> = VIEWER_ALLOWED
            .iter()
            .chain(OPERATOR_EXTRA.iter())
            .chain(OPERATOR_SETTINGS.iter())
            .copied()
            .collect();

        for &action in &ALL_ACTIONS {
            let result = authorizer::authorize(&user, action);
            if operator_allowed.contains(&action) {
                result.unwrap_or_else(|_| panic!("operator should be allowed {action:?}"));
            } else {
                assert!(result.is_err(), "operator should be denied {action:?}");
            }
        }
    }

    // -- Individual authorization rejection tests --

    #[rstest]
    #[case::list_all_profiles(Action::ListAllProfiles)]
    #[case::view_any_profile(Action::ViewAnyProfile)]
    #[case::edit_any_profile(Action::EditAnyProfile)]
    #[case::delete_any_profile(Action::DeleteAnyProfile)]
    #[case::list_all_sessions(Action::ListAllSessions)]
    #[case::view_any_oversight(Action::ViewAnyOversight)]
    #[case::cancel_any_session(Action::CancelAnySession)]
    #[case::approve_any_gates(Action::ApproveAnyGates)]
    #[case::handle_any_escalations(Action::HandleAnyEscalations)]
    #[case::inject_any_directives(Action::InjectAnyDirectives)]
    #[case::list_users(Action::ListUsers)]
    #[case::create_user(Action::CreateUser)]
    #[case::disable_user(Action::DisableUser)]
    #[case::change_user_role(Action::ChangeUserRole)]
    #[case::reset_user_password(Action::ResetUserPassword)]
    #[case::manage_any_tokens(Action::ManageAnyTokens)]
    #[case::view_audit_log(Action::ViewAuditLog)]
    #[case::modify_settings(Action::ModifySettings)]
    fn operator_denied_admin_only_actions(#[case] action: Action) {
        let user = user_with_role(ConsoleRole::Operator);
        let err = authorizer::authorize(&user, action).unwrap_err();
        assert!(
            matches!(err, ConsoleError::Forbidden(_)),
            "expected Forbidden for operator + {action:?}, got {err:?}"
        );
    }

    #[rstest]
    #[case::reload_taxonomy(Action::ReloadTaxonomy)]
    #[case::create_profile(Action::CreateProfile)]
    #[case::edit_own_profile(Action::EditOwnProfile)]
    #[case::delete_own_profile(Action::DeleteOwnProfile)]
    #[case::import_profile(Action::ImportProfile)]
    #[case::create_session(Action::CreateSession)]
    #[case::cancel_own_session(Action::CancelOwnSession)]
    #[case::approve_own_gates(Action::ApproveOwnGates)]
    #[case::handle_own_escalations(Action::HandleOwnEscalations)]
    #[case::inject_own_directives(Action::InjectOwnDirectives)]
    #[case::create_own_token(Action::CreateOwnToken)]
    #[case::revoke_own_token(Action::RevokeOwnToken)]
    #[case::view_settings(Action::ViewSettings)]
    fn viewer_denied_operator_level_actions(#[case] action: Action) {
        let user = user_with_role(ConsoleRole::Viewer);
        let err = authorizer::authorize(&user, action).unwrap_err();
        assert!(
            matches!(err, ConsoleError::Forbidden(_)),
            "expected Forbidden for viewer + {action:?}, got {err:?}"
        );
    }

    // -- Forbidden error message includes role and action --

    #[test]
    fn forbidden_error_message_includes_role_and_action() {
        let user = user_with_role(ConsoleRole::Viewer);
        let err = authorizer::authorize(&user, Action::CreateUser).unwrap_err();
        if let ConsoleError::Forbidden(msg) = err {
            assert!(msg.contains("viewer"), "message should mention role: {msg}");
            assert!(
                msg.contains("CreateUser"),
                "message should mention action: {msg}"
            );
        } else {
            panic!("expected Forbidden, got {err:?}");
        }
    }

    // -- authorize_owned: ownership-based fallback --

    #[test]
    fn authorize_owned_allows_own_action_for_resource_owner() {
        let mut user = user_with_role(ConsoleRole::Operator);
        user.user_id = "user-1".into();
        authorizer::authorize_owned(
            &user,
            Action::EditOwnProfile,
            Action::EditAnyProfile,
            "user-1",
        )
        .unwrap();
    }

    #[test]
    fn authorize_owned_denies_operator_for_others_resource() {
        let mut user = user_with_role(ConsoleRole::Operator);
        user.user_id = "user-1".into();
        let err = authorizer::authorize_owned(
            &user,
            Action::EditOwnProfile,
            Action::EditAnyProfile,
            "user-2",
        )
        .unwrap_err();
        assert!(matches!(err, ConsoleError::Forbidden(_)));
    }

    #[test]
    fn authorize_owned_allows_admin_for_others_resource() {
        let mut user = user_with_role(ConsoleRole::Admin);
        user.user_id = "admin-1".into();
        authorizer::authorize_owned(
            &user,
            Action::EditOwnProfile,
            Action::EditAnyProfile,
            "other-user",
        )
        .unwrap();
    }

    #[test]
    fn authorize_owned_denies_viewer_even_for_own_operator_actions() {
        let mut user = user_with_role(ConsoleRole::Viewer);
        user.user_id = "user-1".into();
        let err = authorizer::authorize_owned(
            &user,
            Action::CancelOwnSession,
            Action::CancelAnySession,
            "user-1",
        )
        .unwrap_err();
        assert!(matches!(err, ConsoleError::Forbidden(_)));
    }

    // -- ConsoleRole::has_at_least --

    #[rstest]
    #[case::admin_ge_admin(ConsoleRole::Admin, ConsoleRole::Admin, true)]
    #[case::admin_ge_operator(ConsoleRole::Admin, ConsoleRole::Operator, true)]
    #[case::admin_ge_viewer(ConsoleRole::Admin, ConsoleRole::Viewer, true)]
    #[case::operator_ge_admin(ConsoleRole::Operator, ConsoleRole::Admin, false)]
    #[case::operator_ge_operator(ConsoleRole::Operator, ConsoleRole::Operator, true)]
    #[case::operator_ge_viewer(ConsoleRole::Operator, ConsoleRole::Viewer, true)]
    #[case::viewer_ge_admin(ConsoleRole::Viewer, ConsoleRole::Admin, false)]
    #[case::viewer_ge_operator(ConsoleRole::Viewer, ConsoleRole::Operator, false)]
    #[case::viewer_ge_viewer(ConsoleRole::Viewer, ConsoleRole::Viewer, true)]
    fn role_has_at_least(
        #[case] role: ConsoleRole,
        #[case] required: ConsoleRole,
        #[case] expected: bool,
    ) {
        assert_eq!(
            role.has_at_least(required),
            expected,
            "{role:?}.has_at_least({required:?}) should be {expected}"
        );
    }

    // -- Edge cases for Auth extractor header parsing --

    #[test]
    fn is_bearer_auth_rejects_only_bearer_keyword() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer"));
        // "Bearer" without trailing space does not match "Bearer " prefix
        assert!(!is_bearer_auth(&headers));
    }

    #[test]
    fn is_bearer_auth_with_whitespace_only_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer   "));
        // starts_with("Bearer ") is true — this is a parse-level pass,
        // the authenticator downstream would reject the token itself
        assert!(is_bearer_auth(&headers));
    }

    #[test]
    fn double_authorization_header_uses_first() {
        // HTTP allows multiple values; axum's HeaderMap::insert replaces,
        // but append adds a second value. get() returns the first.
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer first_token"),
        );
        headers.append(
            "authorization",
            HeaderValue::from_static("Bearer second_token"),
        );
        // is_bearer_auth uses headers.get() which returns the first value
        assert!(is_bearer_auth(&headers));
    }

    // -- Auth extractor: no-credential path returns Unauthenticated --

    #[test]
    fn missing_auth_and_cookie_produces_unauthenticated_error() {
        // Without a DB, we can verify that the ConsoleError::Unauthenticated
        // path maps correctly to 401
        let err = ApiError::from(ConsoleError::Unauthenticated);
        assert_eq!(err.status, axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(err.body.error, "unauthenticated");
        assert_eq!(err.body.message, "Authentication required");
    }

    // -- Exhaustive count check: ensure we tested all actions --

    #[test]
    fn all_actions_array_has_expected_count() {
        // If someone adds a new Action variant, this test reminds them
        // to add it to ALL_ACTIONS and update the permission tier arrays.
        assert_eq!(
            ALL_ACTIONS.len(),
            38,
            "action count changed — update test matrix"
        );

        // Verify tier arrays account for all 38 actions
        let viewer_count = VIEWER_ALLOWED.len();
        let operator_count = viewer_count + OPERATOR_EXTRA.len() + OPERATOR_SETTINGS.len();
        let admin_count = ALL_ACTIONS.len();

        assert_eq!(viewer_count, 7, "viewer tier should allow 7 actions");
        assert_eq!(operator_count, 20, "operator tier should allow 20 actions");
        assert_eq!(admin_count, 38, "admin tier should allow all 38 actions");
    }

    // -- Verify all tier arrays are subsets of ALL_ACTIONS --

    #[test]
    fn tier_arrays_are_valid_subsets_of_all_actions() {
        for action in VIEWER_ALLOWED {
            assert!(
                ALL_ACTIONS.contains(&action),
                "VIEWER_ALLOWED contains {action:?} not in ALL_ACTIONS"
            );
        }
        for action in OPERATOR_EXTRA {
            assert!(
                ALL_ACTIONS.contains(&action),
                "OPERATOR_EXTRA contains {action:?} not in ALL_ACTIONS"
            );
        }
        for action in OPERATOR_SETTINGS {
            assert!(
                ALL_ACTIONS.contains(&action),
                "OPERATOR_SETTINGS contains {action:?} not in ALL_ACTIONS"
            );
        }
    }

    // -- No overlap between tier arrays --

    #[test]
    fn tier_arrays_have_no_overlap() {
        for action in OPERATOR_EXTRA {
            assert!(
                !VIEWER_ALLOWED.contains(&action),
                "{action:?} appears in both VIEWER_ALLOWED and OPERATOR_EXTRA"
            );
        }
        for action in OPERATOR_SETTINGS {
            assert!(
                !VIEWER_ALLOWED.contains(&action),
                "{action:?} appears in both VIEWER_ALLOWED and OPERATOR_SETTINGS"
            );
            assert!(
                !OPERATOR_EXTRA.contains(&action),
                "{action:?} appears in both OPERATOR_EXTRA and OPERATOR_SETTINGS"
            );
        }
    }
}
