//! E2E test harness — starts a real gRPC runtime and provides client helpers.
//!
//! The harness assembles the coordinator, gRPC server, and event loop from
//! individual crates (wacp-runtime is a binary crate and cannot be used as
//! a library dependency). It provides helpers to dispatch workspaces, connect
//! agent/highway clients, and cleanly shut down.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use wacp_coordinator::Coordinator;
use wacp_permissions::PermissionEngine;
use wacp_recovery::RecoveryEngine;
use wacp_taxonomy::Taxonomy;
use wacp_trail::InMemoryTrailStorage;
use wacp_transport::wacp_v1::agent_service_client::AgentServiceClient;
use wacp_transport::wacp_v1::highway_service_client::HighwayServiceClient;
use wacp_transport::{AgentRequest, GrpcServerConfig, HighwayRequest, start_grpc_server};
use wacp_types::*;
use wacp_workspace::WorkspaceEvent;

use crate::dispatch;

/// Port counter for unique port allocation across parallel tests.
static PORT_COUNTER: AtomicU16 = AtomicU16::new(41000);

fn next_port_triple() -> (u16, u16, u16) {
    let a = PORT_COUNTER.fetch_add(3, Ordering::Relaxed);
    (a, a + 1, a + 2)
}

/// A running E2E test environment with gRPC server and coordinator.
pub struct E2eHarness {
    pub agent_url: String,
    pub highway_url: String,
    /// Direct access to the coordinator for dispatching workspaces.
    pub coordinator: Coordinator,
    /// Workspace event receiver (for draining events).
    pub event_rx: mpsc::Receiver<WorkspaceEvent>,
    /// Handle to the background event loop.
    loop_handle: Option<JoinHandle<()>>,
}

impl E2eHarness {
    /// Start a full E2E environment: taxonomy, recovery, coordinator, gRPC server, event loop.
    pub async fn start() -> Self {
        let taxonomy = Taxonomy::empty("0.1");
        let trail = InMemoryTrailStorage::new();
        let _recovered = RecoveryEngine::recover(&trail).unwrap();
        let _permissions = PermissionEngine::new(&taxonomy);

        let (event_tx, event_rx) = mpsc::channel(256);
        let coordinator = Coordinator::new(
            WorkspaceId::from("ws-root"),
            UserId::from("system"),
            event_tx,
        );

        let (agent_port, highway_port, coordinator_port) = next_port_triple();
        let agent_addr: SocketAddr = ([127, 0, 0, 1], agent_port).into();
        let highway_addr: SocketAddr = ([127, 0, 0, 1], highway_port).into();
        let coordinator_addr: SocketAddr = ([127, 0, 0, 1], coordinator_port).into();

        let handles = start_grpc_server(GrpcServerConfig {
            agent_addr,
            highway_addr,
            coordinator_addr,
            tls: None,
        })
        .await
        .unwrap();

        let agent_url = format!("http://127.0.0.1:{agent_port}");
        let highway_url = format!("http://127.0.0.1:{highway_port}");

        // Spawn event loop to process agent/highway requests.
        // Coordinator requests are dropped for now — E2E coordinator tests
        // will be added when self-orchestration is wired (26R.2).
        drop(handles.coordinator_request_rx);
        let loop_handle = tokio::spawn(Self::event_loop(
            handles.agent_request_rx,
            handles.highway_request_rx,
        ));

        // Brief yield to let the server start accepting.
        tokio::task::yield_now().await;

        E2eHarness {
            agent_url,
            highway_url,
            coordinator,
            event_rx,
            loop_handle: Some(loop_handle),
        }
    }

    /// Dispatch a workspace into the coordinator.
    pub fn dispatch_workspace(&mut self, ws_id: &str, task_id: &str) {
        dispatch(&mut self.coordinator, ws_id, task_id);
    }

    /// Drain events from the workspace event channel.
    pub async fn drain(&mut self, max: usize) {
        crate::drain_events(&mut self.coordinator, &mut self.event_rx, max, 200).await;
    }

    /// Connect an agent gRPC client.
    pub async fn agent_client(&self) -> AgentServiceClient<tonic::transport::Channel> {
        // Retry briefly if the server isn't ready yet.
        for _ in 0..5 {
            match AgentServiceClient::connect(self.agent_url.clone()).await {
                Ok(c) => return c,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            }
        }
        AgentServiceClient::connect(self.agent_url.clone())
            .await
            .expect("failed to connect agent client")
    }

    /// Connect a highway gRPC client.
    pub async fn highway_client(&self) -> HighwayServiceClient<tonic::transport::Channel> {
        for _ in 0..5 {
            match HighwayServiceClient::connect(self.highway_url.clone()).await {
                Ok(c) => return c,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            }
        }
        HighwayServiceClient::connect(self.highway_url.clone())
            .await
            .expect("failed to connect highway client")
    }

    /// Shut down the harness.
    pub async fn shutdown(mut self) {
        if let Some(h) = self.loop_handle.take() {
            h.abort();
        }
    }

