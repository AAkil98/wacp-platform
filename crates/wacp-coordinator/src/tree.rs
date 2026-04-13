use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wacp_types::{Originator, TaskId, UserId, WorkspaceId, WorkspaceState};

/// Error from tree operations.
#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    #[error("parent not found: {0}")]
    ParentNotFound(String),
    #[error("node already exists: {0}")]
    DuplicateNode(String),
    #[error("node not found: {0}")]
    NodeNotFound(String),
    #[error("transfer_owner called with current owner")]
    SameOwner,
}

/// A node in the workspace tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceNode {
    pub id: WorkspaceId,
    pub parent: Option<WorkspaceId>,
    pub children: Vec<WorkspaceId>,
    pub owner: UserId,
    pub originator: Originator,
    pub status: WorkspaceState,
    pub task_id: Option<TaskId>,
}

/// The workspace tree (PROTOCOL.md §6.5, topology.md §2).
///
/// Flat table with three indices for O(1) lookup + O(k) traversal.
#[derive(Serialize, Deserialize)]
pub struct WorkspaceTree {
    nodes: HashMap<String, WorkspaceNode>,
    root: WorkspaceId,
    originator_index: HashMap<Originator, Vec<WorkspaceId>>,
    owner_index: HashMap<UserId, Vec<WorkspaceId>>,
}

impl WorkspaceTree {
    /// Create a tree with a root node.
    pub fn new(root_id: WorkspaceId, owner: UserId) -> Self {
        let node = WorkspaceNode {
            id: root_id.clone(),
            parent: None,
            children: Vec::new(),
            owner: owner.clone(),
            originator: Originator::System,
            status: WorkspaceState::Active,
            task_id: None,
        };

        let mut originator_index = HashMap::new();
        originator_index
            .entry(Originator::System)
            .or_insert_with(Vec::new)
            .push(root_id.clone());

        let mut owner_index = HashMap::new();
        owner_index
            .entry(owner)
            .or_insert_with(Vec::new)
            .push(root_id.clone());

        let mut nodes = HashMap::new();
        nodes.insert(root_id.to_string(), node);

        Self {
            nodes,
            root: root_id,
            originator_index,
            owner_index,
        }
    }

    pub fn root(&self) -> &WorkspaceId {
        &self.root
    }

    /// Insert a child node. Parent must exist.
    pub fn insert(&mut self, node: WorkspaceNode) -> Result<(), TreeError> {
        let id_str = node.id.to_string();
        if self.nodes.contains_key(&id_str) {
            return Err(TreeError::DuplicateNode(id_str));
        }

        let parent_id = node
            .parent
            .as_ref()
            .ok_or_else(|| TreeError::ParentNotFound("None".into()))?;

        let parent_str = parent_id.to_string();
        if !self.nodes.contains_key(&parent_str) {
            return Err(TreeError::ParentNotFound(parent_str));
        }

        let child_id = node.id.clone();
        let originator = node.originator.clone();
        let owner = node.owner.clone();

        self.nodes.insert(id_str, node);
        self.nodes
            .get_mut(&parent_str)
            .unwrap()
            .children
            .push(child_id.clone());

        // Maintain indices.
        self.originator_index
            .entry(originator)
            .or_default()
            .push(child_id.clone());
        self.owner_index.entry(owner).or_default().push(child_id);

        Ok(())
    }

    pub fn get(&self, id: &WorkspaceId) -> Option<&WorkspaceNode> {
        self.nodes.get(id.as_ref())
    }

    pub fn get_mut(&mut self, id: &WorkspaceId) -> Option<&mut WorkspaceNode> {
        self.nodes.get_mut(id.as_ref())
    }

    /// Direct children of a node.
    pub fn children(&self, id: &WorkspaceId) -> Vec<&WorkspaceId> {
        self.nodes
            .get(id.as_ref())
            .map(|n| n.children.iter().collect())
            .unwrap_or_default()
    }

    /// Siblings — children of the same parent, excluding self.
    pub fn siblings(&self, id: &WorkspaceId) -> Vec<WorkspaceId> {
        let node = match self.nodes.get(id.as_ref()) {
            Some(n) => n,
            None => return vec![],
        };
        let parent = match &node.parent {
            Some(p) => p,
            None => return vec![], // root has no siblings
        };
        self.nodes
            .get(parent.as_ref())
            .map(|p| p.children.iter().filter(|c| *c != id).cloned().collect())
            .unwrap_or_default()
    }

    /// All descendants (recursive BFS).
    pub fn descendants(&self, id: &WorkspaceId) -> Vec<WorkspaceId> {
        let mut result = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(id.clone());
        while let Some(current) = queue.pop_front() {
            if let Some(node) = self.nodes.get(current.as_ref()) {
                for child in &node.children {
                    result.push(child.clone());
                    queue.push_back(child.clone());
                }
            }
        }
        result
    }

    /// Path from node to root (excluding the node itself).
    pub fn parent_chain(&self, id: &WorkspaceId) -> Vec<WorkspaceId> {
        let mut chain = Vec::new();
        let mut current = id.clone();
        while let Some(node) = self.nodes.get(current.as_ref()) {
            if let Some(ref parent) = node.parent {
                chain.push(parent.clone());
                current = parent.clone();
            } else {
                break;
            }
        }
        chain
    }

