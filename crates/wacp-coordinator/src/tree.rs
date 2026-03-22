use std::collections::HashMap;

use wacp_types::{TaskId, UserId, WorkspaceId, WorkspaceState};

/// Error from tree operations.
#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    #[error("parent not found: {0}")]
    ParentNotFound(String),
    #[error("node already exists: {0}")]
    DuplicateNode(String),
    #[error("node not found: {0}")]
    NodeNotFound(String),
}

/// A node in the workspace tree.
#[derive(Debug, Clone)]
pub struct WorkspaceNode {
    pub id: WorkspaceId,
    pub parent: Option<WorkspaceId>,
    pub children: Vec<WorkspaceId>,
    pub owner: UserId,
    pub status: WorkspaceState,
    pub task_id: Option<TaskId>,
}

/// The workspace tree (PROTOCOL.md §6.5).
pub struct WorkspaceTree {
    nodes: HashMap<String, WorkspaceNode>,
    root: WorkspaceId,
}

impl WorkspaceTree {
    /// Create a tree with a root node.
    pub fn new(root_id: WorkspaceId, owner: UserId) -> Self {
        let node = WorkspaceNode {
            id: root_id.clone(),
            parent: None,
            children: Vec::new(),
            owner,
            status: WorkspaceState::Active,
            task_id: None,
        };
        let mut nodes = HashMap::new();
        nodes.insert(root_id.to_string(), node);
        Self {
            nodes,
            root: root_id,
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
        self.nodes.insert(id_str, node);
        self.nodes
            .get_mut(&parent_str)
            .unwrap()
            .children
            .push(child_id);
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

    /// All descendants (recursive BFS).
    pub fn descendants(&self, id: &WorkspaceId) -> Vec<WorkspaceId> {
        let mut result = Vec::new();
        let mut queue = vec![id.clone()];
        while let Some(current) = queue.pop() {
            if let Some(node) = self.nodes.get(current.as_ref()) {
                for child in &node.children {
                    result.push(child.clone());
                    queue.push(child.clone());
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
            let child_owner = self
                .nodes
                .get(child_id.as_ref())
                .map(|n| n.owner.clone());

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
                && let Some(parent_node) = self.nodes.get_mut(old_parent.as_ref()) {
                    parent_node.children.retain(|c| c != child);
                }

        // Update child's parent.
        if let Some(node) = self.nodes.get_mut(child.as_ref()) {
            node.parent = Some(new_parent.clone());
        }

        // Add to new parent's children.
        if let Some(parent_node) = self.nodes.get_mut(new_parent.as_ref())
            && !parent_node.children.contains(child) {
                parent_node.children.push(child.clone());
            }
    }

    /// Update a node's status.
    pub fn update_status(&mut self, id: &WorkspaceId, status: WorkspaceState) {
        if let Some(n) = self.nodes.get_mut(id.as_ref()) {
            n.status = status;
        }
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
