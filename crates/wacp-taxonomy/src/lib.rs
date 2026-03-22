//! WACP — Taxonomy loader and validation.
//!
//! Parses YAML/JSON taxonomy definitions, validates against protocol rules,
//! resolves derived roles, and builds lookup tables for the permission engine.

pub mod definition;
pub mod error;
mod loader;
pub mod resolved;

pub use definition::{
    CheckpointTypeDefinition, EnvelopePermission, EnvelopeTypeDefinition, RoleDefinition,
    TaxonomyDefinition,
};
pub use error::TaxonomyError;
pub use resolved::{EnvelopePermissionRow, ResolvedRole, Taxonomy};

impl Taxonomy {
    /// Load a taxonomy from YAML.
    pub fn load_yaml(yaml: &str, protocol_version: &str) -> Result<Self, TaxonomyError> {
        let def: TaxonomyDefinition =
            serde_yaml::from_str(yaml).map_err(|e| TaxonomyError::ParseError(e.to_string()))?;
        loader::load(def, protocol_version)
    }

    /// Load a taxonomy from JSON.
    pub fn load_json(json: &str, protocol_version: &str) -> Result<Self, TaxonomyError> {
        let def: TaxonomyDefinition =
            serde_json::from_str(json).map_err(|e| TaxonomyError::ParseError(e.to_string()))?;
        loader::load(def, protocol_version)
    }

    /// Create an empty taxonomy — base types only.
    pub fn empty(protocol_version: &str) -> Self {
        let def = TaxonomyDefinition {
            id: "empty".into(),
            version: "0.0.0".into(),
            protocol_version: protocol_version.into(),
            roles: vec![],
            envelope_types: vec![],
            checkpoint_types: vec![],
        };
        // Empty taxonomy always validates.
        loader::load(def, protocol_version).unwrap()
    }
}

#[cfg(test)]
mod tests;
