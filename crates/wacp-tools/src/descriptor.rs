use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Validation errors for tool descriptors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DescriptorError {
    #[error("invalid tool name '{name}': {reason}")]
    InvalidName { name: String, reason: String },

    #[error("invalid version '{version}': expected major.minor.patch")]
    InvalidVersion { version: String },

    #[error("descriptor must have at least one capability")]
    NoCapabilities,

    #[error("duplicate capability name: '{name}'")]
    DuplicateCapability { name: String },

    #[error("capability '{name}': {reason}")]
    InvalidCapability { name: String, reason: String },

    #[error("invalid config_schema: {reason}")]
    InvalidConfigSchema { reason: String },
}

/// A tool's complete declaration — name, version, capabilities, config schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// Unique name. Lowercase, alphanumeric + underscores. Max 64 chars.
    pub name: String,

    /// Semantic version (major.minor.patch).
    pub version: String,

    /// Human-readable description. Used in LLM tool-use prompts.
    pub description: String,

    /// The tool's callable capabilities.
    pub capabilities: Vec<Capability>,

    /// Per-deployment configuration schema (JSON Schema). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_schema: Option<serde_json::Value>,

    /// Metadata tags for filtering and discovery.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl ToolDescriptor {
    /// Validate the descriptor. Returns Ok(()) if valid, Err with the first violation.
    pub fn validate(&self) -> Result<(), DescriptorError> {
        validate_name(&self.name)?;
        validate_version(&self.version)?;

        if self.capabilities.is_empty() {
            return Err(DescriptorError::NoCapabilities);
        }

        let mut seen = HashSet::new();
        for cap in &self.capabilities {
            if !seen.insert(&cap.name) {
                return Err(DescriptorError::DuplicateCapability {
                    name: cap.name.clone(),
                });
            }
            cap.validate()?;
        }

        if let Some(schema) = &self.config_schema {
            validate_json_schema(schema).map_err(|reason| {
                DescriptorError::InvalidConfigSchema { reason }
            })?;
        }

        Ok(())
    }

    /// Find a capability by name.
    pub fn capability(&self, name: &str) -> Option<&Capability> {
        self.capabilities.iter().find(|c| c.name == name)
    }
}

/// A single callable operation within a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// Capability name, scoped to the tool.
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// JSON Schema for input validation and LLM function-calling.
    pub input_schema: serde_json::Value,

    /// JSON Schema for output documentation.
    pub output_schema: serde_json::Value,

    /// Default timeout in milliseconds. None = use framework default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,

    /// Whether calling twice with the same input produces the same result.
    #[serde(default)]
    pub idempotent: bool,

    /// Whether this capability modifies external state.
    #[serde(default)]
    pub side_effects: bool,
}

impl Capability {
    fn validate(&self) -> Result<(), DescriptorError> {
        validate_name(&self.name).map_err(|e| match e {
            DescriptorError::InvalidName { name, reason } => {
                DescriptorError::InvalidCapability { name, reason }
            }
            other => other,
        })?;

        if self.description.is_empty() {
            return Err(DescriptorError::InvalidCapability {
                name: self.name.clone(),
                reason: "description must not be empty".into(),
            });
        }

        validate_json_schema(&self.input_schema).map_err(|reason| {
            DescriptorError::InvalidCapability {
                name: self.name.clone(),
                reason: format!("invalid input_schema: {reason}"),
            }
        })?;

        Ok(())
    }
}

/// Validate a tool or capability name: lowercase alphanumeric + underscores, 1-64 chars.
fn validate_name(name: &str) -> Result<(), DescriptorError> {
    if name.is_empty() {
        return Err(DescriptorError::InvalidName {
            name: name.to_string(),
            reason: "name must not be empty".into(),
        });
    }
    if name.len() > 64 {
        return Err(DescriptorError::InvalidName {
            name: name.to_string(),
            reason: format!("name exceeds 64 characters ({})", name.len()),
        });
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err(DescriptorError::InvalidName {
            name: name.to_string(),
            reason: "name must contain only lowercase alphanumeric characters and underscores"
                .into(),
        });
    }
    if name.starts_with('_') || name.ends_with('_') {
        return Err(DescriptorError::InvalidName {
            name: name.to_string(),
            reason: "name must not start or end with underscore".into(),
        });
    }
    Ok(())
}

