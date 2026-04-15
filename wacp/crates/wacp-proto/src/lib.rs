//! Generated protobuf types and gRPC service stubs for the `wacp.v1` package.
//!
//! Single source of truth for proto codegen across the workspace. Both
//! `wacp-transport` (runtime side: client + server) and `console-runtime`
//! (console side: client) consume this crate instead of re-running
//! `tonic_build` in their own `build.rs`. Server-side and client-side
//! stubs are both generated; consumers pick which sides they need.

pub mod wacp_v1 {
    tonic::include_proto!("wacp.v1");
}
