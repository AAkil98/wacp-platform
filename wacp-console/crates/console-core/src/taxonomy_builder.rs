//! TaxonomyIndex builder — constructs the index from protocol taxonomy + vertical manifests.
//!
//! Spec: `wcon-discovery` §3

use std::collections::HashMap;

use console_runtime::{
    BaseRole, Taxonomy, ToolPolicyKind, VerticalManifest,
};
use tracing::{info, warn};

use crate::taxonomy::{
    CheckpointTypeEntry, EnvelopeTypeEntry, RoleEntry, TaxonomyIndex, ToolEntry, VerticalEntry,
    VerticalCheckpointType,
};

/// Result of building the taxonomy index.
pub struct BuildResult {
    pub index: TaxonomyIndex,
    pub warnings: Vec<String>,
}

/// Build a TaxonomyIndex from a resolved protocol taxonomy and a set of vertical manifests.
///
/// `failed_verticals` contains (id, error_message) for verticals whose manifest fetch failed.
pub fn build_index(
    taxonomy: Option<&Taxonomy>,
    manifests: &[VerticalManifest],
    failed_verticals: &[(String, String)],
) -> BuildResult {
    let mut index = TaxonomyIndex::default();
    let mut warnings = Vec::new();

    // Step 1: Base roles — hardcoded protocol constants.
    insert_base_roles(&mut index);

    // Step 2: Protocol taxonomy — derived roles, envelope types, checkpoint types.
    if let Some(tax) = taxonomy {
        insert_protocol_taxonomy(&mut index, tax);
    }

    // Step 3: Vertical manifests — project each manifest into the index.
    for manifest in manifests {
        insert_vertical(&mut index, manifest, &mut warnings);
    }

    // Step 3b: Failed vertical stubs.
    for (id, error) in failed_verticals {
        insert_failed_vertical(&mut index, id, error);
    }

    info!(
        roles = index.roles.len(),
        tools = index.tools.len(),
        verticals = index.verticals.len(),
        envelope_types = index.envelope_types.len(),
        checkpoint_types = index.checkpoint_types.len(),
        warnings = warnings.len(),
        "taxonomy index built"
    );

    BuildResult { index, warnings }
}

fn insert_base_roles(index: &mut TaxonomyIndex) {
    // Coordinator
    index.roles.insert(
        "coordinator".to_string(),
        RoleEntry {
            name: "coordinator".to_string(),
            base_role: "coordinator".to_string(),
            extends: None,
            capabilities_added: vec![],
            capabilities_removed: vec![],
            tools: vec![],
            vertical: None,
        },
    );

    // Worker
    index.roles.insert(
        "worker".to_string(),
        RoleEntry {
            name: "worker".to_string(),
            base_role: "worker".to_string(),
            extends: None,
            capabilities_added: vec![
                "send: query → coordinator".to_string(),
                "create: artifact".to_string(),
            ],
            capabilities_removed: vec![],
            tools: vec![],
            vertical: None,
        },
    );

    // Observer
    index.roles.insert(
        "observer".to_string(),
        RoleEntry {
            name: "observer".to_string(),
            base_role: "observer".to_string(),
            extends: None,
            capabilities_added: vec!["create: observation".to_string()],
            capabilities_removed: vec![],
            tools: vec![],
            vertical: None,
        },
    );

    // Base envelope types
    for (name, desc, senders, receivers) in [
        (
            "directive",
            "Task assignment from coordinator to worker",
            vec!["coordinator"],
            vec!["worker"],
        ),
        (
            "feedback",
            "Status report from worker to coordinator",
            vec!["worker"],
            vec!["coordinator"],
        ),
        (
            "query",
            "Information request from worker to coordinator",
            vec!["worker"],
            vec!["coordinator"],
        ),
    ] {
        index.envelope_types.insert(
            name.to_string(),
            EnvelopeTypeEntry {
                name: name.to_string(),
                description: desc.to_string(),
                sender_roles: senders.into_iter().map(String::from).collect(),
                receiver_roles: receivers.into_iter().map(String::from).collect(),
            },
        );
    }

    // Base checkpoint types
    for (name, desc, roles) in [
        (
            "artifact",
            "Work product created by a worker",
            vec!["worker"],
        ),
        (
            "observation",
            "Monitoring record created by an observer",
            vec!["observer"],
        ),
    ] {
        index.checkpoint_types.insert(
            name.to_string(),
            CheckpointTypeEntry {
                name: name.to_string(),
                description: desc.to_string(),
                allowed_roles: roles.into_iter().map(String::from).collect(),
            },
        );
    }
}

