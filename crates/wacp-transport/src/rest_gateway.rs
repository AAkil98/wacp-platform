use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, delete},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// REST gateway — translates HTTP requests into gRPC calls.
///
/// Pure translation layer: no business logic, no state, no caching.
pub struct RestGateway {
    router: Router,
}

/// Shared state for all handlers.
#[derive(Clone)]
pub struct GatewayState {
    // In production, these would be gRPC clients.
    // For now, we define the structure and endpoint routing.
    pub _placeholder: Arc<()>,
}

/// Standard JSON error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
}

/// Map gRPC status codes to HTTP status codes.
pub fn grpc_to_http_status(grpc_code: i32) -> StatusCode {
    match grpc_code {
        0 => StatusCode::OK,                       // OK
        3 => StatusCode::BAD_REQUEST,               // INVALID_ARGUMENT
        16 => StatusCode::UNAUTHORIZED,             // UNAUTHENTICATED
        7 => StatusCode::FORBIDDEN,                 // PERMISSION_DENIED
        5 => StatusCode::NOT_FOUND,                 // NOT_FOUND
        6 => StatusCode::CONFLICT,                  // ALREADY_EXISTS
        8 => StatusCode::TOO_MANY_REQUESTS,         // RESOURCE_EXHAUSTED
        13 => StatusCode::INTERNAL_SERVER_ERROR,     // INTERNAL
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Map HTTP status code to a standard error code string.
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

impl RestGateway {
    /// Create a new REST gateway with routes.
    pub fn new(state: GatewayState) -> Self {
        let router = Router::new()
            // Health
            .route("/v1/health", get(health_handler))
            // Sessions
            .route("/v1/sessions", post(create_session_handler))
            .route("/v1/sessions/{id}", delete(delete_session_handler))
            // Goals
            .route("/v1/goals", post(submit_goal_handler))
            // Tasks
            .route("/v1/tasks", get(get_ready_tasks_handler))
            .route("/v1/tasks/graph", get(get_task_graph_handler))
            // Workspaces
            .route("/v1/workspaces/{id}", get(get_workspace_handler))
            .route("/v1/workspaces/{id}/dispatch", post(dispatch_handler))
            .route("/v1/workspaces/{id}/abort", post(abort_handler))
            .route("/v1/workspaces/{id}/suspend", post(suspend_handler))
            .route("/v1/workspaces/{id}/resume", post(resume_handler))
            .route("/v1/workspaces/{id}/inject", post(inject_handler))
            .route("/v1/workspaces/{id}/integrate", post(integrate_handler))
            // Gates
            .route("/v1/gates", get(list_gates_handler))
            .route("/v1/gates/{id}/respond", post(respond_gate_handler))
            // Escalations
            .route("/v1/escalations", get(list_escalations_handler))
            .route("/v1/escalations/{id}/respond", post(respond_escalation_handler))
            // Trail
            .route("/v1/trail", get(query_trail_handler))
            // SSE streams
            .route("/v1/events/trail", get(stream_trail_handler))
            .route("/v1/events/gates", get(stream_gates_handler))
            .route("/v1/events/escalations", get(stream_escalations_handler))
            .route("/v1/events/signals", get(stream_signals_handler))
            .route("/v1/events/workspaces", get(stream_workspaces_handler))
            .with_state(state);

        Self { router }
    }

    /// Get the axum router for embedding in a server.
    pub fn into_router(self) -> Router {
        self.router
    }
}

// ── Endpoint handlers (stubs — will connect to gRPC clients in Phase 27) ──

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn create_session_handler(
    State(_state): State<GatewayState>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse {
        error: "session creation not yet wired".into(),
        code: "not_implemented".into(),
    }))
}

async fn delete_session_handler(
    State(_state): State<GatewayState>,
    Path(_id): Path<String>,
) -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

async fn submit_goal_handler(
    State(_state): State<GatewayState>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse {
        error: "goal submission not yet wired".into(),
        code: "not_implemented".into(),
    }))
}

async fn get_ready_tasks_handler(State(_state): State<GatewayState>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!([])))
}

async fn get_task_graph_handler(State(_state): State<GatewayState>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({})))
}

async fn get_workspace_handler(
    State(_state): State<GatewayState>,
    Path(_id): Path<String>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({})))
}

async fn dispatch_handler(State(_state): State<GatewayState>, Path(_id): Path<String>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn abort_handler(State(_state): State<GatewayState>, Path(_id): Path<String>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn suspend_handler(State(_state): State<GatewayState>, Path(_id): Path<String>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn resume_handler(State(_state): State<GatewayState>, Path(_id): Path<String>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn inject_handler(State(_state): State<GatewayState>, Path(_id): Path<String>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn integrate_handler(State(_state): State<GatewayState>, Path(_id): Path<String>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn list_gates_handler(State(_state): State<GatewayState>) -> impl IntoResponse { (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!([]))) }
async fn respond_gate_handler(State(_state): State<GatewayState>, Path(_id): Path<String>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn list_escalations_handler(State(_state): State<GatewayState>) -> impl IntoResponse { (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!([]))) }
async fn respond_escalation_handler(State(_state): State<GatewayState>, Path(_id): Path<String>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn query_trail_handler(State(_state): State<GatewayState>) -> impl IntoResponse { (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!([]))) }
async fn stream_trail_handler(State(_state): State<GatewayState>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn stream_gates_handler(State(_state): State<GatewayState>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn stream_escalations_handler(State(_state): State<GatewayState>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn stream_signals_handler(State(_state): State<GatewayState>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }
async fn stream_workspaces_handler(State(_state): State<GatewayState>) -> StatusCode { StatusCode::NOT_IMPLEMENTED }

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn test_app() -> Router {
        RestGateway::new(GatewayState {
            _placeholder: Arc::new(()),
        })
        .into_router()
    }

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        let app = test_app();
        let response = app
            .oneshot(Request::get("/v1/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let app = test_app();
        let response = app
            .oneshot(Request::get("/v1/nonexistent").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn goal_endpoint_exists() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::post("/v1/goals")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Endpoint exists (returns 501 not implemented, not 404)
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn workspace_endpoint_exists() {
        let app = test_app();
        let response = app
            .oneshot(Request::get("/v1/workspaces/ws-1").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn trail_endpoint_exists() {
        let app = test_app();
        let response = app
            .oneshot(Request::get("/v1/trail").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }

    // --- Error mapping ---

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

    #[test]
    fn http_status_to_code_mapping() {
        assert_eq!(http_status_to_code(StatusCode::BAD_REQUEST), "invalid_request");
        assert_eq!(http_status_to_code(StatusCode::UNAUTHORIZED), "unauthenticated");
        assert_eq!(http_status_to_code(StatusCode::NOT_FOUND), "not_found");
        assert_eq!(http_status_to_code(StatusCode::TOO_MANY_REQUESTS), "rate_limited");
    }

    #[tokio::test]
    async fn sse_trail_endpoint_exists() {
        let app = test_app();
        let response = app
            .oneshot(Request::get("/v1/events/trail").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // Exists (not 404)
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn sse_signals_endpoint_exists() {
        let app = test_app();
        let response = app
            .oneshot(Request::get("/v1/events/signals").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }
}
