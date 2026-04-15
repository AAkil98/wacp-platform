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
        handle.pending.gates.write().await.retain(|g| g.gate_id != gid);
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

    let mut client = state
        .grpc_pool
        .highway()
        .await
        .ok_or_else(|| ApiError::runtime_unavailable("HighwayService", "RespondToEscalation"))?;

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

// --- Cross-session pending queries ---

#[derive(Deserialize)]
#[allow(dead_code)]
struct PendingParams {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

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

    fn auth_request(method: &str, uri: &str, token: &str, body: serde_json::Value) -> Request<Body> {
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
            (tonic::Code::DeadlineExceeded, StatusCode::SERVICE_UNAVAILABLE),
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
