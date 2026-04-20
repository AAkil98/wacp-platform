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
