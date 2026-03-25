use std::collections::{HashSet, VecDeque};

use wacp_types::*;

// ── Existing types (preserved for backward compat) ──────────────────

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
            MergeStrategy::Direct => IntegrationResult::Success,
            MergeStrategy::Layered => IntegrationResult::Success,
            MergeStrategy::Evaluated => IntegrationResult::Success,
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
        let _ = request;
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

// ── New types (task 11.1) ───────────────────────────────────────────

/// Reference to a checkpoint for integration decisions.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckpointRef {
    pub checkpoint_id: CheckpointId,
    pub content_hash: String,
    pub intent: String,
    pub confidence: Confidence,
}

/// Coordinator's integration decision (integration.md §3).
#[derive(Debug, Clone, PartialEq)]
pub enum IntegrationDecision {
    Accept { strategy: MergeStrategy },
    Revise { feedback: String },
    Reject { reason: String },
}

/// Sequential integration queue — one workspace at a time (integration.md §8).
pub struct IntegrationQueue {
    pending: VecDeque<WorkspaceId>,
    in_progress: Option<WorkspaceId>,
}

impl IntegrationQueue {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            in_progress: None,
        }
    }

    /// Add a workspace to the integration queue.
    pub fn push(&mut self, workspace_id: WorkspaceId) {
        self.pending.push_back(workspace_id);
    }

    /// Pop the next workspace if no integration is in progress.
    pub fn take_next(&mut self) -> Option<WorkspaceId> {
        if self.in_progress.is_some() {
            return None;
        }
        let id = self.pending.pop_front()?;
        self.in_progress = Some(id.clone());
        Some(id)
    }

    /// Mark the current integration as complete.
    pub fn complete(&mut self) {
        self.in_progress = None;
    }

    /// Is an integration currently in progress?
    pub fn is_active(&self) -> bool {
        self.in_progress.is_some()
    }

    /// Number of workspaces waiting.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// The workspace currently being integrated.
    pub fn current(&self) -> Option<&WorkspaceId> {
        self.in_progress.as_ref()
    }
}

impl Default for IntegrationQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Stateless integration pipeline operations (integration.md §2–3).
pub struct IntegrationPipeline;

impl IntegrationPipeline {
    /// Find the most recent final checkpoint in a list.
    pub fn find_final_checkpoint(checkpoints: &[Checkpoint]) -> Option<CheckpointRef> {
        checkpoints
            .iter()
            .rev()
            .find(|c| c.status == CheckpointStatus::Final)
            .map(|c| CheckpointRef {
                checkpoint_id: c.id.clone(),
                content_hash: c.content_hash.clone(),
                intent: c.intent.clone(),
                confidence: c.confidence,
            })
    }

    /// Rule-based integration decision (integration.md §3.2).
    /// Low confidence → revise. Otherwise → accept with strategy from confidence.
    pub fn decide(checkpoint: &CheckpointRef) -> IntegrationDecision {
        if checkpoint.confidence == Confidence::Low {
            return IntegrationDecision::Revise {
                feedback: "Checkpoint confidence is low; please verify and resubmit".into(),
            };
        }

        let strategy = Self::select_strategy(checkpoint.confidence);
        IntegrationDecision::Accept { strategy }
    }

    /// Map confidence to merge strategy (integration.md §4).
    /// High → Direct (no conflict possible), Medium → Layered, Low → Evaluated.
    pub fn select_strategy(confidence: Confidence) -> MergeStrategy {
        match confidence {
            Confidence::High => MergeStrategy::Direct,
            Confidence::Medium => MergeStrategy::Layered,
            Confidence::Low => MergeStrategy::Evaluated,
        }
    }
}

// ── Merge executor (task 11.2) ──────────────────────────────────────

/// Context for a merge operation — carries extracted resource sets.
#[derive(Debug, Clone)]
pub struct MergeContext {
    pub source_id: WorkspaceId,
    pub target_id: WorkspaceId,
    /// Resources modified by the source workspace's checkpoint.
    pub source_resources: HashSet<String>,
    /// Resources modified by prior integrations into the parent.
    pub parent_resources: HashSet<String>,
    pub checkpoint: CheckpointRef,
}

/// Result of a merge operation.
#[derive(Debug)]
pub enum MergeResult {
    Success,
    Conflicts(Vec<Conflict>),
}

/// Executes merge strategies against a MergeContext (integration.md §4).
pub struct MergeExecutor;

impl MergeExecutor {
    /// Execute the given strategy.
    pub fn execute(strategy: MergeStrategy, ctx: &MergeContext) -> MergeResult {
        match strategy {
            MergeStrategy::Direct => Self::merge_direct(ctx),
            MergeStrategy::Layered => Self::merge_layered(ctx),
            MergeStrategy::Evaluated => Self::merge_evaluated(ctx),
        }
    }

    /// Direct: pass-through, no conflict detection.
    pub fn merge_direct(_ctx: &MergeContext) -> MergeResult {
        MergeResult::Success
    }

    /// Layered: detect content overlap only.
    pub fn merge_layered(ctx: &MergeContext) -> MergeResult {
        let overlapping: Vec<&String> = ctx
            .source_resources
            .intersection(&ctx.parent_resources)
            .collect();

        if overlapping.is_empty() {
            return MergeResult::Success;
        }

        let conflicts = overlapping
            .into_iter()
            .map(|resource| Conflict {
                conflict_type: ConflictType::ContentOverlap,
                description: format!(
                    "Resource '{}' modified by both source and prior integration",
                    resource
                ),
                workspace_id: ctx.source_id.clone(),
            })
            .collect();

        MergeResult::Conflicts(conflicts)
    }

    /// Evaluated: full 4-type conflict scan.
    /// Content overlap is detected mechanically. Semantic contradiction,
    /// dependency violation, and constraint breach require application-level
    /// analysis — hooks for those are provided but return no conflicts in
    /// the initial implementation.
    pub fn merge_evaluated(ctx: &MergeContext) -> MergeResult {
        let mut conflicts = Vec::new();

        // 1. Content overlap (same as layered).
        for resource in ctx.source_resources.intersection(&ctx.parent_resources) {
            conflicts.push(Conflict {
                conflict_type: ConflictType::ContentOverlap,
                description: format!("Resource '{}' modified by both", resource),
                workspace_id: ctx.source_id.clone(),
            });
        }

        // 2–4. Semantic contradiction, dependency violation, constraint breach:
        // application-level hooks — no mechanical detection in initial impl.

        if conflicts.is_empty() {
            MergeResult::Success
        } else {
            MergeResult::Conflicts(conflicts)
        }
    }
}
