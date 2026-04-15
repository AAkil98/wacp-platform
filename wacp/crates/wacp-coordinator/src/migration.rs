//! Agent migration coordinator.
//!
//! Tracks active migrations, validates preconditions, manages timeouts,
//! and provides lifecycle methods (start/complete/fail). Pure state —
//! no async, no IO. The orchestrator integrates this into its event loop.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wacp_types::{WorkspaceId, WorkspaceState};
use wacp_workspace::MigrationSnapshot;

// ── Types ──

/// Identifies a replacement agent. Opaque to the runtime.
#[derive(Debug, Clone)]
pub struct AgentRef {
    pub agent_type: String,
    pub config: Option<Vec<u8>>,
}

/// Coordinator-internal migration request.
#[derive(Debug)]
pub struct MigrationRequest {
    pub workspace_id: WorkspaceId,
    pub new_agent: AgentRef,
    pub reason: String,
}

/// Tracks a single in-progress migration.
#[derive(Debug)]
pub struct MigrationContext {
    pub workspace_id: WorkspaceId,
    pub pre_migration_state: WorkspaceState,
    pub old_agent: String,
    pub new_agent: AgentRef,
    pub reason: String,
    pub snapshot: Option<MigrationSnapshot>,
    pub started_at_ms: u64,
}

/// Migration errors.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(String),

    #[error("invalid state for migration: {0:?} (must be Active or Blocked)")]
    InvalidState(WorkspaceState),

    #[error("workspace already migrating: {0}")]
    AlreadyMigrating(String),

    #[error("workspace not migrating: {0}")]
    NotMigrating(String),

    #[error("snapshot already set for: {0}")]
    SnapshotAlreadySet(String),
}

// ── Trail event types ──

/// Trail event: migration started (written at step 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStartedEvent {
    pub workspace_id: WorkspaceId,
    pub old_agent: String,
    pub new_agent: String,
    pub reason: String,
    pub pre_migration_state: String,
}

/// Trail event: migration completed (written at step 7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationCompletedEvent {
    pub workspace_id: WorkspaceId,
    pub old_agent: String,
    pub new_agent: String,
    pub duration_ms: u64,
    pub restored_state: String,
}

/// Trail event: migration failed (written on failure after step 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationFailedEvent {
    pub workspace_id: WorkspaceId,
    pub old_agent: String,
    pub new_agent: String,
    pub reason: String,
    pub error: String,
    pub failed_at_step: u32,
    pub duration_ms: u64,
}

// ── MigrationCoordinator ──

/// Tracks all active migrations. Pure state — no async.
pub struct MigrationCoordinator {
    active: HashMap<String, MigrationContext>,
    timeout_ms: u64,
}

