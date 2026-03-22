use serde::{Deserialize, Serialize};

/// Raw taxonomy definition — deserialized from YAML/JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyDefinition {
    pub id: String,
    pub version: String,
    pub protocol_version: String,
    #[serde(default)]
    pub roles: Vec<RoleDefinition>,
    #[serde(default)]
    pub envelope_types: Vec<EnvelopeTypeDefinition>,
    #[serde(default)]
    pub checkpoint_types: Vec<CheckpointTypeDefinition>,
}

/// A derived role definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleDefinition {
    pub name: String,
    pub extends: String,
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
    #[serde(default)]
    pub checkpoint_types: Option<Vec<String>>,
}

/// A custom envelope type with send/receive permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeTypeDefinition {
    pub name: String,
    pub permissions: Vec<EnvelopePermission>,
}

/// A single permission row: sender_role may send this type to receiver_role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopePermission {
    pub sender_role: String,
    pub receiver_role: String,
}

/// A custom checkpoint type with permitted roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointTypeDefinition {
    pub name: String,
    pub permitted_roles: Vec<String>,
}
