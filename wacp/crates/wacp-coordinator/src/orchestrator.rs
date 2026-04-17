use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use wacp_types::*;
use wacp_workspace::{
    CoordinatorCommand, WorkspaceActor, WorkspaceConfig, WorkspaceEvent, WorkspaceHandle,
};

use crate::integration::{IntegrationEngine, IntegrationRequest, IntegrationResult};
use crate::migration::{MigrationCoordinator, MigrationRequest};
use crate::task_graph::TaskGraph;
use crate::tree::{WorkspaceNode, WorkspaceTree};

/// Request to dispatch a task to a new workspace.
#[derive(Debug)]
pub struct DispatchRequest {
    pub task_id: TaskId,
    pub config: WorkspaceConfig,
}

/// Serializable system snapshot — coordinator state at a point in time.
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub sequence: u64,
    pub tree: serde_json::Value,
    pub task_graph: serde_json::Value,
}

/// The coordinator — owns tree, task graph, workspace handles, migration state.
pub struct Coordinator {
    pub tree: WorkspaceTree,
    pub task_graph: TaskGraph,
    pub migration: MigrationCoordinator,
    workspace_handles: HashMap<String, WorkspaceHandle>,
    event_tx: mpsc::Sender<WorkspaceEvent>,
    /// WA3.6: per-workspace last-checkpoint cache, populated by
    /// `WorkspaceEvent::CheckpointCreated` and consumed by the
    /// auto-integration trigger when a workspace transitions to Integrating.
    pub(crate) last_checkpoint: HashMap<String, Checkpoint>,
}

impl Coordinator {
    /// Create a new coordinator with a root workspace.
    pub fn new(
        root_id: WorkspaceId,
        owner: UserId,
        event_tx: mpsc::Sender<WorkspaceEvent>,
    ) -> Self {
        Self {
            tree: WorkspaceTree::new(root_id, owner),
            task_graph: TaskGraph::new(),
            migration: MigrationCoordinator::new(60_000),
            workspace_handles: HashMap::new(),
            event_tx,
            last_checkpoint: HashMap::new(),
        }
    }

    /// Dispatch: create a workspace for a task, insert in tree, spawn actor.
    pub fn dispatch(&mut self, request: DispatchRequest) {
        let ws_id = request.config.id.clone();
        let parent = request.config.parent.clone();
        let owner = request.config.owner.clone();

        // Insert in tree.
        let originator = request.config.originator.clone();
        let node = WorkspaceNode {
            id: ws_id.clone(),
            parent: Some(parent),
            children: Vec::new(),
            owner,
            originator,
            status: WorkspaceState::Idle,
            task_id: Some(request.task_id),
        };
        // Ignore tree error for simplicity — caller should validate.
        let _ = self.tree.insert(node);

        // Spawn workspace actor.
        let handle = WorkspaceActor::spawn(request.config, self.event_tx.clone());
        self.workspace_handles.insert(ws_id.to_string(), handle);
    }

    /// Process a workspace event.
    ///
    /// WA3.6: when a workspace transitions to `Integrating`, this method
    /// also drives `IntegrationEngine::integrate` against the workspace's
    /// last cached checkpoint and forwards the result back to the workspace
    /// actor via `CoordinatorCommand::IntegrationSucceeded` /
    /// `IntegrationFailed` / `ConflictDetected`. Required to be `async` so
    /// the coordinator-tx send can complete before the next event is
    /// processed.
    pub async fn handle_event(&mut self, event: &WorkspaceEvent) {
        match event {
            WorkspaceEvent::StateChanged {
                workspace_id, to, ..
            } => {
                self.tree.update_status(workspace_id, *to);
                if *to == WorkspaceState::Failed {
                    let _reparented = self.tree.cascade_failure(workspace_id);
                }
                if *to == WorkspaceState::Integrating {
                    self.auto_integrate(workspace_id).await;
                }
            }
            WorkspaceEvent::CheckpointCreated(cp) => {
                self.last_checkpoint
                    .insert(cp.workspace_id.to_string(), cp.clone());
            }
            WorkspaceEvent::Terminated(archived) => {
                self.tree
                    .update_status(&archived.id, archived.terminal_state);
                self.workspace_handles.remove(archived.id.as_ref());
                self.last_checkpoint.remove(archived.id.as_ref());
            }
            WorkspaceEvent::MigrationSnapshot {
                workspace_id,
                snapshot,
            } => {
                let _ = self
                    .migration
                    .set_snapshot(workspace_id.as_ref(), snapshot.clone());
            }
            _ => {}
        }
    }

