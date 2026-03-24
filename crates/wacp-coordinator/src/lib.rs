//! WACP — Coordinator actor.
//!
//! The orchestrator: owns the workspace tree, task graph, and integration engine.
//! Dispatches work, processes signals, routes envelopes.

pub mod dispatch;
pub mod gate;
pub mod integration;
pub mod orchestrator;
pub mod ownership;
pub mod port_rights;
pub mod task_graph;
pub mod topology;
pub mod tree;
pub mod visibility;

pub use dispatch::{DispatchAction, DispatchConfig, Dispatcher};
pub use gate::{GateController, GateFallback, GateResolution, PendingGate};
pub use integration::{
    Conflict, ConflictResolution, IntegrationEngine, IntegrationRequest, IntegrationResult,
    ResolutionOutcome,
};
pub use orchestrator::{Coordinator, DispatchRequest};
pub use ownership::{resolve_originator, resolve_owner, EscalationRouter};
pub use port_rights::{PortRightEntry, PortRightError, PortRightStatus, PortRightsGraph};
pub use task_graph::{GraphError, TaskGraph};
pub use topology::{CascadeEffect, CreateWorkspaceParams, TopologySet};
pub use tree::{TreeError, WorkspaceNode, WorkspaceTree};
pub use visibility::{VisibilityError, VisibilityGraph};

#[cfg(test)]
mod tests;
