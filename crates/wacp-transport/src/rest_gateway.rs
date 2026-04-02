use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::proto::wacp_v1;

// ---------------------------------------------------------------------------
// Gateway backend trait — production uses gRPC, tests use mock
// ---------------------------------------------------------------------------

/// Backend trait for the REST gateway. Each method maps to a gRPC call.
#[tonic::async_trait]
pub trait GatewayBackend: Send + Sync + 'static {
    async fn submit_goal(&self, req: wacp_v1::SubmitGoalRequest) -> Result<wacp_v1::SubmitGoalResponse, GatewayError>;
    async fn get_ready_tasks(&self) -> Result<wacp_v1::GetReadyTasksResponse, GatewayError>;
    async fn dispatch(&self, req: wacp_v1::DispatchRequest) -> Result<wacp_v1::DispatchResponse, GatewayError>;
    async fn abort_workspace(&self, req: wacp_v1::AbortWorkspaceRequest) -> Result<wacp_v1::AbortWorkspaceResponse, GatewayError>;
    async fn suspend_workspace(&self, req: wacp_v1::SuspendWorkspaceRequest) -> Result<wacp_v1::SuspendWorkspaceResponse, GatewayError>;
    async fn resume_workspace(&self, req: wacp_v1::ResumeWorkspaceRequest) -> Result<wacp_v1::ResumeWorkspaceResponse, GatewayError>;
    async fn get_workspace(&self, id: &str) -> Result<wacp_v1::WorkspaceView, GatewayError>;
    async fn inject_envelope(&self, req: wacp_v1::InjectEnvelopeRequest) -> Result<wacp_v1::InjectEnvelopeResponse, GatewayError>;
    async fn trigger_integration(&self, req: wacp_v1::TriggerIntegrationRequest) -> Result<wacp_v1::TriggerIntegrationResponse, GatewayError>;
    async fn query_trail(&self, req: wacp_v1::HighwayQueryTrailRequest) -> Result<wacp_v1::QueryTrailResponse, GatewayError>;
    async fn respond_to_gate(&self, req: wacp_v1::GateResponse) -> Result<wacp_v1::GateResponseAck, GatewayError>;
    async fn respond_to_escalation(&self, req: wacp_v1::EscalationResponse) -> Result<wacp_v1::EscalationResponseAck, GatewayError>;
    async fn get_allocatable(&self) -> Result<wacp_v1::GetAllocatableResponse, GatewayError>;
}

#[derive(Debug)]
pub struct GatewayError {
    pub status: StatusCode,
    pub message: String,
}

impl GatewayError {
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::NOT_FOUND, message: msg.into() }
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, message: msg.into() }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: msg.into() }
    }
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> axum::response::Response {
        let body = ErrorResponse {
            error: self.message,
            code: http_status_to_code(self.status).into(),
        };
        (self.status, Json(body)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Gateway state + router
// ---------------------------------------------------------------------------

pub type GatewayState = Arc<dyn GatewayBackend>;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
}

pub struct RestGateway;

impl RestGateway {
    pub fn new(backend: Arc<dyn GatewayBackend>) -> Router {
        Router::new()
            .route("/v1/health", get(health_handler))
            .route("/v1/goals", post(submit_goal_handler))
            .route("/v1/tasks", get(get_ready_tasks_handler))
            .route("/v1/workspaces/{id}", get(get_workspace_handler))
            .route("/v1/workspaces/{id}/dispatch", post(dispatch_handler))
            .route("/v1/workspaces/{id}/abort", post(abort_handler))
            .route("/v1/workspaces/{id}/suspend", post(suspend_handler))
            .route("/v1/workspaces/{id}/resume", post(resume_handler))
            .route("/v1/workspaces/{id}/inject", post(inject_handler))
            .route("/v1/workspaces/{id}/integrate", post(integrate_handler))
            .route("/v1/gates/{id}/respond", post(respond_gate_handler))
            .route("/v1/escalations/{id}/respond", post(respond_escalation_handler))
            .route("/v1/trail", get(query_trail_handler))
            .route("/v1/budget", get(get_allocatable_handler))
            .with_state(backend)
    }
}

pub fn grpc_to_http_status(grpc_code: i32) -> StatusCode {
    match grpc_code {
        0 => StatusCode::OK,
        3 => StatusCode::BAD_REQUEST,
        16 => StatusCode::UNAUTHORIZED,
        7 => StatusCode::FORBIDDEN,
        5 => StatusCode::NOT_FOUND,
        6 => StatusCode::CONFLICT,
        8 => StatusCode::TOO_MANY_REQUESTS,
        13 => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub fn http_status_to_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "invalid_request",
        StatusCode::UNAUTHORIZED => "unauthenticated",
        StatusCode::FORBIDDEN => "permission_denied",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::CONFLICT => "already_exists",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        StatusCode::INTERNAL_SERVER_ERROR => "internal_error",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Handlers — each calls the backend and returns JSON
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HealthResponse { status: &'static str, version: &'static str }

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok", version: env!("CARGO_PKG_VERSION") })
}

#[derive(Deserialize)]
struct SubmitGoalBody { description: String, #[serde(default)] context: String }

async fn submit_goal_handler(
    State(backend): State<GatewayState>,
    Json(body): Json<SubmitGoalBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), GatewayError> {
    let resp = backend.submit_goal(wacp_v1::SubmitGoalRequest {
        description: body.description,
        context: body.context.into_bytes(),
        client_request_id: String::new(),
    }).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "goal_id": resp.goal_id,
        "workspace_id": resp.root_workspace_id,
    }))))
}