    /// All workspaces with the given originator.
    pub fn by_originator(&self, originator: &Originator) -> &[WorkspaceId] {
        self.originator_index
            .get(originator)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// All workspaces owned by the given user.
    pub fn by_owner(&self, owner: &UserId) -> &[WorkspaceId] {
        self.owner_index
            .get(owner)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Descendants of `id` that have the given originator.
    /// Intersection of descendants(id) and by_originator(originator).
    pub fn causal_descendants(
        &self,
        id: &WorkspaceId,
        originator: &Originator,
    ) -> Vec<WorkspaceId> {
        let descendants = self.descendants(id);
        let originator_set = self.by_originator(originator);
        descendants
            .into_iter()
            .filter(|d| originator_set.contains(d))
            .collect()
    }

    /// Transfer ownership of a node. Updates owner_index.
    /// Returns the old owner. Originator is immutable — no transfer method.
    pub fn transfer_owner(
        &mut self,
        id: &WorkspaceId,
        new_owner: UserId,
    ) -> Result<UserId, TreeError> {
        let node = self
            .nodes
            .get_mut(id.as_ref())
            .ok_or_else(|| TreeError::NodeNotFound(id.to_string()))?;

        if node.owner == new_owner {
            return Err(TreeError::SameOwner);
        }

        let old_owner = std::mem::replace(&mut node.owner, new_owner.clone());

        // Remove from old owner's index.
        if let Some(entries) = self.owner_index.get_mut(&old_owner) {
            entries.retain(|w| w != id);
        }

        // Add to new owner's index.
        self.owner_index
            .entry(new_owner)
            .or_default()
            .push(id.clone());

        Ok(old_owner)
    }

    /// Cascade failure: mark node + children Failed within same owner boundary.
    /// Returns IDs of cross-owner children that were reparented to root.
    pub fn cascade_failure(&mut self, id: &WorkspaceId) -> Vec<WorkspaceId> {
        let owner = match self.nodes.get(id.as_ref()) {
            Some(n) => n.owner.clone(),
            None => return vec![],
        };

        // Mark the node itself as Failed.
        if let Some(n) = self.nodes.get_mut(id.as_ref()) {
            n.status = WorkspaceState::Failed;
        }

        let desc = self.descendants(id);
        let mut reparented = Vec::new();

        for child_id in desc {
            let child_owner = self.nodes.get(child_id.as_ref()).map(|n| n.owner.clone());

            if let Some(co) = child_owner {
                if co == owner {
                    // Same owner — cascade failure.
                    if let Some(n) = self.nodes.get_mut(child_id.as_ref()) {
                        n.status = WorkspaceState::Failed;
                    }
                } else {
                    // Different owner — reparent to root.
                    self.reparent(&child_id, &self.root.clone());
                    reparented.push(child_id);
                }
            }
        }

        reparented
    }

    /// Reparent a node to a new parent.
    pub fn reparent(&mut self, child: &WorkspaceId, new_parent: &WorkspaceId) {
        // Remove from old parent's children.
        if let Some(node) = self.nodes.get(child.as_ref())
            && let Some(ref old_parent) = node.parent.clone()
            && let Some(parent_node) = self.nodes.get_mut(old_parent.as_ref())
        {
            parent_node.children.retain(|c| c != child);
        }

        // Update child's parent.
        if let Some(node) = self.nodes.get_mut(child.as_ref()) {
            node.parent = Some(new_parent.clone());
        }

        // Add to new parent's children.
        if let Some(parent_node) = self.nodes.get_mut(new_parent.as_ref())
            && !parent_node.children.contains(child)
        {
            parent_node.children.push(child.clone());
        }
    }

    /// Update a node's status.
    pub fn update_status(&mut self, id: &WorkspaceId, status: WorkspaceState) {
        if let Some(n) = self.nodes.get_mut(id.as_ref()) {
            n.status = status;
        }
    }

    /// Active (non-terminal) workspaces with the given user as originator.
    /// Used for causal impact queries when a human's state changes.
    pub fn causal_impact(&self, user_id: &UserId) -> Vec<WorkspaceId> {
        let originator = Originator::User(user_id.clone());
        self.by_originator(&originator)
            .iter()
            .filter(|id| {
                self.nodes
                    .get(id.as_ref())
                    .is_some_and(|n| !n.status.is_terminal())
            })
            .cloned()
            .collect()
    }

    /// True if this node's originator differs from its parent's.
    /// Root returns false (no parent). Detects injection points.
    pub fn is_causal_boundary(&self, id: &WorkspaceId) -> bool {
        let node = match self.nodes.get(id.as_ref()) {
            Some(n) => n,
            None => return false,
        };
        let parent_id = match &node.parent {
            Some(p) => p,
            None => return false, // root
        };
        let parent = match self.nodes.get(parent_id.as_ref()) {
            Some(n) => n,
            None => return false,
        };
        node.originator != parent.originator
    }

    /// All non-terminal workspace IDs.
    pub fn active_workspaces(&self) -> Vec<&WorkspaceId> {
        self.nodes
            .values()
            .filter(|n| !n.status.is_terminal())
            .map(|n| &n.id)
            .collect()
    }
}
