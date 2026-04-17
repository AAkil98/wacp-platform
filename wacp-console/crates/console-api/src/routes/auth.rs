//! Auth endpoints: login, logout, whoami, change-password.
//!
//! Spec: `wcon-api` §3.5, `wcon-auth` §3

use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::SET_COOKIE;
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;

use console_core::audit::{AuditAction, AuditEntry, log_audit};
use console_core::authenticator::hash_token;
use console_core::error::ConsoleError;
use console_core::password::{hash_password, validate_password_strength, verify_password};
use console_core::rate_limit::{check_login_rate_limit, record_login_attempt};
use console_db::queries::{user_sessions, users};

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::{
    Auth, AuthAllowPendingChange, RequestContext, is_bearer_auth, validate_csrf,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/whoami", get(whoami))
        .route("/api/auth/change-password", post(change_password))
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

async fn login(
    State(state): State<Arc<AppState>>,
    ctx: RequestContext,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Rate limiting
    check_login_rate_limit(&state.db, &ctx.ip, &body.username)
        .await
        .map_err(ApiError::from)?;

    // Find user
    let user = users::get_by_username(&state.db, &body.username)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    let Some(user) = user else {
        // Record failed attempt even if user doesn't exist (timing-safe)
        record_login_attempt(&state.db, &ctx.ip, &body.username, false)
            .await
            .ok();
        return Err(ApiError::from(ConsoleError::Unauthenticated));
    };

    if user.disabled_at.is_some() {
        record_login_attempt(&state.db, &ctx.ip, &body.username, false)
            .await
            .ok();
        return Err(ApiError::from(ConsoleError::Unauthenticated));
    }

    // Verify password
    let valid = verify_password(&body.password, &user.password_hash).map_err(ApiError::from)?;

    if !valid {
        record_login_attempt(&state.db, &ctx.ip, &body.username, false)
            .await
            .ok();
        return Err(ApiError::from(ConsoleError::Unauthenticated));
    }

    // Record success
    record_login_attempt(&state.db, &ctx.ip, &body.username, true)
        .await
        .ok();

    // Rotate old sessions for this user
    user_sessions::delete_user_sessions(&state.db, &user.id)
        .await
        .ok();

    // Create new session
    let session_token = generate_session_token();
    let token_hash = hash_token(&session_token);
    let csrf_token = generate_csrf_token();
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::hours(24);

    user_sessions::insert_session(
        &state.db,
        &session_id,
        &user.id,
        &token_hash,
        &ctx.ip,
        &ctx.user_agent,
        &now.to_rfc3339(),
        &expires.to_rfc3339(),
    )
    .await
    .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    // Audit
    log_audit(
        &state.db,
        AuditEntry {
            user_id: user.id.clone(),
            action: AuditAction::AuthLogin,
            target_id: user.id.clone(),
            detail: None,
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    // Set cookies
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        format!(
            "wcon_sid={session_token}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=86400"
        )
        .parse()
        .map_err(|_| {
            ApiError::from(ConsoleError::Internal(
                "invalid session cookie header".into(),
            ))
        })?,
    );
    headers.append(
        SET_COOKIE,
        format!("wcon_csrf={csrf_token}; Secure; SameSite=Strict; Path=/; Max-Age=86400")
            .parse()
            .map_err(|_| {
                ApiError::from(ConsoleError::Internal("invalid csrf cookie header".into()))
            })?,
    );

    let response_body = serde_json::json!({
        "user_id": user.id,
        "username": user.username,
        "console_role": user.console_role,
        "must_change_password": user.must_change_password,
    });

    Ok((headers, Json(response_body)))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    // Delete all sessions for this user
    user_sessions::delete_user_sessions(&state.db, &auth.user_id)
        .await
        .ok();

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::AuthLogout,
            target_id: auth.user_id.clone(),
            detail: None,
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        SET_COOKIE,
        "wcon_sid=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0"
            .parse()
            .map_err(|_| ApiError::from(ConsoleError::Internal("invalid cookie header".into())))?,
    );
    resp_headers.append(
        SET_COOKIE,
        "wcon_csrf=; Secure; SameSite=Strict; Path=/; Max-Age=0"
            .parse()
            .map_err(|_| ApiError::from(ConsoleError::Internal("invalid cookie header".into())))?,
    );

    Ok((resp_headers, axum::http::StatusCode::NO_CONTENT))
}

async fn whoami(auth: Auth) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "user_id": auth.user_id,
        "username": auth.username,
        "console_role": auth.console_role,
    }))
}

#[derive(Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<Arc<AppState>>,
    auth: AuthAllowPendingChange,
    ctx: RequestContext,
    headers: HeaderMap,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Bearer tokens are `AuthAllowPendingChange::Rejection`-ed upstream; CSRF
    // enforcement here is for cookie-auth'd state-changing requests only, and
    // that's what this handler accepts. `is_bearer_auth(&headers)` always
    // evaluates to `false` for this route — kept for symmetry with the other
    // state-changing handlers.
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    // Get user with password hash
    let user = users::get_by_id(&state.db, &auth.user_id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::from(ConsoleError::Unauthenticated))?;

    // Verify current password
    let valid =
        verify_password(&body.current_password, &user.password_hash).map_err(ApiError::from)?;
    if !valid {
        return Err(ApiError::from(ConsoleError::Unauthenticated));
    }

    // Validate new password strength
    validate_password_strength(&body.new_password).map_err(ApiError::from)?;

    // Hash and update
    let new_hash = hash_password(&body.new_password).map_err(ApiError::from)?;
    let now = chrono::Utc::now().to_rfc3339();
    users::update_password(&state.db, &auth.user_id, &new_hash, &now)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::AuthChangePassword,
            target_id: auth.user_id.clone(),
            detail: None,
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    Ok(axum::http::StatusCode::NO_CONTENT)
}

fn generate_session_token() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

fn generate_csrf_token() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}
