use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use wacp_coordinator::Coordinator;
use wacp_permissions::PermissionEngine;
use wacp_recovery::RecoveryEngine;
use wacp_taxonomy::{Taxonomy, VerticalManifest};
use wacp_trail::{FileTrailConfig, FileTrailStorage, InMemoryTrailStorage};
use wacp_transport::{start_grpc_server, GrpcServerConfig};
use wacp_types::*;
use wacp_workspace::WorkspaceEvent;

use crate::config::{RuntimeConfig, PROTOCOL_VERSION};

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
}

impl Runtime {
    /// Full initialization sequence — production mode with filesystem storage and gRPC.
    pub async fn init(config: RuntimeConfig) -> Result<Self, RuntimeError> {
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
        let recovered = RecoveryEngine::recover(&trail)
            .map_err(|e| RuntimeError::Recovery(e.to_string()))?;

        tracing::info!(
            workspaces_recovered = recovered.workspace_states.len(),
            tasks = recovered.task_statuses.len(),
            in_flight_envelopes = recovered.in_flight_envelopes.len(),
            last_sequence = recovered.last_sequence,
            "recovery completed"
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

        let coordinator_addr: SocketAddr = config
            .server
            .coordinator_listen
            .parse()
            .map_err(|e| RuntimeError::Transport(format!("invalid coordinator address: {e}")))?;

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
        })
    }

    /// Initialize with in-memory storage for testing (no gRPC, no filesystem).
    pub fn init_in_memory(config: RuntimeConfig) -> Result<Self, RuntimeError> {
        let taxonomy = Self::load_taxonomy(&config)?;
        let verticals = Self::load_vertical_manifests(&config);

        let trail = InMemoryTrailStorage::new();
        let _recovered = RecoveryEngine::recover(&trail)
            .map_err(|e| RuntimeError::Recovery(e.to_string()))?;

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

                // Shutdown signal.
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("received shutdown signal");
                    break;
                }
            }
        }

        self.shutdown().await;
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
            AgentRequest::SendEnvelope { reply, .. } => {
                let _ = reply.send(Err(tonic::Status::unimplemented(
                    "envelope routing via gRPC pending full wiring",
                )));
            }
            AgentRequest::CreateCheckpoint { reply, .. } => {
                let _ = reply.send(Err(tonic::Status::unimplemented(
                    "checkpoint creation via gRPC pending full wiring",
                )));
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
                        parent: node.parent.as_ref().map(|p| p.to_string()).unwrap_or_default(),
                        owner: node.owner.to_string(),
                        originator: String::new(),
                        task_id: node.task_id.as_ref().map(|t| t.to_string()).unwrap_or_default(),
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
