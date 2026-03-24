use std::collections::{HashMap, HashSet};

use wacp_types::WorkspaceId;

/// Error from visibility operations.
#[derive(Debug, thiserror::Error)]
pub enum VisibilityError {
    #[error("grantor cannot see target: {0}")]
    GrantorCannotSee(WorkspaceId),
}

/// Directed visibility graph (topology.md §4).
///
/// `A → B` means workspace A can read workspace B's state.
/// Self-visibility is implicit — every workspace can see itself
/// without being stored in the graph (invariant VI-1).
/// Grants are additive only — no revoke method (invariant VI-2).
pub struct VisibilityGraph {
    /// Forward: workspace → set of workspaces it can see
    can_see: HashMap<String, HashSet<WorkspaceId>>,
    /// Reverse: workspace → set of workspaces that can see it
    seen_by: HashMap<String, HashSet<WorkspaceId>>,
}

impl VisibilityGraph {
    /// Create an empty visibility graph.
    pub fn new() -> Self {
        Self {
            can_see: HashMap::new(),
            seen_by: HashMap::new(),
        }
    }

    /// Register a workspace in the graph with an empty visibility set.
    /// Idempotent — registering an already-registered workspace is a no-op.
    pub fn register(&mut self, id: &WorkspaceId) {
        self.can_see.entry(id.to_string()).or_default();
        self.seen_by.entry(id.to_string()).or_default();
    }

    /// Grant visibility: `viewer` can see `target`.
    /// Returns `true` if newly granted, `false` if already visible.
    /// No grantor check — used by the coordinator (total visibility).
    pub fn grant(&mut self, viewer: &WorkspaceId, target: &WorkspaceId) -> bool {
        // Self-visibility is implicit — don't store it.
        if viewer == target {
            return false;
        }

        let newly_inserted = self
            .can_see
            .entry(viewer.to_string())
            .or_default()
            .insert(target.clone());

        if newly_inserted {
            self.seen_by
                .entry(target.to_string())
                .or_default()
                .insert(viewer.clone());
        }

        newly_inserted
    }

    /// Grant with grantor scope validation.
    /// The grantor must be able to see `target` for the grant to succeed.
    /// Returns `Ok(true)` if newly granted, `Ok(false)` if already visible.
    pub fn grant_checked(
        &mut self,
        viewer: &WorkspaceId,
        target: &WorkspaceId,
        grantor: &WorkspaceId,
    ) -> Result<bool, VisibilityError> {
        if !self.can_see(grantor, target) {
            return Err(VisibilityError::GrantorCannotSee(target.clone()));
        }
        Ok(self.grant(viewer, target))
    }

    /// Can `viewer` see `target`?
    /// Self-visibility is implicit (VI-1): returns true when viewer == target.
    pub fn can_see(&self, viewer: &WorkspaceId, target: &WorkspaceId) -> bool {
        if viewer == target {
            return true;
        }
        self.can_see
            .get(viewer.as_ref())
            .is_some_and(|set| set.contains(target))
    }

    /// Full visibility set for a workspace, including implicit self.
    pub fn visible_to(&self, viewer: &WorkspaceId) -> HashSet<WorkspaceId> {
        let mut result = self
            .can_see
            .get(viewer.as_ref())
            .cloned()
            .unwrap_or_default();
        result.insert(viewer.clone());
        result
    }

    /// Reverse query: all workspaces that can see `target`, including implicit self.
    pub fn who_can_see(&self, target: &WorkspaceId) -> HashSet<WorkspaceId> {
        let mut result = self
            .seen_by
            .get(target.as_ref())
            .cloned()
            .unwrap_or_default();
        result.insert(target.clone());
        result
    }

    /// Total number of directed edges (excluding implicit self-edges).
    pub fn grant_count(&self) -> usize {
        self.can_see.values().map(|s| s.len()).sum()
    }
}

impl Default for VisibilityGraph {
    fn default() -> Self {
        Self::new()
    }
}
