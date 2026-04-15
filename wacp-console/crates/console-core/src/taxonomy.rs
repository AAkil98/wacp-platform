//! TaxonomyIndex — in-memory index of roles, tools, types, and verticals.
//!
//! Spec: `wcon-data-model` §6, `wcon-discovery` §3

use std::collections::HashMap;

use console_runtime::{
    CheckpointField, ContextField, ProfileSummary, QualityCriterion, TaskTypeDescriptor,
    ToolPolicy, ToolSummary, WorkflowSummary,
};
use serde::Serialize;

/// The complete taxonomy index — served from memory, rebuilt atomically.
#[derive(Debug, Clone, Default)]
pub struct TaxonomyIndex {
    pub roles: HashMap<String, RoleEntry>,
    pub tools: HashMap<String, ToolEntry>,
    pub envelope_types: HashMap<String, EnvelopeTypeEntry>,
    pub checkpoint_types: HashMap<String, CheckpointTypeEntry>,
    pub verticals: HashMap<String, VerticalEntry>,
}

/// A role in the taxonomy (base or derived).
#[derive(Debug, Clone, Serialize)]
pub struct RoleEntry {
    pub name: String,
    pub base_role: String,
    pub extends: Option<String>,
    pub capabilities_added: Vec<String>,
    pub capabilities_removed: Vec<String>,
    pub tools: Vec<String>,
    pub vertical: Option<String>,
}

/// A tool exposed by a vertical.
#[derive(Debug, Clone, Serialize)]
pub struct ToolEntry {
    pub name: String,
    pub description: String,
    pub vertical: Option<String>,
    pub roles: Vec<String>,
    pub policy: Option<ToolPolicy>,
}

/// A protocol-level envelope type with permission matrix.
#[derive(Debug, Clone, Serialize)]
pub struct EnvelopeTypeEntry {
    pub name: String,
    pub description: String,
    pub sender_roles: Vec<String>,
    pub receiver_roles: Vec<String>,
}

/// A protocol-level checkpoint type.
#[derive(Debug, Clone, Serialize)]
pub struct CheckpointTypeEntry {
    pub name: String,
    pub description: String,
    pub allowed_roles: Vec<String>,
}

/// A vertical with its full manifest projected into the index.
#[derive(Debug, Clone, Serialize)]
pub struct VerticalEntry {
    pub id: String,
    pub name: String,
    pub load_error: Option<String>,
    pub defining_constraint: String,
    pub roles: Vec<String>,
    pub context_schema: HashMap<String, ContextField>,
    pub tool_policies: HashMap<String, ToolPolicy>,
    pub checkpoint_types: HashMap<String, VerticalCheckpointType>,
    pub quality_criteria: Vec<QualityCriterion>,
    pub task_types: Vec<TaskTypeDescriptor>,
    pub workflows: Vec<WorkflowSummary>,
    pub default_profiles: Vec<ProfileSummary>,
    pub tools: Vec<ToolSummary>,
    pub raw_manifest: serde_json::Value,
}

/// A vertical-specific checkpoint type with field schema and required_by cross-ref.
#[derive(Debug, Clone, Serialize)]
pub struct VerticalCheckpointType {
    pub description: String,
    pub fields: Vec<CheckpointField>,
    pub required_by: Vec<String>,
}

impl TaxonomyIndex {
    pub fn get_role(&self, id: &str) -> Option<&RoleEntry> {
        self.roles.get(id)
    }

    pub fn get_tool(&self, name: &str) -> Option<&ToolEntry> {
        self.tools.get(name)
    }

    pub fn get_vertical(&self, id: &str) -> Option<&VerticalEntry> {
        self.verticals.get(id)
    }

    pub fn list_roles(&self, base_role: Option<&str>, vertical: Option<&str>) -> Vec<&RoleEntry> {
        let mut result: Vec<&RoleEntry> = self
            .roles
            .values()
            .filter(|r| {
                base_role.is_none_or(|br| r.base_role == br)
                    && vertical.is_none_or(|v| r.vertical.as_deref() == Some(v))
            })
            .collect();
        result.sort_by_key(|r| &r.name);
        result
    }

    pub fn list_tools(&self, vertical: Option<&str>, has_policy: Option<bool>) -> Vec<&ToolEntry> {
        let mut result: Vec<&ToolEntry> = self
            .tools
            .values()
            .filter(|t| {
                vertical.is_none_or(|v| t.vertical.as_deref() == Some(v))
                    && has_policy.is_none_or(|hp| hp == t.policy.is_some())
            })
            .collect();
        result.sort_by_key(|t| &t.name);
        result
    }

    pub fn list_verticals(&self) -> Vec<&VerticalEntry> {
        let mut result: Vec<&VerticalEntry> = self.verticals.values().collect();
        result.sort_by_key(|v| &v.id);
        result
    }

    pub fn list_envelope_types(&self) -> Vec<&EnvelopeTypeEntry> {
        let mut result: Vec<&EnvelopeTypeEntry> = self.envelope_types.values().collect();
        result.sort_by_key(|e| &e.name);
        result
    }

    pub fn list_checkpoint_types(&self) -> Vec<&CheckpointTypeEntry> {
        let mut result: Vec<&CheckpointTypeEntry> = self.checkpoint_types.values().collect();
        result.sort_by_key(|c| &c.name);
        result
    }
}
