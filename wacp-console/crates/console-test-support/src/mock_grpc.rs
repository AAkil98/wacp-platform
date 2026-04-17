//! Minimal gRPC service implementations for the mock runtime.
//!
//! These return `Unimplemented` for most RPCs. Phase 4 will flesh out
//! streaming responses for highway integration tests. For Phase 0, the
//! goal is: the servers start, accept connections, and respond.

use std::sync::Arc;

use tonic::{Request, Response, Status};

// Server-side generated traits sourced from the shared `wacp-proto` crate.
use wacp_proto::wacp_v1 as proto;

pub use proto::agent_service_server::{AgentService, AgentServiceServer};
pub use proto::coordinator_service_server::{CoordinatorService, CoordinatorServiceServer};
pub use proto::highway_service_server::{HighwayService, HighwayServiceServer};

// ---------------------------------------------------------------------------
// AgentService
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct MockAgentService;

#[tonic::async_trait]
impl AgentService for MockAgentService {
    async fn bind(
        &self,
        _request: Request<proto::BindRequest>,
    ) -> Result<Response<proto::BindResponse>, Status> {
        Ok(Response::new(proto::BindResponse {
            workspace_id: String::new(),
            state: 0,
            role: String::new(),
            directive: None,
            context: Vec::new(),
            authority: vec![],
            budget: None,
            visibility: vec![],
        }))
    }

    async fn send_envelope(
        &self,
        _request: Request<proto::SendEnvelopeRequest>,
    ) -> Result<Response<proto::SendEnvelopeResponse>, Status> {
        Err(Status::unimplemented("mock: send_envelope not implemented"))
    }

    async fn emit_signal(
        &self,
        _request: Request<proto::EmitSignalRequest>,
    ) -> Result<Response<proto::EmitSignalResponse>, Status> {
        Err(Status::unimplemented("mock: emit_signal not implemented"))
    }

    async fn create_checkpoint(
        &self,
        _request: Request<proto::CreateCheckpointRequest>,
    ) -> Result<Response<proto::CreateCheckpointResponse>, Status> {
        Err(Status::unimplemented(
            "mock: create_checkpoint not implemented",
        ))
    }

    async fn query_trail(
        &self,
        _request: Request<proto::QueryTrailRequest>,
    ) -> Result<Response<proto::QueryTrailResponse>, Status> {
        Ok(Response::new(proto::QueryTrailResponse {
            entries: vec![],
            has_more: false,
            client_request_id: String::new(),
        }))
    }

    async fn read_resource(
        &self,
        _request: Request<proto::ReadResourceRequest>,
    ) -> Result<Response<proto::ReadResourceResponse>, Status> {
        Err(Status::unimplemented("mock: read_resource not implemented"))
    }

    type ReceiveEnvelopesStream =
        tokio_stream::wrappers::ReceiverStream<Result<proto::Envelope, Status>>;

    async fn receive_envelopes(
        &self,
        _request: Request<proto::ReceiveEnvelopesRequest>,
    ) -> Result<Response<Self::ReceiveEnvelopesStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    type ReceiveCommandsStream =
        tokio_stream::wrappers::ReceiverStream<Result<proto::Command, Status>>;

    async fn receive_commands(
        &self,
        _request: Request<proto::ReceiveCommandsRequest>,
    ) -> Result<Response<Self::ReceiveCommandsStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}

// ---------------------------------------------------------------------------
// HighwayService — configurable
// ---------------------------------------------------------------------------

/// Per-RPC outcome a configurable mock can be programmed to return. Each
/// W4 test that needs a non-happy path attaches a custom outcome.
#[derive(Debug, Clone, Default)]
pub enum HighwayOutcome {
    /// Default — return Ok with `applied=true`.
    #[default]
    Ok,
    /// Return Ok but the runtime claims the action did not apply (gate
    /// already resolved, escalation already closed). Spec wcon-w4 §3.1.
    OkNotApplied,
    /// Map to the listed gRPC status code.
    Err(tonic::Code, &'static str),
}

/// Configurable behaviour driver for `MockHighwayService`. The W4 tests
/// install a config, then make HTTP requests against the console. Per-RPC
/// outcomes can be set independently; default is Ok everywhere.
///
/// Also captures the last request seen on each RPC for assertions.
#[derive(Debug, Default)]
pub struct HighwayConfig {
    pub respond_to_gate: std::sync::Mutex<HighwayOutcome>,
    pub respond_to_escalation: std::sync::Mutex<HighwayOutcome>,
    pub inject_envelope: std::sync::Mutex<HighwayOutcome>,

