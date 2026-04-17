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
// snapshot, applies ownership filtering, sorts by `(session_id, item_id)`
// for stable cursor-based pagination, and returns the page envelope.
// ---------------------------------------------------------------------------
pub(crate) mod pending {
    use super::*;
    use crate::pagination::{PaginationParams, encode_cursor};
    use console_core::ConsoleRole;
    use console_core::event_enricher::{EnrichedEscalation, EnrichedGate};
    use console_core::refusal_synthesizer::Refusal;
    use console_db::queries::sessions as session_queries;
    use serde::Serialize;

    /// Page envelope returned by all three pending endpoints. Mirrors
    /// `pagination::PaginatedResponse` but adds an explicit `session_id` per
    /// item so frontends can group / link without an extra lookup.
    #[derive(Debug, Serialize)]
    pub struct PendingPage<T: Serialize + Clone + std::fmt::Debug> {
        pub items: Vec<PendingItem<T>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub cursor: Option<String>,
        pub has_more: bool,
    }

    #[derive(Debug, Serialize, Clone)]
    pub struct PendingItem<T: Serialize + Clone + std::fmt::Debug> {
        pub session_id: String,
        #[serde(flatten)]
        pub inner: T,
    }

    /// Sort key derived from `(session_id, item_id)`. Cursor is a base64 of
    /// this string (encoded once for the page tail).
    fn sort_key(session_id: &str, item_id: &str) -> String {
        format!("{session_id}\x1f{item_id}")
    }

    /// Resolve the caller's ownership scope: which session ids may they see?
    /// - Admin → unrestricted (returns None).
    /// - Otherwise → the set of session ids they own (looked up once via DB).
    async fn owned_sessions(
        state: &AppState,
        auth: &Auth,
    ) -> Result<Option<std::collections::HashSet<String>>, ApiError> {
        if auth.console_role == ConsoleRole::Admin {
            return Ok(None);
        }
        let rows = session_queries::list_by_owner(&state.db, &auth.user_id, None, 1000, None)
            .await
            .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;
        Ok(Some(rows.into_iter().map(|r| r.id).collect()))
    }

    /// Validate an explicit `session_id` filter against the caller's scope.
    /// Non-admin asking for a session they don't own → 403, by spec §4.2
    /// ("don't silently return empty — make the authz failure explicit").
    fn enforce_session_scope(
        owned: &Option<std::collections::HashSet<String>>,
        sid: &str,
    ) -> Result<(), ApiError> {
        match owned {
            None => Ok(()),
            Some(set) if set.contains(sid) => Ok(()),
            Some(_) => Err(ApiError::from(ConsoleError::Forbidden(
                "Cannot view pending items for a session you do not own".into(),
            ))),
        }
    }

    fn build_page<T: Serialize + Clone + std::fmt::Debug>(
        items: Vec<PendingItem<T>>,
        params: PaginationParams,
    ) -> PendingPage<T> {
        let limit = params.effective_limit();
        let cursor_value = params.decode_cursor();

        let mut filtered: Vec<&PendingItem<T>> = items
            .iter()
            .filter(|item| match cursor_value.as_ref() {
                Some(c) => {
                    // The page-keys are sort_key(sid, item_id); tail item's
                    // key is what the cursor stores. Skip rows ≤ cursor.
                    item_sort_key(item).as_str() > c.as_str()
                }
                None => true,
            })
            .collect();
        filtered.sort_by_key(|a| item_sort_key(a));

        let has_more = filtered.len() > limit;
        let page: Vec<PendingItem<T>> = filtered.into_iter().take(limit).cloned().collect();
        let cursor = if has_more {
            page.last().map(|item| encode_cursor(&item_sort_key(item)))
        } else {
            None
        };
        PendingPage {
            items: page,
            cursor,
            has_more,
        }
    }

    fn item_sort_key<T: Serialize + Clone + std::fmt::Debug>(item: &PendingItem<T>) -> String {
        // Round-trip via JSON to extract the canonical id field. Each kind
        // (gate / escalation / refusal) has a different name; use their
        // serialized shape to pick the first non-empty id.
        let v = serde_json::to_value(&item.inner).unwrap_or(serde_json::Value::Null);
        let id = ["gate_id", "escalation_id", "id"]
            .iter()
            .find_map(|k| v.get(k).and_then(|x| x.as_str()).map(String::from))
            .unwrap_or_default();
        sort_key(&item.session_id, &id)
    }

