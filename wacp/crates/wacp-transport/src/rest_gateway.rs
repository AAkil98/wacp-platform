use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};
use utoipa::ToSchema;
use wacp_taxonomy::VerticalManifest;

use crate::proto::wacp_v1;

// ---------------------------------------------------------------------------
// Gateway backend trait — production uses gRPC, tests use mock
// ---------------------------------------------------------------------------

/// Backend trait for the REST gateway. Each method maps to a gRPC call.
#[tonic::async_trait]
pub trait GatewayBackend: Send + Sync + 'static {
    async fn submit_goal(
        &self,
        req: wacp_v1::SubmitGoalRequest,
    ) -> Result<wacp_v1::SubmitGoalResponse, GatewayError>;
    async fn get_ready_tasks(&self) -> Result<wacp_v1::GetReadyTasksResponse, GatewayError>;
    async fn dispatch(
        &self,
        req: wacp_v1::DispatchRequest,
    ) -> Result<wacp_v1::DispatchResponse, GatewayError>;
    async fn abort_workspace(
        &self,
        req: wacp_v1::AbortWorkspaceRequest,
    ) -> Result<wacp_v1::AbortWorkspaceResponse, GatewayError>;
    async fn suspend_workspace(
        &self,
        req: wacp_v1::SuspendWorkspaceRequest,
    ) -> Result<wacp_v1::SuspendWorkspaceResponse, GatewayError>;
    async fn resume_workspace(
        &self,
        req: wacp_v1::ResumeWorkspaceRequest,
    ) -> Result<wacp_v1::ResumeWorkspaceResponse, GatewayError>;
    async fn get_workspace(&self, id: &str) -> Result<wacp_v1::WorkspaceView, GatewayError>;
    async fn inject_envelope(
        &self,
        req: wacp_v1::InjectEnvelopeRequest,
    ) -> Result<wacp_v1::InjectEnvelopeResponse, GatewayError>;
    async fn trigger_integration(
        &self,
        req: wacp_v1::TriggerIntegrationRequest,
    ) -> Result<wacp_v1::TriggerIntegrationResponse, GatewayError>;
    async fn query_trail(
        &self,
        req: wacp_v1::HighwayQueryTrailRequest,
    ) -> Result<wacp_v1::QueryTrailResponse, GatewayError>;
    async fn respond_to_gate(
        &self,
        req: wacp_v1::GateResponse,
    ) -> Result<wacp_v1::GateResponseAck, GatewayError>;
    async fn respond_to_escalation(
        &self,
        req: wacp_v1::EscalationResponse,
    ) -> Result<wacp_v1::EscalationResponseAck, GatewayError>;
    async fn get_allocatable(&self) -> Result<wacp_v1::GetAllocatableResponse, GatewayError>;
    async fn list_workspaces(
        &self,
        parent_id: Option<&str>,
    ) -> Result<Vec<WorkspaceSummaryItem>, GatewayError>;
}

#[derive(Debug)]
pub struct GatewayError {
    pub status: StatusCode,
    pub message: String,
}

impl GatewayError {
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
    pub fn from_status(s: tonic::Status) -> Self {
        Self {
            status: grpc_to_http_status(s.code() as i32),
            message: s.message().to_string(),
        }
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

/// Loaded vertical manifests shared across all requests. Populated at startup
/// from `vertical.yaml` files; empty if none are configured.
pub type VerticalRegistry = Arc<Vec<VerticalManifest>>;

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
}

/// Summary of a vertical — returned by `GET /v1/verticals`.
#[derive(Debug, Serialize, ToSchema)]
pub struct VerticalSummary {
    pub id: String,
    pub name: String,
    pub defining_constraint: String,
    pub task_type_count: usize,
    pub workflow_count: usize,
    pub tool_count: usize,
}

// ---------------------------------------------------------------------------
// Typed request/response schemas for OpenAPI
// ---------------------------------------------------------------------------

/// Shared runtime lifecycle state for the Console-facing health endpoint.
/// When not provided (e.g. in tests), the handler defaults to "ready".
#[derive(Clone)]
pub struct RuntimeHealth {
    pub state: Arc<AtomicU8>,
    pub start_time: Instant,
}

/// Health-state constants (mirrors `wacp_runtime::health`).
pub const HEALTH_STARTING: u8 = 0;
pub const HEALTH_READY: u8 = 1;
pub const HEALTH_DRAINING: u8 = 2;

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
}

