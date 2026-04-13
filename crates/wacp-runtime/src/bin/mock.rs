//! Mock WACP runtime for integration testing.
//!
//! Starts gRPC + REST on canonical ports with an in-memory backend and
//! fixture vertical manifests. Intended as an E2E sidecar for `wacp-console`.
//!
//! Usage:
//!   wacp-mock-runtime                     # 1 stub vertical
//!   wacp-mock-runtime --fixtures complex  # 7 stub verticals (all ecosystem IDs)
//!   wacp-mock-runtime --rest-port 9193    # override REST port

use std::sync::Arc;

use axum::Router;
use clap::Parser;
use wacp_taxonomy::VerticalManifest;
use wacp_transport::rest_gateway::{
    GatewayBackend, GatewayError, RestGateway, WorkspaceSummaryItem,
};
use wacp_transport::websocket::ws_handler;
use wacp_transport::{GrpcServerConfig, start_grpc_server};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "wacp-mock-runtime", about = "Mock WACP runtime for testing")]
struct Cli {
    /// Fixture profile: "simple" (1 vertical) or "complex" (all 7).
    #[arg(long, default_value = "simple")]
    fixtures: String,

    /// REST gateway + WebSocket port.
    #[arg(long, default_value = "9093")]
    rest_port: u16,

    /// Agent gRPC port.
    #[arg(long, default_value = "9090")]
    agent_port: u16,

    /// Highway gRPC port.
    #[arg(long, default_value = "9091")]
    highway_port: u16,

    /// Coordinator gRPC port.
    #[arg(long, default_value = "9092")]
    coordinator_port: u16,
}

// ---------------------------------------------------------------------------
// In-memory backend
// ---------------------------------------------------------------------------

struct MockBackend;

#[tonic::async_trait]
impl GatewayBackend for MockBackend {
    async fn submit_goal(
        &self,
        req: wacp_transport::wacp_v1::SubmitGoalRequest,
    ) -> Result<wacp_transport::wacp_v1::SubmitGoalResponse, GatewayError> {
        Ok(wacp_transport::wacp_v1::SubmitGoalResponse {
            goal_id: format!(
                "goal-mock-{}",
                &req.description[..8.min(req.description.len())]
            ),
            root_workspace_id: "ws-mock-root".into(),
        })
    }

    async fn get_ready_tasks(
        &self,
    ) -> Result<wacp_transport::wacp_v1::GetReadyTasksResponse, GatewayError> {
        Ok(wacp_transport::wacp_v1::GetReadyTasksResponse { tasks: vec![] })
    }

    async fn dispatch(
        &self,
        req: wacp_transport::wacp_v1::DispatchRequest,
    ) -> Result<wacp_transport::wacp_v1::DispatchResponse, GatewayError> {
        Ok(wacp_transport::wacp_v1::DispatchResponse {
            workspace_id: format!("ws-mock-{}", req.task_id),
            task_id: req.task_id,
        })
    }

    async fn abort_workspace(
        &self,
        _req: wacp_transport::wacp_v1::AbortWorkspaceRequest,
    ) -> Result<wacp_transport::wacp_v1::AbortWorkspaceResponse, GatewayError> {
        Ok(wacp_transport::wacp_v1::AbortWorkspaceResponse::default())
    }

    async fn suspend_workspace(
        &self,
        _req: wacp_transport::wacp_v1::SuspendWorkspaceRequest,
    ) -> Result<wacp_transport::wacp_v1::SuspendWorkspaceResponse, GatewayError> {
        Ok(wacp_transport::wacp_v1::SuspendWorkspaceResponse::default())
    }

    async fn resume_workspace(
        &self,
        _req: wacp_transport::wacp_v1::ResumeWorkspaceRequest,
    ) -> Result<wacp_transport::wacp_v1::ResumeWorkspaceResponse, GatewayError> {
        Ok(wacp_transport::wacp_v1::ResumeWorkspaceResponse::default())
    }

    async fn get_workspace(
        &self,
        id: &str,
    ) -> Result<wacp_transport::wacp_v1::WorkspaceView, GatewayError> {
        Ok(wacp_transport::wacp_v1::WorkspaceView {
            id: id.into(),
            state: 1, // Active
            role: "worker".into(),
            ..Default::default()
        })
    }

    async fn inject_envelope(
        &self,
        _req: wacp_transport::wacp_v1::InjectEnvelopeRequest,
    ) -> Result<wacp_transport::wacp_v1::InjectEnvelopeResponse, GatewayError> {
        Ok(wacp_transport::wacp_v1::InjectEnvelopeResponse {
            envelope_id: "env-mock-1".into(),
            ..Default::default()
        })
    }

    async fn trigger_integration(
        &self,
        _req: wacp_transport::wacp_v1::TriggerIntegrationRequest,
    ) -> Result<wacp_transport::wacp_v1::TriggerIntegrationResponse, GatewayError> {
        Ok(wacp_transport::wacp_v1::TriggerIntegrationResponse {
            result: "accepted".into(),
            detail: "mock".into(),
        })
    }