/// Validate a semver version string (major.minor.patch, each a non-negative integer).
fn validate_version(version: &str) -> Result<(), DescriptorError> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return Err(DescriptorError::InvalidVersion {
            version: version.to_string(),
        });
    }
    for part in &parts {
        if part.is_empty() || part.parse::<u32>().is_err() {
            return Err(DescriptorError::InvalidVersion {
                version: version.to_string(),
            });
        }
    }
    Ok(())
}

/// Check that a JSON value is a plausible JSON Schema (object with "type" field).
fn validate_json_schema(schema: &serde_json::Value) -> Result<(), String> {
    match schema {
        serde_json::Value::Object(map) => {
            if !map.contains_key("type") && !map.contains_key("$ref") && !map.contains_key("oneOf")
                && !map.contains_key("anyOf") && !map.contains_key("allOf")
            {
                return Err(
                    "schema must have 'type', '$ref', 'oneOf', 'anyOf', or 'allOf'".into(),
                );
            }
            Ok(())
        }
        serde_json::Value::Bool(_) => Ok(()), // boolean schemas are valid JSON Schema
        _ => Err("schema must be an object or boolean".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_capability() -> Capability {
        Capability {
            name: "read".into(),
            description: "Read a file".into(),
            input_schema: json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}),
            output_schema: json!({"type": "object", "properties": {"content": {"type": "string"}}}),
            timeout_ms: Some(30_000),
            idempotent: true,
            side_effects: false,
        }
    }

    fn valid_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "read_file".into(),
            version: "1.0.0".into(),
            description: "Read files from the filesystem".into(),
            capabilities: vec![valid_capability()],
            config_schema: None,
            tags: vec!["filesystem".into()],
        }
    }

    // --- Name validation ---

    #[test]
    fn valid_name_accepted() {
        assert!(validate_name("read_file").is_ok());
        assert!(validate_name("a").is_ok());
        assert!(validate_name("tool123").is_ok());
        assert!(validate_name("my_tool_v2").is_ok());
    }

    #[test]
    fn empty_name_rejected() {
        assert!(matches!(
            validate_name(""),
            Err(DescriptorError::InvalidName { .. })
        ));
    }

    #[test]
    fn long_name_rejected() {
        let long = "a".repeat(65);
        assert!(matches!(
            validate_name(&long),
            Err(DescriptorError::InvalidName { .. })
        ));
    }

    #[test]
    fn max_length_name_accepted() {
        let max = "a".repeat(64);
        assert!(validate_name(&max).is_ok());
    }

    #[test]
    fn uppercase_name_rejected() {
        assert!(matches!(
            validate_name("ReadFile"),
            Err(DescriptorError::InvalidName { .. })
        ));
    }

    #[test]
    fn special_chars_rejected() {
        assert!(validate_name("read-file").is_err());
        assert!(validate_name("read.file").is_err());
        assert!(validate_name("read file").is_err());
    }

    #[test]
    fn leading_trailing_underscore_rejected() {
        assert!(validate_name("_read").is_err());
        assert!(validate_name("read_").is_err());
    }

    // --- Version validation ---

    #[test]
    fn valid_version_accepted() {
        assert!(validate_version("1.0.0").is_ok());
        assert!(validate_version("0.1.0").is_ok());
        assert!(validate_version("12.34.56").is_ok());
    }

    #[test]
    fn invalid_version_rejected() {
        assert!(validate_version("1.0").is_err());
        assert!(validate_version("1").is_err());
        assert!(validate_version("1.0.0.0").is_err());
        assert!(validate_version("a.b.c").is_err());
        assert!(validate_version("").is_err());
        assert!(validate_version("1..0").is_err());
    }

    // --- Capability validation ---

    #[test]
    fn valid_capability_accepted() {
        assert!(valid_capability().validate().is_ok());
    }

    #[test]
    fn empty_capability_description_rejected() {
        let mut cap = valid_capability();
        cap.description = String::new();
        assert!(matches!(
            cap.validate(),
            Err(DescriptorError::InvalidCapability { .. })
        ));
    }

    #[test]
    fn invalid_capability_name_rejected() {
        let mut cap = valid_capability();
        cap.name = "Read".into();
        assert!(matches!(
            cap.validate(),
            Err(DescriptorError::InvalidCapability { .. })
        ));
    }

    #[test]
    fn invalid_input_schema_rejected() {
        let mut cap = valid_capability();
        cap.input_schema = json!("not a schema");
        assert!(matches!(
            cap.validate(),
            Err(DescriptorError::InvalidCapability { .. })
        ));
    }

    #[test]
    fn boolean_schema_accepted() {
        let mut cap = valid_capability();
        cap.input_schema = json!(true);
        assert!(cap.validate().is_ok());
    }

    // --- Descriptor validation ---

    #[test]
    fn valid_descriptor_accepted() {
        assert!(valid_descriptor().validate().is_ok());
    }

    #[test]
    fn empty_capabilities_rejected() {
        let mut desc = valid_descriptor();
        desc.capabilities.clear();
        assert!(matches!(
            desc.validate(),
            Err(DescriptorError::NoCapabilities)
        ));
    }

    #[test]
    fn duplicate_capability_names_rejected() {
        let mut desc = valid_descriptor();
        let cap2 = Capability {
            name: "read".into(),
            description: "Read again".into(),
            ..valid_capability()
        };
        desc.capabilities.push(cap2);
        assert!(matches!(
            desc.validate(),
            Err(DescriptorError::DuplicateCapability { .. })
        ));
    }

    #[test]
    fn multiple_unique_capabilities_accepted() {
        let mut desc = valid_descriptor();
        desc.capabilities.push(Capability {
            name: "write".into(),
            description: "Write a file".into(),
            input_schema: json!({"type": "object"}),
            output_schema: json!({"type": "object"}),
            timeout_ms: None,
            idempotent: false,
            side_effects: true,
        });
        assert!(desc.validate().is_ok());
    }

    #[test]
    fn invalid_config_schema_rejected() {
        let mut desc = valid_descriptor();
        desc.config_schema = Some(json!(42));
        assert!(matches!(
            desc.validate(),
            Err(DescriptorError::InvalidConfigSchema { .. })
        ));
    }

    #[test]
    fn valid_config_schema_accepted() {
        let mut desc = valid_descriptor();
        desc.config_schema = Some(json!({"type": "object", "properties": {"api_key": {"type": "string"}}}));
        assert!(desc.validate().is_ok());
    }

    // --- Capability lookup ---

    #[test]
    fn capability_lookup_found() {
        let desc = valid_descriptor();
        assert!(desc.capability("read").is_some());
    }

    #[test]
    fn capability_lookup_not_found() {
        let desc = valid_descriptor();
        assert!(desc.capability("write").is_none());
    }

    // --- Serde roundtrip ---

    #[test]
    fn serde_roundtrip() {
        let desc = valid_descriptor();
        let json = serde_json::to_string(&desc).unwrap();
        let parsed: ToolDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, desc.name);
        assert_eq!(parsed.version, desc.version);
        assert_eq!(parsed.capabilities.len(), desc.capabilities.len());
    }

    #[test]
    fn serde_optional_fields_absent() {
        let json = r#"{
            "name": "test",
            "version": "1.0.0",
            "description": "test tool",
            "capabilities": [{
                "name": "run",
                "description": "run it",
                "input_schema": {"type": "object"},
                "output_schema": {"type": "object"}
            }]
        }"#;
        let desc: ToolDescriptor = serde_json::from_str(json).unwrap();
        assert!(desc.config_schema.is_none());
        assert!(desc.tags.is_empty());
        assert!(desc.capabilities[0].timeout_ms.is_none());
        assert!(!desc.capabilities[0].idempotent);
        assert!(!desc.capabilities[0].side_effects);
    }
}
