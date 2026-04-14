//! API token endpoints.
//!
//! Spec: `wcon-api` §3.7, `wcon-auth` §3.3–3.4

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::{Json, Router, routing::{delete, get}};
use serde::Deserialize;
use std::sync::Arc;

use console_core::audit::{AuditAction, AuditEntry, log_audit};
use console_core::authenticator::hash_token;
use console_core::authorizer::{self, Action};
use console_core::error::ConsoleError;
use console_db::queries::api_tokens;

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::{Auth, RequestContext, is_bearer_auth, validate_csrf};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/tokens", get(list_tokens).post(create_token))
        .route("/api/tokens/{id}", delete(revoke_token))
}

async fn list_tokens(
    State(state): State<Arc<AppState>>,
    auth: Auth,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    authorizer::authorize(&auth, Action::ListOwnTokens).map_err(ApiError::from)?;

    let rows = api_tokens::list_by_user(&state.db, &auth.user_id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    let result: Vec<serde_json::Value> = rows.into_iter().map(|t| serde_json::json!({
        "id": t.id,
        "name": t.name,
        "created_at": t.created_at,
        "expires_at": t.expires_at,
        "last_used_at": t.last_used_at,
    })).collect();

    Ok(Json(result))
}

#[derive(Deserialize)]
struct CreateTokenRequest {
    name: String,
    expires_in_days: Option<i64>,
}

async fn create_token(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Json(body): Json<CreateTokenRequest>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    authorizer::authorize(&auth, Action::CreateOwnToken).map_err(ApiError::from)?;

    let raw_token = generate_api_token();
    let token_hash_value = hash_token(&raw_token);
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let expires_at = body.expires_in_days.map(|d| (now + chrono::Duration::days(d)).to_rfc3339());

    api_tokens::insert_token(
        &state.db, &id, &auth.user_id, &body.name,
        &token_hash_value, &now.to_rfc3339(), expires_at.as_deref(),
    )
    .await
    .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    log_audit(&state.db, AuditEntry {
        user_id: auth.user_id.clone(),
        action: AuditAction::TokenCreate,
        target_id: id.clone(),
        detail: Some(serde_json::json!({"name": body.name})),
        ip: ctx.ip,
        user_agent: ctx.user_agent,
    }).await.ok();

    // Return the raw token ONCE — it cannot be retrieved again
    Ok((axum::http::StatusCode::CREATED, Json(serde_json::json!({
        "id": id,
        "name": body.name,
        "token": raw_token,
        "expires_at": expires_at,
    }))))
}

async fn revoke_token(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    // Check ownership
    let token = api_tokens::get_by_id(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("token", &id))?;

    if token.user_id == auth.user_id {
        authorizer::authorize(&auth, Action::RevokeOwnToken).map_err(ApiError::from)?;
    } else {
        authorizer::authorize(&auth, Action::ManageAnyTokens).map_err(ApiError::from)?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    api_tokens::revoke_token(&state.db, &id, &now)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    log_audit(&state.db, AuditEntry {
        user_id: auth.user_id.clone(),
        action: AuditAction::TokenRevoke,
        target_id: id,
        detail: None,
        ip: ctx.ip,
        user_agent: ctx.user_agent,
    }).await.ok();

    Ok(axum::http::StatusCode::NO_CONTENT)
}

fn generate_api_token() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    format!(
        "wcon_t_{}",
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
    )
}
