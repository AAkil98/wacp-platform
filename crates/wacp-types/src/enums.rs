use serde::{Deserialize, Serialize};

/// Signal types — closed set of 11 (PROTOCOL.md §4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalType {
    Ready,
    Started,
    Blocked,
    Checkpoint,
    Complete,
    Failed,
    Integrate,
    Acknowledged,
    Escalation,
    Suspend,
    Migrate,
}

/// Workspace lifecycle states — closed set of 9 (PROTOCOL.md §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkspaceState {
    Idle,
    Active,
    Blocked,
    Suspended,
    Migrating,
    Integrating,
    Conflicted,
    Closed,
    Failed,
}

impl WorkspaceState {
    /// Returns true for terminal states: Closed and Failed.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Closed | Self::Failed)
    }
}

/// Envelope lifecycle states — closed set of 5 (PROTOCOL.md §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EnvelopeState {
    Created,
    Validated,
    Delivered,
    Acknowledged,
    Rejected,
}

/// Task lifecycle statuses — closed set of 8 (PROTOCOL.md §4.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskStatus {
    Draft,
    Pending,
    Assigned,
    InProgress,
    Completed,
    Failed,
    Integrated,
    Cancelled,
}

/// Checkpoint status — closed set of 2 (PROTOCOL.md §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CheckpointStatus {
    Provisional,
    Final,
}

/// Confidence level — closed set of 3 (PROTOCOL.md §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// Envelope priority — closed set of 3 (PROTOCOL.md §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EnvelopePriority {
    Normal,
    Urgent,
    Blocking,
}

/// Envelope origin — closed set of 2 (PROTOCOL.md §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EnvelopeOrigin {
    Agent,
    Human,
}

/// Base roles — closed set of 3 (PROTOCOL.md §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BaseRole {
    Coordinator,
    Worker,
    Observer,
}

/// Merge strategy — closed set of 3 (PROTOCOL.md §7.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MergeStrategy {
    Direct,
    Layered,
    Evaluated,
}

/// Integration mode — closed set of 2 (PROTOCOL.md §7.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IntegrationMode {
    Normal,
    Salvage,
}

/// Conflict type — closed set of 4 (PROTOCOL.md §7.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConflictType {
    ContentOverlap,
    SemanticContradiction,
    DependencyViolation,
    ConstraintBreach,
}

/// Resolution strategy — closed set of 3 (PROTOCOL.md §7.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResolutionStrategy {
    CoordinatorResolve,
    Escalate,
    AgentRework,
}

/// Port right type — closed set of 3 (PROTOCOL.md §5.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortRightType {
    Send,
    Receive,
    SendOnce,
}

/// Gate type — closed set of 6 (PROTOCOL.md §8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GateType {
    TaskApproval,
    WorkspaceCreate,
    EnvelopeDelivery,
    Integration,
    ConflictResolution,
    WorkspaceAbort,
}

/// Gate decision — closed set of 3 (PROTOCOL.md §8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GateDecision {
    Approve,
    Reject,
    Modify,
}

/// Trail scope — closed set of 2 (PROTOCOL.md §9.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrailScope {
    Local,
    Global,
}

/// Storage tier — closed set of 3 (PROTOCOL.md §9.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageTier {
    Hot,
    Warm,
    Cold,
}

/// Error category — closed set of 8 (runtime spec §15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCategory {
    PermissionDenied,
    IllegalTransition,
    ValidationFailed,
    BudgetExceeded,
    Timeout,
    NotFound,
    DeliveryFailed,
    Internal,
}
