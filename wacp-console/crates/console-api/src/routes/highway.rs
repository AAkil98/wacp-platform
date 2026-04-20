//! Highway action endpoints — gate resolve, escalation respond, directive inject.
//! Also cross-session pending queries.
//!
//! Spec: `wcon-highway` §4–§6, `wcon-api` §9

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::{
    Json, Router,
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;

use console_core::audit::{AuditAction, AuditEntry, log_audit};
use console_core::authorizer::{self, Action};
use console_core::error::ConsoleError;
use console_db::queries::sessions;
use console_runtime::proto;

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
struct GateResolveRequest {
    decision: String, // "approve" | "reject" | "modify"
    reason: Option<String>,
    modifications: Option<serde_json::Value>,
}

/// Map the JSON `decision` string into the proto enum. Unknown values produce
/// a 400 — the runtime would also reject them, but failing fast keeps the
/// error message meaningful.
fn parse_gate_decision(s: &str) -> Result<proto::GateDecision, ApiError> {
    match s {
        "approve" => Ok(proto::GateDecision::Approve),
        "reject" => Ok(proto::GateDecision::Reject),
        "modify" => Ok(proto::GateDecision::Modify),
        other => Err(ApiError::bad_request(format!(
            "unknown gate decision '{other}' (expected approve|reject|modify)"
        ))),
    }
}

fn audit_action_for_decision(d: proto::GateDecision) -> AuditAction {
    match d {
        proto::GateDecision::Reject => AuditAction::SessionGateReject,
        _ => AuditAction::SessionGateApprove,
    }
}

async fn resolve_gate(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path((sid, gid)): Path<(String, String)>,
    Json(body): Json<GateResolveRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let session = sessions::get_by_id(&state.db, &sid)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &sid))?;

    check_session_action_access(&auth, &session)?;

    let decision = parse_gate_decision(&body.decision)?;
    let modifications = body
        .modifications
        .as_ref()
        .map(|v| serde_json::to_vec(v).unwrap_or_default())
        .unwrap_or_default();

    let mut client = state
        .grpc_pool
        .highway()
        .await
        .ok_or_else(|| ApiError::runtime_unavailable("HighwayService", "RespondToGate"))?;

    let req = proto::GateResponse {
        gate_id: gid.clone(),
        decision: decision as i32,
        modifications,
        client_request_id: String::new(),
    };
    let ack = client
        .respond_to_gate(req)
        .await
        .map_err(|s| ApiError::from_tonic(s, "HighwayService", "RespondToGate"))?
        .into_inner();

    // Audit only on a successful runtime response. The W3 monitor's stream
    // will eventually drop this gate from `pending`; pre-empt it so the
    // cross-session pending endpoint (W6) doesn't list a resolved gate.
    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: audit_action_for_decision(decision),
            target_id: gid.clone(),
            detail: Some(serde_json::json!({
                "session_id": sid,
                "decision": body.decision,
                "reason": body.reason,
                "runtime_applied": ack.applied,
            })),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    if let Some(handle) = state.active_sessions.read().await.get(&sid) {
        handle
            .pending
            .gates
            .write()
            .await
            .retain(|g| g.gate_id != gid);
    }

    Ok(Json(serde_json::json!({
        "gate_id": gid,
        "applied": ack.applied,
    })))
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

    let mut client = state
        .grpc_pool
        .highway()
        .await
        .ok_or_else(|| ApiError::runtime_unavailable("HighwayService", "RespondToGate"))?;

    // Per spec §3.3: sequential, partial-failure tolerant. Each item carries
    // its own outcome; the response always returns 200 with per-gate status
    // so the caller can act on partial results.
    let mut results = Vec::with_capacity(body.gates.len());
    let active_sessions = state.active_sessions.read().await;
    let monitor = active_sessions.get(&sid).cloned();
    drop(active_sessions);

    for item in &body.gates {
        let decision = match parse_gate_decision(&item.decision) {
            Ok(d) => d,
            Err(_) => {
                results.push(serde_json::json!({
                    "gate_id": item.gate_id,
                    "status": "bad_request",
                    "message": format!("unknown decision '{}'", item.decision),
                }));
                continue;
            }
        };
        let req = proto::GateResponse {
            gate_id: item.gate_id.clone(),
            decision: decision as i32,
            modifications: Vec::new(),
            client_request_id: String::new(),
        };
        match client.respond_to_gate(req).await {
            Ok(resp) => {
                let ack = resp.into_inner();
                log_audit(
                    &state.db,
                    AuditEntry {
                        user_id: auth.user_id.clone(),
                        action: audit_action_for_decision(decision),
                        target_id: item.gate_id.clone(),
                        detail: Some(serde_json::json!({
                            "session_id": sid,
                            "decision": item.decision,
                            "reason": item.reason,
                            "batch": true,
                            "runtime_applied": ack.applied,
                        })),
                        ip: ctx.ip.clone(),
                        user_agent: ctx.user_agent.clone(),
                    },
                )
                .await
                .ok();
                if let Some(h) = monitor.as_ref() {
                    h.pending
                        .gates
                        .write()
                        .await
                        .retain(|g| g.gate_id != item.gate_id);
                }
                results.push(serde_json::json!({
                    "gate_id": item.gate_id,
                    "status": "applied",
                    "applied": ack.applied,
                }));
            }
            Err(s) => {
                results.push(serde_json::json!({
                    "gate_id": item.gate_id,
                    "status": "runtime_rejected",
                    "code": format!("{:?}", s.code()),
                    "message": s.message(),
                }));
            }
        }
    }

    Ok(Json(serde_json::json!({ "results": results })))
}