fn insert_protocol_taxonomy(index: &mut TaxonomyIndex, taxonomy: &Taxonomy) {
    // Derived roles from the resolved taxonomy.
    for (name, resolved) in &taxonomy.resolved_roles {
        let base_role_str = match resolved.base {
            BaseRole::Coordinator => "coordinator",
            BaseRole::Worker => "worker",
            BaseRole::Observer => "observer",
        };

        // Compute capabilities added/removed by diffing with the base.
        let caps_added: Vec<String> = resolved.capabilities.iter().cloned().collect();

        index.roles.insert(
            name.clone(),
            RoleEntry {
                name: name.clone(),
                base_role: base_role_str.to_string(),
                extends: Some(base_role_str.to_string()),
                capabilities_added: caps_added,
                capabilities_removed: vec![],
                tools: vec![],
                vertical: None,
            },
        );
    }

    // Custom envelope types from envelope_permissions.
    // Group permissions by envelope type name.
    let mut envelope_map: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();
    for perm in &taxonomy.envelope_permissions {
        let entry = envelope_map
            .entry(perm.envelope_type.clone())
            .or_insert_with(|| (vec![], vec![]));
        if !entry.0.contains(&perm.sender_role) {
            entry.0.push(perm.sender_role.clone());
        }
        if !entry.1.contains(&perm.receiver_role) {
            entry.1.push(perm.receiver_role.clone());
        }
    }
    for (name, (senders, receivers)) in envelope_map {
        // Only insert if not already a base envelope type
        index.envelope_types.entry(name.clone()).or_insert_with(|| {
            EnvelopeTypeEntry {
                name,
                description: String::new(),
                sender_roles: senders,
                receiver_roles: receivers,
            }
        });
    }

    // Custom checkpoint types from taxonomy.
    for (name, permitted_roles) in &taxonomy.checkpoint_types {
        index
            .checkpoint_types
            .entry(name.clone())
            .or_insert_with(|| CheckpointTypeEntry {
                name: name.clone(),
                description: String::new(),
                allowed_roles: permitted_roles.iter().cloned().collect(),
            });
    }
}

