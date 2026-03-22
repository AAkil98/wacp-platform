use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::proto::wacp_v1;
use crate::proto::wacp_v1::agent_service_server::AgentService;

/// Channel-based agent service implementation.
/// Each bound agent gets its own pair of channels for envelope/command streaming.
pub struct AgentServiceImpl {
    /// Sender to forward agent requests to the coordinator.
    coordinator_tx: mpsc::Sender<AgentRequest>,
}

/// An agent request forwarded to the coordinator for processing.
#[derive(Debug)]
pub enum AgentRequest {
    Bind {
        request: wacp_v1::BindRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::BindResponse, Status>>,
    },
    SendEnvelope {
        workspace_id: String,
        request: wacp_v1::SendEnvelopeRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::SendEnvelopeResponse, Status>>,
    },
    EmitSignal {
        workspace_id: String,
        request: wacp_v1::EmitSignalRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::EmitSignalResponse, Status>>,
    },
    CreateCheckpoint {
        workspace_id: String,
        request: wacp_v1::CreateCheckpointRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::CreateCheckpointResponse, Status>>,
    },
    QueryTrail {
        workspace_id: String,
        request: wacp_v1::QueryTrailRequest,
        reply: tokio::sync::oneshot::Sender<Result<wacp_v1::QueryTrailResponse, Status>>,
    },
    SubscribeEnvelopes {
        workspace_id: String,
        tx: mpsc::Sender<Result<wacp_v1::Envelope, Status>>,
    },
    SubscribeCommands {
        workspace_id: String,
        tx: mpsc::Sender<Result<wacp_v1::Command, Status>>,
    },
}

impl AgentServiceImpl {
    pub fn new(coordinator_tx: mpsc::Sender<AgentRequest>) -> Self {
        Self { coordinator_tx }
    }
}

#[tonic::async_trait]
impl AgentService for AgentServiceImpl {
    async fn bind(
        &self,
        request: Request<wacp_v1::BindRequest>,
    ) -> Result<Response<wacp_v1::BindResponse>, Status> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.coordinator_tx
            .send(AgentRequest::Bind {
                request: request.into_inner(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| Status::internal("coordinator unavailable"))?;

        reply_rx
            .await
            .map_err(|_| Status::internal("coordinator dropped reply"))?
            .map(Response::new)
    }

    async fn send_envelope(
        &self,
        request: Request<wacp_v1::SendEnvelopeRequest>,
    ) -> Result<Response<wacp_v1::SendEnvelopeResponse>, Status> {
        let inner = request.into_inner();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.coordinator_tx
            .send(AgentRequest::SendEnvelope {
                workspace_id: String::new(), // TODO: extract from connection metadata
                request: inner,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Status::internal("coordinator unavailable"))?;

        reply_rx
            .await
            .map_err(|_| Status::internal("coordinator dropped reply"))?
            .map(Response::new)
    }

    async fn emit_signal(
        &self,
        request: Request<wacp_v1::EmitSignalRequest>,
    ) -> Result<Response<wacp_v1::EmitSignalResponse>, Status> {
        let inner = request.into_inner();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.coordinator_tx
            .send(AgentRequest::EmitSignal {
                workspace_id: String::new(),
                request: inner,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Status::internal("coordinator unavailable"))?;

        reply_rx
            .await
            .map_err(|_| Status::internal("coordinator dropped reply"))?
            .map(Response::new)
    }

    async fn create_checkpoint(
        &self,
        request: Request<wacp_v1::CreateCheckpointRequest>,
    ) -> Result<Response<wacp_v1::CreateCheckpointResponse>, Status> {
        let inner = request.into_inner();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.coordinator_tx
            .send(AgentRequest::CreateCheckpoint {
                workspace_id: String::new(),
                request: inner,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Status::internal("coordinator unavailable"))?;

        reply_rx
            .await
            .map_err(|_| Status::internal("coordinator dropped reply"))?
            .map(Response::new)
    }

    async fn query_trail(
        &self,
        request: Request<wacp_v1::QueryTrailRequest>,
    ) -> Result<Response<wacp_v1::QueryTrailResponse>, Status> {
        let inner = request.into_inner();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.coordinator_tx
            .send(AgentRequest::QueryTrail {
                workspace_id: String::new(),
                request: inner,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Status::internal("coordinator unavailable"))?;

        reply_rx
            .await
            .map_err(|_| Status::internal("coordinator dropped reply"))?
            .map(Response::new)
    }

    async fn read_resource(
        &self,
        _request: Request<wacp_v1::ReadResourceRequest>,
    ) -> Result<Response<wacp_v1::ReadResourceResponse>, Status> {
        Err(Status::unimplemented("read_resource not yet implemented"))
    }

    type ReceiveEnvelopesStream = ReceiverStream<Result<wacp_v1::Envelope, Status>>;

    async fn receive_envelopes(
        &self,
        _request: Request<wacp_v1::ReceiveEnvelopesRequest>,
    ) -> Result<Response<Self::ReceiveEnvelopesStream>, Status> {
        let (tx, rx) = mpsc::channel(64);

        self.coordinator_tx
            .send(AgentRequest::SubscribeEnvelopes {
                workspace_id: String::new(),
                tx,
            })
            .await
            .map_err(|_| Status::internal("coordinator unavailable"))?;

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type ReceiveCommandsStream = ReceiverStream<Result<wacp_v1::Command, Status>>;

    async fn receive_commands(
        &self,
        _request: Request<wacp_v1::ReceiveCommandsRequest>,
    ) -> Result<Response<Self::ReceiveCommandsStream>, Status> {
        let (tx, rx) = mpsc::channel(64);

        self.coordinator_tx
            .send(AgentRequest::SubscribeCommands {
                workspace_id: String::new(),
                tx,
            })
            .await
            .map_err(|_| Status::internal("coordinator unavailable"))?;

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