    pub last_gate: std::sync::Mutex<Option<proto::GateResponse>>,
    pub last_escalation: std::sync::Mutex<Option<proto::EscalationResponse>>,
    pub last_inject: std::sync::Mutex<Option<proto::InjectEnvelopeRequest>>,

    /// Sequence of `respond_to_gate` outcomes consumed in order. When
    /// non-empty, takes precedence over `respond_to_gate`. Allows tests to
    /// alternate Ok / Err across a batch.
    pub gate_sequence: std::sync::Mutex<std::collections::VecDeque<HighwayOutcome>>,
}

impl HighwayConfig {
    pub fn arc() -> Arc<Self> {
        Arc::new(Self::default())
    }
    pub fn set_gate(&self, o: HighwayOutcome) {
        *self.respond_to_gate.lock().unwrap() = o;
    }
    pub fn set_escalation(&self, o: HighwayOutcome) {
        *self.respond_to_escalation.lock().unwrap() = o;
    }
    pub fn set_inject(&self, o: HighwayOutcome) {
        *self.inject_envelope.lock().unwrap() = o;
    }
    pub fn push_gate_sequence(&self, items: impl IntoIterator<Item = HighwayOutcome>) {
        let mut q = self.gate_sequence.lock().unwrap();
        q.extend(items);
    }
}

/// `MockHighwayService` defaults to Ok behaviour everywhere so that existing
/// tests don't have to wire a config. W4 tests that need failure paths
/// construct a `HighwayConfig`, attach it via `MockHighwayService::with_config`,
/// and program per-RPC outcomes.
#[derive(Debug, Default)]
pub struct MockHighwayService {
    config: Option<Arc<HighwayConfig>>,
}

impl MockHighwayService {
    pub fn with_config(config: Arc<HighwayConfig>) -> Self {
        Self {
            config: Some(config),
        }
    }

    fn outcome_for(&self, kind: HighwayRpc) -> HighwayOutcome {
        let Some(cfg) = &self.config else {
            return HighwayOutcome::Ok;
        };
        match kind {
            HighwayRpc::RespondToGate => {
                let mut seq = cfg.gate_sequence.lock().unwrap();
                if let Some(o) = seq.pop_front() {
                    return o;
                }
                cfg.respond_to_gate.lock().unwrap().clone()
            }
            HighwayRpc::RespondToEscalation => cfg.respond_to_escalation.lock().unwrap().clone(),
            HighwayRpc::InjectEnvelope => cfg.inject_envelope.lock().unwrap().clone(),
        }
    }
}

#[derive(Copy, Clone)]
enum HighwayRpc {
    RespondToGate,
    RespondToEscalation,
    InjectEnvelope,
}

#[tonic::async_trait]
impl HighwayService for MockHighwayService {
    async fn authenticate(
        &self,
        _request: Request<proto::AuthenticateRequest>,
    ) -> Result<Response<proto::AuthenticateResponse>, Status> {
        Ok(Response::new(proto::AuthenticateResponse {
            user_id: "mock-user".into(),
            capabilities: vec!["observe".into(), "inject".into(), "gate".into()],
        }))
    }

    async fn inject_envelope(
        &self,
        request: Request<proto::InjectEnvelopeRequest>,
    ) -> Result<Response<proto::InjectEnvelopeResponse>, Status> {
        let req = request.into_inner();
        if let Some(cfg) = &self.config {
            *cfg.last_inject.lock().unwrap() = Some(req.clone());
        }
        match self.outcome_for(HighwayRpc::InjectEnvelope) {
            HighwayOutcome::Ok | HighwayOutcome::OkNotApplied => {
                Ok(Response::new(proto::InjectEnvelopeResponse {
                    envelope_id: format!("env-{}", req.client_request_id),
                    timestamp: None,
                    client_request_id: req.client_request_id,
                }))
            }
            HighwayOutcome::Err(code, msg) => Err(Status::new(code, msg)),
        }
    }

