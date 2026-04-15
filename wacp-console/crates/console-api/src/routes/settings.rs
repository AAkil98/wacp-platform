//! Settings endpoints.
//!
//! Spec: `wcon-data-model` §5.1–5.2, `wcon-api` §10

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::{Json, Router, routing::get};
use std::sync::Arc;

use console_core::audit::{AuditAction, AuditEntry, log_audit};
use console_core::authorizer::{self, Action};
use console_core::settings;

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::{Auth, RequestContext, is_bearer_auth, validate_csrf};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/settings", get(get_all_settings))
        .route(
            "/api/settings/{key}",
            get(get_setting).put(set_setting).delete(delete_setting),
        )
}

async fn get_all_settings(
    State(state): State<Arc<AppState>>,
    auth: Auth,
) -> Result<Json<Vec<settings::Setting>>, ApiError> {
    authorizer::authorize(&auth, Action::ViewSettings).map_err(ApiError::from)?;
    settings::get_all(&state.db)
        .await
        .map(Json)
        .map_err(ApiError::from)
}

async fn get_setting(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(key): Path<String>,
) -> Result<Json<settings::Setting>, ApiError> {
    authorizer::authorize(&auth, Action::ViewSettings).map_err(ApiError::from)?;
    settings::get(&state.db, &key)
        .await
        .map(Json)
        .map_err(ApiError::from)
}

async fn set_setting(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(key): Path<String>,
    Json(value): Json<serde_json::Value>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    authorizer::authorize(&auth, Action::ModifySettings).map_err(ApiError::from)?;

    settings::set(&state.db, &key, &value)
        .await
        .map_err(ApiError::from)?;

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::SettingsUpdate,
            target_id: key,
            detail: Some(value),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn delete_setting(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    authorizer::authorize(&auth, Action::ModifySettings).map_err(ApiError::from)?;

    settings::delete(&state.db, &key)
        .await
        .map_err(ApiError::from)?;

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::SettingsUpdate,
            target_id: key,
            detail: Some(serde_json::json!({"action": "reset_to_default"})),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    Ok(axum::http::StatusCode::NO_CONTENT)
}
