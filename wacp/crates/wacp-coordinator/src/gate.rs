use std::collections::HashMap;

use wacp_types::{CheckpointId, GateDecision, GateEvent, GateId, GateType, TaskId, WorkspaceId};

/// Fallback action when a gate times out without a human response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateFallback {
    AutoApprove,
    Cancel,
}

/// A gate awaiting resolution.
#[derive(Debug, Clone)]
pub struct PendingGate {
    pub gate_id: GateId,
    pub task_id: TaskId,
    pub task_name: String,
    pub timeout_ms: u64,
    pub fallback: GateFallback,
    pub created_at: u64,
}

/// A checkpoint-approval gate awaiting resolution (WA3.5).
///
/// Structurally parallel to `PendingGate` but tracks a checkpoint
/// reference instead of a task. Kept as a distinct type so existing
/// `PendingGate` / `resolve` / `GateResolution` consumers continue to
/// compile without changes.
#[derive(Debug, Clone)]
pub struct PendingCheckpointGate {
    pub gate_id: GateId,
    pub workspace_id: WorkspaceId,
    pub checkpoint_id: CheckpointId,
    pub checkpoint_type: String,
    pub timeout_ms: u64,
    pub fallback: GateFallback,
    pub created_at: u64,
}

/// How a gate was resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateResolution {
    Approved { source: String },
    Rejected,
}

/// Manages task approval gates (task-scheduling.md §3) and, since WA3.5,
/// checkpoint approval gates. Both share the same monotonic id counter so
/// the `gate_id` space is globally unique within a controller.
///
/// When a task enters `draft`, the coordinator opens a task gate. When an
/// agent creates a provisional checkpoint, the runtime opens a checkpoint
/// gate. Both resolve via a human response (approve/reject/modify via
/// highway) or by timeout (fallback action). First response wins.
pub struct GateController {
    pending: HashMap<String, PendingGate>,
    pending_checkpoints: HashMap<String, PendingCheckpointGate>,
    next_id: u64,
    default_timeout_ms: u64,
    default_fallback: GateFallback,
}

impl GateController {
    pub fn new(default_timeout_ms: u64, default_fallback: GateFallback) -> Self {
        Self {
            pending: HashMap::new(),
            pending_checkpoints: HashMap::new(),
            next_id: 1,
            default_timeout_ms,
            default_fallback,
        }
    }

    /// Open a gate for a task. Returns a GateEvent for highway delivery.
    pub fn open_gate(
        &mut self,
        task_id: TaskId,
        task_name: String,
        task_description: &str,
        timeout_ms: Option<u64>,
        fallback: Option<GateFallback>,
    ) -> GateEvent {
        let gate_id = GateId::from(format!("gate-{}", self.next_id));
        self.next_id += 1;

        let timeout = timeout_ms.unwrap_or(self.default_timeout_ms);
        let fb = fallback.unwrap_or(self.default_fallback);

        let pending = PendingGate {
            gate_id: gate_id.clone(),
            task_id: task_id.clone(),
            task_name: task_name.clone(),
            timeout_ms: timeout,
            fallback: fb,
            created_at: 0, // coordinator assigns real timestamp
        };

        self.pending.insert(gate_id.to_string(), pending);

        GateEvent {
            gate_id,
            gate_type: GateType::TaskApproval,
            subject: task_description.as_bytes().to_vec(),
            workspace_id: wacp_types::WorkspaceId::from(""),
            task_id: Some(task_id),
            timeout_ms: timeout,
            fallback_action: match fb {
                GateFallback::AutoApprove => "auto_approve".into(),
                GateFallback::Cancel => "cancel".into(),
            },
            created_at: 0,
        }
    }

