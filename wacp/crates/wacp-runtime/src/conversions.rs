//! Proto ↔ internal enum/struct conversion helpers.
//!
//! Extracted from `init.rs` per `tech-debt-2026-04-18.md` §3.2 B.1 (closeout-plan P4). The
//! handlers in `agent_service.rs` / `highway_service.rs` / `coordinator_service.rs` all
//! depend on these; `pub(crate)` makes them usable from the sibling modules without widening
//! the binary's public surface.
//!
//! Several of these helpers exist specifically to avoid `enum as i32` casts when internal and
//! proto enums disagree on discriminant offsets — see `gate_type_to_proto` / `workspace_state_to_proto`
//! doc-comments for the specific incidents (§11.1, §11.4 audit entries + `wacp/impl/wa3-5-checkpoint-gates.md`).

use wacp_types::*;

pub(crate) fn envelope_to_proto(envelope: &Envelope) -> wacp_transport::wacp_v1::Envelope {
    wacp_transport::wacp_v1::Envelope {
        id: envelope.id.to_string(),
        from_workspace: envelope.from_workspace.to_string(),
        to_workspace: envelope.to_workspace.to_string(),
        r#type: envelope.envelope_type.clone(),
        payload: envelope.payload.clone(),
        in_reply_to: envelope
            .in_reply_to
            .as_ref()
            .map(|e| e.to_string())
            .unwrap_or_default(),
        priority: envelope_priority_to_proto(envelope.priority) as i32,
        timestamp: None,
        origin: envelope_origin_to_proto(envelope.origin) as i32,
    }
}

/// Convert a proto `CheckpointStatus` into the internal enum. Used by WA3
/// when forwarding checkpoints to the workspace actor. Unknown values fall
/// back to `Provisional`.
pub(crate) fn proto_to_checkpoint_status(
    s: wacp_transport::wacp_v1::CheckpointStatus,
) -> CheckpointStatus {
    use wacp_transport::wacp_v1::CheckpointStatus as P;
    match s {
        P::Unspecified | P::Provisional => CheckpointStatus::Provisional,
        P::Final => CheckpointStatus::Final,
    }
}

/// Convert a proto `Confidence` into the internal enum. Used by WA3.
pub(crate) fn proto_to_confidence(c: wacp_transport::wacp_v1::Confidence) -> Confidence {
    use wacp_transport::wacp_v1::Confidence as P;
    match c {
        P::Unspecified | P::High => Confidence::High,
        P::Medium => Confidence::Medium,
        P::Low => Confidence::Low,
    }
}

/// Convert a proto `SignalType` into the internal `wacp_types::SignalType`.
/// Used by `AgentService::EmitSignal` (WA2) to hand the signal to the
/// workspace actor. Mirrors the internal→proto direction in
/// `wacp-sdk::connection::signal_with`.
pub(crate) fn proto_to_signal_type(t: wacp_transport::wacp_v1::SignalType) -> SignalType {
    use wacp_transport::wacp_v1::SignalType as P;
    match t {
        P::Unspecified | P::Ready => SignalType::Ready,
        P::Started => SignalType::Started,
        P::Blocked => SignalType::Blocked,
        P::Checkpoint => SignalType::Checkpoint,
        P::Complete => SignalType::Complete,
        P::Failed => SignalType::Failed,
        P::Integrate => SignalType::Integrate,
        P::Acknowledged => SignalType::Acknowledged,
        P::Escalation => SignalType::Escalation,
        P::Suspend => SignalType::Suspend,
        P::Migrate => SignalType::Migrate,
    }
}

/// Map the internal `GateType` enum onto its proto counterpart. Internal
/// variants start at 0 (TaskApproval), proto variants at 1 (after the
/// Unspecified marker), so `enum as i32` is off by one and produces wrong
/// wire values. WA3.5 is the first production path to emit gate events
/// downstream, so this helper lands alongside the new variant.
pub(crate) fn gate_type_to_proto(gt: GateType) -> wacp_transport::wacp_v1::GateType {
    use wacp_transport::wacp_v1::GateType as P;
    match gt {
        GateType::TaskApproval => P::TaskApproval,
        GateType::WorkspaceCreate => P::WorkspaceCreate,
        GateType::EnvelopeDelivery => P::EnvelopeDelivery,
        GateType::Integration => P::Integration,
        GateType::ConflictResolution => P::ConflictResolution,
        GateType::WorkspaceAbort => P::WorkspaceAbort,
        GateType::CheckpointApproval => P::CheckpointApproval,
    }
}

