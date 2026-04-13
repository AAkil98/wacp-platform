use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wacp_types::{PortRightType, WorkspaceId};

/// Status of a port right in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortRightStatus {
    Active,
    Consumed, // send-once used
    Revoked,  // coordinator revoked
    Expired,  // holder or target reached terminal state
}

/// A port right entry with id and lifecycle status (topology.md §7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRightEntry {
    pub id: String,
    pub holder: WorkspaceId,
    pub target: WorkspaceId,
    pub kind: PortRightType,
    pub status: PortRightStatus,
}

/// Error from port rights operations.
#[derive(Debug, thiserror::Error)]
pub enum PortRightError {
    #[error("port right not found")]
    NotFound,
    #[error("port right is not active")]
    NotActive,
    #[error("right is not send-once")]
    NotSendOnce,
    #[error("receive rights are not transferable")]
    ReceiveNotTransferable,
    #[error("no rights held by sender")]
    NoRights,
    #[error("no active send right to target: {0}")]
    NoRightToTarget(WorkspaceId),
}

/// Directed multigraph of port rights between workspaces (topology.md §7).
///
/// Three indices for O(1) lookup by holder, target, or right id.
/// Rights are created, transferred, consumed (send-once), revoked,
/// or expired (terminal workspace). The most mutable of the six topologies.
#[derive(Serialize, Deserialize)]
pub struct PortRightsGraph {
    by_holder: HashMap<String, Vec<String>>,
    by_target: HashMap<String, Vec<String>>,
    by_id: HashMap<String, PortRightEntry>,
    next_id: u64,
}

impl PortRightsGraph {
    pub fn new() -> Self {
        Self {
            by_holder: HashMap::new(),
            by_target: HashMap::new(),
            by_id: HashMap::new(),
            next_id: 1,
        }
    }

    /// Create a port right. Returns the assigned right id.
    pub fn create(
        &mut self,
        holder: WorkspaceId,
        target: WorkspaceId,
        kind: PortRightType,
    ) -> String {
        let id = format!("pr-{}", self.next_id);
        self.next_id += 1;

        let entry = PortRightEntry {
            id: id.clone(),
            holder: holder.clone(),
            target: target.clone(),
            kind,
            status: PortRightStatus::Active,
        };

        self.by_holder
            .entry(holder.to_string())
            .or_default()
            .push(id.clone());
        self.by_target
            .entry(target.to_string())
            .or_default()
            .push(id.clone());
        self.by_id.insert(id.clone(), entry);

        id
    }

    /// Transfer a right to a new holder. Returns the old holder.
    /// Receive rights are not transferable (CH-5).
    pub fn transfer(
        &mut self,
        right_id: &str,
        new_holder: &WorkspaceId,
    ) -> Result<WorkspaceId, PortRightError> {
        let entry = self.by_id.get(right_id).ok_or(PortRightError::NotFound)?;

        if entry.status != PortRightStatus::Active {
            return Err(PortRightError::NotActive);
        }
        if entry.kind == PortRightType::Receive {
            return Err(PortRightError::ReceiveNotTransferable);
        }

        let old_holder = entry.holder.clone();

        // Remove from old holder's index.
        if let Some(ids) = self.by_holder.get_mut(old_holder.as_ref()) {
            ids.retain(|id| id != right_id);
        }

        // Add to new holder's index.
        self.by_holder
            .entry(new_holder.to_string())
            .or_default()
            .push(right_id.to_string());

        // Update the entry.
        let entry = self.by_id.get_mut(right_id).unwrap();
        entry.holder = new_holder.clone();

        Ok(old_holder)
    }

    /// Consume a send-once right (CH-3).
    pub fn consume(&mut self, right_id: &str) -> Result<(), PortRightError> {
        let entry = self
            .by_id
            .get_mut(right_id)
            .ok_or(PortRightError::NotFound)?;

        if entry.kind != PortRightType::SendOnce {
            return Err(PortRightError::NotSendOnce);
        }
        if entry.status != PortRightStatus::Active {
            return Err(PortRightError::NotActive);
        }

        entry.status = PortRightStatus::Consumed;
        Ok(())
    }

    /// Revoke a right (coordinator action).
    pub fn revoke(&mut self, right_id: &str) -> Result<(), PortRightError> {
        let entry = self
            .by_id
            .get_mut(right_id)
            .ok_or(PortRightError::NotFound)?;

        if entry.status != PortRightStatus::Active {
            return Err(PortRightError::NotActive);
        }

        entry.status = PortRightStatus::Revoked;
        Ok(())
    }

    /// Expire all rights held by or targeting a workspace (terminal state).
    pub fn expire_workspace(&mut self, workspace_id: &WorkspaceId) {
        // Collect right ids held by this workspace.
        let held: Vec<String> = self
            .by_holder
            .get(workspace_id.as_ref())
            .cloned()
            .unwrap_or_default();

        for id in &held {
            if let Some(entry) = self.by_id.get_mut(id)
                && entry.status == PortRightStatus::Active
            {
                entry.status = PortRightStatus::Expired;
            }
        }

        // Collect right ids targeting this workspace.
        let targeted: Vec<String> = self
            .by_target
            .get(workspace_id.as_ref())
            .cloned()
            .unwrap_or_default();

        for id in &targeted {
            if let Some(entry) = self.by_id.get_mut(id)
                && entry.status == PortRightStatus::Active
            {
                entry.status = PortRightStatus::Expired;
            }
        }
    }

    /// Validate that sender has an active send or send-once right to target.
    /// Returns the right id if found (CH-1: no right, no delivery).
    pub fn validate_send(
        &self,
        sender: &WorkspaceId,
        target: &WorkspaceId,
    ) -> Result<String, PortRightError> {
        let right_ids = self
            .by_holder
            .get(sender.as_ref())
            .ok_or(PortRightError::NoRights)?;

        for id in right_ids {
            if let Some(entry) = self.by_id.get(id)
                && &entry.target == target
                && entry.status == PortRightStatus::Active
                && matches!(entry.kind, PortRightType::Send | PortRightType::SendOnce)
            {
                return Ok(id.clone());
            }
        }

        Err(PortRightError::NoRightToTarget(target.clone()))
    }

    /// Look up a right by id.
    pub fn get(&self, right_id: &str) -> Option<&PortRightEntry> {
        self.by_id.get(right_id)
    }

    /// All active rights held by a workspace.
    pub fn rights_held_by(&self, holder: &WorkspaceId) -> Vec<&PortRightEntry> {
        self.by_holder
            .get(holder.as_ref())
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| {
                        let entry = self.by_id.get(id)?;
                        (entry.status == PortRightStatus::Active).then_some(entry)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Count of active rights in the graph.
    pub fn active_count(&self) -> usize {
        self.by_id
            .values()
            .filter(|e| e.status == PortRightStatus::Active)
            .count()
    }
}

impl Default for PortRightsGraph {
    fn default() -> Self {
        Self::new()
    }
}
