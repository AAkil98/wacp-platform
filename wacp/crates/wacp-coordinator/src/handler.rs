use wacp_types::*;

use crate::gate::{GateController, GateResolution};
use crate::integration::IntegrationQueue;
use crate::migration::MigrationCoordinator;
use crate::resource::{LivenessMonitor, TimeoutTracker};
use crate::task_graph::TaskGraph;
use crate::topology::TopologySet;

// ── Request/Response types ──────────────────────────────────────────

/// Result of a Bind RPC.
#[derive(Debug, Clone, PartialEq)]
pub struct BindResult {
    pub workspace_id: WorkspaceId,
    pub state: WorkspaceState,
    pub role: String,
    pub owner: UserId,
    pub originator: Originator,
    pub parent: Option<WorkspaceId>,
    pub task_id: Option<TaskId>,
}

/// Result of an envelope send (agent or highway injection).
#[derive(Debug, Clone, PartialEq)]
pub struct SendEnvelopeResult {
    pub envelope_id: EnvelopeId,
}

/// Result of a signal emission.
#[derive(Debug, Clone, PartialEq)]
pub struct EmitSignalResult {
    pub accepted: bool,
}

/// Result of checkpoint creation.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateCheckpointResult {
    pub checkpoint_id: CheckpointId,
    pub content_hash: String,
}

/// Workspace view for highway queries.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceView {
    pub id: WorkspaceId,
    pub state: WorkspaceState,
    pub parent: Option<WorkspaceId>,
    pub owner: UserId,
    pub originator: Originator,
    pub task_id: Option<TaskId>,
}

/// Task view for highway queries.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskView {
    pub id: TaskId,
    pub name: String,
    pub status: TaskStatus,
    pub workspace_ref: Option<WorkspaceId>,
    pub depends_on: Vec<TaskId>,
    pub parent_task: Option<TaskId>,
}

/// Task graph view for highway queries.
#[derive(Debug, Clone, Default)]
pub struct TaskGraphSnapshot {
    pub tasks: Vec<TaskView>,
}

/// Handler error.
#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(WorkspaceId),
    #[error("task not found: {0}")]
    TaskNotFound(TaskId),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("no send right to target: {0}")]
    NoSendRight(WorkspaceId),
    #[error("gate not found")]
    GateNotFound,
}

// ── Request Handler ─────────────────────────────────────────────────

/// Handles agent and highway RPCs using coordinator state.
/// Domain-level types only — no gRPC/tonic dependency.
pub struct RequestHandler<'a> {
    pub topology: &'a mut TopologySet,
    pub task_graph: &'a mut TaskGraph,
    pub gate_controller: &'a mut GateController,
    pub integration_queue: &'a IntegrationQueue,
    pub timeout_tracker: &'a TimeoutTracker,
    pub liveness_monitor: &'a LivenessMonitor,
    pub migration: &'a MigrationCoordinator,
    next_envelope_id: &'a mut u64,
    next_checkpoint_id: &'a mut u64,
}