// --- Escalation response ---

#[derive(Deserialize)]
struct EscalationResponse {
    /// One of `feedback` (carries `context` body), `abort`, `delegate`.
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
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let session = sessions::get_by_id(&state.db, &sid)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &sid))?;

    check_session_action_access(&auth, &session)?;

    let action = match body.response.as_str() {
        "abort" => Some(proto::escalation_response::Action::Abort(true)),
        "delegate" => Some(proto::escalation_response::Action::DelegateToCoordinator(
            true,
        )),
        "feedback" => {
            let payload = body
                .context
                .as_ref()
                .map(|v| serde_json::to_vec(v).unwrap_or_default())
                .unwrap_or_default();
            Some(proto::escalation_response::Action::Feedback(
                proto::Envelope {
                    id: String::new(),
                    from_workspace: String::new(),
                    to_workspace: String::new(),
                    r#type: "feedback".into(),
                    payload,
                    in_reply_to: eid.clone(),
                    timestamp: None,
                    priority: proto::EnvelopePriority::Normal as i32,
                    origin: proto::EnvelopeOrigin::Unspecified as i32,
                },
            ))
        }
        other => {
            return Err(ApiError::bad_request(format!(
                "unknown escalation response '{other}' (expected feedback|abort|delegate)"
            )));
        }
    };

    let mut client =
        state.grpc_pool.highway().await.ok_or_else(|| {
            ApiError::runtime_unavailable("HighwayService", "RespondToEscalation")
        })?;

    let req = proto::EscalationResponse {
        escalation_id: eid.clone(),
        action,
        client_request_id: String::new(),
    };
    let ack = client
        .respond_to_escalation(req)
        .await
        .map_err(|s| ApiError::from_tonic(s, "HighwayService", "RespondToEscalation"))?
        .into_inner();

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::SessionEscalationRespond,
            target_id: eid.clone(),
            detail: Some(serde_json::json!({
                "session_id": sid,
                "response": body.response,
                "runtime_applied": ack.applied,
            })),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    if let Some(handle) = state.active_sessions.read().await.get(&sid) {
        handle
            .pending
            .escalations
            .write()
            .await
            .retain(|e| e.escalation_id != eid);
    }

    Ok(Json(serde_json::json!({
        "escalation_id": eid,
        "applied": ack.applied,
    })))
}