#[derive(Deserialize, ToSchema)]
pub struct SubmitGoalBody {
    pub description: String,
    #[serde(default)]
    pub context: String,
}

#[derive(Serialize, ToSchema)]
pub struct GoalCreatedResponse {
    pub goal_id: String,
    pub workspace_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct TaskListItem {
    pub task_id: String,
    pub name: String,
    pub status: i32,
}

#[derive(Serialize, ToSchema)]
pub struct WorkspaceResponse {
    pub workspace_id: String,
    pub state: i32,
    pub role: String,
}

#[derive(Deserialize, ToSchema)]
pub struct DispatchBody {
    pub task_id: String,
    pub role: String,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct DispatchCreatedResponse {
    pub workspace_id: String,
    pub task_id: String,
}

#[derive(Deserialize, ToSchema)]
pub struct AbortBody {
    #[serde(default)]
    pub reason: String,
}

#[derive(Deserialize, ToSchema)]
pub struct InjectBody {
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub payload: String,
}

#[derive(Serialize, ToSchema)]
pub struct InjectCreatedResponse {
    pub envelope_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct IntegrationResponse {
    pub result: String,
    pub detail: String,
}

#[derive(Deserialize, ToSchema)]
pub struct GateResponseBody {
    pub decision: String,
    #[serde(default)]
    pub modification: String,
}

#[derive(Deserialize, ToSchema)]
pub struct EscalationResponseBody {
    pub action: String,
}

#[derive(Serialize, ToSchema)]
pub struct TrailEntryItem {
    pub id: String,
    pub event_type: String,
    pub workspace_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct BudgetResponse {
    pub max_tokens: u64,
    pub max_wall_time_ms: u64,
    pub max_cost_micros: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceSummaryItem {
    pub id: String,
    pub parent_id: String,
    pub state: i32,
    pub owner: String,
    pub task_id: String,
}

pub struct RestGateway;

impl RestGateway {
    /// Build the Axum router.
    ///
    /// `backend` handles all protocol-level endpoints (gRPC-backed).
    /// `verticals` is static config loaded at startup; an empty vec is valid.
    /// `health` carries the runtime lifecycle state; when `None` the health
    /// endpoint defaults to reporting "ready" (useful in tests).
    pub fn router(
        backend: Arc<dyn GatewayBackend>,
        verticals: VerticalRegistry,
        health: Option<RuntimeHealth>,
    ) -> Router {
        let health = health.unwrap_or(RuntimeHealth {
            state: Arc::new(AtomicU8::new(HEALTH_READY)),
            start_time: Instant::now(),
        });
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
            .route(
                "/v1/escalations/{id}/respond",
                post(respond_escalation_handler),
            )
            .route("/v1/trail", get(query_trail_handler))
            .route("/v1/budget", get(get_allocatable_handler))
            .route(
                "/v1/sessions/{id}/workspaces",
                get(list_session_workspaces_handler),
            )
            // Vertical discovery — read-only, no auth required.
            .route("/v1/verticals", get(list_verticals_handler))
            .route("/v1/verticals/{id}", get(get_vertical_handler))
            .layer(Extension(verticals))
            .layer(Extension(health))
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
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

#[utoipa::path(get, path = "/v1/health", tag = "health",
    responses(
        (status = 200, description = "Runtime ready", body = HealthResponse),
        (status = 503, description = "Runtime starting or draining", body = HealthResponse),
    )
)]
pub async fn health_handler(
    Extension(health): Extension<RuntimeHealth>,
) -> (StatusCode, Json<HealthResponse>) {
    let state_val = health.state.load(Ordering::Relaxed);
    let uptime = health.start_time.elapsed().as_secs();

    let (status_str, http_status) = match state_val {
        HEALTH_STARTING => ("starting", StatusCode::SERVICE_UNAVAILABLE),
        HEALTH_READY => ("ready", StatusCode::OK),
        HEALTH_DRAINING => ("draining", StatusCode::SERVICE_UNAVAILABLE),
        _ => ("unknown", StatusCode::INTERNAL_SERVER_ERROR),
    };

    (
        http_status,
        Json(HealthResponse {
            status: status_str.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            uptime_seconds: uptime,
        }),
    )
}

#[utoipa::path(post, path = "/v1/goals", tag = "goals",
    request_body = SubmitGoalBody,
    responses(
        (status = 201, description = "Goal created", body = GoalCreatedResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
    )
)]
pub async fn submit_goal_handler(
    State(backend): State<GatewayState>,
    Json(body): Json<SubmitGoalBody>,
) -> Result<(StatusCode, Json<GoalCreatedResponse>), GatewayError> {
    let resp = backend
        .submit_goal(wacp_v1::SubmitGoalRequest {
            description: body.description,
            context: body.context.into_bytes(),
            client_request_id: String::new(),
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(GoalCreatedResponse {
            goal_id: resp.goal_id,
            workspace_id: resp.root_workspace_id,
        }),
    ))
}

#[utoipa::path(get, path = "/v1/tasks", tag = "tasks",
    responses((status = 200, description = "Ready tasks", body = Vec<TaskListItem>))
)]
pub async fn get_ready_tasks_handler(
    State(backend): State<GatewayState>,
) -> Result<Json<Vec<TaskListItem>>, GatewayError> {
    let resp = backend.get_ready_tasks().await?;
    let tasks: Vec<TaskListItem> = resp
        .tasks
        .iter()
        .map(|t| TaskListItem {
            task_id: t.task_id.clone(),
            name: t.name.clone(),
            status: t.status,
        })
        .collect();
    Ok(Json(tasks))
}

#[utoipa::path(get, path = "/v1/workspaces/{id}", tag = "workspaces",
    params(("id" = String, Path, description = "Workspace ID")),
    responses(
        (status = 200, description = "Workspace details", body = WorkspaceResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn get_workspace_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
) -> Result<Json<WorkspaceResponse>, GatewayError> {
    let resp = backend.get_workspace(&id).await?;
    Ok(Json(WorkspaceResponse {
        workspace_id: resp.id,
        state: resp.state,
        role: resp.role,
    }))
}

#[utoipa::path(post, path = "/v1/workspaces/{id}/dispatch", tag = "workspaces",
    params(("id" = String, Path, description = "Parent workspace ID")),
    request_body = DispatchBody,
    responses(
        (status = 201, description = "Workspace dispatched", body = DispatchCreatedResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
    )
)]
pub async fn dispatch_handler(
    State(backend): State<GatewayState>,
    Path(_id): Path<String>,
    Json(body): Json<DispatchBody>,
) -> Result<(StatusCode, Json<DispatchCreatedResponse>), GatewayError> {
    let resp = backend
        .dispatch(wacp_v1::DispatchRequest {
            task_id: body.task_id,
            role: body.role,
            directive_payload: vec![],
            tools: body.tools,
            budget: None,
            client_request_id: String::new(),
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(DispatchCreatedResponse {
            workspace_id: resp.workspace_id,
            task_id: resp.task_id,
        }),
    ))
}

#[utoipa::path(post, path = "/v1/workspaces/{id}/abort", tag = "workspaces",
    params(("id" = String, Path, description = "Workspace ID")),
    request_body = AbortBody,
    responses(
        (status = 204, description = "Workspace aborted"),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn abort_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
    Json(body): Json<AbortBody>,
) -> Result<StatusCode, GatewayError> {
    backend
        .abort_workspace(wacp_v1::AbortWorkspaceRequest {
            workspace_id: id,
            reason: body.reason,
            client_request_id: String::new(),
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/v1/workspaces/{id}/suspend", tag = "workspaces",
    params(("id" = String, Path, description = "Workspace ID")),
    responses(
        (status = 204, description = "Workspace suspended"),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn suspend_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
) -> Result<StatusCode, GatewayError> {
    backend
        .suspend_workspace(wacp_v1::SuspendWorkspaceRequest {
            workspace_id: id,
            client_request_id: String::new(),
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/v1/workspaces/{id}/resume", tag = "workspaces",
    params(("id" = String, Path, description = "Workspace ID")),
    responses(
        (status = 204, description = "Workspace resumed"),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn resume_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
) -> Result<StatusCode, GatewayError> {
    backend
        .resume_workspace(wacp_v1::ResumeWorkspaceRequest {
            workspace_id: id,
            client_request_id: String::new(),
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/v1/workspaces/{id}/inject", tag = "workspaces",
    params(("id" = String, Path, description = "Target workspace ID")),
    request_body = InjectBody,
    responses(
        (status = 201, description = "Envelope injected", body = InjectCreatedResponse),
    )
)]
pub async fn inject_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
    Json(body): Json<InjectBody>,
) -> Result<(StatusCode, Json<InjectCreatedResponse>), GatewayError> {
    let resp = backend
        .inject_envelope(wacp_v1::InjectEnvelopeRequest {
            to_workspace: id,
            r#type: body.r#type,
            payload: body.payload.into_bytes(),
            priority: 0,
            client_request_id: String::new(),
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(InjectCreatedResponse {
            envelope_id: resp.envelope_id,
        }),
    ))
}

#[utoipa::path(post, path = "/v1/workspaces/{id}/integrate", tag = "workspaces",
    params(("id" = String, Path, description = "Workspace ID")),
    responses(
        (status = 200, description = "Integration result", body = IntegrationResponse),
    )
)]
pub async fn integrate_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
) -> Result<Json<IntegrationResponse>, GatewayError> {
    let resp = backend
        .trigger_integration(wacp_v1::TriggerIntegrationRequest {
            workspace_id: id,
            client_request_id: String::new(),
        })
        .await?;
    Ok(Json(IntegrationResponse {
        result: resp.result,
        detail: resp.detail,
    }))
}

#[utoipa::path(post, path = "/v1/gates/{id}/respond", tag = "gates",
    params(("id" = String, Path, description = "Gate ID")),
    request_body = GateResponseBody,
    responses(
        (status = 204, description = "Gate response accepted"),
        (status = 404, description = "Gate not found", body = ErrorResponse),
    )
)]
pub async fn respond_gate_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
    Json(body): Json<GateResponseBody>,
) -> Result<StatusCode, GatewayError> {
    let decision = match body.decision.as_str() {
        "approve" => 1,
        "reject" => 2,
        "modify" => 3,
        _ => {
            return Err(GatewayError::bad_request(format!(
                "invalid gate decision: '{}' (expected approve, reject, or modify)",
                body.decision
            )));
        }
    };
    backend
        .respond_to_gate(wacp_v1::GateResponse {
            gate_id: id,
            decision,
            modifications: body.modification.into_bytes(),
            client_request_id: String::new(),
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/v1/escalations/{id}/respond", tag = "escalations",
    params(("id" = String, Path, description = "Escalation ID")),
    request_body = EscalationResponseBody,
    responses(
        (status = 204, description = "Escalation response accepted"),
        (status = 404, description = "Escalation not found", body = ErrorResponse),
    )
)]
pub async fn respond_escalation_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
    Json(body): Json<EscalationResponseBody>,
) -> Result<StatusCode, GatewayError> {
    let action = match body.action.as_str() {
        "abort" => Some(wacp_v1::escalation_response::Action::Abort(true)),
        "delegate" => Some(wacp_v1::escalation_response::Action::DelegateToCoordinator(
            true,
        )),
        _ => {
            return Err(GatewayError::bad_request(format!(
                "invalid escalation action: '{}' (expected abort or delegate)",
                body.action
            )));
        }
    };
    backend
        .respond_to_escalation(wacp_v1::EscalationResponse {
            escalation_id: id,
            action,
            client_request_id: String::new(),
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/v1/trail", tag = "trail",
    responses((status = 200, description = "Trail entries", body = Vec<TrailEntryItem>))
)]
pub async fn query_trail_handler(
    State(backend): State<GatewayState>,
) -> Result<Json<Vec<TrailEntryItem>>, GatewayError> {
    let trail_req = wacp_v1::HighwayQueryTrailRequest {
        limit: 100,
        ..Default::default()
    };
    let resp = backend.query_trail(trail_req).await?;
    let entries: Vec<TrailEntryItem> = resp
        .entries
        .iter()
        .map(|e| TrailEntryItem {
            id: e.id.clone(),
            event_type: e.event_type.clone(),
            workspace_id: e.workspace_id.clone(),
        })
        .collect();
    Ok(Json(entries))
}

#[utoipa::path(get, path = "/v1/budget", tag = "resources",
    responses((status = 200, description = "Allocatable budget", body = BudgetResponse))
)]
pub async fn get_allocatable_handler(
    State(backend): State<GatewayState>,
) -> Result<Json<BudgetResponse>, GatewayError> {
    let resp = backend.get_allocatable().await?;
    let budget = resp.remaining.unwrap_or_default();
    Ok(Json(BudgetResponse {
        max_tokens: budget.max_tokens,
        max_wall_time_ms: budget.max_wall_time_ms,
        max_cost_micros: budget.max_cost_micros,
    }))
}

// ---------------------------------------------------------------------------
// Session workspace listing
// ---------------------------------------------------------------------------

#[utoipa::path(get, path = "/v1/sessions/{id}/workspaces", tag = "workspaces",
    params(("id" = String, Path, description = "Session (root workspace) ID")),
    responses(
        (status = 200, description = "Workspaces in session", body = Vec<WorkspaceSummaryItem>),
        (status = 404, description = "Session not found", body = ErrorResponse),
    )
)]
pub async fn list_session_workspaces_handler(
    State(backend): State<GatewayState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<WorkspaceSummaryItem>>, GatewayError> {
    backend.list_workspaces(Some(&id)).await.map(Json)
}

// ---------------------------------------------------------------------------
// Vertical discovery handlers — read-only, served from in-memory registry
// ---------------------------------------------------------------------------

#[utoipa::path(get, path = "/v1/verticals", tag = "verticals",
    responses((status = 200, description = "All verticals", body = Vec<VerticalSummary>))
)]
pub async fn list_verticals_handler(
    Extension(verticals): Extension<VerticalRegistry>,
) -> Json<Vec<VerticalSummary>> {
    Json(
        verticals
            .iter()
            .map(|v| VerticalSummary {
                id: v.id.clone(),
                name: v.name.clone(),
                defining_constraint: v.defining_constraint.clone(),
                task_type_count: v.task_types.len(),
                workflow_count: v.workflows.len(),
                tool_count: v.tools.len(),
            })
            .collect(),
    )
}

