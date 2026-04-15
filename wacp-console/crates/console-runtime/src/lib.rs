/// Compiled protobuf types and gRPC client stubs for WACP runtime services.
///
/// Sourced from the shared `wacp-proto` crate; re-exported as `proto` to
/// preserve the existing `console_runtime::proto::*` import path.
pub use wacp_proto::wacp_v1 as proto;

pub mod grpc_pool;
pub mod rest_client;

// Re-export upstream taxonomy types used throughout the console.
pub use wacp_taxonomy::{
    CheckpointField, CheckpointSchema, ContextField, FieldType, ProfileSummary, QualityCriterion,
    TaskTypeDescriptor, ToolPolicy, ToolPolicyKind, ToolSummary, VerticalManifest, WorkflowSummary,
};

// Re-export taxonomy definition types for protocol taxonomy parsing.
pub use wacp_taxonomy::{
    CheckpointTypeDefinition, EnvelopePermission, EnvelopeTypeDefinition, RoleDefinition,
    TaxonomyDefinition,
};

// Re-export resolved taxonomy types.
pub use wacp_taxonomy::{EnvelopePermissionRow, ResolvedRole, Taxonomy};

// Re-export upstream protocol types.
pub use wacp_types::{BaseRole, GateDecision, GateType, TaskStatus, WorkspaceState};