    async fn respond_to_gate(
        &self,
        request: Request<proto::GateResponse>,
    ) -> Result<Response<proto::GateResponseAck>, Status> {
        let req = request.into_inner();
        if let Some(cfg) = &self.config {
            *cfg.last_gate.lock().unwrap() = Some(req.clone());
        }
        match self.outcome_for(HighwayRpc::RespondToGate) {
            HighwayOutcome::Ok => Ok(Response::new(proto::GateResponseAck {
                gate_id: req.gate_id,
                applied: true,
                client_request_id: req.client_request_id,
            })),
            HighwayOutcome::OkNotApplied => Ok(Response::new(proto::GateResponseAck {
                gate_id: req.gate_id,
                applied: false,
                client_request_id: req.client_request_id,
            })),
            HighwayOutcome::Err(code, msg) => Err(Status::new(code, msg)),
        }
    }

    async fn respond_to_escalation(
        &self,
        request: Request<proto::EscalationResponse>,
    ) -> Result<Response<proto::EscalationResponseAck>, Status> {
        let req = request.into_inner();
        if let Some(cfg) = &self.config {
            *cfg.last_escalation.lock().unwrap() = Some(req.clone());
        }
        match self.outcome_for(HighwayRpc::RespondToEscalation) {
            HighwayOutcome::Ok => Ok(Response::new(proto::EscalationResponseAck {
                escalation_id: req.escalation_id,
                applied: true,
                client_request_id: req.client_request_id,
            })),
            HighwayOutcome::OkNotApplied => Ok(Response::new(proto::EscalationResponseAck {
                escalation_id: req.escalation_id,
                applied: false,
                client_request_id: req.client_request_id,
            })),
            HighwayOutcome::Err(code, msg) => Err(Status::new(code, msg)),
        }
    }

    async fn query_trail(
        &self,
        _request: Request<proto::HighwayQueryTrailRequest>,
    ) -> Result<Response<proto::QueryTrailResponse>, Status> {
        Ok(Response::new(proto::QueryTrailResponse {
            entries: vec![],
            has_more: false,
            client_request_id: String::new(),
        }))
    }

    async fn get_workspace(
        &self,
        _request: Request<proto::GetWorkspaceRequest>,
    ) -> Result<Response<proto::WorkspaceView>, Status> {
        Err(Status::unimplemented("mock: get_workspace not implemented"))
    }

    async fn get_task_graph(
        &self,
        _request: Request<proto::GetTaskGraphRequest>,
    ) -> Result<Response<proto::TaskGraphView>, Status> {
        Err(Status::unimplemented(
            "mock: get_task_graph not implemented",
        ))
    }

    async fn get_checkpoint(
        &self,
        _request: Request<proto::GetCheckpointRequest>,
    ) -> Result<Response<proto::CheckpointView>, Status> {
        Err(Status::unimplemented(
            "mock: get_checkpoint not implemented",
        ))
    }

    type StreamTrailStream =
        tokio_stream::wrappers::ReceiverStream<Result<proto::TrailEntry, Status>>;

    async fn stream_trail(
        &self,
        _request: Request<proto::StreamTrailRequest>,
    ) -> Result<Response<Self::StreamTrailStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    type StreamGatesStream =
        tokio_stream::wrappers::ReceiverStream<Result<proto::GateEvent, Status>>;

    async fn stream_gates(
        &self,
        _request: Request<proto::StreamGatesRequest>,
    ) -> Result<Response<Self::StreamGatesStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    type StreamEscalationsStream =
        tokio_stream::wrappers::ReceiverStream<Result<proto::EscalationEvent, Status>>;

    async fn stream_escalations(
        &self,
        _request: Request<proto::StreamEscalationsRequest>,
    ) -> Result<Response<Self::StreamEscalationsStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    type StreamWorkspaceChangesStream =
        tokio_stream::wrappers::ReceiverStream<Result<proto::WorkspaceStateChange, Status>>;

