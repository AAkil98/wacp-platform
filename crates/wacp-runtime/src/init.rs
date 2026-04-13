use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::time::Instant;

use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use wacp_coordinator::Coordinator;
use wacp_permissions::PermissionEngine;
use wacp_recovery::RecoveryEngine;
use wacp_taxonomy::{Taxonomy, VerticalManifest};
use wacp_trail::{
    CheckpointStorage, FileCheckpointStorage, FileTrailConfig, FileTrailStorage,
    InMemoryCheckpointStorage, InMemoryTrailStorage,
};
use wacp_transport::rest_gateway::RuntimeHealth;
use wacp_transport::{GrpcServerConfig, RestGateway, start_grpc_server};
use wacp_types::*;
use wacp_workspace::WorkspaceEvent;

use crate::config::{PROTOCOL_VERSION, RuntimeConfig};

/// Errors during runtime initialization.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("taxonomy error: {0}")]
    Taxonomy(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("recovery error: {0}")]
    Recovery(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// The initialized runtime — owns all components.
pub struct Runtime {
    pub config: RuntimeConfig,
    pub coordinator: Coordinator,
    pub taxonomy: Taxonomy,
    /// Vertical manifests loaded from `taxonomy.verticals_dir` at startup.
    /// Empty when no directory is configured. Passed to the REST gateway as
    /// the read-only `VerticalRegistry`.
    pub verticals: Arc<Vec<VerticalManifest>>,
    pub permissions: PermissionEngine,
    pub event_rx: mpsc::Receiver<WorkspaceEvent>,
    pub agent_request_rx: Option<mpsc::Receiver<wacp_transport::AgentRequest>>,
    pub highway_request_rx: Option<mpsc::Receiver<wacp_transport::HighwayRequest>>,
    pub coordinator_request_rx: Option<mpsc::Receiver<wacp_transport::CoordinatorRequest>>,

    // Stream subscribers — populated by StreamSignals / highway streaming RPCs.
    signal_subs: Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::SignalEvent, tonic::Status>>>,
    ws_change_subs:
        Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::WorkspaceStateChange, tonic::Status>>>,
    trail_subs: Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::TrailEntry, tonic::Status>>>,
    gate_subs: Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::GateEvent, tonic::Status>>>,
    escalation_subs:
        Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::EscalationEvent, tonic::Status>>>,

    // Checkpoint content-addressable store.
    checkpoint_storage: Arc<dyn CheckpointStorage>,

    // Monotonic counters for ID generation.
    next_envelope_id: u64,
    next_checkpoint_id: u64,
    stream_seq: u64,
}

impl Runtime {
    /// Full initialization sequence — production mode with filesystem storage and gRPC.
    ///
    /// `health_state` carries the lifecycle flag shared with the ops health
    /// endpoint (`/healthz`). When provided, the Console-facing `/v1/health`
    /// on the REST gateway reflects the same state so both endpoints agree.
    pub async fn init(
        config: RuntimeConfig,
        health_state: Option<(Arc<AtomicU8>, Instant)>,
    ) -> Result<Self, RuntimeError> {
        let data_dir = PathBuf::from(&config.storage.data_dir);

        // 1. Create data directories.
        fs::create_dir_all(data_dir.join("trail"))?;
        fs::create_dir_all(data_dir.join("checkpoints"))?;
        fs::create_dir_all(data_dir.join("snapshots"))?;

        // 2. Load taxonomy and vertical manifests.
        let taxonomy = Self::load_taxonomy(&config)?;
        let verticals = Self::load_vertical_manifests(&config);
        tracing::info!(count = verticals.len(), "vertical manifests loaded");

        // 3. Open trail storage with crash recovery.
        let trail = FileTrailStorage::recover(FileTrailConfig {
            dir: data_dir.join("trail"),
            max_segment_size: config.storage.trail.segment_size_bytes,
        })
        .map_err(|e| RuntimeError::Storage(e.to_string()))?;

        // 4. Run recovery — reconstruct state from trail.
        let recovered =
            RecoveryEngine::recover(&trail).map_err(|e| RuntimeError::Recovery(e.to_string()))?;

        tracing::info!(
            workspaces_recovered = recovered.workspace_states.len(),
            tasks = recovered.task_statuses.len(),
            in_flight_envelopes = recovered.in_flight_envelopes.len(),
            last_sequence = recovered.last_sequence,
            "recovery completed"
        );

        // 4b. Open checkpoint content-addressable store.
        let checkpoint_storage: Arc<dyn CheckpointStorage> = Arc::new(
            FileCheckpointStorage::open(data_dir.join("checkpoints"))
                .map_err(|e| RuntimeError::Storage(e.to_string()))?,
        );

        // 5. Build permission engine from taxonomy.
        let permissions = PermissionEngine::new(&taxonomy);

        // 6. Create coordinator.
        let root_id = WorkspaceId::from("ws-root");
        let owner = UserId::from("system");
        let (event_tx, event_rx) = mpsc::channel(256);
        let coordinator = Coordinator::new(root_id, owner, event_tx);

        // 7. Build TLS configuration if enabled.
        let tls_config = if config.tls.enabled {
            Some(
                crate::tls::build_tls_config(&config.tls)
                    .map_err(|e| RuntimeError::Transport(format!("TLS error: {e}")))?,
            )
        } else {
            None
        };

        // 8. Start gRPC server on configured addresses.
        let agent_addr: SocketAddr = config
            .server
            .agent_listen
            .parse()
            .map_err(|e| RuntimeError::Transport(format!("invalid agent address: {e}")))?;
        let highway_addr: SocketAddr = config
            .server
            .highway_listen
            .parse()
            .map_err(|e| RuntimeError::Transport(format!("invalid highway address: {e}")))?;

        let coordinator_addr: SocketAddr =
            config.server.coordinator_listen.parse().map_err(|e| {
                RuntimeError::Transport(format!("invalid coordinator address: {e}"))
            })?;

        let grpc_handles = start_grpc_server(GrpcServerConfig {
            agent_addr,
            highway_addr,
            coordinator_addr,
            tls: tls_config,
        })
        .await
        .map_err(|e| RuntimeError::Transport(e.to_string()))?;

        tracing::info!(
            agent = %config.server.agent_listen,
            highway = %config.server.highway_listen,
            coordinator = %config.server.coordinator_listen,
            "gRPC endpoints listening"
        );

        // 9. Start REST gateway + WebSocket on the configured address.
        let rest_addr: SocketAddr = config
            .server
            .rest_listen
            .parse()
            .map_err(|e| RuntimeError::Transport(format!("invalid REST address: {e}")))?;

        let backend: std::sync::Arc<dyn wacp_transport::GatewayBackend> =
            crate::channel_backend::ChannelBackend::new(
                grpc_handles.highway_request_tx,
                grpc_handles.coordinator_request_tx,
            );
        let runtime_health =
            health_state.map(|(state, start_time)| RuntimeHealth { state, start_time });
        let rest_router = RestGateway::router(backend.clone(), verticals.clone(), runtime_health);
        let ws_router = axum::Router::new()
            .route(
                "/v1/ws",
                axum::routing::get(wacp_transport::websocket::ws_handler),
            )
            .with_state(backend);
        let app = rest_router.merge(ws_router);

        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(rest_addr)
                .await
                .expect("REST gateway bind failed");
            tracing::info!(addr = %rest_addr, "REST + WebSocket gateway listening");
            axum::serve(listener, app)
                .await
                .expect("REST gateway failed");
        });

        Ok(Runtime {
            config,
            coordinator,
            taxonomy,
            verticals,
            permissions,
            event_rx,
            agent_request_rx: Some(grpc_handles.agent_request_rx),
            highway_request_rx: Some(grpc_handles.highway_request_rx),
            coordinator_request_rx: Some(grpc_handles.coordinator_request_rx),
            signal_subs: Vec::new(),
            ws_change_subs: Vec::new(),
            trail_subs: Vec::new(),
            gate_subs: Vec::new(),
            escalation_subs: Vec::new(),
            checkpoint_storage,
            next_envelope_id: 0,
            next_checkpoint_id: 0,
            stream_seq: 0,
        })
    }

    /// Initialize with in-memory storage for testing (no gRPC, no filesystem).
    pub fn init_in_memory(config: RuntimeConfig) -> Result<Self, RuntimeError> {
        let taxonomy = Self::load_taxonomy(&config)?;
        let verticals = Self::load_vertical_manifests(&config);

        let trail = InMemoryTrailStorage::new();
        let _recovered =
            RecoveryEngine::recover(&trail).map_err(|e| RuntimeError::Recovery(e.to_string()))?;

        let permissions = PermissionEngine::new(&taxonomy);

        let root_id = WorkspaceId::from("ws-root");
        let owner = UserId::from("system");
        let (event_tx, event_rx) = mpsc::channel(256);
        let coordinator = Coordinator::new(root_id, owner, event_tx);

        Ok(Runtime {
            config,
            coordinator,
            taxonomy,
            verticals,
            permissions,
            event_rx,
            agent_request_rx: None,
            highway_request_rx: None,
            coordinator_request_rx: None,
            signal_subs: Vec::new(),
            ws_change_subs: Vec::new(),
            trail_subs: Vec::new(),
            gate_subs: Vec::new(),
            escalation_subs: Vec::new(),
            checkpoint_storage: Arc::new(InMemoryCheckpointStorage::new()),
            next_envelope_id: 0,
            next_checkpoint_id: 0,
            stream_seq: 0,
        })
    }

    /// Run the coordinator event processing loop until shutdown.
    pub async fn run(&mut self) {
        tracing::info!("entering event loop");

        loop {
            tokio::select! {
                biased;

                // Process workspace events (signals, state changes, terminations).
                Some(event) = self.event_rx.recv() => {
                    self.coordinator.handle_event(&event);
                    self.fan_out_event(&event);
                }

                // Process gRPC agent requests if server is running.
                Some(req) = async {
                    if let Some(ref mut rx) = self.agent_request_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    self.handle_agent_request(req).await;
                }

                // Process gRPC highway requests if server is running.
                Some(req) = async {
                    if let Some(ref mut rx) = self.highway_request_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    self.handle_highway_request(req).await;
                }

                // Process coordinator requests (gRPC + REST gateway).
                Some(req) = async {
                    if let Some(ref mut rx) = self.coordinator_request_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    self.handle_coordinator_request(req).await;
                }

                // Shutdown signal.
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("received shutdown signal");
                    break;
                }
            }
        }

        self.shutdown().await;
    }

    /// Fan out a workspace event to all active stream subscribers.
    /// Dead subscribers (disconnected clients) are pruned automatically.
    fn fan_out_event(&mut self, event: &WorkspaceEvent) {
        use wacp_transport::wacp_v1;

        match event {
            WorkspaceEvent::Signal(signal) => {
                let proto_ev = Ok(wacp_v1::SignalEvent {
                    workspace_id: signal.workspace_id.to_string(),
                    signal_type: signal.signal_type as i32,
                    reason: signal.reason.clone().unwrap_or_default(),
                    context: signal.context.clone().unwrap_or_default(),
                    timestamp: signal.timestamp,
                });
                self.signal_subs
                    .retain(|tx| tx.try_send(proto_ev.clone()).is_ok());

                // Escalation signals also fan out to escalation subscribers.
                if signal.signal_type == SignalType::Escalation {
                    let owner = self
                        .coordinator
                        .tree
                        .get(&signal.workspace_id)
                        .map(|n| n.owner.to_string())
                        .unwrap_or_default();
                    self.stream_seq += 1;
                    let esc_ev = Ok(wacp_v1::EscalationEvent {
                        escalation_id: format!("esc-{}", self.stream_seq),
                        workspace_id: signal.workspace_id.to_string(),
                        owner,
                        context: signal.context.clone().unwrap_or_default(),
                        created_at: None,
                    });
                    self.escalation_subs
                        .retain(|tx| tx.try_send(esc_ev.clone()).is_ok());
                }

                // All signals are trail-worthy.
                self.emit_trail_entry(
                    signal.workspace_id.as_ref(),
                    "signal",
                    serde_json::json!({
                        "signal_type": format!("{:?}", signal.signal_type),
                        "reason": signal.reason,
                    })
                    .to_string()
                    .into_bytes(),
                );
            }
            WorkspaceEvent::StateChanged {
                workspace_id,
                from,
                to,
            } => {
                let proto_ev = Ok(wacp_v1::WorkspaceStateChange {
                    workspace_id: workspace_id.to_string(),
                    previous: *from as i32,
                    current: *to as i32,
                    trigger: String::new(),
                    timestamp: None,
                });
                self.ws_change_subs
                    .retain(|tx| tx.try_send(proto_ev.clone()).is_ok());

                self.emit_trail_entry(
                    workspace_id.as_ref(),
                    "state_changed",
                    serde_json::json!({
                        "from": format!("{from:?}"),
                        "to": format!("{to:?}"),
                    })
                    .to_string()
                    .into_bytes(),
                );
            }
            WorkspaceEvent::CheckpointCreated(cp) => {
                self.emit_trail_entry(
                    cp.workspace_id.as_ref(),
                    "checkpoint_created",
                    serde_json::json!({
                        "checkpoint_id": cp.id.to_string(),
                        "checkpoint_type": cp.checkpoint_type,
                        "content_hash": cp.content_hash,
                        "intent": cp.intent,
                        "status": format!("{:?}", cp.status),
                    })
                    .to_string()
                    .into_bytes(),
                );
            }
            WorkspaceEvent::Terminated(archived) => {
                // Terminal state change for workspace change subscribers.
                let proto_ev = Ok(wacp_v1::WorkspaceStateChange {
                    workspace_id: archived.id.to_string(),
                    previous: 0, // unknown prior state
                    current: archived.terminal_state as i32,
                    trigger: "terminated".into(),
                    timestamp: None,
                });
                self.ws_change_subs
                    .retain(|tx| tx.try_send(proto_ev.clone()).is_ok());

                self.emit_trail_entry(
                    archived.id.as_ref(),
                    "terminated",
                    serde_json::json!({
                        "terminal_state": format!("{:?}", archived.terminal_state),
                        "checkpoints": archived.checkpoints.len(),
                    })
                    .to_string()
                    .into_bytes(),
                );
            }
            WorkspaceEvent::Error {
                workspace_id,
                message,
            } => {
                self.emit_trail_entry(
                    workspace_id.as_ref(),
                    "error",
                    serde_json::json!({ "message": message })
                        .to_string()
                        .into_bytes(),
                );
            }
            WorkspaceEvent::MigrationSnapshot {
                workspace_id,
                snapshot: _,
            } => {
                self.emit_trail_entry(workspace_id.as_ref(), "migration_snapshot", Vec::new());
            }
        }
    }

    /// Emit a trail entry to all active trail stream subscribers.
    fn emit_trail_entry(&mut self, workspace_id: &str, event_type: &str, body: Vec<u8>) {
        use wacp_transport::wacp_v1;

        self.stream_seq += 1;
        let entry = Ok(wacp_v1::TrailEntry {
            id: format!("te-{}", self.stream_seq),
            timestamp: None,
            workspace_id: workspace_id.to_string(),
            actor: "protocol".into(),
            event_type: event_type.into(),
            body,
            sequence_number: self.stream_seq,
            chain_hash: Vec::new(),
        });
        self.trail_subs
            .retain(|tx| tx.try_send(entry.clone()).is_ok());
    }

    /// Graceful shutdown — abort all active workspaces, wait for termination.
    pub async fn shutdown(&mut self) {
        tracing::info!("shutting down");

        let active: Vec<WorkspaceId> = self
            .coordinator
            .tree
            .active_workspaces()
            .into_iter()
            .filter(|id| id.as_ref() != "ws-root")
            .cloned()
            .collect();

        for ws_id in &active {
            self.coordinator.abort_workspace(ws_id).await;
        }

        // Drain remaining events.
        while let Ok(event) =
            tokio::time::timeout(std::time::Duration::from_millis(500), self.event_rx.recv()).await
        {
            if let Some(e) = event {
                self.coordinator.handle_event(&e);
            } else {
                break;
            }
        }

        tracing::info!("shutdown complete");
    }

    /// Handle an agent gRPC request by forwarding to the coordinator.
    async fn handle_agent_request(&mut self, req: wacp_transport::AgentRequest) {
        use wacp_transport::AgentRequest;

        match req {
            AgentRequest::Bind { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                if let Some(node) = self.coordinator.tree.get(&ws_id) {
                    let response = wacp_transport::wacp_v1::BindResponse {
                        workspace_id: ws_id.to_string(),
                        state: node.status as i32,
                        role: String::new(),
                        directive: None,
                        context: vec![],
                        visibility: vec![],
                        authority: vec![],
                        budget: None,
                    };
                    let _ = reply.send(Ok(response));
                } else {
                    let _ = reply.send(Err(tonic::Status::not_found("workspace not found")));
                }
            }
            AgentRequest::EmitSignal {
                workspace_id: _,
                request,
                reply,
            } => {
                let response = wacp_transport::wacp_v1::EmitSignalResponse {
                    timestamp: None,
                    client_request_id: request.client_request_id,
                };
                let _ = reply.send(Ok(response));
            }
            AgentRequest::SendEnvelope {
                workspace_id,
                request,
                reply,
            } => {
                let from_ws = WorkspaceId::from(workspace_id.as_str());
                let to_ws = WorkspaceId::from(request.to_workspace.as_str());

                // Validate target exists in the tree.
                if self.coordinator.tree.get(&to_ws).is_none() {
                    let _ = reply.send(Err(tonic::Status::not_found(format!(
                        "target workspace '{}' not found",
                        request.to_workspace
                    ))));
                    return;
                }

                let env_id = self.next_envelope_id;
                self.next_envelope_id += 1;
                let envelope_id = EnvelopeId::from(format!("env-{env_id}"));

                let priority = match request.priority {
                    2 => EnvelopePriority::Urgent,
                    3 => EnvelopePriority::Blocking,
                    _ => EnvelopePriority::Normal,
                };

                let envelope = Envelope {
                    id: envelope_id.clone(),
                    from_workspace: from_ws,
                    to_workspace: to_ws.clone(),
                    envelope_type: request.r#type,
                    payload: request.payload,
                    in_reply_to: if request.in_reply_to.is_empty() {
                        None
                    } else {
                        Some(EnvelopeId::from(request.in_reply_to.as_str()))
                    },
                    timestamp: 0,
                    priority,
                    origin: EnvelopeOrigin::Agent,
                    state: EnvelopeState::Created,
                };

                // Route through the coordinator to the target workspace actor.
                if self.coordinator.route_envelope(&to_ws, envelope).await {
                    let response = wacp_transport::wacp_v1::SendEnvelopeResponse {
                        envelope_id: envelope_id.to_string(),
                        timestamp: None,
                        client_request_id: request.client_request_id,
                    };
                    let _ = reply.send(Ok(response));
                } else {
                    let _ = reply.send(Err(tonic::Status::unavailable(format!(
                        "workspace '{}' is not running",
                        to_ws
                    ))));
                }
            }
            AgentRequest::CreateCheckpoint {
                workspace_id: _,
                request,
                reply,
            } => {
                // SHA-256 content hash.
                let mut hasher = Sha256::new();
                hasher.update(&request.payload);
                let hash_bytes: [u8; 32] = hasher.finalize().into();
                let hash_hex: String = hash_bytes.iter().map(|b| format!("{b:02x}")).collect();

                // Persist to content-addressable store.
                if let Err(e) = self.checkpoint_storage.store(&hash_bytes, &request.payload) {
                    tracing::warn!(error = %e, "checkpoint persistence failed");
                }

                let cp_id = self.next_checkpoint_id;
                self.next_checkpoint_id += 1;

                let response = wacp_transport::wacp_v1::CreateCheckpointResponse {
                    checkpoint_id: format!("cp-{cp_id}"),
                    content_hash: hash_hex,
                    timestamp: None,
                    client_request_id: request.client_request_id,
                };
                let _ = reply.send(Ok(response));
            }
            AgentRequest::QueryTrail { reply, .. } => {
                let response = wacp_transport::wacp_v1::QueryTrailResponse {
                    entries: vec![],
                    has_more: false,
                    client_request_id: String::new(),
                };
                let _ = reply.send(Ok(response));
            }
            AgentRequest::SubscribeEnvelopes { .. } => {}
            AgentRequest::SubscribeCommands { .. } => {}
        }
    }

    /// Handle a highway gRPC request.
    async fn handle_highway_request(&mut self, req: wacp_transport::HighwayRequest) {
        use wacp_transport::HighwayRequest;

        match req {
            HighwayRequest::Authenticate { request, reply } => {
                let response = wacp_transport::wacp_v1::AuthenticateResponse {
                    user_id: format!("user-{}", request.auth_token),
                    capabilities: vec!["observe".into(), "inject".into(), "gate".into()],
                };
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::GetTaskGraph { reply } => {
                let response = wacp_transport::wacp_v1::TaskGraphView { tasks: vec![] };
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::InjectEnvelope { reply, .. } => {
                let _ = reply.send(Err(tonic::Status::unimplemented(
                    "envelope injection via gRPC pending full wiring",
                )));
            }
            HighwayRequest::RespondToGate { reply, .. } => {
                let _ = reply.send(Err(tonic::Status::unimplemented("gate response pending")));
            }
            HighwayRequest::RespondToEscalation { reply, .. } => {
                let _ = reply.send(Err(tonic::Status::unimplemented(
                    "escalation response pending",
                )));
            }
            HighwayRequest::QueryTrail { reply, .. } => {
                let response = wacp_transport::wacp_v1::QueryTrailResponse {
                    entries: vec![],
                    has_more: false,
                    client_request_id: String::new(),
                };
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::GetWorkspace { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                if let Some(node) = self.coordinator.tree.get(&ws_id) {
                    let response = wacp_transport::wacp_v1::WorkspaceView {
                        id: node.id.to_string(),
                        state: node.status as i32,
                        role: String::new(),
                        parent: node
                            .parent
                            .as_ref()
                            .map(|p| p.to_string())
                            .unwrap_or_default(),
                        owner: node.owner.to_string(),
                        originator: String::new(),
                        task_id: node
                            .task_id
                            .as_ref()
                            .map(|t| t.to_string())
                            .unwrap_or_default(),
                        current_usage: None,
                        budget: None,
                        checkpoint_count: 0,
                        created_at: None,
                        last_activity: None,
                    };
                    let _ = reply.send(Ok(response));
                } else {
                    let _ = reply.send(Err(tonic::Status::not_found("workspace not found")));
                }
            }
            HighwayRequest::GetCheckpoint { reply, .. } => {
                let _ = reply.send(Err(tonic::Status::not_found("checkpoint not found")));
            }
            HighwayRequest::ListWorkspaces { parent_id, reply } => {
                let items: Vec<wacp_transport::WorkspaceSummaryItem> = self
                    .coordinator
                    .tree
                    .active_workspaces()
                    .iter()
                    .filter_map(|ws_id| {
                        let node = self.coordinator.tree.get(ws_id)?;
                        let node_parent = node.parent.as_ref().map(|p| p.to_string());
                        if let Some(ref filter) = parent_id
                            && node_parent.as_deref() != Some(filter)
                        {
                            return None;
                        }
                        Some(wacp_transport::WorkspaceSummaryItem {
                            id: ws_id.to_string(),
                            parent_id: node_parent.unwrap_or_default(),
                            state: node.status as i32,
                            owner: node.owner.to_string(),
                            task_id: node
                                .task_id
                                .as_ref()
                                .map(|t| t.to_string())
                                .unwrap_or_default(),
                        })
                    })
                    .collect();
                let _ = reply.send(Ok(items));
            }
            HighwayRequest::SubscribeTrail { tx } => {
                self.trail_subs.push(tx);
            }
            HighwayRequest::SubscribeGates { tx } => {
                self.gate_subs.push(tx);
            }
            HighwayRequest::SubscribeEscalations { tx } => {
                self.escalation_subs.push(tx);
            }
            HighwayRequest::SubscribeWorkspaceChanges { tx } => {
                self.ws_change_subs.push(tx);
            }
        }
    }

    /// Handle a coordinator gRPC/REST request.
    async fn handle_coordinator_request(&mut self, req: wacp_transport::CoordinatorRequest) {
        use wacp_transport::CoordinatorRequest;

        match req {
            CoordinatorRequest::SubmitGoal { request, reply } => {
                let goal_id = format!("goal-{}", request.description.len());
                let response = wacp_transport::wacp_v1::SubmitGoalResponse {
                    goal_id: goal_id.clone(),
                    root_workspace_id: "ws-root".into(),
                };
                let _ = reply.send(Ok(response));
            }
            CoordinatorRequest::GetReadyTasks { reply, .. } => {
                let response = wacp_transport::wacp_v1::GetReadyTasksResponse { tasks: vec![] };
                let _ = reply.send(Ok(response));
            }
            CoordinatorRequest::Dispatch { request, reply } => {
                let ws_id = format!("ws-{}", request.task_id);
                let response = wacp_transport::wacp_v1::DispatchResponse {
                    workspace_id: ws_id,
                    task_id: request.task_id,
                };
                let _ = reply.send(Ok(response));
            }
            CoordinatorRequest::AbortWorkspace { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                self.coordinator.abort_workspace(&ws_id).await;
                let _ = reply.send(Ok(
                    wacp_transport::wacp_v1::AbortWorkspaceResponse::default(),
                ));
            }
            CoordinatorRequest::SuspendWorkspace { reply, .. } => {
                let _ = reply.send(Ok(
                    wacp_transport::wacp_v1::SuspendWorkspaceResponse::default(),
                ));
            }
            CoordinatorRequest::ResumeWorkspace { reply, .. } => {
                let _ = reply.send(Ok(
                    wacp_transport::wacp_v1::ResumeWorkspaceResponse::default(),
                ));
            }
            CoordinatorRequest::Decompose { reply, .. } => {
                let _ = reply.send(Ok(wacp_transport::wacp_v1::DecomposeResponse::default()));
            }
            CoordinatorRequest::CancelTask { reply, .. } => {
                let _ = reply.send(Ok(wacp_transport::wacp_v1::CancelTaskResponse::default()));
            }
            CoordinatorRequest::SendDirective { reply, .. } => {
                let _ = reply.send(Ok(wacp_transport::wacp_v1::SendDirectiveResponse::default()));
            }
            CoordinatorRequest::SendFeedback { reply, .. } => {
                let _ = reply.send(Ok(wacp_transport::wacp_v1::SendFeedbackResponse::default()));
            }
            CoordinatorRequest::TriggerIntegration { reply, .. } => {
                let _ = reply.send(Ok(wacp_transport::wacp_v1::TriggerIntegrationResponse {
                    result: "accepted".into(),
                    detail: String::new(),
                }));
            }
            CoordinatorRequest::GetAllocatable { reply, .. } => {
                let _ = reply.send(Ok(wacp_transport::wacp_v1::GetAllocatableResponse {
                    remaining: None,
                }));
            }
            CoordinatorRequest::StreamSignals { tx, .. } => {
                self.signal_subs.push(tx);
            }
        }
    }

    fn load_taxonomy(config: &RuntimeConfig) -> Result<Taxonomy, RuntimeError> {
        if config.taxonomy.file.is_empty() {
            return Ok(Taxonomy::empty(PROTOCOL_VERSION));
        }
        let path = std::path::Path::new(&config.taxonomy.file);
        let content = fs::read_to_string(path).map_err(|e| {
            RuntimeError::Taxonomy(format!("failed to read {}: {e}", path.display()))
        })?;
        if path.extension().is_some_and(|ext| ext == "json") {
            Taxonomy::load_json(&content, PROTOCOL_VERSION)
                .map_err(|e| RuntimeError::Taxonomy(e.to_string()))
        } else {
            Taxonomy::load_yaml(&content, PROTOCOL_VERSION)
                .map_err(|e| RuntimeError::Taxonomy(e.to_string()))
        }
    }

    /// Scan `taxonomy.verticals_dir` for `<id>/vertical.yaml` files and load
    /// each as a `VerticalManifest`. Errors on individual files are logged and
    /// skipped; a missing or unconfigured directory returns an empty registry.
    fn load_vertical_manifests(config: &RuntimeConfig) -> Arc<Vec<VerticalManifest>> {
        let dir = &config.taxonomy.verticals_dir;
        if dir.is_empty() {
            return Arc::new(vec![]);
        }
        let root = std::path::Path::new(dir);
        if !root.is_dir() {
            tracing::warn!(dir = %dir, "taxonomy.verticals_dir is not a directory — no manifests loaded");
            return Arc::new(vec![]);
        }

        let entries = match fs::read_dir(root) {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(dir = %dir, error = %err, "failed to read verticals_dir — no manifests loaded");
                return Arc::new(vec![]);
            }
        };

        let mut manifests: Vec<VerticalManifest> = entries
            .flatten()
            .filter_map(|entry| {
                let manifest_path = entry.path().join("vertical.yaml");
                if !manifest_path.is_file() {
                    return None;
                }
                match fs::read_to_string(&manifest_path) {
                    Ok(content) => match VerticalManifest::load_yaml(&content) {
                        Ok(m) => {
                            tracing::debug!(id = %m.id, "loaded vertical manifest");
                            Some(m)
                        }
                        Err(err) => {
                            tracing::warn!(
                                path = %manifest_path.display(),
                                error = %err,
                                "failed to parse vertical manifest — skipped"
                            );
                            None
                        }
                    },
                    Err(err) => {
                        tracing::warn!(
                            path = %manifest_path.display(),
                            error = %err,
                            "failed to read vertical manifest — skipped"
                        );
                        None
                    }
                }
            })
            .collect();

        // Stable ordering for deterministic registry layout.
        manifests.sort_by(|a, b| a.id.cmp(&b.id));
        Arc::new(manifests)
    }
}
