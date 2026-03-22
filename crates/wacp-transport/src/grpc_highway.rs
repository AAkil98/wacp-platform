use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::proto::wacp_v1;
use crate::proto::wacp_v1::highway_service_server::HighwayService;

/// Channel-based highway service implementation.
pub struct HighwayServiceImpl {
    coordinator_tx: mpsc::Sender<HighwayRequest>,
}

/// A highway request forwarded to the coordinator.
#[derive(Debug)]
pub enum HighwayRequest {
    Authenticate {
        request: wacp_v1::AuthenticateRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::AuthenticateResponse, Status>>,
    },
    InjectEnvelope {
        request: wacp_v1::InjectEnvelopeRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::InjectEnvelopeResponse, Status>>,
    },
    RespondToGate {
        request: wacp_v1::GateResponse,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::GateResponseAck, Status>>,
    },
    RespondToEscalation {
        request: wacp_v1::EscalationResponse,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::EscalationResponseAck, Status>>,
    },
    QueryTrail {
        request: wacp_v1::HighwayQueryTrailRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::QueryTrailResponse, Status>>,
    },
    GetWorkspace {
        request: wacp_v1::GetWorkspaceRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::WorkspaceView, Status>>,
    },
    GetTaskGraph {
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::TaskGraphView, Status>>,
    },
    GetCheckpoint {
        request: wacp_v1::GetCheckpointRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::CheckpointView, Status>>,
    },
}

impl HighwayServiceImpl {
    pub fn new(coordinator_tx: mpsc::Sender<HighwayRequest>) -> Self {
        Self { coordinator_tx }
    }

    async fn send_and_recv<T>(
        &self,
        make_req: impl FnOnce(tokio::sync::oneshot::Sender<Result<T, Status>>) -> HighwayRequest,
    ) -> Result<Response<T>, Status> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.coordinator_tx
            .send(make_req(reply_tx))
            .await
            .map_err(|_| Status::internal("coordinator unavailable"))?;
        reply_rx
            .await
            .map_err(|_| Status::internal("coordinator dropped reply"))?
            .map(Response::new)
    }
}

#[tonic::async_trait]
impl HighwayService for HighwayServiceImpl {
    async fn authenticate(
        &self,
        request: Request<wacp_v1::AuthenticateRequest>,
    ) -> Result<Response<wacp_v1::AuthenticateResponse>, Status> {
        let inner = request.into_inner();
        self.send_and_recv(|reply| HighwayRequest::Authenticate {
            request: inner,
            reply,
        })
        .await
    }

    async fn inject_envelope(
        &self,
        request: Request<wacp_v1::InjectEnvelopeRequest>,
    ) -> Result<Response<wacp_v1::InjectEnvelopeResponse>, Status> {
        let inner = request.into_inner();
        self.send_and_recv(|reply| HighwayRequest::InjectEnvelope {
            request: inner,
            reply,
        })
        .await
    }

    async fn respond_to_gate(
        &self,
        request: Request<wacp_v1::GateResponse>,
    ) -> Result<Response<wacp_v1::GateResponseAck>, Status> {
        let inner = request.into_inner();
        self.send_and_recv(|reply| HighwayRequest::RespondToGate {
            request: inner,
            reply,
        })
        .await
    }

    async fn respond_to_escalation(
        &self,
        request: Request<wacp_v1::EscalationResponse>,
    ) -> Result<Response<wacp_v1::EscalationResponseAck>, Status> {
        let inner = request.into_inner();
        self.send_and_recv(|reply| HighwayRequest::RespondToEscalation {
            request: inner,
            reply,
        })
        .await
    }

    async fn query_trail(
        &self,
        request: Request<wacp_v1::HighwayQueryTrailRequest>,
    ) -> Result<Response<wacp_v1::QueryTrailResponse>, Status> {
        let inner = request.into_inner();
        self.send_and_recv(|reply| HighwayRequest::QueryTrail {
            request: inner,
            reply,
        })
        .await
    }

    async fn get_workspace(
        &self,
        request: Request<wacp_v1::GetWorkspaceRequest>,
    ) -> Result<Response<wacp_v1::WorkspaceView>, Status> {
        let inner = request.into_inner();
        self.send_and_recv(|reply| HighwayRequest::GetWorkspace {
            request: inner,
            reply,
        })
        .await
    }

    async fn get_task_graph(
        &self,
        _request: Request<wacp_v1::GetTaskGraphRequest>,
    ) -> Result<Response<wacp_v1::TaskGraphView>, Status> {
        self.send_and_recv(|reply| HighwayRequest::GetTaskGraph { reply })
            .await
    }

    async fn get_checkpoint(
        &self,
        request: Request<wacp_v1::GetCheckpointRequest>,
    ) -> Result<Response<wacp_v1::CheckpointView>, Status> {
        let inner = request.into_inner();
        self.send_and_recv(|reply| HighwayRequest::GetCheckpoint {
            request: inner,
            reply,
        })
        .await
    }

    type StreamTrailStream = ReceiverStream<Result<wacp_v1::TrailEntry, Status>>;

    async fn stream_trail(
        &self,
        _request: Request<wacp_v1::StreamTrailRequest>,
    ) -> Result<Response<Self::StreamTrailStream>, Status> {
        let (_tx, rx) = mpsc::channel(64);
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type StreamGatesStream = ReceiverStream<Result<wacp_v1::GateEvent, Status>>;

    async fn stream_gates(
        &self,
        _request: Request<wacp_v1::StreamGatesRequest>,
    ) -> Result<Response<Self::StreamGatesStream>, Status> {
        let (_tx, rx) = mpsc::channel(64);
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type StreamEscalationsStream = ReceiverStream<Result<wacp_v1::EscalationEvent, Status>>;

    async fn stream_escalations(
        &self,
        _request: Request<wacp_v1::StreamEscalationsRequest>,
    ) -> Result<Response<Self::StreamEscalationsStream>, Status> {
        let (_tx, rx) = mpsc::channel(64);
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type StreamWorkspaceChangesStream =
        ReceiverStream<Result<wacp_v1::WorkspaceStateChange, Status>>;

    async fn stream_workspace_changes(
        &self,
        _request: Request<wacp_v1::StreamWorkspaceChangesRequest>,
    ) -> Result<Response<Self::StreamWorkspaceChangesStream>, Status> {
        let (_tx, rx) = mpsc::channel(64);
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
