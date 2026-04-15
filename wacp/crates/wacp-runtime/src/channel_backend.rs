//! `ChannelBackend` — a `GatewayBackend` implementation that sends requests
//! through the same mpsc channels the gRPC handlers use, so both REST and
//! gRPC traffic is processed by the single coordinator event loop.

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tonic::Status;
use wacp_transport::{
    CoordinatorRequest, GatewayBackend, GatewayError, HighwayRequest, WorkspaceSummaryItem,
    proto::wacp_v1,
};

/// Bridges REST gateway calls to the coordinator event loop via channels.
pub struct ChannelBackend {
    highway_tx: mpsc::Sender<HighwayRequest>,
    coordinator_tx: mpsc::Sender<CoordinatorRequest>,
}

impl ChannelBackend {
    pub fn new(
        highway_tx: mpsc::Sender<HighwayRequest>,
        coordinator_tx: mpsc::Sender<CoordinatorRequest>,
    ) -> Arc<Self> {
        Arc::new(Self {
            highway_tx,
            coordinator_tx,
        })
    }

    async fn send_highway<T>(
        &self,
        make_req: impl FnOnce(oneshot::Sender<Result<T, Status>>) -> HighwayRequest,
    ) -> Result<T, GatewayError> {
        let (tx, rx) = oneshot::channel();
        self.highway_tx
            .send(make_req(tx))
            .await
            .map_err(|_| GatewayError::internal("coordinator unavailable"))?;
        rx.await
            .map_err(|_| GatewayError::internal("coordinator dropped reply"))?
            .map_err(GatewayError::from_status)
    }

    async fn send_coordinator<T>(
        &self,
        make_req: impl FnOnce(oneshot::Sender<Result<T, Status>>) -> CoordinatorRequest,
    ) -> Result<T, GatewayError> {
        let (tx, rx) = oneshot::channel();
        self.coordinator_tx
            .send(make_req(tx))
            .await
            .map_err(|_| GatewayError::internal("coordinator unavailable"))?;
        rx.await
            .map_err(|_| GatewayError::internal("coordinator dropped reply"))?
            .map_err(GatewayError::from_status)
    }
}

#[tonic::async_trait]
impl GatewayBackend for ChannelBackend {
    async fn submit_goal(
        &self,
        req: wacp_v1::SubmitGoalRequest,
    ) -> Result<wacp_v1::SubmitGoalResponse, GatewayError> {
        self.send_coordinator(|reply| CoordinatorRequest::SubmitGoal {
            request: req,
            reply,
        })
        .await
    }

    async fn get_ready_tasks(&self) -> Result<wacp_v1::GetReadyTasksResponse, GatewayError> {
        self.send_coordinator(|reply| CoordinatorRequest::GetReadyTasks {
            request: wacp_v1::GetReadyTasksRequest::default(),
            reply,
        })
        .await
    }

    async fn dispatch(
        &self,
        req: wacp_v1::DispatchRequest,
    ) -> Result<wacp_v1::DispatchResponse, GatewayError> {
        self.send_coordinator(|reply| CoordinatorRequest::Dispatch {
            request: req,
            reply,
        })
        .await
    }

    async fn abort_workspace(
        &self,
        req: wacp_v1::AbortWorkspaceRequest,
    ) -> Result<wacp_v1::AbortWorkspaceResponse, GatewayError> {
        self.send_coordinator(|reply| CoordinatorRequest::AbortWorkspace {
            request: req,
            reply,
        })
        .await
    }

    async fn suspend_workspace(
        &self,
        req: wacp_v1::SuspendWorkspaceRequest,
    ) -> Result<wacp_v1::SuspendWorkspaceResponse, GatewayError> {
        self.send_coordinator(|reply| CoordinatorRequest::SuspendWorkspace {
            request: req,
            reply,
        })
        .await
    }

    async fn resume_workspace(
        &self,
        req: wacp_v1::ResumeWorkspaceRequest,
    ) -> Result<wacp_v1::ResumeWorkspaceResponse, GatewayError> {
        self.send_coordinator(|reply| CoordinatorRequest::ResumeWorkspace {
            request: req,
            reply,
        })
        .await
    }

    async fn get_workspace(&self, id: &str) -> Result<wacp_v1::WorkspaceView, GatewayError> {
        let ws_id = id.to_string();
        self.send_highway(|reply| HighwayRequest::GetWorkspace {
            request: wacp_v1::GetWorkspaceRequest {
                workspace_id: ws_id,
            },
            reply,
        })
        .await
    }

    async fn inject_envelope(
        &self,
        req: wacp_v1::InjectEnvelopeRequest,
    ) -> Result<wacp_v1::InjectEnvelopeResponse, GatewayError> {
        self.send_highway(|reply| HighwayRequest::InjectEnvelope {
            request: req,
            reply,
        })
        .await
    }

    async fn trigger_integration(
        &self,
        req: wacp_v1::TriggerIntegrationRequest,
    ) -> Result<wacp_v1::TriggerIntegrationResponse, GatewayError> {
        self.send_coordinator(|reply| CoordinatorRequest::TriggerIntegration {
            request: req,
            reply,
        })
        .await
    }

    async fn query_trail(
        &self,
        req: wacp_v1::HighwayQueryTrailRequest,
    ) -> Result<wacp_v1::QueryTrailResponse, GatewayError> {
        self.send_highway(|reply| HighwayRequest::QueryTrail {
            request: req,
            reply,
        })
        .await
    }

    async fn respond_to_gate(
        &self,
        req: wacp_v1::GateResponse,
    ) -> Result<wacp_v1::GateResponseAck, GatewayError> {
        self.send_highway(|reply| HighwayRequest::RespondToGate {
            request: req,
            reply,
        })
        .await
    }

    async fn respond_to_escalation(
        &self,
        req: wacp_v1::EscalationResponse,
    ) -> Result<wacp_v1::EscalationResponseAck, GatewayError> {
        self.send_highway(|reply| HighwayRequest::RespondToEscalation {
            request: req,
            reply,
        })
        .await
    }

    async fn get_allocatable(&self) -> Result<wacp_v1::GetAllocatableResponse, GatewayError> {
        self.send_coordinator(|reply| CoordinatorRequest::GetAllocatable {
            request: wacp_v1::GetAllocatableRequest::default(),
            reply,
        })
        .await
    }

    async fn list_workspaces(
        &self,
        parent_id: Option<&str>,
    ) -> Result<Vec<WorkspaceSummaryItem>, GatewayError> {
        let parent = parent_id.map(|s| s.to_string());
        let (tx, rx) = oneshot::channel();
        self.highway_tx
            .send(HighwayRequest::ListWorkspaces {
                parent_id: parent,
                reply: tx,
            })
            .await
            .map_err(|_| GatewayError::internal("coordinator unavailable"))?;
        rx.await
            .map_err(|_| GatewayError::internal("coordinator dropped reply"))?
            .map_err(GatewayError::from_status)
    }
}