#[utoipa::path(get, path = "/v1/verticals/{id}", tag = "verticals",
    params(("id" = String, Path, description = "Vertical ID")),
    responses(
        (status = 200, description = "Vertical manifest", body = VerticalManifest),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn get_vertical_handler(
    Extension(verticals): Extension<VerticalRegistry>,
    Path(id): Path<String>,
) -> Result<Json<VerticalManifest>, GatewayError> {
    verticals
        .iter()
        .find(|v| v.id == id)
        .cloned()
        .map(Json)
        .ok_or_else(|| GatewayError::not_found(format!("vertical '{id}' not found")))
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
        async fn submit_goal(
            &self,
            _req: wacp_v1::SubmitGoalRequest,
        ) -> Result<wacp_v1::SubmitGoalResponse, GatewayError> {
            Ok(wacp_v1::SubmitGoalResponse {
                goal_id: "goal-1".into(),
                root_workspace_id: "ws-root".into(),
            })
        }
        async fn get_ready_tasks(&self) -> Result<wacp_v1::GetReadyTasksResponse, GatewayError> {
            let mut task = wacp_v1::TaskView::default();
            task.task_id = "t-1".into();
            task.name = "plan".into();
            task.status = 2;
            Ok(wacp_v1::GetReadyTasksResponse { tasks: vec![task] })
        }
        async fn dispatch(
            &self,
            req: wacp_v1::DispatchRequest,
        ) -> Result<wacp_v1::DispatchResponse, GatewayError> {
            Ok(wacp_v1::DispatchResponse {
                workspace_id: "ws-1".into(),
                task_id: req.task_id,
            })
        }
        async fn abort_workspace(
            &self,
            _req: wacp_v1::AbortWorkspaceRequest,
        ) -> Result<wacp_v1::AbortWorkspaceResponse, GatewayError> {
            Ok(wacp_v1::AbortWorkspaceResponse::default())
        }
        async fn suspend_workspace(
            &self,
            _req: wacp_v1::SuspendWorkspaceRequest,
        ) -> Result<wacp_v1::SuspendWorkspaceResponse, GatewayError> {
            Ok(wacp_v1::SuspendWorkspaceResponse::default())
        }
        async fn resume_workspace(
            &self,
            _req: wacp_v1::ResumeWorkspaceRequest,
        ) -> Result<wacp_v1::ResumeWorkspaceResponse, GatewayError> {
            Ok(wacp_v1::ResumeWorkspaceResponse::default())
        }
        async fn get_workspace(&self, id: &str) -> Result<wacp_v1::WorkspaceView, GatewayError> {
            let mut view = wacp_v1::WorkspaceView::default();
            view.id = id.into();
            view.state = wacp_v1::WorkspaceState::Active.into();
            view.role = "worker".into();
            Ok(view)
        }
        async fn inject_envelope(
            &self,
            _req: wacp_v1::InjectEnvelopeRequest,
        ) -> Result<wacp_v1::InjectEnvelopeResponse, GatewayError> {
            let mut resp = wacp_v1::InjectEnvelopeResponse::default();
            resp.envelope_id = "env-1".into();
            Ok(resp)
        }
        async fn trigger_integration(
            &self,
            _req: wacp_v1::TriggerIntegrationRequest,
        ) -> Result<wacp_v1::TriggerIntegrationResponse, GatewayError> {
            Ok(wacp_v1::TriggerIntegrationResponse {
                result: "accepted".into(),
                detail: "all passed".into(),
            })
        }
        async fn query_trail(
            &self,
            _req: wacp_v1::HighwayQueryTrailRequest,
        ) -> Result<wacp_v1::QueryTrailResponse, GatewayError> {
            Ok(wacp_v1::QueryTrailResponse::default())
        }
        async fn respond_to_gate(
            &self,
            _req: wacp_v1::GateResponse,
        ) -> Result<wacp_v1::GateResponseAck, GatewayError> {
            Ok(wacp_v1::GateResponseAck::default())
        }
        async fn respond_to_escalation(
            &self,
            _req: wacp_v1::EscalationResponse,
        ) -> Result<wacp_v1::EscalationResponseAck, GatewayError> {
            Ok(wacp_v1::EscalationResponseAck::default())
        }
        async fn get_allocatable(&self) -> Result<wacp_v1::GetAllocatableResponse, GatewayError> {
            Ok(wacp_v1::GetAllocatableResponse {
                remaining: Some(wacp_v1::ResourceBudget {
                    max_tokens: 100_000,
                    max_wall_time_ms: 300_000,
                    max_storage_bytes: 0,
                    max_network_bytes: 0,
                    max_cost_micros: 5_000_000,
                    warning_threshold: 0.8,
                }),
            })
        }
        async fn list_workspaces(
            &self,
            parent_id: Option<&str>,
        ) -> Result<Vec<WorkspaceSummaryItem>, GatewayError> {
            let pid = parent_id.unwrap_or("ws-root");
            Ok(vec![WorkspaceSummaryItem {
                id: format!("{pid}-child-1"),
                parent_id: pid.into(),
                state: 1,
                owner: "system".into(),
                task_id: "t-1".into(),
            }])
        }
    }

    fn test_app() -> Router {
        RestGateway::router(Arc::new(MockBackend), Arc::new(vec![]), None)
    }

    /// Build a test app pre-loaded with the given vertical manifests.
    fn test_app_with_verticals(verticals: Vec<wacp_taxonomy::VerticalManifest>) -> Router {
        RestGateway::router(Arc::new(MockBackend), Arc::new(verticals), None)
    }

    /// Minimal VerticalManifest for use in tests.
    fn stub_vertical(id: &str) -> wacp_taxonomy::VerticalManifest {
        wacp_taxonomy::VerticalManifest {
            id: id.into(),
            name: format!("{id} vertical"),
            defining_constraint: format!("{id} constraint"),
            context_schema: Default::default(),
            tool_policies: Default::default(),
            checkpoint_types: Default::default(),
            quality_criteria: vec![],
            task_types: vec![],
            workflows: vec![],
            profiles: vec![],
            tools: vec![],
        }
    }

    async fn get_json(app: &Router, path: &str) -> (StatusCode, serde_json::Value) {
        let resp = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let json = serde_json::from_slice(&body).unwrap_or(serde_json::json!(null));
        (status, json)
    }

    async fn post_json(
        app: &Router,
        path: &str,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let resp = app
            .clone()
            .oneshot(
                Request::post(path)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::json!(null));
        (status, json)
    }

    #[tokio::test]
    async fn health_returns_ready() {
        let (status, json) = get_json(&test_app(), "/v1/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["status"], "ready");
        assert!(json["version"].is_string());
        assert!(json["uptime_seconds"].is_number());
    }

    #[tokio::test]
    async fn health_starting_returns_503() {
        let health = RuntimeHealth {
            state: Arc::new(AtomicU8::new(HEALTH_STARTING)),
            start_time: Instant::now(),
        };
        let app = RestGateway::router(Arc::new(MockBackend), Arc::new(vec![]), Some(health));
        let (status, json) = get_json(&app, "/v1/health").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(json["status"], "starting");
    }

    #[tokio::test]
    async fn health_draining_returns_503() {
        let health = RuntimeHealth {
            state: Arc::new(AtomicU8::new(HEALTH_DRAINING)),
            start_time: Instant::now(),
        };
        let app = RestGateway::router(Arc::new(MockBackend), Arc::new(vec![]), Some(health));
        let (status, json) = get_json(&app, "/v1/health").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(json["status"], "draining");
    }

    #[tokio::test]
    async fn submit_goal_returns_created() {
        let (status, json) = post_json(
            &test_app(),
            "/v1/goals",
            serde_json::json!({"description": "implement auth"}),
        )
        .await;
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
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(json["workspace_id"], "ws-1");
    }

    #[tokio::test]
    async fn abort_returns_no_content() {
        let (status, _) = post_json(
            &test_app(),
            "/v1/workspaces/ws-1/abort",
            serde_json::json!({"reason": "timeout"}),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn suspend_returns_no_content() {
        let resp = test_app()
            .oneshot(
                Request::post("/v1/workspaces/ws-1/suspend")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn resume_returns_no_content() {
        let resp = test_app()
            .oneshot(
                Request::post("/v1/workspaces/ws-1/resume")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn inject_returns_created() {
        let (status, json) = post_json(
            &test_app(),
            "/v1/workspaces/ws-1/inject",
            serde_json::json!({"type": "directive", "payload": "do work"}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(json["envelope_id"], "env-1");
    }

    #[tokio::test]
    async fn integrate_returns_result() {
        let resp = test_app()
            .oneshot(
                Request::post("/v1/workspaces/ws-1/integrate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
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
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn respond_escalation_returns_no_content() {
        let (status, _) = post_json(
            &test_app(),
            "/v1/escalations/esc-1/respond",
            serde_json::json!({"action": "abort"}),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let resp = test_app()
            .oneshot(Request::get("/v1/nonexistent").body(Body::empty()).unwrap())
            .await
            .unwrap();
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

    // --- Vertical discovery ---

    #[tokio::test]
    async fn list_verticals_empty_registry() {
        let (status, json) = get_json(&test_app(), "/v1/verticals").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json, serde_json::json!([]));
    }

    #[tokio::test]
    async fn list_verticals_returns_summaries() {
        let app = test_app_with_verticals(vec![stub_vertical("swe"), stub_vertical("finance")]);
        let (status, json) = get_json(&app, "/v1/verticals").await;
        assert_eq!(status, StatusCode::OK);
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["id"], "swe");
        assert_eq!(arr[0]["name"], "swe vertical");
        assert!(arr[0]["defining_constraint"].is_string());
        assert_eq!(arr[0]["task_type_count"], 0);
        assert_eq!(arr[1]["id"], "finance");
    }

    #[tokio::test]
    async fn get_vertical_returns_full_manifest() {
        let app = test_app_with_verticals(vec![stub_vertical("swe")]);
        let (status, json) = get_json(&app, "/v1/verticals/swe").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["id"], "swe");
        assert_eq!(json["name"], "swe vertical");
        assert_eq!(json["defining_constraint"], "swe constraint");
    }

    #[tokio::test]
    async fn get_vertical_unknown_returns_404() {
        let (status, json) = get_json(&test_app(), "/v1/verticals/nonexistent").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(json["code"], "not_found");
    }

    // --- Session workspace listing ---

    #[tokio::test]
    async fn list_session_workspaces_returns_children() {
        let (status, json) = get_json(&test_app(), "/v1/sessions/ws-root/workspaces").await;
        assert_eq!(status, StatusCode::OK);
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "ws-root-child-1");
        assert_eq!(arr[0]["parent_id"], "ws-root");
    }
}
