//! Audit log endpoint — admin only.
//!
//! Spec: `wcon-api` §3.8

use axum::extract::{Query, State};
use axum::{Json, Router, routing::get};
use serde::Deserialize;
use std::sync::Arc;

use console_core::authorizer::{self, Action};
use console_core::error::ConsoleError;
use console_db::queries::audit_log;

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::Auth;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/audit-log", get(list_audit_log))
}

#[derive(Deserialize)]
struct AuditLogParams {
    user_id: Option<String>,
    action: Option<String>,
    target_kind: Option<String>,
    since: Option<String>,
    until: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    cursor: Option<String>,
}

fn default_limit() -> i64 { 50 }

async fn list_audit_log(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<AuditLogParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::ViewAuditLog).map_err(ApiError::from)?;

    let entries = audit_log::list_entries(
        &state.db,
        params.user_id.as_deref(),
        params.action.as_deref(),
        params.target_kind.as_deref(),
        params.since.as_deref(),
        params.until.as_deref(),
        params.limit,
        params.cursor.as_deref(),
    )
    .await
    .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    let items: Vec<serde_json::Value> = entries.into_iter().map(|e| serde_json::json!({
        "id": e.id,
        "user_id": e.user_id,
        "timestamp": e.timestamp,
        "action": e.action,
        "target_kind": e.target_kind,
        "target_id": e.target_id,
        "detail": e.detail.and_then(|d| serde_json::from_str::<serde_json::Value>(&d).ok()),
        "ip": e.ip,
        "user_agent": e.user_agent,
    })).collect();

    let next_cursor = items.last().map(|e| e["timestamp"].as_str().unwrap_or("").to_string());

    Ok(Json(serde_json::json!({
        "items": items,
        "next_cursor": next_cursor,
    })))
}
