use std::collections::HashMap;

use tokio::sync::mpsc;
use wacp_types::*;
use wacp_workspace::{
    CoordinatorCommand, WorkspaceActor, WorkspaceConfig, WorkspaceEvent, WorkspaceHandle,
};

use crate::task_graph::TaskGraph;
use crate::tree::{WorkspaceNode, WorkspaceTree};

/// Request to dispatch a task to a new workspace.
#[derive(Debug)]
pub struct DispatchRequest {
    pub task_id: TaskId,
    pub config: WorkspaceConfig,
}

/// The coordinator — owns tree, task graph, workspace handles.
pub struct Coordinator {
    pub tree: WorkspaceTree,
    pub task_graph: TaskGraph,
    workspace_handles: HashMap<String, WorkspaceHandle>,
    event_tx: mpsc::Sender<WorkspaceEvent>,
}

impl Coordinator {
    /// Create a new coordinator with a root workspace.
    pub fn new(root_id: WorkspaceId, owner: UserId, event_tx: mpsc::Sender<WorkspaceEvent>) -> Self {
        Self {
            tree: WorkspaceTree::new(root_id, owner),
            task_graph: TaskGraph::new(),
            workspace_handles: HashMap::new(),
            event_tx,
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
    pub fn handle_event(&mut self, event: &WorkspaceEvent) {
        match event {
            WorkspaceEvent::StateChanged {
                workspace_id, to, ..
            } => {
                self.tree.update_status(workspace_id, *to);
                if *to == WorkspaceState::Failed {
                    let _reparented = self.tree.cascade_failure(workspace_id);
                }
            }
            WorkspaceEvent::Terminated(archived) => {
                self.tree
                    .update_status(&archived.id, archived.terminal_state);
                self.workspace_handles.remove(archived.id.as_ref());
            }
            _ => {}
        }
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
}