    /// Background event loop that processes gRPC requests.
    /// Implements a minimal subset of Runtime::handle_agent_request / handle_highway_request.
    async fn event_loop(
        mut agent_rx: mpsc::Receiver<AgentRequest>,
        mut highway_rx: mpsc::Receiver<HighwayRequest>,
    ) {
        loop {
            tokio::select! {
                Some(req) = agent_rx.recv() => {
                    Self::handle_agent_request(req);
                }
                Some(req) = highway_rx.recv() => {
                    Self::handle_highway_request(req);
                }
                else => break,
            }
        }
    }

    fn handle_agent_request(req: AgentRequest) {
        use wacp_transport::wacp_v1;

        match req {
            AgentRequest::Bind { request, reply } => {
                let _ = reply.send(Ok(wacp_v1::BindResponse {
                    workspace_id: request.workspace_id,
                    state: wacp_v1::WorkspaceState::Active.into(),
                    role: "worker".into(),
                    directive: None,
                    context: vec![],
                    visibility: vec![],
                    authority: vec![],
                    budget: None,
                }));
            }
            AgentRequest::EmitSignal { request, reply, .. } => {
                let _ = reply.send(Ok(wacp_v1::EmitSignalResponse {
                    timestamp: None,
                    client_request_id: request.client_request_id,
                }));
            }
            AgentRequest::CreateCheckpoint { request, reply, .. } => {
                let _ = reply.send(Ok(wacp_v1::CreateCheckpointResponse {
                    checkpoint_id: format!("cp-{}", uuid::Uuid::new_v4()),
                    content_hash: "sha256-test".into(),
                    timestamp: None,
                    client_request_id: request.client_request_id,
                }));
            }
            AgentRequest::SendEnvelope { reply, .. } => {
                let _ = reply.send(Ok(wacp_v1::SendEnvelopeResponse {
                    envelope_id: format!("env-{}", uuid::Uuid::new_v4()),
                    timestamp: None,
                    client_request_id: String::new(),
                }));
            }
            AgentRequest::QueryTrail { reply, .. } => {
                let _ = reply.send(Ok(wacp_v1::QueryTrailResponse {
                    entries: vec![],
                    has_more: false,
                    client_request_id: String::new(),
                }));
            }
            AgentRequest::SubscribeEnvelopes { .. } => {}
            AgentRequest::SubscribeCommands { .. } => {}
            AgentRequest::ReadResource { reply, request, .. } => {
                let _ = reply.send(Ok(wacp_v1::ReadResourceResponse {
                    content: vec![],
                    client_request_id: request.client_request_id,
                }));
            }
        }
    }

    fn handle_highway_request(req: HighwayRequest) {
        use wacp_transport::wacp_v1;

        match req {
            HighwayRequest::Authenticate { request, reply } => {
                let _ = reply.send(Ok(wacp_v1::AuthenticateResponse {
                    user_id: format!("user-{}", request.auth_token),
                    capabilities: vec!["observe".into(), "inject".into(), "gate".into()],
                }));
            }
            HighwayRequest::GetTaskGraph { reply } => {
                let _ = reply.send(Ok(wacp_v1::TaskGraphView { tasks: vec![] }));
            }
            HighwayRequest::GetWorkspace { request, reply } => {
                let _ = reply.send(Ok(wacp_v1::WorkspaceView {
                    id: request.workspace_id,
                    state: wacp_v1::WorkspaceState::Active.into(),
                    ..Default::default()
                }));
            }
            HighwayRequest::GetCheckpoint { reply, .. } => {
                let _ = reply.send(Err(tonic::Status::not_found("not found")));
            }
            HighwayRequest::InjectEnvelope { reply, .. } => {
                let _ = reply.send(Ok(wacp_v1::InjectEnvelopeResponse {
                    envelope_id: format!("env-{}", uuid::Uuid::new_v4()),
                    timestamp: None,
                    client_request_id: String::new(),
                }));
            }
            HighwayRequest::RespondToGate { reply, .. } => {
                let _ = reply.send(Ok(wacp_v1::GateResponseAck {
                    gate_id: String::new(),
                    applied: true,
                    client_request_id: String::new(),
                }));
            }
            HighwayRequest::RespondToEscalation { reply, .. } => {
                let _ = reply.send(Ok(wacp_v1::EscalationResponseAck {
                    escalation_id: String::new(),
                    applied: true,
                    client_request_id: String::new(),
                }));
            }
            HighwayRequest::QueryTrail { reply, .. } => {
                let _ = reply.send(Ok(wacp_v1::QueryTrailResponse {
                    entries: vec![],
                    has_more: false,
                    client_request_id: String::new(),
                }));
            }
            HighwayRequest::ListWorkspaces { reply, .. } => {
                let _ = reply.send(Ok(vec![]));
            }
            HighwayRequest::SubscribeTrail { .. }
            | HighwayRequest::SubscribeGates { .. }
            | HighwayRequest::SubscribeEscalations { .. }
            | HighwayRequest::SubscribeWorkspaceChanges { .. } => {
                // Streaming subscribers — senders are dropped, closing the stream.
            }
        }
    }
}