// --- Directive injection ---

#[derive(Deserialize)]
struct InjectDirectiveRequest {
    workspace_id: String,
    content: serde_json::Value,
    /// Optional envelope type — defaults to `"directive"`.
    #[serde(default)]
    envelope_type: Option<String>,
}

async fn inject_directive(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(sid): Path<String>,
    Json(body): Json<InjectDirectiveRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let session = sessions::get_by_id(&state.db, &sid)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &sid))?;

    check_session_action_access(&auth, &session)?;

    if session.state != console_core::session_state::ACTIVE {
        return Err(ApiError::from(ConsoleError::Conflict(
            "Directives can only be injected into active sessions".into(),
        )));
    }

    let payload = serde_json::to_vec(&body.content).unwrap_or_default();
    let env_type = body
        .envelope_type
        .clone()
        .unwrap_or_else(|| "directive".into());

    let mut client = state
        .grpc_pool
        .highway()
        .await
        .ok_or_else(|| ApiError::runtime_unavailable("HighwayService", "InjectEnvelope"))?;

    let req = proto::InjectEnvelopeRequest {
        to_workspace: body.workspace_id.clone(),
        r#type: env_type,
        payload,
        priority: proto::EnvelopePriority::Normal as i32,
        client_request_id: String::new(),
    };
    let ack = client
        .inject_envelope(req)
        .await
        .map_err(|s| ApiError::from_tonic(s, "HighwayService", "InjectEnvelope"))?
        .into_inner();

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::SessionInjectDirective,
            target_id: sid,
            detail: Some(serde_json::json!({
                "workspace_id": body.workspace_id,
                "envelope_id": ack.envelope_id,
            })),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    Ok(Json(serde_json::json!({
        "envelope_id": ack.envelope_id,
    })))
}

// --- Cross-session pending queries (W6) ---

#[derive(Deserialize)]
pub(crate) struct PendingParams {
    /// Defaults match `pagination::PaginationParams`: limit clamped to
    /// [1, 200] downstream; missing → 50.
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) cursor: Option<String>,
    /// Optional explicit session scope. If the caller is not the owner (or
    /// admin), supplying a `session_id` they don't own returns 403 — see
    /// `pending::aggregate_*` for the rule.
    #[serde(default)]
    pub(crate) session_id: Option<String>,
}

async fn pending_gates(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<PendingParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::ApproveOwnGates).map_err(ApiError::from)?;
    pending::aggregate_gates(&state, &auth, params).await
}

async fn pending_escalations(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<PendingParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::HandleOwnEscalations).map_err(ApiError::from)?;
    pending::aggregate_escalations(&state, &auth, params).await
}

async fn pending_refusals(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<PendingParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::ViewOwnOversight).map_err(ApiError::from)?;
    pending::aggregate_refusals(&state, &auth, params).await
}

// ---------------------------------------------------------------------------
// W6 — pending aggregation
//
// Walks `AppState.active_sessions`, collects each monitor's `PendingState`
// for stable cursor-based pagination, and returns the page envelope.
// Implementation lives in sibling `highway_pending.rs` per
// `impl/archive/bucket-b-refactor-plan.md` §B.4 follow-up.
// ---------------------------------------------------------------------------
#[path = "highway_pending.rs"]
pub(crate) mod pending;

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

// ---------------------------------------------------------------------------
// Tests — W4 highway forwarding
//
// Cover the four endpoints' happy + failure paths against the configurable
// MockHighwayService. Auth is bearer (CSRF-exempt) so requests are minimal.
// Tests construct a real AppState with a GrpcPool dialed at the mock and
// assert (a) HTTP status codes, (b) audit-on-success-only, (c) optimistic
// pending mutation when a monitor handle is registered.
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "highway_tests.rs"]
mod tests;
