//! WACP — Transport abstraction.
//!
//! Defines the transport trait and provides two implementations:
//! - `InProcessTransport` for testing (channel-based, zero serialization)
//! - gRPC transport for production (`tonic` server with agent + highway services)

pub mod auth;
pub mod error_mapping;
pub mod grpc_agent;
pub mod grpc_highway;
pub mod grpc_server;
pub mod in_process;
pub mod messages;
pub mod proto;

pub use error_mapping::{error_category_to_grpc_code, protocol_error_to_status};
pub use grpc_agent::{AgentRequest, AgentServiceImpl};
pub use grpc_highway::{HighwayRequest, HighwayServiceImpl};
pub use grpc_server::{start_grpc_server, GrpcServerConfig, GrpcServerHandles};
pub use in_process::{InProcessAgentClient, InProcessAgentSession, InProcessTransport};
pub use messages::{AgentInbound, AgentOutbound, HighwayInbound, HighwayOutbound};
pub use proto::wacp_v1;

/// Transport-level errors.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("connection closed")]
    ConnectionClosed,
    #[error("send failed: {0}")]
    SendFailed(String),
    #[error("transport error: {0}")]
    Other(String),
}

#[cfg(test)]
mod tests;
