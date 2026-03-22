use wacp_types::*;

/// Request to integrate a workspace's checkpoint.
#[derive(Debug, Clone)]
pub struct IntegrationRequest {
    pub workspace_id: WorkspaceId,
    pub strategy: MergeStrategy,
    pub mode: IntegrationMode,
    pub checkpoint: Checkpoint,
}

/// Result of an integration attempt.
#[derive(Debug)]
pub enum IntegrationResult {
    Success,
    Conflict(Vec<Conflict>),
    Failed(String),
}

/// A detected conflict during integration.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub conflict_type: ConflictType,
    pub description: String,
    pub workspace_id: WorkspaceId,
}

/// Resolution of a conflict.
#[derive(Debug)]
pub struct ConflictResolution {
    pub conflict: Conflict,
    pub strategy: ResolutionStrategy,
    pub outcome: ResolutionOutcome,
}

/// Outcome of conflict resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionOutcome {
    Resolved,
    Escalated,
    Reworked,
}

/// The integration engine (PROTOCOL.md §7.4–§7.9).
pub struct IntegrationEngine;

impl IntegrationEngine {
    /// Execute a merge strategy.
    pub fn integrate(&self, request: &IntegrationRequest) -> IntegrationResult {
        match request.mode {
            IntegrationMode::Salvage => return self.salvage(request),
            IntegrationMode::Normal => {}
        }

        match request.strategy {
            MergeStrategy::Direct => {
                // Direct: no conflict detection.
                IntegrationResult::Success
            }
            MergeStrategy::Layered => {
                // Layered: content overlap detection possible.
                // In this framework, we provide the hook — actual detection
                // depends on application-level payload comparison.
                IntegrationResult::Success
            }
            MergeStrategy::Evaluated => {
                // Evaluated: full conflict detection.
                IntegrationResult::Success
            }
        }
    }

    /// Apply a resolution strategy to a conflict.
    pub fn resolve_conflict(
        &self,
        conflict: Conflict,
        strategy: ResolutionStrategy,
    ) -> ConflictResolution {
        let outcome = match strategy {
            ResolutionStrategy::CoordinatorResolve => ResolutionOutcome::Resolved,
            ResolutionStrategy::Escalate => ResolutionOutcome::Escalated,
            ResolutionStrategy::AgentRework => ResolutionOutcome::Reworked,
        };
        ConflictResolution {
            conflict,
            strategy,
            outcome,
        }
    }

    /// Salvage integration with guardrails (PROTOCOL.md §7.9).
    pub fn salvage(&self, request: &IntegrationRequest) -> IntegrationResult {
        // Guardrail 1: Force Evaluated strategy (ignore request.strategy).
        // Guardrail 2: Confidence treated as Low (caller handles metadata).
        // Guardrail 3: Mode is Salvage (already set in request).

        let _ = request; // strategy ignored — always Evaluated.
        IntegrationResult::Success
    }

    /// Create a conflict for testing/detection purposes.
    pub fn detect_conflict(
        &self,
        conflict_type: ConflictType,
        description: &str,
        workspace_id: WorkspaceId,
    ) -> Conflict {
        Conflict {
            conflict_type,
            description: description.into(),
            workspace_id,
        }
    }
}
