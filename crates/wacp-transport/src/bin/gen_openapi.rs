//! Generates `openapi.yaml` from the utoipa-annotated REST gateway handlers.
//!
//! Run: `cargo run -p wacp-transport --bin gen_openapi`
//! CI:  `cargo run -p wacp-transport --bin gen_openapi > openapi.yaml && git diff --exit-code openapi.yaml`

use utoipa::OpenApi;
use wacp_transport::rest_gateway::*;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "WACP Runtime REST API",
        version = "0.1.0",
        description = "REST gateway for the Workspace Agent Coordination Protocol runtime.",
        license(name = "Apache-2.0"),
    ),
    paths(
        health_handler,
        submit_goal_handler,
        get_ready_tasks_handler,
        get_workspace_handler,
        dispatch_handler,
        abort_handler,
        suspend_handler,
        resume_handler,
        inject_handler,
        integrate_handler,
        respond_gate_handler,
        respond_escalation_handler,
        query_trail_handler,
        get_allocatable_handler,
        list_verticals_handler,
        get_vertical_handler,
    ),
    components(schemas(
        ErrorResponse,
        HealthResponse,
        SubmitGoalBody,
        GoalCreatedResponse,
        TaskListItem,
        WorkspaceResponse,
        DispatchBody,
        DispatchCreatedResponse,
        AbortBody,
        InjectBody,
        InjectCreatedResponse,
        IntegrationResponse,
        GateResponseBody,
        EscalationResponseBody,
        TrailEntryItem,
        BudgetResponse,
        VerticalSummary,
    )),
    tags(
        (name = "health", description = "Runtime health"),
        (name = "goals", description = "Goal submission"),
        (name = "tasks", description = "Task queries"),
        (name = "workspaces", description = "Workspace lifecycle"),
        (name = "gates", description = "Gate responses"),
        (name = "escalations", description = "Escalation responses"),
        (name = "trail", description = "Audit trail"),
        (name = "resources", description = "Resource budget"),
        (name = "verticals", description = "Vertical discovery"),
    )
)]
struct ApiDoc;

fn main() {
    let doc = ApiDoc::openapi();
    let yaml = doc.to_yaml().expect("serialization");
    print!("{yaml}");
}
