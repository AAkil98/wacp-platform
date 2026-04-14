//! Highway action endpoints — gate resolve, escalation respond, directive inject.
//! Also cross-session pending queries.
//!
//! Spec: `wcon-highway` §4–§6, `wcon-api` §9

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::{Json, Router, routing::{get, post}};
use serde::Deserialize;
use std::sync::Arc;

use console_core::audit::{AuditAction, AuditEntry, log_audit};
use console_core::authorizer::{self, Action};
use console_core::error::ConsoleError;
use console_db::queries::sessions;

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::{Auth, RequestContext, is_bearer_auth, validate_csrf};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/sessions/{sid}/gates/{gid}", post(resolve_gate))
        .route(
            "/api/sessions/{sid}/gates/batch-resolve",
            post(batch_resolve_gates),
        )
        .route(
            "/api/sessions/{sid}/escalations/{eid}",
            post(respond_escalation),
        )
        .route("/api/sessions/{sid}/inject", post(inject_directive))
        // Cross-session pending queries
        .route("/api/gates/pending", get(pending_gates))
        .route("/api/escalations/pending", get(pending_escalations))
        .route("/api/refusals/pending", get(pending_refusals))
}

// --- Gate resolution ---

#[derive(Deserialize)]
#[allow(dead_code)]
struct GateResolveRequest {
    decision: String, // "approve" | "reject" | "modify"
    reason: Option<String>,
    modifications: Option<serde_json::Value>,
}

async fn resolve_gate(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path((sid, gid)): Path<(String, String)>,
    Json(body): Json<GateResolveRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let session = sessions::get_by_id(&state.db, &sid)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &sid))?;

    check_session_action_access(&auth, &session)?;

    // TODO: Forward to HighwayService.ResolveGate via gRPC when connected
    // For now, record the decision in the audit log.

    log_audit(&state.db, AuditEntry {
        user_id: auth.user_id.clone(),
        action: AuditAction::SessionGateApprove,
        target_id: gid,
        detail: Some(serde_json::json!({
            "session_id": sid,
            "decision": body.decision,
            "reason": body.reason,
        })),
        ip: ctx.ip,
        user_agent: ctx.user_agent,
    }).await.ok();

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --- Batch gate resolution ---

#[derive(Deserialize)]
struct BatchGateResolve {
    gates: Vec<BatchGateItem>,
}

#[derive(Deserialize)]
struct BatchGateItem {
    gate_id: String,
    decision: String,
    reason: Option<String>,
}

async fn batch_resolve_gates(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(sid): Path<String>,
    Json(body): Json<BatchGateResolve>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let session = sessions::get_by_id(&state.db, &sid)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &sid))?;

    check_session_action_access(&auth, &session)?;

    let mut resolved = Vec::new();
    let failed: Vec<String> = Vec::new();

    for item in &body.gates {
        // TODO: Forward each to HighwayService.ResolveGate
        // For now, record in audit log
        log_audit(&state.db, AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::SessionGateApprove,
            target_id: item.gate_id.clone(),
            detail: Some(serde_json::json!({
                "session_id": sid,
                "decision": item.decision,
                "reason": item.reason,
                "batch": true,
            })),
            ip: ctx.ip.clone(),
            user_agent: ctx.user_agent.clone(),
        }).await.ok();

        resolved.push(item.gate_id.clone());
    }

    Ok(Json(serde_json::json!({
        "resolved": resolved,
        "failed": failed,
    })))
}

// --- Escalation response ---

#[derive(Deserialize)]
#[allow(dead_code)]
struct EscalationResponse {
    response: String,
    context: Option<serde_json::Value>,
}

async fn respond_escalation(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path((sid, eid)): Path<(String, String)>,
    Json(body): Json<EscalationResponse>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let session = sessions::get_by_id(&state.db, &sid)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &sid))?;

    check_session_action_access(&auth, &session)?;

    log_audit(&state.db, AuditEntry {
        user_id: auth.user_id.clone(),
        action: AuditAction::SessionEscalationRespond,
        target_id: eid,
        detail: Some(serde_json::json!({
            "session_id": sid,
            "response": body.response,
        })),
        ip: ctx.ip,
        user_agent: ctx.user_agent,
    }).await.ok();

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --- Directive injection ---

#[derive(Deserialize)]
#[allow(dead_code)]
struct InjectDirectiveRequest {
    workspace_id: String,
    content: serde_json::Value,
}

async fn inject_directive(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(sid): Path<String>,
    Json(body): Json<InjectDirectiveRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let session = sessions::get_by_id(&state.db, &sid)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &sid))?;

    check_session_action_access(&auth, &session)?;

    if session.state != "active" {
        return Err(ApiError::from(ConsoleError::Conflict(
            "Directives can only be injected into active sessions".into(),
        )));
    }

    log_audit(&state.db, AuditEntry {
        user_id: auth.user_id.clone(),
        action: AuditAction::SessionInjectDirective,
        target_id: sid,
        detail: Some(serde_json::json!({
            "workspace_id": body.workspace_id,
        })),
        ip: ctx.ip,
        user_agent: ctx.user_agent,
    }).await.ok();

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --- Cross-session pending queries ---

#[derive(Deserialize)]
#[allow(dead_code)]
struct PendingParams {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 { 50 }

async fn pending_gates(
    State(_state): State<Arc<AppState>>,
    auth: Auth,
    Query(_params): Query<PendingParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::ApproveOwnGates).map_err(ApiError::from)?;

    // Pending gates are tracked in-memory by the session monitor.
    // The monitor broadcasts them via WebSocket. This endpoint returns
    // the current snapshot for polling fallback / initial load.
    // Full implementation deferred until session monitor is active.
    Ok(Json(serde_json::json!({ "items": [] })))
}

async fn pending_escalations(
    State(_state): State<Arc<AppState>>,
    auth: Auth,
    Query(_params): Query<PendingParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::HandleOwnEscalations).map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "items": [] })))
}

async fn pending_refusals(
    State(_state): State<Arc<AppState>>,
    auth: Auth,
    Query(_params): Query<PendingParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::ViewOwnOversight).map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "items": [] })))
}

// --- Helpers ---

fn check_session_action_access(
    auth: &Auth,
    session: &sessions::SessionRow,
) -> Result<(), ApiError> {
    if auth.console_role == console_core::ConsoleRole::Admin {
        return Ok(());
    }
    if session.owner_user_id == auth.user_id {
        return Ok(());
    }
    Err(ApiError::from(ConsoleError::Forbidden(
        "Only the session owner or an admin can perform actions on this session".into(),
    )))
}