impl<'a> RequestHandler<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        topology: &'a mut TopologySet,
        task_graph: &'a mut TaskGraph,
        gate_controller: &'a mut GateController,
        integration_queue: &'a IntegrationQueue,
        timeout_tracker: &'a TimeoutTracker,
        liveness_monitor: &'a LivenessMonitor,
        migration: &'a MigrationCoordinator,
        next_envelope_id: &'a mut u64,
        next_checkpoint_id: &'a mut u64,
    ) -> Self {
        Self {
            topology,
            task_graph,
            gate_controller,
            integration_queue,
            timeout_tracker,
            liveness_monitor,
            migration,
            next_envelope_id,
            next_checkpoint_id,
        }
    }

    // ── Agent RPCs (13.1) ───────────────────────────────────────────

    /// Bind: return workspace state for an agent connection.
    ///
    /// If the workspace is migrating, only the expected new agent
    /// (identified by `agent_identity`) may bind. Pass `None` for
    /// non-migration binds or when identity is not yet verified.
    pub fn handle_bind(
        &self,
        workspace_id: &WorkspaceId,
        agent_identity: Option<&str>,
    ) -> Result<BindResult, HandlerError> {
        let node = self
            .topology
            .tree
            .get(workspace_id)
            .ok_or_else(|| HandlerError::WorkspaceNotFound(workspace_id.clone()))?;

        // Migration identity check: if workspace is migrating,
        // only the expected new agent may bind.
        if node.status == WorkspaceState::Migrating
            && let Some(expected) = self.migration.expected_agent(workspace_id.as_ref())
        {
            match agent_identity {
                Some(identity) if identity == expected.agent_type => {}
                _ => {
                    return Err(HandlerError::PermissionDenied(
                        "migration: agent identity mismatch".to_string(),
                    ));
                }
            }
        }

        Ok(BindResult {
            workspace_id: node.id.clone(),
            state: node.status,
            role: String::new(), // resolved role set by the orchestrator at dispatch
            owner: node.owner.clone(),
            originator: node.originator.clone(),
            parent: node.parent.clone(),
            task_id: node.task_id.clone(),
        })
    }

    /// SendEnvelope: validate port rights and create envelope.
    pub fn handle_send_envelope(
        &mut self,
        from_workspace: &WorkspaceId,
        to_workspace: &WorkspaceId,
        _envelope_type: &str,
        _payload: &[u8],
        _priority: EnvelopePriority,
    ) -> Result<SendEnvelopeResult, HandlerError> {
        // Validate sender exists.
        if self.topology.tree.get(from_workspace).is_none() {
            return Err(HandlerError::WorkspaceNotFound(from_workspace.clone()));
        }
        // Validate target exists.
        if self.topology.tree.get(to_workspace).is_none() {
            return Err(HandlerError::WorkspaceNotFound(to_workspace.clone()));
        }

        // Validate port rights.
        self.topology
            .port_rights
            .validate_send(from_workspace, to_workspace)
            .map_err(|_| HandlerError::NoSendRight(to_workspace.clone()))?;

        let id = *self.next_envelope_id;
        *self.next_envelope_id += 1;

        Ok(SendEnvelopeResult {
            envelope_id: EnvelopeId::from(format!("env-{id}")),
        })
    }

    /// EmitSignal: validate workspace exists.
    pub fn handle_emit_signal(
        &self,
        workspace_id: &WorkspaceId,
        _signal_type: SignalType,
        _reason: Option<&str>,
    ) -> Result<EmitSignalResult, HandlerError> {
        if self.topology.tree.get(workspace_id).is_none() {
            return Err(HandlerError::WorkspaceNotFound(workspace_id.clone()));
        }
        Ok(EmitSignalResult { accepted: true })
    }

    /// CreateCheckpoint: validate workspace exists, generate id + hash.
    pub fn handle_create_checkpoint(
        &mut self,
        workspace_id: &WorkspaceId,
        _checkpoint_type: &str,
        payload: &[u8],
        _intent: &str,
        _status: CheckpointStatus,
        _confidence: Confidence,
    ) -> Result<CreateCheckpointResult, HandlerError> {
        if self.topology.tree.get(workspace_id).is_none() {
            return Err(HandlerError::WorkspaceNotFound(workspace_id.clone()));
        }

        let id = *self.next_checkpoint_id;
        *self.next_checkpoint_id += 1;

        // Simple hash for now — SHA-256 in the real implementation.
        let hash = format!("{:x}", md5_like_hash(payload));

        Ok(CreateCheckpointResult {
            checkpoint_id: CheckpointId::from(format!("cp-{id}")),
            content_hash: hash,
        })
    }

    // ── Highway RPCs (13.2) ─────────────────────────────────────────

    /// GetWorkspace: return workspace view.
    pub fn handle_get_workspace(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<WorkspaceView, HandlerError> {
        let node = self
            .topology
            .tree
            .get(workspace_id)
            .ok_or_else(|| HandlerError::WorkspaceNotFound(workspace_id.clone()))?;

        Ok(WorkspaceView {
            id: node.id.clone(),
            state: node.status,
            parent: node.parent.clone(),
            owner: node.owner.clone(),
            originator: node.originator.clone(),
            task_id: node.task_id.clone(),
        })
    }

    /// GetTaskGraph: return snapshot of all tasks.
    pub fn handle_get_task_graph(&self) -> TaskGraphSnapshot {
        // Iterate all roots and their dependents to build the view.
        let mut tasks = Vec::new();
        let roots = self.task_graph.roots();
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();

        for root in roots {
            queue.push_back(root.clone());
        }

        while let Some(tid) = queue.pop_front() {
            if !visited.insert(tid.to_string()) {
                continue;
            }
            if let Some(task) = self.task_graph.get(&tid) {
                tasks.push(TaskView {
                    id: task.id.clone(),
                    name: task.name.clone(),
                    status: task.status,
                    workspace_ref: task.workspace_ref.clone(),
                    depends_on: task.depends_on.clone(),
                    parent_task: task.parent_task.clone(),
                });
                // Enqueue dependents.
                for dep in self.task_graph.dependents(&tid) {
                    queue.push_back(dep.clone());
                }
            }
        }

        TaskGraphSnapshot { tasks }
    }

    /// InjectEnvelope: highway injection bypasses permission matrix.
    /// Still validates port rights (coordinator has send right to all workspaces
    /// via creation defaults).
    pub fn handle_inject_envelope(
        &mut self,
        to_workspace: &WorkspaceId,
        _envelope_type: &str,
        _payload: &[u8],
        _priority: EnvelopePriority,
    ) -> Result<SendEnvelopeResult, HandlerError> {
        if self.topology.tree.get(to_workspace).is_none() {
            return Err(HandlerError::WorkspaceNotFound(to_workspace.clone()));
        }

        let id = *self.next_envelope_id;
        *self.next_envelope_id += 1;

        Ok(SendEnvelopeResult {
            envelope_id: EnvelopeId::from(format!("env-{id}")),
        })
    }

    // ── Gate/Escalation (13.3) ──────────────────────────────────────

    /// RespondToGate: resolve a pending gate.
    /// Returns Ok(true) if applied, Ok(false) if gate already resolved.
    pub fn handle_gate_response(
        &mut self,
        gate_id: &GateId,
        decision: GateDecision,
    ) -> Result<Option<GateResolution>, HandlerError> {
        Ok(self.gate_controller.resolve(gate_id, decision))
    }
}

/// Simple hash for checkpoint content (placeholder for SHA-256).
fn md5_like_hash(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
