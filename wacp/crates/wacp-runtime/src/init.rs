use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;
use wacp_coordinator::{Coordinator, GateController, GateFallback};
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
use wacp_workspace::state::WorkspaceConfig;

use crate::config::{PROTOCOL_VERSION, RuntimeConfig};
use crate::conversions::{envelope_to_proto, signal_type_to_proto, workspace_state_to_proto};

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
pub(crate) struct CheckpointRecord {
    pub(crate) content_hash: [u8; 32],
    pub(crate) workspace_id: String,
    pub(crate) checkpoint_type: String,
    pub(crate) intent: String,
    pub(crate) status: i32,
    pub(crate) confidence: i32,
    pub(crate) resource_usage: Option<wacp_transport::wacp_v1::ResourceUsage>,
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
    pub(crate) signal_subs:
        Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::SignalEvent, tonic::Status>>>,
    pub(crate) ws_change_subs:
        Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::WorkspaceStateChange, tonic::Status>>>,
    pub(crate) trail_subs:
        Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::TrailEntry, tonic::Status>>>,
    pub(crate) gate_subs:
        Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::GateEvent, tonic::Status>>>,
    pub(crate) escalation_subs:
        Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::EscalationEvent, tonic::Status>>>,

    // Agent per-workspace subscribers for envelope/command delivery.
    pub(crate) envelope_subs: HashMap<
        String,
        Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::Envelope, tonic::Status>>>,
    >,
    pub(crate) command_subs:
        HashMap<String, Vec<mpsc::Sender<Result<wacp_transport::wacp_v1::Command, tonic::Status>>>>,

    // Checkpoint content-addressable store.
    pub(crate) checkpoint_storage: Arc<dyn CheckpointStorage>,

    // Checkpoint ID -> content hash mapping for GetCheckpoint lookups.
    pub(crate) checkpoint_index: HashMap<String, CheckpointRecord>,

    // Trail index for historical trail queries.
    pub(crate) trail_index: TrailIndex,

    // Gate controller for pending task approval gates.
    pub(crate) gate_controller: GateController,

    // Escalation ID -> workspace ID mapping for routing responses.
    pub(crate) escalation_index: HashMap<String, String>,

    // Workspace timestamps: (created_at_us, last_activity_us) in microseconds since epoch.
    pub(crate) workspace_timestamps: HashMap<String, (u64, u64)>,

    // Workspace config cache — populated on SubmitGoal / Dispatch, read by
    // AgentService::Bind, dropped on WorkspaceEvent::Terminated. Gives the
    // Bind handler access to the role/directive/context/visibility/authority/
    // budget set at creation time without having to round-trip through the
    // workspace actor. See `wacp/impl/wa1-bind-projection.md`.
    pub(crate) workspace_configs: HashMap<String, WorkspaceConfig>,

    // Monotonic counters for ID generation.
    pub(crate) next_goal_id: u64,
    pub(crate) next_workspace_id: u64,
    pub(crate) next_envelope_id: u64,
    pub(crate) next_checkpoint_id: u64,
    pub(crate) stream_seq: u64,
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
            workspace_configs: HashMap::new(),
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
            workspace_configs: HashMap::new(),
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
                    self.coordinator.handle_event(&event).await;
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
    pub(crate) fn fan_out_event(&mut self, event: &WorkspaceEvent) {
        use wacp_transport::wacp_v1;

        match event {
            WorkspaceEvent::Signal(signal) => {
                let proto_ev = Ok(wacp_v1::SignalEvent {
                    workspace_id: signal.workspace_id.to_string(),
                    signal_type: signal_type_to_proto(signal.signal_type) as i32,
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
                let trigger = match to {
                    WorkspaceState::Active if *from == WorkspaceState::Idle => "dispatched",
                    WorkspaceState::Active if *from == WorkspaceState::Suspended => "resumed",
                    WorkspaceState::Active => "activated",
                    WorkspaceState::Suspended => "suspended",
                    WorkspaceState::Blocked => "blocked",
                    WorkspaceState::Migrating => "migrating",
                    WorkspaceState::Integrating => "integrating",
                    WorkspaceState::Conflicted => "conflicted",
                    WorkspaceState::Closed => "closed",
                    WorkspaceState::Failed => "failed",
                    WorkspaceState::Idle => "reset",
                };
                let proto_ev = Ok(wacp_v1::WorkspaceStateChange {
                    workspace_id: workspace_id.to_string(),
                    previous: workspace_state_to_proto(*from) as i32,
                    current: workspace_state_to_proto(*to) as i32,
                    trigger: trigger.into(),
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
                    current: workspace_state_to_proto(archived.terminal_state) as i32,
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

                // WA1: workspace is done — no more Bind calls should pull
                // its config. Drop the cached copy so the map stays bounded.
                self.workspace_configs.remove(archived.id.as_ref());
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
    pub(crate) fn emit_trail_entry(&mut self, workspace_id: &str, event_type: &str, body: Vec<u8>) {
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
    pub(crate) fn query_trail_index(
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
    pub(crate) fn record_workspace_created(&mut self, ws_id: &str) {
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
    pub(crate) fn notify_envelope_subs(&mut self, to_workspace: &str, envelope: &Envelope) {
        if let Some(subs) = self.envelope_subs.get_mut(to_workspace) {
            let proto_env = Ok(envelope_to_proto(envelope));
            subs.retain(|tx| tx.try_send(proto_env.clone()).is_ok());
        }
    }

    // No per-instance state is needed — these two helpers are plain
    // projections. Defined as free functions below the impl block.

    /// Push a command to any agents subscribed to the target workspace.
    pub(crate) fn notify_command_subs(
        &mut self,
        workspace_id: &str,
        command_type: &str,
        _payload: Vec<u8>,
    ) {
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
                self.coordinator.handle_event(&e).await;
            } else {
                break;
            }
        }

        tracing::info!("shutdown complete");
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

        // readdir order is filesystem-dependent. Downstream consumers index
        // manifests by `manifest.id` into HashMap-keyed structures, so order
        // doesn't affect lookup correctness. Not sorting here is intentional —
        // adding a sort would only matter for log-emission ordering and
        // first-load-wins on duplicate ids (the latter is itself a misuse;
        // operators should ensure unique vertical ids in the manifests dir).
        // Documented per v0.1.0-gate-enforcement-plan §5.C-C.5 to inoculate
        // against future "should we sort here?" review questions.

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