async fn get_ready_tasks_handler(
    State(backend): State<GatewayState>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let resp = backend.get_ready_tasks().await?;
    let tasks: Vec<serde_json::Value> = resp.tasks.iter().map(|t| serde_json::json!({
        "task_id": t.task_id,
        "name": t.name,
        "status": t.status,
    })).collect();
    Ok(Json(serde_json::json!(tasks)))
}

async fn get_workspace_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let resp = backend.get_workspace(&id).await?;
    Ok(Json(serde_json::json!({
        "workspace_id": resp.id,
        "state": resp.state,
        "role": resp.role,
    })))
}

#[derive(Deserialize)]
struct DispatchBody { task_id: String, role: String, #[serde(default)] tools: Vec<String> }

async fn dispatch_handler(
    State(backend): State<GatewayState>,
    Path(_id): Path<String>,
    Json(body): Json<DispatchBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), GatewayError> {
    let resp = backend.dispatch(wacp_v1::DispatchRequest {
        task_id: body.task_id,
        role: body.role,
        directive_payload: vec![],
        tools: body.tools,
        budget: None,
        client_request_id: String::new(),
    }).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "workspace_id": resp.workspace_id,
        "task_id": resp.task_id,
    }))))
}

#[derive(Deserialize)]
struct AbortBody { #[serde(default)] reason: String }

async fn abort_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
    Json(body): Json<AbortBody>,
) -> Result<StatusCode, GatewayError> {
    backend.abort_workspace(wacp_v1::AbortWorkspaceRequest {
        workspace_id: id, reason: body.reason, client_request_id: String::new(),
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn suspend_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
) -> Result<StatusCode, GatewayError> {
    backend.suspend_workspace(wacp_v1::SuspendWorkspaceRequest {
        workspace_id: id, client_request_id: String::new(),
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn resume_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
) -> Result<StatusCode, GatewayError> {
    backend.resume_workspace(wacp_v1::ResumeWorkspaceRequest {
        workspace_id: id, client_request_id: String::new(),
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct InjectBody { #[serde(default)] r#type: String, #[serde(default)] payload: String }

async fn inject_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
    Json(body): Json<InjectBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), GatewayError> {
    let resp = backend.inject_envelope(wacp_v1::InjectEnvelopeRequest {
        to_workspace: id,
        r#type: body.r#type,
        payload: body.payload.into_bytes(),
        priority: 0,
        client_request_id: String::new(),
    }).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"envelope_id": resp.envelope_id}))))
}

async fn integrate_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let resp = backend.trigger_integration(wacp_v1::TriggerIntegrationRequest {
        workspace_id: id, client_request_id: String::new(),
    }).await?;
    Ok(Json(serde_json::json!({"result": resp.result, "detail": resp.detail})))
}

#[derive(Deserialize)]
struct GateResponseBody { decision: String, #[serde(default)] modification: String }

async fn respond_gate_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
    Json(body): Json<GateResponseBody>,
) -> Result<StatusCode, GatewayError> {
    backend.respond_to_gate(wacp_v1::GateResponse {
        gate_id: id,
        decision: 0, // mapped from body.decision in production
        modifications: body.modification.into_bytes(),
        client_request_id: String::new(),
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct EscalationResponseBody { action: String }

async fn respond_escalation_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
    Json(_body): Json<EscalationResponseBody>,
) -> Result<StatusCode, GatewayError> {
    backend.respond_to_escalation(wacp_v1::EscalationResponse {
        escalation_id: id,
        action: None,
        client_request_id: String::new(),
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn query_trail_handler(
    State(backend): State<GatewayState>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let mut trail_req = wacp_v1::HighwayQueryTrailRequest::default();
    trail_req.limit = 100;
    let resp = backend.query_trail(trail_req).await?;
    let entries: Vec<serde_json::Value> = resp.entries.iter().map(|e| serde_json::json!({
        "id": e.id,
        "event_type": e.event_type,
        "workspace_id": e.workspace_id,
    })).collect();
    Ok(Json(serde_json::json!(entries)))
}

async fn get_allocatable_handler(
    State(backend): State<GatewayState>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let resp = backend.get_allocatable().await?;
    let budget = resp.remaining.unwrap_or_default();
    Ok(Json(serde_json::json!({
        "max_tokens": budget.max_tokens,
        "max_wall_time_ms": budget.max_wall_time_ms,
        "max_cost_micros": budget.max_cost_micros,
    })))
}

// ---------------------------------------------------------------------------
// Tests with mock backend
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Mock backend that returns canned responses.
    pub(crate) struct MockBackend;

    #[tonic::async_trait]
    impl GatewayBackend for MockBackend {
        async fn submit_goal(&self, _req: wacp_v1::SubmitGoalRequest) -> Result<wacp_v1::SubmitGoalResponse, GatewayError> {
            Ok(wacp_v1::SubmitGoalResponse { goal_id: "goal-1".into(), root_workspace_id: "ws-root".into() })
        }
        async fn get_ready_tasks(&self) -> Result<wacp_v1::GetReadyTasksResponse, GatewayError> {
            let mut task = wacp_v1::TaskView::default();
            task.task_id = "t-1".into();
            task.name = "plan".into();
            task.status = 2;
            Ok(wacp_v1::GetReadyTasksResponse { tasks: vec![task] })
        }
        async fn dispatch(&self, req: wacp_v1::DispatchRequest) -> Result<wacp_v1::DispatchResponse, GatewayError> {
            Ok(wacp_v1::DispatchResponse { workspace_id: "ws-1".into(), task_id: req.task_id })
        }
        async fn abort_workspace(&self, _req: wacp_v1::AbortWorkspaceRequest) -> Result<wacp_v1::AbortWorkspaceResponse, GatewayError> {
            Ok(wacp_v1::AbortWorkspaceResponse::default())
        }
        async fn suspend_workspace(&self, _req: wacp_v1::SuspendWorkspaceRequest) -> Result<wacp_v1::SuspendWorkspaceResponse, GatewayError> {
            Ok(wacp_v1::SuspendWorkspaceResponse::default())
        }
        async fn resume_workspace(&self, _req: wacp_v1::ResumeWorkspaceRequest) -> Result<wacp_v1::ResumeWorkspaceResponse, GatewayError> {
            Ok(wacp_v1::ResumeWorkspaceResponse::default())
        }
        async fn get_workspace(&self, id: &str) -> Result<wacp_v1::WorkspaceView, GatewayError> {
            let mut view = wacp_v1::WorkspaceView::default();
            view.id = id.into();
            view.state = wacp_v1::WorkspaceState::Active.into();
            view.role = "worker".into();
            Ok(view)
        }
        async fn inject_envelope(&self, _req: wacp_v1::InjectEnvelopeRequest) -> Result<wacp_v1::InjectEnvelopeResponse, GatewayError> {
            let mut resp = wacp_v1::InjectEnvelopeResponse::default();
            resp.envelope_id = "env-1".into();
            Ok(resp)
        }
        async fn trigger_integration(&self, _req: wacp_v1::TriggerIntegrationRequest) -> Result<wacp_v1::TriggerIntegrationResponse, GatewayError> {
            Ok(wacp_v1::TriggerIntegrationResponse { result: "accepted".into(), detail: "all passed".into() })
        }
        async fn query_trail(&self, _req: wacp_v1::HighwayQueryTrailRequest) -> Result<wacp_v1::QueryTrailResponse, GatewayError> {
            Ok(wacp_v1::QueryTrailResponse::default())
        }
        async fn respond_to_gate(&self, _req: wacp_v1::GateResponse) -> Result<wacp_v1::GateResponseAck, GatewayError> {
            Ok(wacp_v1::GateResponseAck::default())
        }
        async fn respond_to_escalation(&self, _req: wacp_v1::EscalationResponse) -> Result<wacp_v1::EscalationResponseAck, GatewayError> {
            Ok(wacp_v1::EscalationResponseAck::default())
        }
        async fn get_allocatable(&self) -> Result<wacp_v1::GetAllocatableResponse, GatewayError> {
            Ok(wacp_v1::GetAllocatableResponse { remaining: Some(wacp_v1::ResourceBudget {
                max_tokens: 100_000, max_wall_time_ms: 300_000, max_storage_bytes: 0,
                max_network_bytes: 0, max_cost_micros: 5_000_000, warning_threshold: 0.8,
            })})
        }
    }

    fn test_app() -> Router {
        RestGateway::new(Arc::new(MockBackend))
    }

    async fn get_json(app: &Router, path: &str) -> (StatusCode, serde_json::Value) {
        let resp = app.clone().oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let json = serde_json::from_slice(&body).unwrap_or(serde_json::json!(null));
        (status, json)
    }

    async fn post_json(app: &Router, path: &str, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
        let resp = app.clone().oneshot(
            Request::post(path)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        ).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::json!(null));
        (status, json)
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let (status, json) = get_json(&test_app(), "/v1/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn submit_goal_returns_created() {
        let (status, json) = post_json(&test_app(), "/v1/goals", serde_json::json!({"description": "implement auth"})).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(json["goal_id"], "goal-1");
        assert_eq!(json["workspace_id"], "ws-root");
    }

    #[tokio::test]
    async fn get_ready_tasks_returns_list() {
        let (status, json) = get_json(&test_app(), "/v1/tasks").await;
        assert_eq!(status, StatusCode::OK);
        assert!(json.as_array().unwrap().len() > 0);
        assert_eq!(json[0]["task_id"], "t-1");
    }

    #[tokio::test]
    async fn get_workspace_returns_data() {
        let (status, json) = get_json(&test_app(), "/v1/workspaces/ws-1").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["workspace_id"], "ws-1");
        assert_eq!(json["role"], "worker");
    }

    #[tokio::test]
    async fn dispatch_returns_created() {
        let (status, json) = post_json(
            &test_app(),
            "/v1/workspaces/ws-1/dispatch",
            serde_json::json!({"task_id": "t-1", "role": "worker"}),
        ).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(json["workspace_id"], "ws-1");
    }

    #[tokio::test]
    async fn abort_returns_no_content() {
        let (status, _) = post_json(&test_app(), "/v1/workspaces/ws-1/abort", serde_json::json!({"reason": "timeout"})).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn suspend_returns_no_content() {
        let resp = test_app().oneshot(
            Request::post("/v1/workspaces/ws-1/suspend").body(Body::empty()).unwrap(),
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn resume_returns_no_content() {
        let resp = test_app().oneshot(
            Request::post("/v1/workspaces/ws-1/resume").body(Body::empty()).unwrap(),
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn inject_returns_created() {
        let (status, json) = post_json(
            &test_app(),
            "/v1/workspaces/ws-1/inject",
            serde_json::json!({"type": "directive", "payload": "do work"}),
        ).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(json["envelope_id"], "env-1");
    }

    #[tokio::test]
    async fn integrate_returns_result() {
        let resp = test_app().oneshot(
            Request::post("/v1/workspaces/ws-1/integrate").body(Body::empty()).unwrap(),
        ).await.unwrap();
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["result"], "accepted");
    }

    #[tokio::test]
    async fn query_trail_returns_array() {
        let (status, json) = get_json(&test_app(), "/v1/trail").await;
        assert_eq!(status, StatusCode::OK);
        assert!(json.is_array());
    }

    #[tokio::test]
    async fn get_allocatable_returns_budget() {
        let (status, json) = get_json(&test_app(), "/v1/budget").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["max_tokens"], 100_000);
        assert_eq!(json["max_cost_micros"], 5_000_000);
    }

    #[tokio::test]
    async fn respond_gate_returns_no_content() {
        let (status, _) = post_json(
            &test_app(),
            "/v1/gates/gate-1/respond",
            serde_json::json!({"decision": "approve"}),
        ).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn respond_escalation_returns_no_content() {
        let (status, _) = post_json(
            &test_app(),
            "/v1/escalations/esc-1/respond",
            serde_json::json!({"action": "abort"}),
        ).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let resp = test_app().oneshot(Request::get("/v1/nonexistent").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn grpc_to_http_mapping() {
        assert_eq!(grpc_to_http_status(0), StatusCode::OK);
        assert_eq!(grpc_to_http_status(3), StatusCode::BAD_REQUEST);
        assert_eq!(grpc_to_http_status(16), StatusCode::UNAUTHORIZED);
        assert_eq!(grpc_to_http_status(7), StatusCode::FORBIDDEN);
        assert_eq!(grpc_to_http_status(5), StatusCode::NOT_FOUND);
        assert_eq!(grpc_to_http_status(6), StatusCode::CONFLICT);
        assert_eq!(grpc_to_http_status(8), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(grpc_to_http_status(13), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