    async fn stream_workspace_changes(
        &self,
        _request: Request<proto::StreamWorkspaceChangesRequest>,
    ) -> Result<Response<Self::StreamWorkspaceChangesStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}

// ---------------------------------------------------------------------------
// CoordinatorService
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct MockCoordinatorService;

#[tonic::async_trait]
impl CoordinatorService for MockCoordinatorService {
    async fn submit_goal(
        &self,
        _request: Request<proto::SubmitGoalRequest>,
    ) -> Result<Response<proto::SubmitGoalResponse>, Status> {
        Ok(Response::new(proto::SubmitGoalResponse {
            goal_id: "mock-goal-1".into(),
            root_workspace_id: "mock-root-ws-1".into(),
        }))
    }

    async fn decompose(
        &self,
        _request: Request<proto::DecomposeRequest>,
    ) -> Result<Response<proto::DecomposeResponse>, Status> {
        Ok(Response::new(proto::DecomposeResponse {
            task_ids: vec!["mock-task-1".into()],
        }))
    }

    async fn get_ready_tasks(
        &self,
        _request: Request<proto::GetReadyTasksRequest>,
    ) -> Result<Response<proto::GetReadyTasksResponse>, Status> {
        Ok(Response::new(proto::GetReadyTasksResponse {
            tasks: vec![],
        }))
    }

    async fn cancel_task(
        &self,
        _request: Request<proto::CancelTaskRequest>,
    ) -> Result<Response<proto::CancelTaskResponse>, Status> {
        Ok(Response::new(proto::CancelTaskResponse {}))
    }

    async fn dispatch(
        &self,
        _request: Request<proto::DispatchRequest>,
    ) -> Result<Response<proto::DispatchResponse>, Status> {
        Ok(Response::new(proto::DispatchResponse {
            workspace_id: "mock-workspace-1".into(),
            task_id: "mock-task-1".into(),
        }))
    }

    async fn abort_workspace(
        &self,
        _request: Request<proto::AbortWorkspaceRequest>,
    ) -> Result<Response<proto::AbortWorkspaceResponse>, Status> {
        Ok(Response::new(proto::AbortWorkspaceResponse {}))
    }

    async fn suspend_workspace(
        &self,
        _request: Request<proto::SuspendWorkspaceRequest>,
    ) -> Result<Response<proto::SuspendWorkspaceResponse>, Status> {
        Ok(Response::new(proto::SuspendWorkspaceResponse {}))
    }

    async fn resume_workspace(
        &self,
        _request: Request<proto::ResumeWorkspaceRequest>,
    ) -> Result<Response<proto::ResumeWorkspaceResponse>, Status> {
        Ok(Response::new(proto::ResumeWorkspaceResponse {}))
    }

    async fn send_directive(
        &self,
        _request: Request<proto::SendDirectiveRequest>,
    ) -> Result<Response<proto::SendDirectiveResponse>, Status> {
        Ok(Response::new(proto::SendDirectiveResponse {
            envelope_id: "mock-env-1".into(),
        }))
    }

    async fn send_feedback(
        &self,
        _request: Request<proto::SendFeedbackRequest>,
    ) -> Result<Response<proto::SendFeedbackResponse>, Status> {
        Ok(Response::new(proto::SendFeedbackResponse {
            envelope_id: "mock-env-2".into(),
        }))
    }

    async fn trigger_integration(
        &self,
        _request: Request<proto::TriggerIntegrationRequest>,
    ) -> Result<Response<proto::TriggerIntegrationResponse>, Status> {
        Ok(Response::new(proto::TriggerIntegrationResponse {
            result: "accepted".into(),
            detail: String::new(),
        }))
    }

    async fn get_allocatable(
        &self,
        _request: Request<proto::GetAllocatableRequest>,
    ) -> Result<Response<proto::GetAllocatableResponse>, Status> {
        Ok(Response::new(proto::GetAllocatableResponse {
            remaining: None,
        }))
    }

    type StreamSignalsStream =
        tokio_stream::wrappers::ReceiverStream<Result<proto::SignalEvent, Status>>;

    async fn stream_signals(
        &self,
        _request: Request<proto::StreamSignalsRequest>,
    ) -> Result<Response<Self::StreamSignalsStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}
