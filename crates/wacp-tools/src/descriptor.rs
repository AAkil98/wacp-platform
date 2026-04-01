use serde::{Deserialize, Serialize};

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