impl MigrationCoordinator {
    /// Create a new migration coordinator.
    /// `timeout_ms` is the maximum duration from start to completion (default 60s).
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            active: HashMap::new(),
            timeout_ms,
        }
    }

    /// Start a migration. Validates preconditions:
    /// - Workspace state must be Active or Blocked.
    /// - Workspace must not already be migrating.
    pub fn start(
        &mut self,
        request: MigrationRequest,
        current_state: WorkspaceState,
        old_agent: String,
        now_ms: u64,
    ) -> Result<&MigrationContext, MigrationError> {
        // Validate state
        match current_state {
            WorkspaceState::Active | WorkspaceState::Blocked => {}
            _ => return Err(MigrationError::InvalidState(current_state)),
        }

        let ws_key = request.workspace_id.to_string();

        // Check not already migrating
        if self.active.contains_key(&ws_key) {
            return Err(MigrationError::AlreadyMigrating(ws_key));
        }

        let ctx = MigrationContext {
            workspace_id: request.workspace_id,
            pre_migration_state: current_state,
            old_agent,
            new_agent: request.new_agent,
            reason: request.reason,
            snapshot: None,
            started_at_ms: now_ms,
        };

        self.active.insert(ws_key.clone(), ctx);
        Ok(self.active.get(&ws_key).unwrap())
    }

    /// Look up an active migration.
    pub fn get(&self, workspace_id: &str) -> Option<&MigrationContext> {
        self.active.get(workspace_id)
    }

    /// Mutable lookup.
    pub fn get_mut(&mut self, workspace_id: &str) -> Option<&mut MigrationContext> {
        self.active.get_mut(workspace_id)
    }

    /// Store the workspace snapshot captured at step 4.
    pub fn set_snapshot(
        &mut self,
        workspace_id: &str,
        snapshot: MigrationSnapshot,
    ) -> Result<(), MigrationError> {
        let ctx = self
            .active
            .get_mut(workspace_id)
            .ok_or_else(|| MigrationError::NotMigrating(workspace_id.to_string()))?;

        if ctx.snapshot.is_some() {
            return Err(MigrationError::SnapshotAlreadySet(workspace_id.to_string()));
        }

        ctx.snapshot = Some(snapshot);
        Ok(())
    }

    /// Complete a migration successfully. Removes and returns the context.
    pub fn complete(&mut self, workspace_id: &str) -> Result<MigrationContext, MigrationError> {
        self.active
            .remove(workspace_id)
            .ok_or_else(|| MigrationError::NotMigrating(workspace_id.to_string()))
    }

    /// Fail a migration. Removes and returns the context.
    pub fn fail(
        &mut self,
        workspace_id: &str,
        _error: String,
        _step: u32,
    ) -> Result<MigrationContext, MigrationError> {
        self.active
            .remove(workspace_id)
            .ok_or_else(|| MigrationError::NotMigrating(workspace_id.to_string()))
    }

    /// Return workspace IDs whose migration has exceeded the timeout.
    pub fn check_timeouts(&self, now_ms: u64) -> Vec<WorkspaceId> {
        self.active
            .values()
            .filter(|ctx| now_ms.saturating_sub(ctx.started_at_ms) >= self.timeout_ms)
            .map(|ctx| ctx.workspace_id.clone())
            .collect()
    }

    /// Check if a workspace is currently migrating.
    pub fn is_migrating(&self, workspace_id: &str) -> bool {
        self.active.contains_key(workspace_id)
    }

    /// Number of active migrations.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Get the expected new agent for a migrating workspace (for bind identity verification).
    pub fn expected_agent(&self, workspace_id: &str) -> Option<&AgentRef> {
        self.active.get(workspace_id).map(|ctx| &ctx.new_agent)
    }

    // ── Trail event builders ──

    /// Build trail event for migration start.
    pub fn started_event(ctx: &MigrationContext) -> MigrationStartedEvent {
        MigrationStartedEvent {
            workspace_id: ctx.workspace_id.clone(),
            old_agent: ctx.old_agent.clone(),
            new_agent: ctx.new_agent.agent_type.clone(),
            reason: ctx.reason.clone(),
            pre_migration_state: format!("{:?}", ctx.pre_migration_state),
        }
    }

    /// Build trail event for migration completion.
    pub fn completed_event(ctx: &MigrationContext, now_ms: u64) -> MigrationCompletedEvent {
        MigrationCompletedEvent {
            workspace_id: ctx.workspace_id.clone(),
            old_agent: ctx.old_agent.clone(),
            new_agent: ctx.new_agent.agent_type.clone(),
            duration_ms: now_ms.saturating_sub(ctx.started_at_ms),
            restored_state: format!("{:?}", ctx.pre_migration_state),
        }
    }

    /// Build trail event for migration failure.
    pub fn failed_event(
        ctx: &MigrationContext,
        error: &str,
        step: u32,
        now_ms: u64,
    ) -> MigrationFailedEvent {
        MigrationFailedEvent {
            workspace_id: ctx.workspace_id.clone(),
            old_agent: ctx.old_agent.clone(),
            new_agent: ctx.new_agent.agent_type.clone(),
            reason: ctx.reason.clone(),
            error: error.to_string(),
            failed_at_step: step,
            duration_ms: now_ms.saturating_sub(ctx.started_at_ms),
        }
    }
}