fn insert_vertical(
    index: &mut TaxonomyIndex,
    manifest: &VerticalManifest,
    warnings: &mut Vec<String>,
) {
    let vertical_id = &manifest.id;

    // Synthesize roles from profiles[].role_id (deduplicated, sorted).
    let mut role_ids: Vec<String> = manifest
        .profiles
        .iter()
        .map(|p| p.role_id.clone())
        .collect();
    role_ids.sort();
    role_ids.dedup();

    // Register vertical roles in the global roles map.
    let all_tool_names: Vec<String> = manifest.tools.iter().map(|t| t.name.clone()).collect();
    for role_id in &role_ids {
        let qualified = format!("{vertical_id}:{role_id}");
        index.roles.insert(
            qualified.clone(),
            RoleEntry {
                name: qualified,
                base_role: "worker".to_string(), // vertical roles extend worker by convention
                extends: Some("worker".to_string()),
                capabilities_added: vec![],
                capabilities_removed: vec![],
                tools: all_tool_names.clone(),
                vertical: Some(vertical_id.clone()),
            },
        );
    }

    // Register tools in the global tools map.
    let all_role_names: Vec<String> = role_ids
        .iter()
        .map(|r| format!("{vertical_id}:{r}"))
        .collect();
    for tool in &manifest.tools {
        let policy = manifest.tool_policies.get(&tool.name).cloned();
        index.tools.insert(
            tool.name.clone(),
            ToolEntry {
                name: tool.name.clone(),
                description: tool.description.clone(),
                vertical: Some(vertical_id.clone()),
                roles: all_role_names.clone(),
                policy,
            },
        );
    }

    // Build vertical checkpoint types with required_by cross-references.
    let mut v_checkpoint_types: HashMap<String, VerticalCheckpointType> = HashMap::new();
    for (cp_name, cp_schema) in &manifest.checkpoint_types {
        v_checkpoint_types.insert(
            cp_name.clone(),
            VerticalCheckpointType {
                description: cp_schema.description.clone(),
                fields: cp_schema.fields.clone(),
                required_by: vec![],
            },
        );
    }

    // Cross-link: tool policies → checkpoint required_by.
    for (tool_name, policy) in &manifest.tool_policies {
        if matches!(policy.kind, ToolPolicyKind::RequiresCheckpoint)
            && let Some(ref cp_type) = policy.checkpoint_type
        {
            if let Some(cp) = v_checkpoint_types.get_mut(cp_type) {
                cp.required_by.push(tool_name.clone());
            } else {
                warnings.push(format!(
                    "vertical '{vertical_id}': tool '{tool_name}' references checkpoint type '{cp_type}' \
                     not declared in this vertical's checkpoint_types"
                ));
            }
        }
    }

    // Sort required_by for determinism.
    for cp in v_checkpoint_types.values_mut() {
        cp.required_by.sort();
    }

    // Serialize the manifest as raw JSON for forward-compatibility.
    let raw_manifest = serde_json::to_value(manifest).unwrap_or_default();

    let qualified_roles: Vec<String> = role_ids
        .iter()
        .map(|r| format!("{vertical_id}:{r}"))
        .collect();

    index.verticals.insert(
        vertical_id.clone(),
        VerticalEntry {
            id: vertical_id.clone(),
            name: manifest.name.clone(),
            load_error: None,
            defining_constraint: manifest.defining_constraint.clone(),
            roles: qualified_roles,
            context_schema: manifest.context_schema.clone(),
            tool_policies: manifest.tool_policies.clone(),
            checkpoint_types: v_checkpoint_types,
            quality_criteria: manifest.quality_criteria.clone(),
            task_types: manifest.task_types.clone(),
            workflows: manifest.workflows.clone(),
            default_profiles: manifest.profiles.clone(),
            tools: manifest.tools.clone(),
            raw_manifest,
        },
    );
}

