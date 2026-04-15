//! WACP — Coordinator actor.
//!
//! The orchestrator: owns the workspace tree, task graph, and integration engine.
//! Dispatches work, processes signals, routes envelopes.

pub mod dispatch;
pub mod events;
pub mod gate;
pub mod handler;
pub mod integration;
pub mod migration;
pub mod orchestrator;
pub mod ownership;
pub mod port_rights;
pub mod resource;
pub mod scheduling;
pub mod task_graph;
pub mod topology;
pub mod tree;
pub mod visibility;

pub use dispatch::{DispatchAction, DispatchConfig, Dispatcher};
pub use events::{CoordinatorEvent, EventBus};
pub use gate::{GateController, GateFallback, GateResolution, PendingGate};
pub use handler::{
    BindResult, CreateCheckpointResult, EmitSignalResult, HandlerError, RequestHandler,
    SendEnvelopeResult, TaskGraphSnapshot, TaskView, WorkspaceView,
};
pub use integration::{
    CheckpointRef, Conflict, ConflictResolution, ConflictResolutionResult, ConflictResolver,
    IntegrationDecision, IntegrationEngine, IntegrationPipeline, IntegrationQueue,
    IntegrationRequest, IntegrationResult, MergeContext, MergeExecutor, MergeResult,
    ResolutionOutcome, SalvageIntegration,
};
pub use migration::{
    AgentRef, MigrationCompletedEvent, MigrationContext, MigrationCoordinator, MigrationError,
    MigrationFailedEvent, MigrationRequest, MigrationStartedEvent,
};
pub use orchestrator::{Coordinator, DispatchRequest, SystemSnapshot};
pub use ownership::{EscalationRouter, resolve_originator, resolve_owner};
pub use port_rights::{PortRightEntry, PortRightError, PortRightStatus, PortRightsGraph};
pub use resource::{BudgetCheckResult, BudgetEnforcer, LivenessMonitor, TimeoutTracker};
pub use scheduling::{DependencyOutput, RetryPolicy, SchedulingOps, TaskContext};
pub use task_graph::{GraphError, TaskGraph};
pub use topology::{CascadeEffect, CreateWorkspaceParams, TopologySet};
pub use tree::{TreeError, WorkspaceNode, WorkspaceTree};
pub use visibility::{VisibilityError, VisibilityGraph};

#[cfg(test)]
mod tests;
