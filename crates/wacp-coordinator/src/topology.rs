use serde::{Deserialize, Serialize};
use wacp_types::{Originator, PortRightType, TaskId, UserId, WorkspaceId, WorkspaceState};

use crate::ownership::EscalationRouter;
use crate::port_rights::PortRightsGraph;
use crate::tree::{TreeError, WorkspaceNode, WorkspaceTree};
use crate::visibility::VisibilityGraph;

/// Parameters for compound workspace creation.
#[derive(Debug, Clone)]
pub struct CreateWorkspaceParams {
    pub id: WorkspaceId,
    pub parent: WorkspaceId,
    pub owner: UserId,
    pub originator: Originator,
    pub status: WorkspaceState,
    pub task_id: Option<TaskId>,
}

/// Result of a failure cascade across topologies.
#[derive(Debug, Default)]
pub struct CascadeEffect {
    pub failed: Vec<WorkspaceId>,
    pub reparented: Vec<WorkspaceId>,
    pub rights_expired: usize,
}

/// Bundles the four topology structures and provides compound operations
/// that span all of them (topology.md §8).
///
/// `TaskGraph` is separate — managed by the coordinator's scheduling logic.
#[derive(Serialize, Deserialize)]
pub struct TopologySet {
    pub tree: WorkspaceTree,
    pub visibility: VisibilityGraph,
    pub escalation: EscalationRouter,
    pub port_rights: PortRightsGraph,
}

impl TopologySet {
    /// Initialize all structures with a root workspace.
    pub fn new(root_id: WorkspaceId, root_owner: UserId) -> Self {
        let tree = WorkspaceTree::new(root_id.clone(), root_owner.clone());

        let mut visibility = VisibilityGraph::new();
        visibility.register(&root_id);

        let mut escalation = EscalationRouter::new();
        escalation.register(&root_id, &root_owner);

        let port_rights = PortRightsGraph::new();

        Self {
            tree,
            visibility,
            escalation,
            port_rights,
        }
    }

    /// Compound workspace creation — updates all 4 structures.
    ///
    /// 1. Tree: insert node (+ originator_index, owner_index)
    /// 2. Visibility: register + coordinator grant
    /// 3. Escalation: register routing
    /// 4. Port rights: bidirectional send with parent + self receive
    pub fn create_workspace(&mut self, params: CreateWorkspaceParams) -> Result<(), TreeError> {
        let node = WorkspaceNode {
            id: params.id.clone(),
            parent: Some(params.parent.clone()),
            children: Vec::new(),
            owner: params.owner.clone(),
            originator: params.originator,
            status: params.status,
            task_id: params.task_id,
        };

        // 1. Tree
        self.tree.insert(node)?;

        // 2. Visibility: register + coordinator sees new workspace
        self.visibility.register(&params.id);
        let root = self.tree.root().clone();
        self.visibility.grant(&root, &params.id);

        // 3. Escalation
        self.escalation.register(&params.id, &params.owner);

        // 4. Port rights: parent → child send, child → parent send, child self-receive
        self.port_rights
            .create(params.parent.clone(), params.id.clone(), PortRightType::Send);
        self.port_rights
            .create(params.id.clone(), params.parent, PortRightType::Send);
        self.port_rights
            .create(params.id.clone(), params.id, PortRightType::Receive);

        Ok(())
    }

    /// Compound workspace termination — updates tree, port rights, cascade.
    pub fn terminate_workspace(
        &mut self,
        id: &WorkspaceId,
        status: WorkspaceState,
    ) -> CascadeEffect {
        let mut effect = CascadeEffect::default();

        // 1. Tree: update status
        self.tree.update_status(id, status);

        // 2. Port rights: expire
        let before = self.port_rights.active_count();
        self.port_rights.expire_workspace(id);
        effect.rights_expired = before - self.port_rights.active_count();

        // 3. If failed: cascade
        if status == WorkspaceState::Failed {
            let reparented = self.tree.cascade_failure(id);

            // Collect same-owner children that were failed by cascade.
            // cascade_failure already marked them Failed in the tree.
            // We need to expire their port rights too.
            let descendants = self.tree.descendants(id);
            for desc_id in &descendants {
                if let Some(node) = self.tree.get(desc_id)
                    && node.status == WorkspaceState::Failed
                {
                    let before = self.port_rights.active_count();
                    self.port_rights.expire_workspace(desc_id);
                    effect.rights_expired += before - self.port_rights.active_count();
                    effect.failed.push(desc_id.clone());
                }
            }

            effect.reparented = reparented;
        }

        effect
    }

    /// Compound ownership transfer — updates tree + escalation.
    pub fn transfer_ownership(
        &mut self,
        id: &WorkspaceId,
        new_owner: UserId,
    ) -> Result<UserId, TreeError> {
        let old_owner = self.tree.transfer_owner(id, new_owner.clone())?;
        self.escalation.update(id, &new_owner);
        Ok(old_owner)
    }
}