/// Map the internal `WorkspaceState` enum onto its proto counterpart. Same
/// enum-offset trap as `gate_type_to_proto`: internal `Idle = 0`, proto
/// `UNSPECIFIED = 0` + `IDLE = 1`. Miscasting surfaces as garbage state
/// values on StreamWorkspaceChanges — the Console's monitor then fails
/// its completion-detection comparison against `proto::Closed`. First
/// observed biting T7.3: `Closed as i32 = 7` but proto 7 is `Conflicted`.
pub(crate) fn workspace_state_to_proto(
    ws: WorkspaceState,
) -> wacp_transport::wacp_v1::WorkspaceState {
    use wacp_transport::wacp_v1::WorkspaceState as P;
    match ws {
        WorkspaceState::Idle => P::Idle,
        WorkspaceState::Active => P::Active,
        WorkspaceState::Blocked => P::Blocked,
        WorkspaceState::Suspended => P::Suspended,
        WorkspaceState::Migrating => P::Migrating,
        WorkspaceState::Integrating => P::Integrating,
        WorkspaceState::Conflicted => P::Conflicted,
        WorkspaceState::Closed => P::Closed,
        WorkspaceState::Failed => P::Failed,
    }
}

pub(crate) fn signal_type_to_proto(st: SignalType) -> wacp_transport::wacp_v1::SignalType {
    use wacp_transport::wacp_v1::SignalType as P;
    match st {
        SignalType::Ready => P::Ready,
        SignalType::Started => P::Started,
        SignalType::Blocked => P::Blocked,
        SignalType::Checkpoint => P::Checkpoint,
        SignalType::Complete => P::Complete,
        SignalType::Failed => P::Failed,
        SignalType::Integrate => P::Integrate,
        SignalType::Acknowledged => P::Acknowledged,
        SignalType::Escalation => P::Escalation,
        SignalType::Suspend => P::Suspend,
        SignalType::Migrate => P::Migrate,
    }
}

pub(crate) fn task_status_to_proto(ts: TaskStatus) -> wacp_transport::wacp_v1::TaskStatus {
    use wacp_transport::wacp_v1::TaskStatus as P;
    match ts {
        TaskStatus::Draft => P::Draft,
        TaskStatus::Pending => P::Pending,
        TaskStatus::Assigned => P::Assigned,
        TaskStatus::InProgress => P::InProgress,
        TaskStatus::Completed => P::Completed,
        TaskStatus::Failed => P::Failed,
        TaskStatus::Integrated => P::Integrated,
        TaskStatus::Cancelled => P::Cancelled,
    }
}

pub(crate) fn envelope_priority_to_proto(
    p: EnvelopePriority,
) -> wacp_transport::wacp_v1::EnvelopePriority {
    use wacp_transport::wacp_v1::EnvelopePriority as P;
    match p {
        EnvelopePriority::Normal => P::Normal,
        EnvelopePriority::Urgent => P::Urgent,
        EnvelopePriority::Blocking => P::Blocking,
    }
}

pub(crate) fn envelope_origin_to_proto(
    o: EnvelopeOrigin,
) -> wacp_transport::wacp_v1::EnvelopeOrigin {
    use wacp_transport::wacp_v1::EnvelopeOrigin as P;
    match o {
        EnvelopeOrigin::Agent => P::Agent,
        EnvelopeOrigin::Human => P::Human,
    }
}

/// Convert an internal `ResourceBudget` to the proto form. Internal fields
/// are `Option<u64>` (None = unlimited); proto fields are plain `u64` and
/// treat 0 as "no limit" (matching how `GetAllocatable` reports
/// `BudgetConfig` values at the config level). `warning_threshold` carries
/// through from the internal budget.
pub(crate) fn budget_to_proto(budget: &ResourceBudget) -> wacp_transport::wacp_v1::ResourceBudget {
    wacp_transport::wacp_v1::ResourceBudget {
        max_tokens: budget.max_tokens.unwrap_or(0),
        max_wall_time_ms: budget.max_wall_time_ms.unwrap_or(0),
        max_storage_bytes: budget.max_storage_bytes.unwrap_or(0),
        max_network_bytes: budget.max_network_bytes.unwrap_or(0),
        max_cost_micros: budget.max_cost_micros.unwrap_or(0),
        warning_threshold: budget.warning_threshold,
    }
}