    async fn query_trail(
        &self,
        _req: wacp_transport::wacp_v1::HighwayQueryTrailRequest,
    ) -> Result<wacp_transport::wacp_v1::QueryTrailResponse, GatewayError> {
        Ok(wacp_transport::wacp_v1::QueryTrailResponse::default())
    }

    async fn respond_to_gate(
        &self,
        _req: wacp_transport::wacp_v1::GateResponse,
    ) -> Result<wacp_transport::wacp_v1::GateResponseAck, GatewayError> {
        Ok(wacp_transport::wacp_v1::GateResponseAck::default())
    }

    async fn respond_to_escalation(
        &self,
        _req: wacp_transport::wacp_v1::EscalationResponse,
    ) -> Result<wacp_transport::wacp_v1::EscalationResponseAck, GatewayError> {
        Ok(wacp_transport::wacp_v1::EscalationResponseAck::default())
    }

    async fn get_allocatable(
        &self,
    ) -> Result<wacp_transport::wacp_v1::GetAllocatableResponse, GatewayError> {
        Ok(wacp_transport::wacp_v1::GetAllocatableResponse {
            remaining: Some(wacp_transport::wacp_v1::ResourceBudget {
                max_tokens: 1_000_000,
                max_wall_time_ms: 300_000,
                max_storage_bytes: 104_857_600,
                max_network_bytes: 52_428_800,
                max_cost_micros: 10_000_000,
                warning_threshold: 0.8,
            }),
        })
    }

    async fn list_workspaces(
        &self,
        parent_id: Option<&str>,
    ) -> Result<Vec<WorkspaceSummaryItem>, GatewayError> {
        let pid = parent_id.unwrap_or("ws-mock-root");
        Ok(vec![WorkspaceSummaryItem {
            id: format!("{pid}-child-1"),
            parent_id: pid.into(),
            state: 1,
            owner: "system".into(),
            task_id: "t-mock-1".into(),
        }])
    }
}

// ---------------------------------------------------------------------------
// Fixture manifests
// ---------------------------------------------------------------------------

fn stub_vertical(id: &str) -> VerticalManifest {
    use wacp_taxonomy::{TaskTypeDescriptor, ToolSummary, WorkflowSummary};
    VerticalManifest {
        id: id.into(),
        name: format!("{id} vertical"),
        defining_constraint: format!("{id} mock constraint"),
        context_schema: Default::default(),
        tool_policies: Default::default(),
        checkpoint_types: Default::default(),
        quality_criteria: vec![],
        task_types: vec![TaskTypeDescriptor {
            id: format!("{id}:task-1"),
            name: "default task".into(),
            description: "mock task type".into(),
            workflow_id: format!("{id}:default-workflow"),
            keywords: vec![id.into()],
        }],
        workflows: vec![WorkflowSummary {
            id: format!("{id}:default-workflow"),
            name: "default workflow".into(),
            description: "mock workflow".into(),
            stage_count: 3,
            gated_stage_count: 1,
        }],
        profiles: vec![],
        tools: vec![ToolSummary {
            name: format!("{id}:tool-1"),
            description: "mock tool".into(),
        }],
    }
}

fn load_fixtures(profile: &str) -> Vec<VerticalManifest> {
    match profile {
        "complex" => vec![
            stub_vertical("swe"),
            stub_vertical("devops"),
            stub_vertical("mlops"),
            stub_vertical("finance"),
            stub_vertical("healthcare"),
            stub_vertical("analytics"),
            stub_vertical("datasci"),
        ],
        _ => vec![stub_vertical("swe")],
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let manifests = load_fixtures(&cli.fixtures);

    eprintln!(
        "wacp-mock-runtime starting — fixtures={}, verticals={}, ports=agent:{}/highway:{}/coordinator:{}/rest:{}",
        cli.fixtures,
        manifests.len(),
        cli.agent_port,
        cli.highway_port,
        cli.coordinator_port,
        cli.rest_port,
    );

    // Start gRPC services.
    let grpc_config = GrpcServerConfig {
        agent_addr: ([0, 0, 0, 0], cli.agent_port).into(),
        highway_addr: ([0, 0, 0, 0], cli.highway_port).into(),
        coordinator_addr: ([0, 0, 0, 0], cli.coordinator_port).into(),
        tls: None,
    };
    let _grpc = start_grpc_server(grpc_config)
        .await
        .expect("gRPC server failed to start");
    eprintln!("  gRPC services listening");

    // Build REST + WebSocket router.
    let backend: Arc<dyn GatewayBackend> = Arc::new(MockBackend);
    let rest_router = RestGateway::router(backend.clone(), Arc::new(manifests));
    let ws_router = Router::new()
        .route("/v1/ws", axum::routing::get(ws_handler))
        .with_state(backend);
    let app = rest_router.merge(ws_router);

    // Start REST gateway.
    let rest_addr = std::net::SocketAddr::from(([0, 0, 0, 0], cli.rest_port));
    let listener = tokio::net::TcpListener::bind(rest_addr)
        .await
        .expect("REST bind failed");
    eprintln!("  REST gateway listening on {rest_addr}");
    eprintln!("wacp-mock-runtime ready");

    axum::serve(listener, app)
        .await
        .expect("REST server failed");
}
