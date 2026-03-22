/// Errors produced during taxonomy loading and validation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum TaxonomyError {
    #[error("parse error: {0}")]
    ParseError(String),

    #[error("protocol version mismatch: expected {expected}, found {found}")]
    ProtocolVersionMismatch { expected: String, found: String },

    #[error("duplicate role name: {0}")]
    DuplicateRoleName(String),

    #[error("duplicate envelope type name: {0}")]
    DuplicateEnvelopeType(String),

    #[error("duplicate checkpoint type name: {0}")]
    DuplicateCheckpointType(String),

    #[error("name collides with base definition: {0}")]
    BaseNameCollision(String),

    #[error("invalid inheritance: role {role} extends {extends} (must extend worker or observer)")]
    InvalidInheritance { role: String, extends: String },

    #[error("privilege escalation: role {role} adds coordinator capability {capability}")]
    PrivilegeEscalation { role: String, capability: String },

    #[error("cross-registry reference missing: role {role} references unregistered {reference}")]
    CrossRegistryMissing { role: String, reference: String },

    #[error("envelope type {envelope_type} references unknown role {role}")]
    EnvelopeRoleNotFound {
        envelope_type: String,
        role: String,
    },

    #[error("checkpoint type {checkpoint_type} references unknown role {role}")]
    CheckpointRoleNotFound {
        checkpoint_type: String,
        role: String,
    },

    #[error("envelope type {0} has no permission rows")]
    EmptyPermissions(String),

    #[error("checkpoint type {0} has no permitted roles")]
    EmptyRoleList(String),
}
