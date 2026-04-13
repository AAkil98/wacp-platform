//! WACP — Vertical manifest types and loader.
//!
//! A `VerticalManifest` is loaded from a `vertical.yaml` file produced by the
//! `generate-manifests` script in the ecosystem packages. It carries all domain
//! metadata the Console (and any other consumer) needs: context schema, tool
//! policies, checkpoint schemas, quality criteria, task types, and summaries of
//! workflows / profiles / tools.
//!
//! This is distinct from `TaxonomyDefinition`, which describes protocol-level
//! extensions (derived roles, custom envelope/checkpoint types). A vertical
//! manifest is display/config metadata; a taxonomy definition is a protocol
//! contract.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::TaxonomyError;

// ---------------------------------------------------------------------------
// Field type enum
// ---------------------------------------------------------------------------

/// Data type for context fields and checkpoint fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Enum,
}

// ---------------------------------------------------------------------------
// Context schema
// ---------------------------------------------------------------------------

/// A typed field in a vertical's session-launch context schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextField {
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub required: bool,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Tool policies
// ---------------------------------------------------------------------------

/// Discriminant for tool-layer enforcement rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPolicyKind {
    RequiresCheckpoint,
    RequiresGate,
    BudgetLimited,
    ClassificationGated,
}

/// A tool-layer policy rule — describes how a vertical enforces a pre-condition
/// before allowing a tool to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPolicy {
    pub kind: ToolPolicyKind,
    pub description: String,
    // requires_checkpoint
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matching_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_after_ms: Option<u64>,
    // requires_gate
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_condition: Option<String>,
    // budget_limited
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_field: Option<String>,
    // classification_gated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_classifications: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_flag: Option<String>,
}

// ---------------------------------------------------------------------------
// Checkpoint schemas
// ---------------------------------------------------------------------------

/// A field within a vertical-specific checkpoint type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

/// Schema for a vertical-specific checkpoint type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointSchema {
    pub description: String,
    pub fields: Vec<CheckpointField>,
}

// ---------------------------------------------------------------------------
// Quality criteria
// ---------------------------------------------------------------------------

/// A quality criterion — serialisable view of a quality dimension (no eval fn).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityCriterion {
    pub id: String,
    pub name: String,
    pub description: String,
    pub weight: f64,
}

// ---------------------------------------------------------------------------
// Task types
// ---------------------------------------------------------------------------

/// Declarative task type descriptor with representative detection keywords.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTypeDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub workflow_id: String,
    pub keywords: Vec<String>,
}

// ---------------------------------------------------------------------------
// Summary types (for workflow / profile / tool listings)
// ---------------------------------------------------------------------------

/// Summary of a vertical's workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub stage_count: u32,
    pub gated_stage_count: u32,
}

/// Summary of a vertical's agent profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSummary {
    pub role_id: String,
    pub autonomy: String,
}

/// Summary of a vertical's tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSummary {
    pub name: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// VerticalManifest
// ---------------------------------------------------------------------------

/// A complete vertical manifest — loaded from `vertical.yaml`.
///
/// Contains all domain metadata needed to configure and operate a session:
/// context schema, tool policies, checkpoint schemas, quality criteria,
/// task types, and summaries of workflows, profiles, and tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerticalManifest {
    pub id: String,
    pub name: String,
    pub defining_constraint: String,
    #[serde(default)]
    pub context_schema: HashMap<String, ContextField>,
    #[serde(default)]
    pub tool_policies: HashMap<String, ToolPolicy>,
    #[serde(default)]
    pub checkpoint_types: HashMap<String, CheckpointSchema>,
    #[serde(default)]
    pub quality_criteria: Vec<QualityCriterion>,
    #[serde(default)]
    pub task_types: Vec<TaskTypeDescriptor>,
    #[serde(default)]
    pub workflows: Vec<WorkflowSummary>,
    #[serde(default)]
    pub profiles: Vec<ProfileSummary>,
    #[serde(default)]
    pub tools: Vec<ToolSummary>,
}

impl VerticalManifest {
    /// Parse a `VerticalManifest` from YAML. Unknown fields are tolerated
    /// (`#[serde(default)]` on every collection) for forward-compatibility.
    pub fn load_yaml(yaml: &str) -> Result<Self, TaxonomyError> {
        serde_yaml::from_str(yaml).map_err(|e| TaxonomyError::ParseError(e.to_string()))
    }
}
