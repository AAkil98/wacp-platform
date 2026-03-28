//! WACP — Workspace actor.
//!
//! The core runtime unit. Each workspace is an actor owning its nine components,
//! processing messages through a biased two-channel select loop.

pub mod actor;
pub mod state;

pub use actor::{
    AgentMessage, CoordinatorCommand, WorkspaceActor, WorkspaceEvent, WorkspaceHandle,
};
pub use state::{ArchivedWorkspace, MigrationSnapshot, ResourceMeter, WorkspaceConfig, WorkspaceState};

#[cfg(test)]
mod tests;