    /// Resolve a gate via human response. Returns None if already resolved.
    pub fn resolve(&mut self, gate_id: &GateId, decision: GateDecision) -> Option<GateResolution> {
        let gate = self.pending.remove(gate_id.as_ref())?;
        let _ = gate; // consumed

        Some(match decision {
            GateDecision::Approve | GateDecision::Modify => GateResolution::Approved {
                source: "human".into(),
            },
            GateDecision::Reject => GateResolution::Rejected,
        })
    }

    /// Resolve a gate via timeout. Returns None if already resolved.
    /// Applies the gate's configured fallback action.
    pub fn timeout(&mut self, gate_id: &GateId) -> Option<GateResolution> {
        let gate = self.pending.remove(gate_id.as_ref())?;

        Some(match gate.fallback {
            GateFallback::AutoApprove => GateResolution::Approved {
                source: "timeout_auto_approve".into(),
            },
            GateFallback::Cancel => GateResolution::Rejected,
        })
    }

    /// Check if a gate is still pending.
    pub fn is_pending(&self, gate_id: &GateId) -> bool {
        self.pending.contains_key(gate_id.as_ref())
    }

    /// Find the pending gate for a task.
    pub fn pending_for_task(&self, task_id: &TaskId) -> Option<&PendingGate> {
        self.pending.values().find(|g| &g.task_id == task_id)
    }

    /// Number of pending task gates.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    // ── WA3.5 — checkpoint-approval gates ─────────────────────────

    /// Open a checkpoint-approval gate. Returns a GateEvent for highway delivery.
    ///
    /// Structural mirror of `open_gate` but carries a checkpoint reference
    /// instead of a task. `subject` carries the checkpoint_id bytes so
    /// subscribers have a stable id for dedup / debug even before the full
    /// payload is fetched via `HighwayRequest::GetCheckpoint`.
    pub fn open_checkpoint_gate(
        &mut self,
        workspace_id: WorkspaceId,
        checkpoint_id: CheckpointId,
        checkpoint_type: String,
        timeout_ms: Option<u64>,
        fallback: Option<GateFallback>,
    ) -> GateEvent {
        let gate_id = GateId::from(format!("gate-{}", self.next_id));
        self.next_id += 1;

        let timeout = timeout_ms.unwrap_or(self.default_timeout_ms);
        let fb = fallback.unwrap_or(self.default_fallback);

        let pending = PendingCheckpointGate {
            gate_id: gate_id.clone(),
            workspace_id: workspace_id.clone(),
            checkpoint_id: checkpoint_id.clone(),
            checkpoint_type,
            timeout_ms: timeout,
            fallback: fb,
            created_at: 0, // coordinator assigns real timestamp
        };

        self.pending_checkpoints
            .insert(gate_id.to_string(), pending);

        GateEvent {
            gate_id,
            gate_type: GateType::CheckpointApproval,
            subject: checkpoint_id.as_ref().as_bytes().to_vec(),
            workspace_id,
            task_id: None,
            timeout_ms: timeout,
            fallback_action: match fb {
                GateFallback::AutoApprove => "auto_approve".into(),
                GateFallback::Cancel => "cancel".into(),
            },
            created_at: 0,
        }
    }

    /// Detach a pending checkpoint gate for routing to the workspace actor.
    /// Returns `None` if the gate is not a checkpoint gate (or was never
    /// opened / already resolved). Callers apply their own decision — this
    /// method does not return a `GateResolution` because checkpoint gates
    /// route through `CoordinatorCommand::CheckpointApproved/Rejected`, not
    /// through the task-gate `GateResolution` path.
    pub fn resolve_checkpoint(&mut self, gate_id: &GateId) -> Option<PendingCheckpointGate> {
        self.pending_checkpoints.remove(gate_id.as_ref())
    }

    /// Check if a checkpoint gate is still pending.
    pub fn is_checkpoint_pending(&self, gate_id: &GateId) -> bool {
        self.pending_checkpoints.contains_key(gate_id.as_ref())
    }

    /// Number of pending checkpoint gates.
    pub fn pending_checkpoint_count(&self) -> usize {
        self.pending_checkpoints.len()
    }
}