    pub async fn aggregate_gates(
        state: &AppState,
        auth: &Auth,
        params: PendingParams,
    ) -> Result<Json<serde_json::Value>, ApiError> {
        let owned = owned_sessions(state, auth).await?;
        if let Some(sid) = params.session_id.as_deref() {
            enforce_session_scope(&owned, sid)?;
        }

        let mut items: Vec<PendingItem<EnrichedGate>> = Vec::new();
        let active = state.active_sessions.read().await;
        for (sid, handle) in active.iter() {
            if !visible(&owned, sid) {
                continue;
            }
            if let Some(scope) = params.session_id.as_deref()
                && scope != sid
            {
                continue;
            }
            let gates = handle.pending.gates.read().await;
            for g in gates.iter() {
                items.push(PendingItem {
                    session_id: sid.clone(),
                    inner: g.clone(),
                });
            }
        }
        drop(active);

        let page = build_page(
            items,
            PaginationParams {
                limit: params.limit,
                cursor: params.cursor,
            },
        );
        Ok(Json(serde_json::to_value(page).unwrap_or_default()))
    }

    pub async fn aggregate_escalations(
        state: &AppState,
        auth: &Auth,
        params: PendingParams,
    ) -> Result<Json<serde_json::Value>, ApiError> {
        let owned = owned_sessions(state, auth).await?;
        if let Some(sid) = params.session_id.as_deref() {
            enforce_session_scope(&owned, sid)?;
        }

        let mut items: Vec<PendingItem<EnrichedEscalation>> = Vec::new();
        let active = state.active_sessions.read().await;
        for (sid, handle) in active.iter() {
            if !visible(&owned, sid) {
                continue;
            }
            if let Some(scope) = params.session_id.as_deref()
                && scope != sid
            {
                continue;
            }
            let escalations = handle.pending.escalations.read().await;
            for e in escalations.iter() {
                items.push(PendingItem {
                    session_id: sid.clone(),
                    inner: e.clone(),
                });
            }
        }
        drop(active);

        let page = build_page(
            items,
            PaginationParams {
                limit: params.limit,
                cursor: params.cursor,
            },
        );
        Ok(Json(serde_json::to_value(page).unwrap_or_default()))
    }

    pub async fn aggregate_refusals(
        state: &AppState,
        auth: &Auth,
        params: PendingParams,
    ) -> Result<Json<serde_json::Value>, ApiError> {
        let owned = owned_sessions(state, auth).await?;
        if let Some(sid) = params.session_id.as_deref() {
            enforce_session_scope(&owned, sid)?;
        }

        let mut items: Vec<PendingItem<Refusal>> = Vec::new();
        let active = state.active_sessions.read().await;
        for (sid, handle) in active.iter() {
            if !visible(&owned, sid) {
                continue;
            }
            if let Some(scope) = params.session_id.as_deref()
                && scope != sid
            {
                continue;
            }
            let refusals = handle.pending.refusals.read().await;
            for r in refusals.iter() {
                items.push(PendingItem {
                    session_id: sid.clone(),
                    inner: r.clone(),
                });
            }
        }
        drop(active);

        let page = build_page(
            items,
            PaginationParams {
                limit: params.limit,
                cursor: params.cursor,
            },
        );
        Ok(Json(serde_json::to_value(page).unwrap_or_default()))
    }

    fn visible(owned: &Option<std::collections::HashSet<String>>, sid: &str) -> bool {
        match owned {
            None => true,
            Some(set) => set.contains(sid),
        }
    }

    #[cfg(test)]
    pub(crate) fn sort_key_for_test(sid: &str, item_id: &str) -> String {
        sort_key(sid, item_id)
    }