    /// WA3.6: drive integration once a workspace has reached Integrating.
    ///
    /// If the workspace has a cached checkpoint, build an IntegrationRequest
    /// (default Normal mode + Direct strategy — the engine is currently
    /// trivially-Success but the call lets the future implementation thread
    /// real conflict detection here without further changes). When no
    /// checkpoint was ever created, fall back to `IntegrationSucceeded` —
    /// matches the blind behaviour of the existing `TriggerIntegration`
    /// gRPC handler so an agent that emits Complete without checkpoints
    /// still terminates cleanly.
    async fn auto_integrate(&mut self, workspace_id: &WorkspaceId) {
        let handle = match self.workspace_handles.get(workspace_id.as_ref()) {
            Some(h) => h,
            None => return,
        };
        let cmd = match self.last_checkpoint.get(workspace_id.as_ref()) {
            Some(checkpoint) => {
                let request = IntegrationRequest {
                    workspace_id: workspace_id.clone(),
                    strategy: MergeStrategy::Direct,
                    mode: IntegrationMode::Normal,
                    checkpoint: checkpoint.clone(),
                };
                match IntegrationEngine.integrate(&request) {
                    IntegrationResult::Success => CoordinatorCommand::IntegrationSucceeded,
                    IntegrationResult::Conflict(_) => CoordinatorCommand::ConflictDetected,
                    IntegrationResult::Failed(_) => CoordinatorCommand::IntegrationFailed,
                }
            }
            None => CoordinatorCommand::IntegrationSucceeded,
        };
        let _ = handle.coordinator_tx.send(cmd).await;
    }

    /// Route an envelope to a target workspace.
    pub async fn route_envelope(&self, to: &WorkspaceId, envelope: Envelope) -> bool {
        if let Some(handle) = self.workspace_handles.get(to.as_ref()) {
            handle
                .coordinator_tx
                .send(CoordinatorCommand::DeliverEnvelope(envelope))
                .await
                .is_ok()
        } else {
            false
        }
    }

    /// Send an abort command to a workspace.
    pub async fn abort_workspace(&self, id: &WorkspaceId) -> bool {
        if let Some(handle) = self.workspace_handles.get(id.as_ref()) {
            handle
                .coordinator_tx
                .send(CoordinatorCommand::Abort)
                .await
                .is_ok()
        } else {
            false
        }
    }

    /// Get a workspace handle.
    pub fn handle(&self, id: &WorkspaceId) -> Option<&WorkspaceHandle> {
        self.workspace_handles.get(id.as_ref())
    }

    /// Start a migration for a workspace. Validates preconditions and sends
    /// MigrateBegin to the workspace actor.
    pub async fn start_migration(
        &mut self,
        request: MigrationRequest,
        old_agent: String,
        now_ms: u64,
    ) -> Result<(), crate::migration::MigrationError> {
        let ws_id = request.workspace_id.clone();

        // Read current state from tree for precondition check
        let current_state = self.tree.get(&ws_id).map(|n| n.status).ok_or_else(|| {
            crate::migration::MigrationError::WorkspaceNotFound(ws_id.to_string())
        })?;

        self.migration
            .start(request, current_state, old_agent, now_ms)?;

        // Send MigrateBegin to workspace actor
        if let Some(handle) = self.workspace_handles.get(ws_id.as_ref()) {
            let _ = handle
                .coordinator_tx
                .send(CoordinatorCommand::MigrateBegin)
                .await;
        }

        Ok(())
    }

    /// Complete a migration after the new agent has bound.
    pub async fn complete_migration(
        &mut self,
        workspace_id: &WorkspaceId,
    ) -> Option<crate::migration::MigrationContext> {
        let ctx = self.migration.get(workspace_id.as_ref())?;
        let restore_blocked = ctx.pre_migration_state == WorkspaceState::Blocked;

        if let Some(handle) = self.workspace_handles.get(workspace_id.as_ref()) {
            let _ = handle
                .coordinator_tx
                .send(CoordinatorCommand::MigrationComplete { restore_blocked })
                .await;
        }

        self.migration.complete(workspace_id.as_ref()).ok()
    }

    /// Fail a migration. Sends MigrationFailed to workspace actor.
    pub async fn fail_migration(
        &mut self,
        workspace_id: &WorkspaceId,
        error: String,
        step: u32,
    ) -> Option<crate::migration::MigrationContext> {
        let ctx = self
            .migration
            .fail(workspace_id.as_ref(), error, step)
            .ok()?;

        if let Some(handle) = self.workspace_handles.get(workspace_id.as_ref()) {
            let _ = handle
                .coordinator_tx
                .send(CoordinatorCommand::MigrationFailed)
                .await;
        }

        Some(ctx)
    }

    /// Capture the coordinator's current state as a serializable snapshot.
    pub fn capture_snapshot(&self, sequence: u64) -> SystemSnapshot {
        SystemSnapshot {
            sequence,
            tree: serde_json::to_value(&self.tree).expect("tree serializes"),
            task_graph: serde_json::to_value(&self.task_graph).expect("task_graph serializes"),
        }
    }
}