fn insert_failed_vertical(index: &mut TaxonomyIndex, id: &str, error: &str) {
    warn!(vertical_id = id, error, "vertical manifest fetch failed — creating stub entry");

    index.verticals.insert(
        id.to_string(),
        VerticalEntry {
            id: id.to_string(),
            name: id.to_string(),
            load_error: Some(error.to_string()),
            defining_constraint: String::new(),
            roles: vec![],
            context_schema: HashMap::new(),
            tool_policies: HashMap::new(),
            checkpoint_types: HashMap::new(),
            quality_criteria: vec![],
            task_types: vec![],
            workflows: vec![],
            default_profiles: vec![],
            tools: vec![],
            raw_manifest: serde_json::Value::Null,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use console_runtime::{
        CheckpointSchema, ProfileSummary, ToolPolicy, ToolPolicyKind, ToolSummary,
    };

    fn simple_manifest() -> VerticalManifest {
        VerticalManifest {
            id: "swe".to_string(),
            name: "Software Engineering".to_string(),
            defining_constraint: "Code quality gates".to_string(),
            context_schema: HashMap::new(),
            tool_policies: HashMap::new(),
            checkpoint_types: HashMap::new(),
            quality_criteria: vec![],
            task_types: vec![],
            workflows: vec![],
            profiles: vec![
                ProfileSummary {
                    role_id: "implementer".to_string(),
                    autonomy: "autonomous".to_string(),
                },
                ProfileSummary {
                    role_id: "reviewer".to_string(),
                    autonomy: "gated".to_string(),
                },
            ],
            tools: vec![
                ToolSummary {
                    name: "code_edit".to_string(),
                    description: "Edit source files".to_string(),
                },
                ToolSummary {
                    name: "test_run".to_string(),
                    description: "Run tests".to_string(),
                },
            ],
        }
    }

    #[test]
    fn base_roles_always_present() {
        let result = build_index(None, &[], &[]);
        assert!(result.index.roles.contains_key("coordinator"));
        assert!(result.index.roles.contains_key("worker"));
        assert!(result.index.roles.contains_key("observer"));
        assert_eq!(result.index.roles.len(), 3);
    }

    #[test]
    fn base_envelope_types_present() {
        let result = build_index(None, &[], &[]);
        assert!(result.index.envelope_types.contains_key("directive"));
        assert!(result.index.envelope_types.contains_key("feedback"));
        assert!(result.index.envelope_types.contains_key("query"));
    }

    #[test]
    fn base_checkpoint_types_present() {
        let result = build_index(None, &[], &[]);
        assert!(result.index.checkpoint_types.contains_key("artifact"));
        assert!(result.index.checkpoint_types.contains_key("observation"));
    }

    #[test]
    fn vertical_manifest_ingestion() {
        let manifest = simple_manifest();
        let result = build_index(None, &[manifest], &[]);

        // Vertical entry
        let v = result.index.verticals.get("swe").unwrap();
        assert_eq!(v.name, "Software Engineering");
        assert_eq!(v.roles.len(), 2);
        assert!(v.roles.contains(&"swe:implementer".to_string()));
        assert!(v.roles.contains(&"swe:reviewer".to_string()));

        // Roles registered globally
        assert!(result.index.roles.contains_key("swe:implementer"));
        assert!(result.index.roles.contains_key("swe:reviewer"));
        let impl_role = result.index.roles.get("swe:implementer").unwrap();
        assert_eq!(impl_role.vertical.as_deref(), Some("swe"));
        assert_eq!(impl_role.tools.len(), 2);

        // Tools registered globally
        assert!(result.index.tools.contains_key("code_edit"));
        let tool = result.index.tools.get("code_edit").unwrap();
        assert_eq!(tool.vertical.as_deref(), Some("swe"));
        assert_eq!(tool.roles.len(), 2);
    }

    #[test]
    fn failed_vertical_creates_stub() {
        let result = build_index(
            None,
            &[],
            &[("broken".to_string(), "connection refused".to_string())],
        );
        let v = result.index.verticals.get("broken").unwrap();
        assert_eq!(v.load_error.as_deref(), Some("connection refused"));
        assert!(v.roles.is_empty());
        assert!(v.tools.is_empty());
    }

    #[test]
    fn tool_policy_cross_references() {
        let mut manifest = simple_manifest();
        manifest.checkpoint_types.insert(
            "code_review".to_string(),
            CheckpointSchema {
                description: "Code review checkpoint".to_string(),
                fields: vec![],
            },
        );
        manifest.tool_policies.insert(
            "code_edit".to_string(),
            ToolPolicy {
                kind: ToolPolicyKind::RequiresCheckpoint,
                description: "Needs review".to_string(),
                checkpoint_type: Some("code_review".to_string()),
                matching_field: None,
                expires_after_ms: None,
                gate_condition: None,
                budget_field: None,
                blocked_classifications: None,
                override_flag: None,
            },
        );

        let result = build_index(None, &[manifest], &[]);
        let v = result.index.verticals.get("swe").unwrap();
        let cp = v.checkpoint_types.get("code_review").unwrap();
        assert!(cp.required_by.contains(&"code_edit".to_string()));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn unresolved_checkpoint_reference_warns() {
        let mut manifest = simple_manifest();
        manifest.tool_policies.insert(
            "code_edit".to_string(),
            ToolPolicy {
                kind: ToolPolicyKind::RequiresCheckpoint,
                description: "Needs review".to_string(),
                checkpoint_type: Some("nonexistent".to_string()),
                matching_field: None,
                expires_after_ms: None,
                gate_condition: None,
                budget_field: None,
                blocked_classifications: None,
                override_flag: None,
            },
        );

        let result = build_index(None, &[manifest], &[]);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("nonexistent"));
    }

    #[test]
    fn deterministic_role_order() {
        let result = build_index(None, &[simple_manifest()], &[]);
        let roles = result.index.list_roles(None, None);
        let names: Vec<&str> = roles.iter().map(|r| r.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }
}