    // ----------------------------------------------------------------------
    // Tests — W6 pending aggregation
    // ----------------------------------------------------------------------
    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::AppState;
        use crate::middleware::Auth;
        use arc_swap::ArcSwap;
        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode};
        use console_core::auth::{AuthenticatedUser, ConsoleRole};
        use console_core::authenticator;
        use console_core::config::RuntimeConfig;
        use console_core::event_enricher::{EnrichedEscalation, EnrichedGate};
        use console_core::refusal_synthesizer::{Refusal, RefusalLayer};
        use console_core::session_monitor::{
            Frame, MonitorCmd, PendingState, SessionMonitorHandle,
        };
        use console_core::taxonomy_builder;
        use console_db::DbPool;
        use console_db::create_test_pool;
        use console_db::queries::api_tokens;
        use console_runtime::grpc_pool::GrpcPool;
        use std::collections::HashMap;
        use std::sync::Arc;
        use tokio::sync::{RwLock, broadcast, mpsc};
        use tower::ServiceExt;

        // ---- fixtures ----------------------------------------------------

        async fn build_state() -> Arc<AppState> {
            let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
            let db = create_test_pool().await.expect("db");
            let taxonomy = Arc::new(ArcSwap::from_pointee(
                taxonomy_builder::build_index(None, &[], &[]).index,
            ));
            Arc::new(AppState {
                db,
                taxonomy,
                runtime_config: RuntimeConfig {
                    agent_address: "[::1]:1".into(),
                    highway_address: "[::1]:1".into(),
                    coordinator_address: "[::1]:1".into(),
                    rest_address: "[::1]:1".into(),
                },
                grpc_pool: pool,
                active_sessions: Arc::new(RwLock::new(HashMap::new())),
            })
        }

        async fn seed_user(db: &DbPool, id: &str, role: &str) {
            sqlx::query(
                "INSERT INTO users (id, username, username_lower, display_name, password_hash,
                    console_role, must_change_password, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(id)
            .bind(id)
            .bind(id)
            .bind("h")
            .bind(role)
            .bind(0_i64)
            .bind("2026-04-15T00:00:00Z")
            .bind("2026-04-15T00:00:00Z")
            .execute(db)
            .await
            .expect("insert user");
        }

        async fn seed_session(db: &DbPool, sid: &str, owner: &str) {
            let row = console_db::queries::sessions::SessionRow {
                id: sid.into(),
                name: Some(sid.into()),
                owner_user_id: owner.into(),
                vertical: "swe".into(),
                workflow: "wf".into(),
                context: None,
                coordinator_workspace_id: Some("ws-root".into()),
                state: console_core::session_state::ACTIVE.into(),
                created_at: "2026-04-15T00:00:00Z".into(),
                launched_at: Some("2026-04-15T00:00:00Z".into()),
                closed_at: None,
                budget_max_cost_micros: None,
                budget_max_tokens: None,
                budget_max_wall_time_ms: None,
            };
            console_db::queries::sessions::insert_session(db, &row)
                .await
                .expect("insert");
        }

        async fn mint_bearer(db: &DbPool, owner: &str) -> String {
            let plain = format!("wcon_t_{}", uuid::Uuid::new_v4());
            let hash = authenticator::hash_token(&plain);
            api_tokens::insert_token(
                db,
                &format!("tok-{}", uuid::Uuid::new_v4()),
                owner,
                "test",
                &hash,
                "2026-04-15T00:00:00Z",
                None,
            )
            .await
            .expect("insert token");
            plain
        }

        async fn install_handle(state: &AppState, sid: &str) -> SessionMonitorHandle {
            let handle = SessionMonitorHandle {
                session_id: sid.into(),
                cmd_tx: mpsc::channel::<MonitorCmd>(1).0,
                broadcast_tx: broadcast::channel::<Frame>(8).0,
                pending: Arc::new(PendingState::default()),
            };
            state
                .active_sessions
                .write()
                .await
                .insert(sid.into(), handle.clone());
            handle
        }

        fn dummy_gate(id: &str) -> EnrichedGate {
            EnrichedGate {
                gate_id: id.into(),
                type_: "task_approval".into(),
                workspace_id: "ws-1".into(),
                workspace_label: "ws-1".into(),
                task_id: "t-1".into(),
                timeout_ms: 0,
                fallback_action: String::new(),
                created_at: String::new(),
                subject_len: 0,
            }
        }

        fn dummy_escalation(id: &str) -> EnrichedEscalation {
            EnrichedEscalation {
                escalation_id: id.into(),
                workspace_id: "ws-1".into(),
                workspace_label: "ws-1".into(),
                owner: "u".into(),
                created_at: String::new(),
                context_len: 0,
            }
        }

        fn dummy_refusal(id: &str) -> Refusal {
            Refusal {
                id: id.into(),
                layer: RefusalLayer::ToolLayer,
                workspace_id: "ws-1".into(),
                actor: "agent".into(),
                code: None,
                reason: None,
                sequence_number: 0,
            }
        }

        fn auth(user: &str, role: ConsoleRole) -> Auth {
            Auth(AuthenticatedUser {
                user_id: user.into(),
                username: user.into(),
                console_role: role,
            })
        }

        // ---- pure unit ---------------------------------------------------

        #[test]
        fn cursor_round_trips_through_base64() {
            let key = sort_key_for_test("s-1", "g-7");
            let encoded = encode_cursor(&key);
            let params = PaginationParams {
                limit: None,
                cursor: Some(encoded),
            };
            assert_eq!(params.decode_cursor(), Some(key));
        }

        #[test]
        fn item_sort_key_groups_by_session_then_id() {
            let a = PendingItem {
                session_id: "s-2".into(),
                inner: dummy_gate("g-1"),
            };
            let b = PendingItem {
                session_id: "s-1".into(),
                inner: dummy_gate("g-9"),
            };
            assert!(item_sort_key(&b) < item_sort_key(&a));
        }

        #[test]
        fn build_page_sorts_paginates_and_emits_cursor() {
            let items: Vec<PendingItem<EnrichedGate>> = (0..5)
                .map(|i| PendingItem {
                    session_id: "s-1".into(),
                    inner: dummy_gate(&format!("g-{i:02}")),
                })
                .collect();
            let page = build_page(
                items,
                PaginationParams {
                    limit: Some(2),
                    cursor: None,
                },
            );
            assert_eq!(page.items.len(), 2);
            assert!(page.has_more);
            assert!(page.cursor.is_some());
            assert_eq!(page.items[0].inner.gate_id, "g-00");
            assert_eq!(page.items[1].inner.gate_id, "g-01");
        }

        #[test]
        fn enforce_session_scope_blocks_non_owners() {
            let owned = Some(["s-mine".to_string()].into_iter().collect());
            assert!(enforce_session_scope(&owned, "s-mine").is_ok());
            let err = enforce_session_scope(&owned, "s-theirs").unwrap_err();
            assert_eq!(err.status, StatusCode::FORBIDDEN);
        }

        #[test]
        fn enforce_session_scope_passes_admin() {
            let owned: Option<std::collections::HashSet<String>> = None;
            assert!(enforce_session_scope(&owned, "s-anything").is_ok());
        }

        // ---- handler / aggregation tests ---------------------------------

        #[tokio::test]
        async fn aggregate_gates_returns_owned_session_pending_items() {
            let state = build_state().await;
            seed_user(&state.db, "u-1", "operator").await;
            seed_session(&state.db, "s-mine", "u-1").await;

            let handle = install_handle(&state, "s-mine").await;
            handle.pending.gates.write().await.extend(vec![
                dummy_gate("g-1"),
                dummy_gate("g-2"),
                dummy_gate("g-3"),
            ]);

            let v = aggregate_gates(
                &state,
                &auth("u-1", ConsoleRole::Operator),
                PendingParams {
                    limit: None,
                    cursor: None,
                    session_id: None,
                },
            )
            .await
            .expect("ok");
            let body = v.0;
            let items = body["items"].as_array().expect("items array");
            assert_eq!(items.len(), 3);
            assert!(items[0]["gate_id"].as_str().is_some());
            assert_eq!(items[0]["session_id"], "s-mine");
        }

        #[tokio::test]
        async fn aggregate_gates_hides_other_owners_from_non_admin() {
            let state = build_state().await;
            seed_user(&state.db, "u-mine", "operator").await;
            seed_user(&state.db, "u-other", "operator").await;
            seed_session(&state.db, "s-mine", "u-mine").await;
            seed_session(&state.db, "s-other", "u-other").await;

            let h_mine = install_handle(&state, "s-mine").await;
            let h_other = install_handle(&state, "s-other").await;
            h_mine
                .pending
                .gates
                .write()
                .await
                .push(dummy_gate("mine-1"));
            h_other
                .pending
                .gates
                .write()
                .await
                .push(dummy_gate("other-1"));

            let v = aggregate_gates(
                &state,
                &auth("u-mine", ConsoleRole::Operator),
                PendingParams {
                    limit: None,
                    cursor: None,
                    session_id: None,
                },
            )
            .await
            .expect("ok");
            let items = v.0["items"].as_array().expect("items");
            assert_eq!(items.len(), 1);
            assert_eq!(items[0]["gate_id"], "mine-1");
        }

        #[tokio::test]
        async fn aggregate_gates_admin_sees_everything() {
            let state = build_state().await;
            seed_user(&state.db, "u-mine", "operator").await;
            seed_user(&state.db, "u-other", "operator").await;
            seed_user(&state.db, "u-admin", "admin").await;
            seed_session(&state.db, "s-mine", "u-mine").await;
            seed_session(&state.db, "s-other", "u-other").await;
            let h_a = install_handle(&state, "s-mine").await;
            let h_b = install_handle(&state, "s-other").await;
            h_a.pending.gates.write().await.push(dummy_gate("a"));
            h_b.pending.gates.write().await.push(dummy_gate("b"));

            let v = aggregate_gates(
                &state,
                &auth("u-admin", ConsoleRole::Admin),
                PendingParams {
                    limit: None,
                    cursor: None,
                    session_id: None,
                },
            )
            .await
            .expect("ok");
            let items = v.0["items"].as_array().expect("items");
            assert_eq!(items.len(), 2);
        }

        #[tokio::test]
        async fn aggregate_gates_explicit_scope_to_owned_session() {
            let state = build_state().await;
            seed_user(&state.db, "u-1", "operator").await;
            seed_session(&state.db, "s-a", "u-1").await;
            seed_session(&state.db, "s-b", "u-1").await;
            let ha = install_handle(&state, "s-a").await;
            let hb = install_handle(&state, "s-b").await;
            ha.pending.gates.write().await.push(dummy_gate("a-1"));
            hb.pending.gates.write().await.push(dummy_gate("b-1"));

            let v = aggregate_gates(
                &state,
                &auth("u-1", ConsoleRole::Operator),
                PendingParams {
                    limit: None,
                    cursor: None,
                    session_id: Some("s-a".into()),
                },
            )
            .await
            .expect("ok");
            let items = v.0["items"].as_array().expect("items");
            assert_eq!(items.len(), 1);
            assert_eq!(items[0]["session_id"], "s-a");
        }

        #[tokio::test]
        async fn aggregate_gates_explicit_scope_to_unowned_session_403() {
            let state = build_state().await;
            seed_user(&state.db, "u-mine", "operator").await;
            seed_user(&state.db, "u-other", "operator").await;
            seed_session(&state.db, "s-mine", "u-mine").await;
            seed_session(&state.db, "s-other", "u-other").await;

            let err = aggregate_gates(
                &state,
                &auth("u-mine", ConsoleRole::Operator),
                PendingParams {
                    limit: None,
                    cursor: None,
                    session_id: Some("s-other".into()),
                },
            )
            .await
            .expect_err("must 403");
            assert_eq!(err.status, StatusCode::FORBIDDEN);
        }

        #[tokio::test]
        async fn aggregate_gates_paginates_across_sessions() {
            let state = build_state().await;
            seed_user(&state.db, "u-1", "operator").await;
            // 3 sessions × 40 gates each = 120 items.
            for s in 0..3 {
                let sid = format!("s-{s:02}");
                seed_session(&state.db, &sid, "u-1").await;
                let h = install_handle(&state, &sid).await;
                let mut g = h.pending.gates.write().await;
                for i in 0..40 {
                    g.push(dummy_gate(&format!("g-{i:03}")));
                }
            }

            let mut cursor: Option<String> = None;
            let mut total = 0usize;
            for _ in 0..4 {
                let v = aggregate_gates(
                    &state,
                    &auth("u-1", ConsoleRole::Operator),
                    PendingParams {
                        limit: Some(50),
                        cursor: cursor.clone(),
                        session_id: None,
                    },
                )
                .await
                .expect("ok");
                let items = v.0["items"].as_array().unwrap();
                total += items.len();
                cursor = v.0["cursor"].as_str().map(String::from);
                if cursor.is_none() {
                    break;
                }
            }
            assert_eq!(total, 120);
            assert!(cursor.is_none());
        }

        #[tokio::test]
        async fn aggregate_gates_empty_active_returns_empty_list() {
            let state = build_state().await;
            seed_user(&state.db, "u-1", "operator").await;
            let v = aggregate_gates(
                &state,
                &auth("u-1", ConsoleRole::Operator),
                PendingParams {
                    limit: None,
                    cursor: None,
                    session_id: None,
                },
            )
            .await
            .expect("ok");
            assert_eq!(v.0["items"].as_array().unwrap().len(), 0);
            assert_eq!(v.0["has_more"], false);
        }

        #[tokio::test]
        async fn aggregate_escalations_returns_owned_items() {
            let state = build_state().await;
            seed_user(&state.db, "u-1", "operator").await;
            seed_session(&state.db, "s-mine", "u-1").await;
            let h = install_handle(&state, "s-mine").await;
            h.pending
                .escalations
                .write()
                .await
                .extend(vec![dummy_escalation("e-1"), dummy_escalation("e-2")]);

            let v = aggregate_escalations(
                &state,
                &auth("u-1", ConsoleRole::Operator),
                PendingParams {
                    limit: None,
                    cursor: None,
                    session_id: None,
                },
            )
            .await
            .expect("ok");
            assert_eq!(v.0["items"].as_array().unwrap().len(), 2);
        }

        #[tokio::test]
        async fn aggregate_refusals_returns_owned_items() {
            let state = build_state().await;
            seed_user(&state.db, "u-1", "operator").await;
            seed_session(&state.db, "s-mine", "u-1").await;
            let h = install_handle(&state, "s-mine").await;
            h.pending.refusals.write().await.extend(vec![
                dummy_refusal("r-1"),
                dummy_refusal("r-2"),
                dummy_refusal("r-3"),
            ]);

            let v = aggregate_refusals(
                &state,
                &auth("u-1", ConsoleRole::Operator),
                PendingParams {
                    limit: None,
                    cursor: None,
                    session_id: None,
                },
            )
            .await
            .expect("ok");
            assert_eq!(v.0["items"].as_array().unwrap().len(), 3);
        }

        // ---- end-to-end via router ---------------------------------------

        #[tokio::test]
        async fn http_pending_gates_returns_paginated_json() {
            let state = build_state().await;
            seed_user(&state.db, "u-1", "operator").await;
            seed_session(&state.db, "s-mine", "u-1").await;
            let h = install_handle(&state, "s-mine").await;
            h.pending.gates.write().await.push(dummy_gate("g-1"));
            let token = mint_bearer(&state.db, "u-1").await;
            let app = router().with_state(state.clone());

            let resp = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/gates/pending")
                        .header("authorization", format!("Bearer {token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            let body = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["items"][0]["gate_id"], "g-1");
            assert_eq!(v["items"][0]["session_id"], "s-mine");
        }
    }
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
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use console_core::authenticator;
    use console_core::config::RuntimeConfig;
    use console_core::event_enricher::EnrichedGate;
    use console_core::session_monitor::{Frame, MonitorCmd, PendingState, SessionMonitorHandle};
    use console_core::taxonomy_builder;
    use console_db::create_test_pool;
    use console_db::queries::api_tokens;
    use console_runtime::grpc_pool::GrpcPool;
    use console_test_support::mock_grpc::{HighwayConfig, HighwayOutcome};
    use console_test_support::mock_runtime::MockRuntime;
    use std::collections::HashMap;
    use tokio::sync::{RwLock, broadcast, mpsc};
    use tower::ServiceExt;

    /// Test fixture — runs the configurable mock runtime, builds the AppState
    /// with a connected GrpcPool, and pre-seeds a user + session + bearer
    /// token. The returned `(state, token, sid)` is what every test starts
    /// from.
    struct Fixture {
        state: Arc<AppState>,
        token: String,
        sid: String,
        config: Arc<HighwayConfig>,
    }

    async fn fixture_active_session(owner: &str) -> Fixture {
        fixture(owner, "active").await
    }

    async fn fixture(owner: &str, session_state: &str) -> Fixture {
        let config = HighwayConfig::arc();
        let rt = MockRuntime::start_with_highway_config(Some(config.clone()))
            .await
            .expect("mock runtime");

        let pool = GrpcPool::new(
            &rt.agent_addr.to_string(),
            &rt.highway_addr.to_string(),
            &rt.coordinator_addr.to_string(),
        );
        pool.connect().await;

        let db = create_test_pool().await.expect("db");
        seed_user(&db, owner).await;
        let sid = format!("s-{}", uuid::Uuid::new_v4());
        seed_session(&db, &sid, owner, session_state).await;
        let token = mint_bearer(&db, owner).await;

        let taxonomy = Arc::new(ArcSwap::from_pointee(
            taxonomy_builder::build_index(None, &[], &[]).index,
        ));

        let state = Arc::new(AppState {
            db,
            taxonomy,
            runtime_config: RuntimeConfig {
                agent_address: rt.agent_addr.to_string(),
                highway_address: rt.highway_addr.to_string(),
                coordinator_address: rt.coordinator_addr.to_string(),
                rest_address: rt.rest_addr.to_string(),
            },
            grpc_pool: pool,
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
        });

        // Keep the runtime alive for the test's lifetime by leaking the
        // handle: the fixture is consumed by a single test and dropping
        // mid-request would close the listeners.
        Box::leak(Box::new(rt));

        Fixture {
            state,
            token,
            sid,
            config,
        }
    }

    async fn seed_user(db: &console_db::DbPool, id: &str) {
        sqlx::query(
            "INSERT INTO users (id, username, username_lower, display_name, password_hash,
                console_role, must_change_password, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .bind(id)
        .bind("h")
        .bind("operator")
        .bind(0_i64)
        .bind("2026-04-15T00:00:00Z")
        .bind("2026-04-15T00:00:00Z")
        .execute(db)
        .await
        .expect("insert user");
    }

    async fn seed_session(db: &console_db::DbPool, sid: &str, owner: &str, state: &str) {
        let row = console_db::queries::sessions::SessionRow {
            id: sid.into(),
            name: Some(sid.into()),
            owner_user_id: owner.into(),
            vertical: "swe".into(),
            workflow: "wf".into(),
            context: None,
            coordinator_workspace_id: Some("ws-root".into()),
            state: state.into(),
            created_at: "2026-04-15T00:00:00Z".into(),
            launched_at: Some("2026-04-15T00:00:00Z".into()),
            closed_at: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
        };
        console_db::queries::sessions::insert_session(db, &row)
            .await
            .expect("insert session");
    }

    async fn mint_bearer(db: &console_db::DbPool, owner: &str) -> String {
        let plain = format!("wcon_t_{}", uuid::Uuid::new_v4());
        let hash = authenticator::hash_token(&plain);
        api_tokens::insert_token(
            db,
            &format!("tok-{}", uuid::Uuid::new_v4()),
            owner,
            "test",
            &hash,
            "2026-04-15T00:00:00Z",
            None,
        )
        .await
        .expect("insert token");
        plain
    }

    /// Build an empty SessionMonitorHandle so `active_sessions` can be
    /// pre-populated for optimistic-pending-removal tests.
    async fn dummy_handle(sid: &str, gates: Vec<EnrichedGate>) -> SessionMonitorHandle {
        let pending = Arc::new(PendingState::default());
        *pending.gates.write().await = gates;
        SessionMonitorHandle {
            session_id: sid.into(),
            cmd_tx: mpsc::channel::<MonitorCmd>(1).0,
            broadcast_tx: broadcast::channel::<Frame>(8).0,
            pending,
        }
    }

    fn auth_request(
        method: &str,
        uri: &str,
        token: &str,
        body: serde_json::Value,
    ) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn read_json(resp: axum::response::Response) -> serde_json::Value {
        let body = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null)
    }

    // -- pure unit -----------------------------------------------------------

    #[test]
    fn from_tonic_maps_codes() {
        let cases = [
            (tonic::Code::NotFound, StatusCode::NOT_FOUND),
            (tonic::Code::FailedPrecondition, StatusCode::CONFLICT),
            (tonic::Code::Unavailable, StatusCode::SERVICE_UNAVAILABLE),
            (
                tonic::Code::DeadlineExceeded,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (tonic::Code::PermissionDenied, StatusCode::FORBIDDEN),
            (tonic::Code::Internal, StatusCode::BAD_GATEWAY),
        ];
        for (code, expected) in cases {
            let err = ApiError::from_tonic(tonic::Status::new(code, "x"), "S", "M");
            assert_eq!(err.status, expected, "code {code:?}");
        }
    }

    #[test]
    fn parse_gate_decision_known_values() {
        assert!(matches!(
            parse_gate_decision("approve").unwrap(),
            proto::GateDecision::Approve
        ));
        assert!(matches!(
            parse_gate_decision("reject").unwrap(),
            proto::GateDecision::Reject
        ));
        assert!(matches!(
            parse_gate_decision("modify").unwrap(),
            proto::GateDecision::Modify
        ));
    }

    #[test]
    fn parse_gate_decision_rejects_unknown() {
        let err = parse_gate_decision("yeet").unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn audit_action_for_decision_distinguishes_reject() {
        assert_eq!(
            audit_action_for_decision(proto::GateDecision::Reject),
            AuditAction::SessionGateReject
        );
        assert_eq!(
            audit_action_for_decision(proto::GateDecision::Approve),
            AuditAction::SessionGateApprove
        );
        assert_eq!(
            audit_action_for_decision(proto::GateDecision::Modify),
            AuditAction::SessionGateApprove
        );
    }

    // -- handler tests -------------------------------------------------------

    #[tokio::test]
    async fn resolve_gate_happy_path_audits_and_returns_applied() {
        let fx = fixture_active_session("u-1").await;
        let app = router().with_state(fx.state.clone());

        let resp = app
            .oneshot(auth_request(
                "POST",
                &format!("/api/sessions/{}/gates/g-1", fx.sid),
                &fx.token,
                serde_json::json!({ "decision": "approve" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["gate_id"], "g-1");
        assert_eq!(v["applied"], true);

        // Audit row recorded.
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE target_id = ?")
            .bind("g-1")
            .fetch_one(&fx.state.db)
            .await
            .unwrap();
        assert_eq!(count.0, 1, "audit row inserted on success");

        // Mock captured the request.
        let captured = fx.config.last_gate.lock().unwrap().clone();
        assert_eq!(captured.expect("captured").gate_id, "g-1");
    }

    #[tokio::test]
    async fn resolve_gate_runtime_not_found_returns_404_and_no_audit() {
        let fx = fixture_active_session("u-1").await;
        fx.config
            .set_gate(HighwayOutcome::Err(tonic::Code::NotFound, "no such gate"));
        let app = router().with_state(fx.state.clone());

        let resp = app
            .oneshot(auth_request(
                "POST",
                &format!("/api/sessions/{}/gates/g-missing", fx.sid),
                &fx.token,
                serde_json::json!({ "decision": "approve" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE target_id = ?")
            .bind("g-missing")
            .fetch_one(&fx.state.db)
            .await
            .unwrap();
        assert_eq!(count.0, 0, "no audit row on runtime rejection");
    }

    #[tokio::test]
    async fn resolve_gate_runtime_unavailable_returns_503() {
        let fx = fixture_active_session("u-1").await;
        fx.config.set_gate(HighwayOutcome::Err(
            tonic::Code::Unavailable,
            "runtime down",
        ));
        let app = router().with_state(fx.state.clone());

        let resp = app
            .oneshot(auth_request(
                "POST",
                &format!("/api/sessions/{}/gates/g-1", fx.sid),
                &fx.token,
                serde_json::json!({ "decision": "approve" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn resolve_gate_cross_owner_returns_403() {
        let fx = fixture_active_session("u-owner").await;
        // Mint a token for a different user.
        seed_user(&fx.state.db, "u-stranger").await;
        let stranger_token = mint_bearer(&fx.state.db, "u-stranger").await;
        let app = router().with_state(fx.state.clone());

        let resp = app
            .oneshot(auth_request(
                "POST",
                &format!("/api/sessions/{}/gates/g-1", fx.sid),
                &stranger_token,
                serde_json::json!({ "decision": "approve" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn batch_resolve_mixed_outcomes_reports_per_gate_status() {
        let fx = fixture_active_session("u-1").await;
        // Programmatic sequence: Ok, Err(NotFound), Ok.
        fx.config.push_gate_sequence([
            HighwayOutcome::Ok,
            HighwayOutcome::Err(tonic::Code::NotFound, "no"),
            HighwayOutcome::Ok,
        ]);
        let app = router().with_state(fx.state.clone());

        let resp = app
            .oneshot(auth_request(
                "POST",
                &format!("/api/sessions/{}/gates/batch-resolve", fx.sid),
                &fx.token,
                serde_json::json!({
                    "gates": [
                        { "gate_id": "g-a", "decision": "approve" },
                        { "gate_id": "g-b", "decision": "approve" },
                        { "gate_id": "g-c", "decision": "approve" },
                    ]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        let results = v["results"].as_array().expect("results array");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["status"], "applied");
        assert_eq!(results[1]["status"], "runtime_rejected");
        assert_eq!(results[2]["status"], "applied");

        // Two audit rows (gates a + c), zero for gate b (rejected).
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE target_id IN ('g-a','g-c')")
                .fetch_one(&fx.state.db)
                .await
                .unwrap();
        assert_eq!(count.0, 2);
        let rejected: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE target_id = 'g-b'")
                .fetch_one(&fx.state.db)
                .await
                .unwrap();
        assert_eq!(rejected.0, 0);
    }

    #[tokio::test]
    async fn respond_escalation_happy_path() {
        let fx = fixture_active_session("u-1").await;
        let app = router().with_state(fx.state.clone());

        let resp = app
            .oneshot(auth_request(
                "POST",
                &format!("/api/sessions/{}/escalations/e-1", fx.sid),
                &fx.token,
                serde_json::json!({ "response": "abort" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["escalation_id"], "e-1");
        assert_eq!(v["applied"], true);
    }

    #[tokio::test]
    async fn inject_directive_happy_path_returns_envelope_id() {
        let fx = fixture_active_session("u-1").await;
        let app = router().with_state(fx.state.clone());

        let resp = app
            .oneshot(auth_request(
                "POST",
                &format!("/api/sessions/{}/inject", fx.sid),
                &fx.token,
                serde_json::json!({
                    "workspace_id": "ws-root",
                    "content": { "msg": "do the thing" }
                }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert!(v["envelope_id"].as_str().is_some(), "got {v:?}");

        // Mock recorded payload.
        let captured = fx.config.last_inject.lock().unwrap().clone();
        let req = captured.expect("captured");
        assert_eq!(req.to_workspace, "ws-root");
        assert_eq!(req.r#type, "directive");
    }

    #[tokio::test]
    async fn inject_directive_non_active_session_returns_409() {
        let fx = fixture("u-1", "completed").await;
        let app = router().with_state(fx.state.clone());

        let resp = app
            .oneshot(auth_request(
                "POST",
                &format!("/api/sessions/{}/inject", fx.sid),
                &fx.token,
                serde_json::json!({
                    "workspace_id": "ws-root",
                    "content": {}
                }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        // No request reached the runtime.
        assert!(fx.config.last_inject.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn inject_directive_cross_owner_returns_403() {
        let fx = fixture_active_session("u-owner").await;
        seed_user(&fx.state.db, "u-stranger").await;
        let stranger_token = mint_bearer(&fx.state.db, "u-stranger").await;
        let app = router().with_state(fx.state.clone());

        let resp = app
            .oneshot(auth_request(
                "POST",
                &format!("/api/sessions/{}/inject", fx.sid),
                &stranger_token,
                serde_json::json!({
                    "workspace_id": "ws-root",
                    "content": {}
                }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn resolve_gate_drops_pending_in_active_session_handle() {
        let fx = fixture_active_session("u-1").await;
        // Pre-populate `pending.gates` with a gate that resolve_gate should
        // remove on a successful runtime ack.
        let gate = EnrichedGate {
            gate_id: "g-7".into(),
            type_: "task_approval".into(),
            workspace_id: "ws-root".into(),
            workspace_label: "ws-root".into(),
            task_id: "t".into(),
            timeout_ms: 0,
            fallback_action: String::new(),
            created_at: String::new(),
            subject_len: 0,
        };
        let handle = dummy_handle(&fx.sid, vec![gate]).await;
        fx.state
            .active_sessions
            .write()
            .await
            .insert(fx.sid.clone(), handle.clone());

        let app = router().with_state(fx.state.clone());
        let resp = app
            .oneshot(auth_request(
                "POST",
                &format!("/api/sessions/{}/gates/g-7", fx.sid),
                &fx.token,
                serde_json::json!({ "decision": "approve" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Pending list now empty.
        let pending = fx.state.active_sessions.read().await;
        let h = pending.get(&fx.sid).expect("handle");
        assert!(h.pending.gates.read().await.is_empty());
    }

    #[tokio::test]
    async fn from_tonic_carries_grpc_status_in_details() {
        let err = ApiError::from_tonic(
            tonic::Status::new(tonic::Code::NotFound, "missing"),
            "HighwayService",
            "RespondToGate",
        );
        let body = err.body;
        let details = body.details.expect("details");
        assert_eq!(details["service"], "HighwayService");
        assert_eq!(details["method"], "RespondToGate");
        assert_eq!(details["grpc_status"], "NotFound");
    }
}
