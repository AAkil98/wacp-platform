use serde::{Deserialize, Serialize};

use crate::enums::GateType;
use crate::id::{EscalationId, GateId, TaskId, UserId, WorkspaceId};

/// A gate event awaiting human decision (PROTOCOL.md §8.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateEvent {
    pub gate_id: GateId,
    pub gate_type: GateType,
    /// The full object awaiting approval (serialized).
    pub subject: Vec<u8>,
    pub workspace_id: WorkspaceId,
    pub task_id: Option<TaskId>,
    pub timeout_ms: u64,
    pub fallback_action: String,
    pub created_at: u64,
}

/// An escalation event routed to the highway (PROTOCOL.md §8.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EscalationEvent {
    pub escalation_id: EscalationId,
    pub workspace_id: WorkspaceId,
    pub owner: UserId,
    pub context: Vec<u8>,
    pub created_at: u64,
}
