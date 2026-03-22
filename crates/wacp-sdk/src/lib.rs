//! WACP — Rust agent SDK.
//!
//! Typed async client for writing WACP agents. Connects to the runtime via gRPC,
//! provides a clean API for emitting signals, creating checkpoints, sending envelopes,
//! and receiving commands.

mod builder;
mod connection;
mod error;
mod streams;

pub use builder::{CheckpointBuilder, EnvelopeBuilder};
pub use connection::Agent;
pub use error::Error;
pub use streams::{CommandStream, InboxStream};

/// Configuration for connecting to the runtime.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub runtime_url: String,
    pub workspace_id: wacp_types::WorkspaceId,
    pub auth_token: String,
}

#[cfg(test)]
mod tests;
