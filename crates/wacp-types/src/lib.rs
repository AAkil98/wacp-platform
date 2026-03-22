//! WACP — Protocol type definitions.
//!
//! Pure data types for the WACP protocol. No logic, no I/O, no async.
//! Every other crate in the workspace depends on this one.

pub mod checkpoint;
pub mod enums;
pub mod envelope;
pub mod error;
pub mod gate;
pub mod id;
pub mod originator;
pub mod port;
pub mod resource;
pub mod signal;
pub mod task;
pub mod trail;

// Re-export all public types at crate root for convenience.
pub use checkpoint::Checkpoint;
pub use enums::{
    BaseRole, CheckpointStatus, Confidence, ConflictType, EnvelopeOrigin, EnvelopePriority,
    EnvelopeState, ErrorCategory, GateDecision, GateType, IntegrationMode, MergeStrategy,
    PortRightType, ResolutionStrategy, SignalType, StorageTier, TaskStatus, TrailScope,
    WorkspaceState,
};
pub use envelope::Envelope;
pub use error::ProtocolError;
pub use gate::{EscalationEvent, GateEvent};
pub use id::{
    CheckpointId, EnvelopeId, EscalationId, GateId, TaskId, TrailEntryId, UserId, WorkspaceId,
};
pub use originator::Originator;
pub use port::PortRight;
pub use resource::{ResourceBudget, ResourceUsage};
pub use signal::Signal;
pub use task::Task;
pub use trail::TrailEntry;

#[cfg(test)]
mod tests;
