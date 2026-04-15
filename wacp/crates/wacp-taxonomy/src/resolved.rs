use std::collections::{HashMap, HashSet};

use wacp_types::BaseRole;

/// A validated, immutable taxonomy. Arc-shareable across actors.
#[derive(Debug, Clone)]
pub struct Taxonomy {
    pub id: String,
    pub version: String,
    pub protocol_version: String,
    /// Derived roles only. Base roles are implicit.
    pub resolved_roles: HashMap<String, ResolvedRole>,
    /// All valid envelope type names (base + custom).
    pub envelope_types: HashSet<String>,
    /// Full permission matrix rows (base + custom).
    pub envelope_permissions: Vec<EnvelopePermissionRow>,
    /// Checkpoint type → set of permitted role names (base + custom).
    pub checkpoint_types: HashMap<String, HashSet<String>>,
}

/// A derived role with resolved capabilities.
#[derive(Debug, Clone)]
pub struct ResolvedRole {
    pub name: String,
    pub base: BaseRole,
    pub capabilities: HashSet<String>,
    pub checkpoint_types: HashSet<String>,
}

/// A single row in the permission matrix.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnvelopePermissionRow {
    pub sender_role: String,
    pub envelope_type: String,
    pub receiver_role: String,
}

impl Taxonomy {
    /// Check if a role name is valid (base or derived).
    pub fn is_valid_role(&self, name: &str) -> bool {
        matches!(name, "coordinator" | "worker" | "observer")
            || self.resolved_roles.contains_key(name)
    }

    /// Check if an envelope type name is valid (base or custom).
    pub fn is_valid_envelope_type(&self, name: &str) -> bool {
        self.envelope_types.contains(name)
    }

    /// Check if a checkpoint type name is valid (base or custom).
    pub fn is_valid_checkpoint_type(&self, name: &str) -> bool {
        self.checkpoint_types.contains_key(name)
    }

    /// Get a resolved derived role. Returns None for base roles.
    pub fn resolve_role(&self, name: &str) -> Option<&ResolvedRole> {
        self.resolved_roles.get(name)
    }
}
