use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use wacp_coordinator::{Coordinator, DispatchRequest, GateController, GateFallback};
use wacp_permissions::PermissionEngine;
use wacp_recovery::RecoveryEngine;
use wacp_taxonomy::{Taxonomy, VerticalManifest};
use wacp_trail::{
    CheckpointStorage, FileCheckpointStorage, FileTrailConfig, FileTrailStorage,
    InMemoryCheckpointStorage, InMemoryTrailStorage, TrailIndex, TrailQuery,
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

/// Metadata stored alongside each checkpoint for GetCheckpoint lookups.
struct CheckpointRecord {
    content_hash: [u8; 32],
    workspace_id: String,
    checkpoint_type: String,
    intent: String,
    status: i32,
    confidence: i32,
    resource_usage: Option<wacp_transport::wacp_v1::ResourceUsage>,
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

    // Agent per-workspace subscribers for envelope/command delivery.
    envelope_subs: HashMap<String, Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::Envelope, tonic::Status>>>>,
    command_subs: HashMap<String, Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::Command, tonic::Status>>>>,

    // Checkpoint content-addressable store.
    checkpoint_storage: Arc<dyn CheckpointStorage>,

    // Checkpoint ID -> content hash mapping for GetCheckpoint lookups.
    checkpoint_index: HashMap<String, CheckpointRecord>,

    // Trail index for historical trail queries.
    trail_index: TrailIndex,

    // Gate controller for pending task approval gates.
    gate_controller: GateController,

    // Escalation ID -> workspace ID mapping for routing responses.
    escalation_index: HashMap<String, String>,

    // Workspace timestamps: (created_at_us, last_activity_us) in microseconds since epoch.
    workspace_timestamps: HashMap<String, (u64, u64)>,

    // Monotonic counters for ID generation.
    next_goal_id: u64,
    next_workspace_id: u64,
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

        // 4b. Open trail index for historical queries.
        let trail_index = TrailIndex::open(&data_dir.join("trail_index.sqlite"))
            .map_err(|e| RuntimeError::Storage(format!("trail index: {e}")))?;

        // 4c. Open checkpoint content-addressable store.
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
            envelope_subs: HashMap::new(),
            command_subs: HashMap::new(),
            checkpoint_storage,
            checkpoint_index: HashMap::new(),
            trail_index,
            gate_controller: GateController::new(30_000, GateFallback::AutoApprove),
            escalation_index: HashMap::new(),
            workspace_timestamps: HashMap::new(),
            next_goal_id: 0,
            next_workspace_id: 0,
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

        let trail_index = TrailIndex::open_in_memory()
            .map_err(|e| RuntimeError::Storage(format!("trail index: {e}")))?;

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
            envelope_subs: HashMap::new(),
            command_subs: HashMap::new(),
            checkpoint_storage: Arc::new(InMemoryCheckpointStorage::new()),
            checkpoint_index: HashMap::new(),
            trail_index,
            gate_controller: GateController::new(30_000, GateFallback::AutoApprove),
            escalation_index: HashMap::new(),
            workspace_timestamps: HashMap::new(),
            next_goal_id: 0,
            next_workspace_id: 0,
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
                    // Touch last_activity for the workspace that emitted the event.
                    match &event {
                        WorkspaceEvent::Signal(s) => self.touch_workspace(s.workspace_id.as_ref()),
                        WorkspaceEvent::StateChanged { workspace_id, .. } => self.touch_workspace(workspace_id.as_ref()),
                        WorkspaceEvent::Terminated(archived) => self.touch_workspace(archived.id.as_ref()),
                        _ => {}
                    }
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
                    let esc_id = format!("esc-{}", self.stream_seq);
                    self.escalation_index
                        .insert(esc_id.clone(), signal.workspace_id.to_string());
                    let esc_ev = Ok(wacp_v1::EscalationEvent {
                        escalation_id: esc_id,
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
        use wacp_trail::IndexEntry;
        use wacp_transport::wacp_v1;

        self.stream_seq += 1;
        let seq = self.stream_seq;

        // Index the entry for historical queries.
        let index_entry = IndexEntry {
            sequence_number: seq,
            timestamp_bytes: [0u8; 10], // no wall-clock yet
            workspace_id: if workspace_id.is_empty() {
                None
            } else {
                Some(workspace_id.to_string())
            },
            actor: "protocol".into(),
            event_type: event_type.to_string(),
            segment_id: 0,
            offset: 0,
            length: body.len() as u32,
        };
        if let Err(e) = self.trail_index.insert(&index_entry) {
            tracing::warn!(error = %e, "trail index insert failed");
        }

        let entry = Ok(wacp_v1::TrailEntry {
            id: format!("te-{seq}"),
            timestamp: None,
            workspace_id: workspace_id.to_string(),
            actor: "protocol".into(),
            event_type: event_type.into(),
            body,
            sequence_number: seq,
            chain_hash: Vec::new(),
        });
        self.trail_subs
            .retain(|tx| tx.try_send(entry.clone()).is_ok());
    }

    /// Query the trail index and build a QueryTrailResponse.
    fn query_trail_index(
        &self,
        workspace_id: Option<&str>,
        event_type: Option<&str>,
        actor: Option<&str>,
        limit: u32,
        client_request_id: &str,
    ) -> wacp_transport::wacp_v1::QueryTrailResponse {
        let query = TrailQuery {
            workspace_id: workspace_id.map(|s| s.to_string()),
            event_type: event_type.map(|s| s.to_string()),
            actor: actor.map(|s| s.to_string()),
            from_timestamp: None,
            to_timestamp: None,
            limit: if limit == 0 { 100 } else { limit },
        };
        match self.trail_index.query(&query) {
            Ok(result) => {
                let entries = result
                    .entries
                    .into_iter()
                    .map(|ie| wacp_transport::wacp_v1::TrailEntry {
                        id: format!("te-{}", ie.sequence_number),
                        timestamp: None,
                        workspace_id: ie.workspace_id.unwrap_or_default(),
                        actor: ie.actor,
                        event_type: ie.event_type,
                        body: Vec::new(), // index does not store body
                        sequence_number: ie.sequence_number,
                        chain_hash: Vec::new(),
                    })
                    .collect();
                wacp_transport::wacp_v1::QueryTrailResponse {
                    entries,
                    has_more: result.has_more,
                    client_request_id: client_request_id.to_string(),
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "trail index query failed");
                wacp_transport::wacp_v1::QueryTrailResponse {
                    entries: vec![],
                    has_more: false,
                    client_request_id: client_request_id.to_string(),
                }
            }
        }
    }

    /// Current time in microseconds since Unix epoch.
    fn now_us() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }

    /// Record workspace creation timestamp; update last_activity.
    fn record_workspace_created(&mut self, ws_id: &str) {
        let now = Self::now_us();
        self.workspace_timestamps
            .insert(ws_id.to_string(), (now, now));
    }

    /// Touch last_activity for a workspace.
    fn touch_workspace(&mut self, ws_id: &str) {
        let now = Self::now_us();
        if let Some(ts) = self.workspace_timestamps.get_mut(ws_id) {
            ts.1 = now;
        }
    }

    /// Push an envelope to any agents subscribed to the target workspace.
    fn notify_envelope_subs(&mut self, to_workspace: &str, envelope: &Envelope) {
        if let Some(subs) = self.envelope_subs.get_mut(to_workspace) {
            let proto_env = Ok(wacp_transport::wacp_v1::Envelope {
                id: envelope.id.to_string(),
                from_workspace: envelope.from_workspace.to_string(),
                to_workspace: envelope.to_workspace.to_string(),
                r#type: envelope.envelope_type.clone(),
                payload: envelope.payload.clone(),
                in_reply_to: envelope
                    .in_reply_to
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_default(),
                priority: envelope.priority as i32,
                timestamp: None,
                origin: envelope.origin as i32,
            });
            subs.retain(|tx| tx.try_send(proto_env.clone()).is_ok());
        }
    }

    /// Push a command to any agents subscribed to the target workspace.
    fn notify_command_subs(&mut self, workspace_id: &str, command_type: &str, _payload: Vec<u8>) {
        use wacp_transport::wacp_v1;
        if let Some(subs) = self.command_subs.get_mut(workspace_id) {
            // Wrap the command as a feedback envelope for the oneof.
            let cmd = Ok(wacp_v1::Command {
                command: Some(wacp_v1::command::Command::Feedback(wacp_v1::Envelope {
                    id: String::new(),
                    from_workspace: String::new(),
                    to_workspace: workspace_id.to_string(),
                    r#type: command_type.to_string(),
                    payload: Vec::new(),
                    in_reply_to: String::new(),
                    priority: 0,
                    timestamp: None,
                    origin: 0,
                })),
            });
            subs.retain(|tx| tx.try_send(cmd.clone()).is_ok());
        }
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

                // Notify any agents subscribed to the target workspace.
                self.notify_envelope_subs(to_ws.as_ref(), &envelope);

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
                workspace_id,
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
                let checkpoint_id = format!("cp-{cp_id}");

                // Index the checkpoint for later retrieval via GetCheckpoint.
                self.checkpoint_index.insert(
                    checkpoint_id.clone(),
                    CheckpointRecord {
                        content_hash: hash_bytes,
                        workspace_id,
                        checkpoint_type: request.r#type.clone(),
                        intent: request.intent.clone(),
                        status: request.status,
                        confidence: request.confidence,
                        resource_usage: request.resource_usage,
                    },
                );

                let response = wacp_transport::wacp_v1::CreateCheckpointResponse {
                    checkpoint_id,
                    content_hash: hash_hex,
                    timestamp: None,
                    client_request_id: request.client_request_id,
                };
                let _ = reply.send(Ok(response));
            }
            AgentRequest::QueryTrail { request, reply, .. } => {
                let response = self.query_trail_index(
                    if request.workspace_id.is_empty() {
                        None
                    } else {
                        Some(&request.workspace_id)
                    },
                    if request.event_type.is_empty() {
                        None
                    } else {
                        Some(&request.event_type)
                    },
                    None,
                    request.limit,
                    &request.client_request_id,
                );
                let _ = reply.send(Ok(response));
            }
            AgentRequest::SubscribeEnvelopes {
                workspace_id,
                tx,
            } => {
                self.envelope_subs
                    .entry(workspace_id)
                    .or_default()
                    .push(tx);
            }
            AgentRequest::SubscribeCommands {
                workspace_id,
                tx,
            } => {
                self.command_subs
                    .entry(workspace_id)
                    .or_default()
                    .push(tx);
            }
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
                let mut tasks = Vec::new();
                let roots = self.coordinator.task_graph.roots();
                let mut visited = std::collections::HashSet::new();
                let mut queue = std::collections::VecDeque::from_iter(roots.into_iter().cloned());

                while let Some(tid) = queue.pop_front() {
                    if !visited.insert(tid.to_string()) {
                        continue;
                    }
                    if let Some(task) = self.coordinator.task_graph.get(&tid) {
                        let status = match task.status {
                            TaskStatus::Draft => 1,
                            TaskStatus::Pending => 2,
                            TaskStatus::Assigned => 3,
                            TaskStatus::InProgress => 4,
                            TaskStatus::Completed => 5,
                            TaskStatus::Failed => 6,
                            TaskStatus::Integrated => 7,
                            TaskStatus::Cancelled => 8,
                        };
                        tasks.push(wacp_transport::wacp_v1::Task {
                            id: task.id.to_string(),
                            name: task.name.clone(),
                            description: task.description.clone(),
                            depends_on: task.depends_on.iter().map(|d| d.to_string()).collect(),
                            parent_task: task.parent_task.as_ref().map(|p| p.to_string()).unwrap_or_default(),
                            status,
                            workspace_ref: task.workspace_ref.as_ref().map(|w| w.to_string()).unwrap_or_default(),
                            workspace_history: task.workspace_history.iter().map(|w| w.to_string()).collect(),
                            checkpoint_ref: task.checkpoint_ref.as_ref().map(|c| c.to_string()).unwrap_or_default(),
                        });
                        for dep in self.coordinator.task_graph.dependents(&tid) {
                            queue.push_back(dep.clone());
                        }
                    }
                }

                let response = wacp_transport::wacp_v1::TaskGraphView { tasks };
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::InjectEnvelope { request, reply } => {
                let to_ws = WorkspaceId::from(request.to_workspace.as_str());

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
                    from_workspace: WorkspaceId::from("highway"),
                    to_workspace: to_ws.clone(),
                    envelope_type: request.r#type,
                    payload: request.payload,
                    in_reply_to: None,
                    timestamp: 0,
                    priority,
                    origin: EnvelopeOrigin::Human,
                    state: EnvelopeState::Created,
                };

                self.notify_envelope_subs(to_ws.as_ref(), &envelope);

                if self.coordinator.route_envelope(&to_ws, envelope).await {
                    let response = wacp_transport::wacp_v1::InjectEnvelopeResponse {
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
            HighwayRequest::RespondToGate { request, reply } => {
                let gate_id = GateId::from(request.gate_id.as_str());
                let decision = match request.decision {
                    1 => GateDecision::Approve,
                    2 => GateDecision::Reject,
                    3 => GateDecision::Modify,
                    _ => GateDecision::Approve,
                };

                let applied = self.gate_controller.resolve(&gate_id, decision).is_some();

                let response = wacp_transport::wacp_v1::GateResponseAck {
                    gate_id: request.gate_id,
                    applied,
                    client_request_id: request.client_request_id,
                };
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::RespondToEscalation { request, reply } => {
                let ws_id_str = self.escalation_index.remove(&request.escalation_id);

                let applied = if let Some(ws_id_str) = ws_id_str {
                    let ws_id = WorkspaceId::from(ws_id_str.as_str());
                    match request.action {
                        Some(wacp_transport::wacp_v1::escalation_response::Action::Feedback(
                            env_proto,
                        )) => {
                            let env_id = self.next_envelope_id;
                            self.next_envelope_id += 1;
                            let envelope = Envelope {
                                id: EnvelopeId::from(format!("env-{env_id}")),
                                from_workspace: WorkspaceId::from("highway"),
                                to_workspace: ws_id.clone(),
                                envelope_type: env_proto.r#type,
                                payload: env_proto.payload,
                                in_reply_to: None,
                                timestamp: 0,
                                priority: EnvelopePriority::Normal,
                                origin: EnvelopeOrigin::Human,
                                state: EnvelopeState::Created,
                            };
                            self.notify_envelope_subs(ws_id.as_ref(), &envelope);
                            self.coordinator.route_envelope(&ws_id, envelope).await
                        }
                        Some(wacp_transport::wacp_v1::escalation_response::Action::Abort(true)) => {
                            self.coordinator.abort_workspace(&ws_id).await
                        }
                        Some(
                            wacp_transport::wacp_v1::escalation_response::Action::DelegateToCoordinator(
                                true,
                            ),
                        ) => true,
                        _ => false,
                    }
                } else {
                    false
                };

                let response = wacp_transport::wacp_v1::EscalationResponseAck {
                    escalation_id: request.escalation_id,
                    applied,
                    client_request_id: request.client_request_id,
                };
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::QueryTrail { request, reply } => {
                let response = self.query_trail_index(
                    if request.workspace_id.is_empty() {
                        None
                    } else {
                        Some(&request.workspace_id)
                    },
                    if request.event_type.is_empty() {
                        None
                    } else {
                        Some(&request.event_type)
                    },
                    if request.actor.is_empty() {
                        None
                    } else {
                        Some(&request.actor)
                    },
                    request.limit,
                    &request.client_request_id,
                );
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::GetWorkspace { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                if let Some(node) = self.coordinator.tree.get(&ws_id) {
                    let response = wacp_transport::wacp_v1::WorkspaceView {
                        id: node.id.to_string(),
                        state: node.status as i32,
                        role: node.owner.to_string(),
                        parent: node
                            .parent
                            .as_ref()
                            .map(|p| p.to_string())
                            .unwrap_or_default(),
                        owner: node.owner.to_string(),
                        originator: match &node.originator {
                            Originator::System => "system".into(),
                            Originator::User(uid) => uid.to_string(),
                        },
                        task_id: node
                            .task_id
                            .as_ref()
                            .map(|t| t.to_string())
                            .unwrap_or_default(),
                        current_usage: Some(wacp_transport::wacp_v1::ResourceUsage {
                            tokens: 0,
                            wall_time_ms: 0,
                            storage_bytes: 0,
                            network_bytes: 0,
                            cost_micros: 0,
                        }),
                        budget: Some(wacp_transport::wacp_v1::ResourceBudget {
                            max_tokens: self.config.resources.default_budget.max_tokens,
                            max_wall_time_ms: self.config.resources.default_budget.max_wall_time_ms,
                            max_storage_bytes: self
                                .config
                                .resources
                                .default_budget
                                .max_storage_bytes,
                            max_network_bytes: self
                                .config
                                .resources
                                .default_budget
                                .max_network_bytes,
                            max_cost_micros: self.config.resources.default_budget.max_cost_micros,
                            warning_threshold: self.config.resources.warning_threshold,
                        }),
                        checkpoint_count: self
                            .checkpoint_index
                            .values()
                            .filter(|r| r.workspace_id == ws_id.as_ref())
                            .count() as u32,
                        created_at: self
                            .workspace_timestamps
                            .get(ws_id.as_ref())
                            .map(|(created, _)| wacp_transport::wacp_v1::Timestamp {
                                physical_us: *created,
                                logical: 0,
                            }),
                        last_activity: self
                            .workspace_timestamps
                            .get(ws_id.as_ref())
                            .map(|(_, last)| wacp_transport::wacp_v1::Timestamp {
                                physical_us: *last,
                                logical: 0,
                            }),
                    };
                    let _ = reply.send(Ok(response));
                } else {
                    let _ = reply.send(Err(tonic::Status::not_found("workspace not found")));
                }
            }
            HighwayRequest::GetCheckpoint { request, reply } => {
                let response = match self.checkpoint_index.get(&request.checkpoint_id) {
                    Some(record) => {
                        let hash_hex: String =
                            record.content_hash.iter().map(|b| format!("{b:02x}")).collect();
                        match self.checkpoint_storage.read(&record.content_hash) {
                            Ok(Some(payload)) => Ok(wacp_transport::wacp_v1::CheckpointView {
                                metadata: Some(wacp_transport::wacp_v1::Checkpoint {
                                    id: request.checkpoint_id,
                                    workspace_id: record.workspace_id.clone(),
                                    r#type: record.checkpoint_type.clone(),
                                    payload: Vec::new(), // payload is in the top-level field
                                    content_hash: hash_hex,
                                    intent: record.intent.clone(),
                                    parent_checkpoint: String::new(),
                                    status: record.status,
                                    confidence: record.confidence,
                                    timestamp: None,
                                    resource_usage: record.resource_usage,
                                }),
                                payload,
                            }),
                            Ok(None) => Err(tonic::Status::not_found(
                                "checkpoint blob missing from store",
                            )),
                            Err(e) => Err(tonic::Status::internal(format!(
                                "checkpoint read error: {e}"
                            ))),
                        }
                    }
                    None => Err(tonic::Status::not_found(format!(
                        "checkpoint '{}' not found",
                        request.checkpoint_id
                    ))),
                };
                let _ = reply.send(response);
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
        use wacp_fsm::TaskTrigger;
        use wacp_transport::CoordinatorRequest;
        use wacp_workspace::actor::CoordinatorCommand;

        match req {
            CoordinatorRequest::SubmitGoal { request, reply } => {
                let gid = self.next_goal_id;
                self.next_goal_id += 1;
                let goal_id = format!("goal-{gid}");

                // Create a root-level task for this goal.
                let task_id = TaskId::from(format!("task-{goal_id}").as_str());
                let task = Task {
                    id: task_id.clone(),
                    name: request.description.clone(),
                    description: request.description.clone(),
                    depends_on: vec![],
                    parent_task: None,
                    status: TaskStatus::Draft,
                    workspace_ref: None,
                    workspace_history: vec![],
                    checkpoint_ref: None,
                };
                if let Err(e) = self.coordinator.task_graph.add_task(task) {
                    let _ = reply.send(Err(tonic::Status::internal(format!(
                        "task graph error: {e}"
                    ))));
                    return;
                }
                // Approve the task so it becomes Pending (dispatchable).
                let _ = self
                    .coordinator
                    .task_graph
                    .transition(&task_id, TaskTrigger::Approve);

                // Create a workspace for the goal and dispatch.
                let wsid = self.next_workspace_id;
                self.next_workspace_id += 1;
                let ws_id = WorkspaceId::from(format!("ws-{wsid}").as_str());

                let ws_config = wacp_workspace::state::WorkspaceConfig {
                    id: ws_id.clone(),
                    role: "worker".into(),
                    base_role: BaseRole::Worker,
                    parent: self.coordinator.tree.root().clone(),
                    owner: UserId::from("system"),
                    originator: Originator::System,
                    directive: Envelope {
                        id: EnvelopeId::from(format!("env-goal-{gid}").as_str()),
                        from_workspace: self.coordinator.tree.root().clone(),
                        to_workspace: ws_id.clone(),
                        envelope_type: "directive".into(),
                        payload: request.context,
                        in_reply_to: None,
                        timestamp: 0,
                        priority: EnvelopePriority::Normal,
                        origin: EnvelopeOrigin::Human,
                        state: EnvelopeState::Created,
                    },
                    context: Vec::new(),
                    visibility: std::collections::HashSet::new(),
                    authority: std::collections::HashSet::new(),
                    delegate: false,
                    budget: None,
                };
                self.coordinator.dispatch(DispatchRequest {
                    task_id: task_id.clone(),
                    config: ws_config,
                });
                self.record_workspace_created(ws_id.as_ref());

                // Bind task to workspace and advance to Assigned.
                let _ = self.coordinator.task_graph.bind(&task_id, &ws_id);
                let _ = self
                    .coordinator
                    .task_graph
                    .transition(&task_id, TaskTrigger::Assign);

                let response = wacp_transport::wacp_v1::SubmitGoalResponse {
                    goal_id,
                    root_workspace_id: ws_id.to_string(),
                };
                let _ = reply.send(Ok(response));
            }
            CoordinatorRequest::GetReadyTasks { reply, .. } => {
                let ready: Vec<wacp_transport::wacp_v1::TaskView> = self
                    .coordinator
                    .task_graph
                    .ready_tasks()
                    .into_iter()
                    .filter_map(|tid| {
                        let t = self.coordinator.task_graph.get(tid)?;
                        Some(wacp_transport::wacp_v1::TaskView {
                            task_id: t.id.to_string(),
                            name: t.name.clone(),
                            description: t.description.clone(),
                            status: t.status as i32,
                            assigned_workspace: t
                                .workspace_ref
                                .as_ref()
                                .map(|w| w.to_string())
                                .unwrap_or_default(),
                            depends_on: t.depends_on.iter().map(|d| d.to_string()).collect(),
                        })
                    })
                    .collect();
                let response = wacp_transport::wacp_v1::GetReadyTasksResponse { tasks: ready };
                let _ = reply.send(Ok(response));
            }
            CoordinatorRequest::Dispatch { request, reply } => {
                let task_id = TaskId::from(request.task_id.as_str());

                // Validate the task exists in the graph.
                if self.coordinator.task_graph.get(&task_id).is_none() {
                    let _ = reply.send(Err(tonic::Status::not_found(format!(
                        "task '{}' not found",
                        request.task_id
                    ))));
                    return;
                }

                let wsid = self.next_workspace_id;
                self.next_workspace_id += 1;
                let ws_id = WorkspaceId::from(format!("ws-{wsid}").as_str());

                let ws_config = wacp_workspace::state::WorkspaceConfig {
                    id: ws_id.clone(),
                    role: request.role.clone(),
                    base_role: BaseRole::Worker,
                    parent: self.coordinator.tree.root().clone(),
                    owner: UserId::from("system"),
                    originator: Originator::System,
                    directive: Envelope {
                        id: EnvelopeId::from(
                            format!("env-dispatch-{wsid}").as_str(),
                        ),
                        from_workspace: self.coordinator.tree.root().clone(),
                        to_workspace: ws_id.clone(),
                        envelope_type: "directive".into(),
                        payload: request.directive_payload,
                        in_reply_to: None,
                        timestamp: 0,
                        priority: EnvelopePriority::Normal,
                        origin: EnvelopeOrigin::Human,
                        state: EnvelopeState::Created,
                    },
                    context: Vec::new(),
                    visibility: std::collections::HashSet::new(),
                    authority: std::collections::HashSet::new(),
                    delegate: false,
                    budget: None,
                };
                self.coordinator.dispatch(DispatchRequest {
                    task_id: task_id.clone(),
                    config: ws_config,
                });
                self.record_workspace_created(ws_id.as_ref());

                // Bind task to workspace and advance to Assigned.
                let _ = self.coordinator.task_graph.bind(&task_id, &ws_id);
                let _ = self
                    .coordinator
                    .task_graph
                    .transition(&task_id, TaskTrigger::Assign);

                let response = wacp_transport::wacp_v1::DispatchResponse {
                    workspace_id: ws_id.to_string(),
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
            CoordinatorRequest::SuspendWorkspace { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                let result = match self.coordinator.handle(&ws_id) {
                    Some(handle) => {
                        if handle
                            .coordinator_tx
                            .send(CoordinatorCommand::Suspend)
                            .await
                            .is_ok()
                        {
                            self.notify_command_subs(
                                &request.workspace_id,
                                "suspend",
                                Vec::new(),
                            );
                            Ok(wacp_transport::wacp_v1::SuspendWorkspaceResponse::default())
                        } else {
                            Err(tonic::Status::unavailable("workspace actor not running"))
                        }
                    }
                    None => Err(tonic::Status::not_found(format!(
                        "workspace '{}' not found",
                        request.workspace_id
                    ))),
                };
                let _ = reply.send(result);
            }
            CoordinatorRequest::ResumeWorkspace { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                let result = match self.coordinator.handle(&ws_id) {
                    Some(handle) => {
                        if handle
                            .coordinator_tx
                            .send(CoordinatorCommand::Resume)
                            .await
                            .is_ok()
                        {
                            self.notify_command_subs(
                                &request.workspace_id,
                                "resume",
                                Vec::new(),
                            );
                            Ok(wacp_transport::wacp_v1::ResumeWorkspaceResponse::default())
                        } else {
                            Err(tonic::Status::unavailable("workspace actor not running"))
                        }
                    }
                    None => Err(tonic::Status::not_found(format!(
                        "workspace '{}' not found",
                        request.workspace_id
                    ))),
                };
                let _ = reply.send(result);
            }
            CoordinatorRequest::Decompose { request, reply } => {
                let mut task_ids = Vec::new();
                for task_def in &request.tasks {
                    let gid = self.next_goal_id;
                    self.next_goal_id += 1;
                    let tid = TaskId::from(format!("task-decompose-{gid}").as_str());
                    let deps: Vec<TaskId> = task_def
                        .depends_on
                        .iter()
                        .map(|d| TaskId::from(d.as_str()))
                        .collect();
                    let task = Task {
                        id: tid.clone(),
                        name: task_def.name.clone(),
                        description: task_def.description.clone(),
                        depends_on: deps,
                        parent_task: None,
                        status: TaskStatus::Draft,
                        workspace_ref: None,
                        workspace_history: vec![],
                        checkpoint_ref: None,
                    };
                    match self.coordinator.task_graph.add_task(task) {
                        Ok(()) => {
                            // Approve so it becomes dispatchable.
                            let _ = self
                                .coordinator
                                .task_graph
                                .transition(&tid, TaskTrigger::Approve);
                            task_ids.push(tid.to_string());
                        }
                        Err(e) => {
                            tracing::warn!(task_id = %tid, error = %e, "decompose: task rejected");
                        }
                    }
                }
                let response = wacp_transport::wacp_v1::DecomposeResponse { task_ids };
                let _ = reply.send(Ok(response));
            }
            CoordinatorRequest::CancelTask { request, reply } => {
                let task_id = TaskId::from(request.task_id.as_str());
                match self
                    .coordinator
                    .task_graph
                    .transition(&task_id, TaskTrigger::Cancel)
                {
                    Ok(_) => {
                        // If the task is bound to a workspace, abort the workspace.
                        if let Some(task) = self.coordinator.task_graph.get(&task_id)
                            && let Some(ws_id) = &task.workspace_ref
                        {
                            self.coordinator.abort_workspace(ws_id).await;
                        }
                        let _ = reply.send(Ok(
                            wacp_transport::wacp_v1::CancelTaskResponse::default(),
                        ));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(tonic::Status::failed_precondition(format!(
                            "cannot cancel task: {e}"
                        ))));
                    }
                }
            }
            CoordinatorRequest::SendDirective { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                let env_id = self.next_envelope_id;
                self.next_envelope_id += 1;
                let envelope = Envelope {
                    id: EnvelopeId::from(format!("env-{env_id}").as_str()),
                    from_workspace: self.coordinator.tree.root().clone(),
                    to_workspace: ws_id.clone(),
                    envelope_type: "directive".into(),
                    payload: request.payload,
                    in_reply_to: None,
                    timestamp: 0,
                    priority: EnvelopePriority::Normal,
                    origin: EnvelopeOrigin::Human,
                    state: EnvelopeState::Created,
                };
                let envelope_id = envelope.id.to_string();
                self.notify_envelope_subs(ws_id.as_ref(), &envelope);
                if self.coordinator.route_envelope(&ws_id, envelope).await {
                    let response = wacp_transport::wacp_v1::SendDirectiveResponse { envelope_id };
                    let _ = reply.send(Ok(response));
                } else {
                    let _ = reply.send(Err(tonic::Status::unavailable(format!(
                        "workspace '{}' is not running",
                        request.workspace_id
                    ))));
                }
            }
            CoordinatorRequest::SendFeedback { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                let env_id = self.next_envelope_id;
                self.next_envelope_id += 1;
                let envelope = Envelope {
                    id: EnvelopeId::from(format!("env-{env_id}").as_str()),
                    from_workspace: self.coordinator.tree.root().clone(),
                    to_workspace: ws_id.clone(),
                    envelope_type: "feedback".into(),
                    payload: request.payload,
                    in_reply_to: if request.in_reply_to.is_empty() {
                        None
                    } else {
                        Some(EnvelopeId::from(request.in_reply_to.as_str()))
                    },
                    timestamp: 0,
                    priority: EnvelopePriority::Normal,
                    origin: EnvelopeOrigin::Human,
                    state: EnvelopeState::Created,
                };
                let envelope_id = envelope.id.to_string();
                self.notify_envelope_subs(ws_id.as_ref(), &envelope);
                if self.coordinator.route_envelope(&ws_id, envelope).await {
                    let response = wacp_transport::wacp_v1::SendFeedbackResponse { envelope_id };
                    let _ = reply.send(Ok(response));
                } else {
                    let _ = reply.send(Err(tonic::Status::unavailable(format!(
                        "workspace '{}' is not running",
                        request.workspace_id
                    ))));
                }
            }
            CoordinatorRequest::TriggerIntegration { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                match self.coordinator.handle(&ws_id) {
                    Some(handle) => {
                        if handle
                            .coordinator_tx
                            .send(CoordinatorCommand::IntegrationSucceeded)
                            .await
                            .is_ok()
                        {
                            let _ = reply.send(Ok(
                                wacp_transport::wacp_v1::TriggerIntegrationResponse {
                                    result: "accepted".into(),
                                    detail: format!("integration triggered for {}", request.workspace_id),
                                },
                            ));
                        } else {
                            let _ = reply.send(Err(tonic::Status::unavailable(
                                "workspace actor not running",
                            )));
                        }
                    }
                    None => {
                        let _ = reply.send(Err(tonic::Status::not_found(format!(
                            "workspace '{}' not found",
                            request.workspace_id
                        ))));
                    }
                }
            }
            CoordinatorRequest::GetAllocatable { reply, .. } => {
                let budget = &self.config.resources.default_budget;
                let _ = reply.send(Ok(wacp_transport::wacp_v1::GetAllocatableResponse {
                    remaining: Some(wacp_transport::wacp_v1::ResourceBudget {
                        max_tokens: budget.max_tokens,
                        max_wall_time_ms: budget.max_wall_time_ms,
                        max_storage_bytes: budget.max_storage_bytes,
                        max_network_bytes: budget.max_network_bytes,
                        max_cost_micros: budget.max_cost_micros,
                        warning_threshold: self.config.resources.warning_threshold,
                    }),
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
