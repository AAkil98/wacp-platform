use crate::*;

const PV: &str = "0.1";

/// The canonical reviewer taxonomy from TAXONOMY.md §8.1.
fn reviewer_yaml() -> &'static str {
    r#"
id: test-taxonomy
version: "1.0"
protocol_version: "0.1"
roles:
  - name: reviewer
    extends: worker
    add:
      - "send: report → coordinator"
      - "read: assigned_workspace"
    remove:
      - "send: query → coordinator"
      - "create: artifact"
    checkpoint_types:
      - review
envelope_types:
  - name: report
    permissions:
      - sender_role: reviewer
        receiver_role: coordinator
checkpoint_types:
  - name: review
    permitted_roles:
      - reviewer
"#
}

fn minimal_yaml() -> String {
    format!(
        r#"
id: minimal
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types: []
checkpoint_types: []
"#
    )
}

#[test]
fn empty_taxonomy_valid() {
    let t = Taxonomy::empty(PV);
    assert!(t.is_valid_role("coordinator"));
    assert!(t.is_valid_role("worker"));
    assert!(t.is_valid_role("observer"));
    assert!(!t.is_valid_role("ghost"));

    assert!(t.is_valid_envelope_type("directive"));
    assert!(t.is_valid_envelope_type("feedback"));
    assert!(t.is_valid_envelope_type("query"));
    assert!(!t.is_valid_envelope_type("report"));

    assert!(t.is_valid_checkpoint_type("artifact"));
    assert!(t.is_valid_checkpoint_type("observation"));
    assert!(!t.is_valid_checkpoint_type("review"));

    assert_eq!(t.envelope_permissions.len(), 3);
}

#[test]
fn load_yaml_reviewer() {
    let t = Taxonomy::load_yaml(reviewer_yaml(), PV).unwrap();

    assert!(t.is_valid_role("reviewer"));
    assert!(t.is_valid_envelope_type("report"));
    assert!(t.is_valid_checkpoint_type("review"));

    let resolved = t.resolve_role("reviewer").unwrap();
    assert_eq!(resolved.base, wacp_types::BaseRole::Worker);
    assert!(resolved.capabilities.contains("send: report → coordinator"));
    assert!(resolved.capabilities.contains("read: assigned_workspace"));
    assert!(!resolved.capabilities.contains("send: query → coordinator"));
    assert!(!resolved.capabilities.contains("create: artifact"));
    assert!(resolved.checkpoint_types.contains("review"));
    assert!(!resolved.checkpoint_types.contains("artifact"));
}

#[test]
fn load_json_roundtrip() {
    let yaml_t = Taxonomy::load_yaml(reviewer_yaml(), PV).unwrap();
    let json_str = serde_json::to_string(&serde_yaml::from_str::<TaxonomyDefinition>(reviewer_yaml()).unwrap()).unwrap();
    let json_t = Taxonomy::load_json(&json_str, PV).unwrap();

    assert_eq!(yaml_t.id, json_t.id);
    assert_eq!(yaml_t.envelope_types, json_t.envelope_types);
    assert_eq!(yaml_t.resolved_roles.len(), json_t.resolved_roles.len());
}

#[test]
fn reject_protocol_version_mismatch() {
    let yaml = minimal_yaml().replace(PV, "99.0");
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::ProtocolVersionMismatch { .. }));
}

#[test]
fn reject_duplicate_role_name() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles:
  - name: reviewer
    extends: worker
  - name: reviewer
    extends: observer
envelope_types: []
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::DuplicateRoleName(n) if n == "reviewer"));
}

#[test]
fn reject_base_role_collision() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles:
  - name: worker
    extends: worker
envelope_types: []
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::BaseNameCollision(n) if n == "worker"));
}

#[test]
fn reject_duplicate_envelope_type() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types:
  - name: report
    permissions:
      - sender_role: worker
        receiver_role: coordinator
  - name: report
    permissions:
      - sender_role: worker
        receiver_role: coordinator
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::DuplicateEnvelopeType(n) if n == "report"));
}

#[test]
fn reject_base_envelope_collision() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types:
  - name: directive
    permissions:
      - sender_role: worker
        receiver_role: coordinator
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::BaseNameCollision(n) if n == "directive"));
}

#[test]
fn reject_duplicate_checkpoint_type() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types: []
checkpoint_types:
  - name: review
    permitted_roles: [worker]
  - name: review
    permitted_roles: [worker]
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::DuplicateCheckpointType(n) if n == "review"));
}

#[test]
fn reject_base_checkpoint_collision() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types: []
checkpoint_types:
  - name: artifact
    permitted_roles: [worker]
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::BaseNameCollision(n) if n == "artifact"));
}

#[test]
fn reject_extends_coordinator() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles:
  - name: superadmin
    extends: coordinator
envelope_types: []
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::InvalidInheritance { .. }));
}

#[test]
fn reject_extends_derived() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles:
  - name: base_custom
    extends: worker
  - name: derived_custom
    extends: base_custom
envelope_types: []
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::InvalidInheritance { .. }));
}

#[test]
fn reject_privilege_escalation() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles:
  - name: sneaky
    extends: worker
    add:
      - create_workspace
envelope_types: []
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::PrivilegeEscalation { .. }));
}

#[test]
fn reject_cross_registry_missing() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles:
  - name: custom
    extends: worker
    checkpoint_types:
      - nonexistent
envelope_types: []
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::CrossRegistryMissing { .. }));
}

#[test]
fn reject_envelope_role_not_found() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types:
  - name: report
    permissions:
      - sender_role: ghost
        receiver_role: coordinator
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::EnvelopeRoleNotFound { .. }));
}

#[test]
fn reject_checkpoint_role_not_found() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types: []
checkpoint_types:
  - name: review
    permitted_roles: [ghost]
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::CheckpointRoleNotFound { .. }));
}

#[test]
fn reject_empty_permissions() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types:
  - name: report
    permissions: []
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::EmptyPermissions(n) if n == "report"));
}

#[test]
fn reject_empty_role_list() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types: []
checkpoint_types:
  - name: review
    permitted_roles: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::EmptyRoleList(n) if n == "review"));
}

#[test]
fn resolve_role_add_remove() {
    let t = Taxonomy::load_yaml(reviewer_yaml(), PV).unwrap();
    let role = t.resolve_role("reviewer").unwrap();

    // Added
    assert!(role.capabilities.contains("send: report → coordinator"));
    assert!(role.capabilities.contains("read: assigned_workspace"));

    // Removed (was in worker base)
    assert!(!role.capabilities.contains("send: query → coordinator"));
    assert!(!role.capabilities.contains("create: artifact"));
}

#[test]
fn is_valid_role_base() {
    let t = Taxonomy::empty(PV);
    assert!(t.is_valid_role("worker"));
    assert!(t.is_valid_role("coordinator"));
    assert!(t.is_valid_role("observer"));
}

#[test]
fn is_valid_role_derived() {
    let t = Taxonomy::load_yaml(reviewer_yaml(), PV).unwrap();
    assert!(t.is_valid_role("reviewer"));
}

#[test]
fn is_valid_role_unknown() {
    let t = Taxonomy::empty(PV);
    assert!(!t.is_valid_role("ghost"));
}

// ── Phase 18a.5: Taxonomy edge cases ──

#[test]
fn reject_malformed_yaml() {
    let yaml = "invalid: [unclosed bracket";
    let err = Taxonomy::load_yaml(yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::ParseError(_)));
}

#[test]
fn reject_malformed_json() {
    let json = r#"{"id": "test", invalid json}"#;
    let err = Taxonomy::load_json(json, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::ParseError(_)));
}

#[test]
fn reject_yaml_missing_required_field() {
    // Missing "id" field.
    let yaml = format!(
        r#"
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types: []
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::ParseError(_)));
}

#[test]
fn reject_json_missing_required_field() {
    let json = format!(
        r#"{{"version": "1.0", "protocol_version": "{PV}", "roles": [], "envelope_types": [], "checkpoint_types": []}}"#
    );
    let err = Taxonomy::load_json(&json, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::ParseError(_)));
}

#[test]
fn reject_base_role_collision_coordinator() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles:
  - name: coordinator
    extends: worker
envelope_types: []
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::BaseNameCollision(n) if n == "coordinator"));
}

#[test]
fn reject_base_role_collision_observer() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles:
  - name: observer
    extends: worker
envelope_types: []
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::BaseNameCollision(n) if n == "observer"));
}

#[test]
fn reject_base_envelope_collision_feedback() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types:
  - name: feedback
    permissions:
      - sender_role: worker
        receiver_role: coordinator
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::BaseNameCollision(n) if n == "feedback"));
}

#[test]
fn reject_base_envelope_collision_query() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types:
  - name: query
    permissions:
      - sender_role: worker
        receiver_role: coordinator
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::BaseNameCollision(n) if n == "query"));
}

#[test]
fn reject_base_checkpoint_collision_observation() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types: []
checkpoint_types:
  - name: observation
    permitted_roles: [observer]
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::BaseNameCollision(n) if n == "observation"));
}

#[test]
fn reject_privilege_escalation_all_coordinator_caps() {
    let coordinator_caps = [
        "create_workspace",
        "perform_integration",
        "manage_budgets",
        "abort_workspace",
        "assign_roles",
    ];
    for cap in coordinator_caps {
        let yaml = format!(
            r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles:
  - name: sneaky
    extends: worker
    add:
      - {cap}
envelope_types: []
checkpoint_types: []
"#
        );
        let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
        assert!(
            matches!(&err, TaxonomyError::PrivilegeEscalation { role, capability }
                if role == "sneaky" && capability.contains(cap)),
            "{cap} should trigger PrivilegeEscalation, got {err:?}"
        );
    }
}

#[test]
fn reject_extends_unknown_base() {
    let yaml = format!(
        r#"
id: test
version: "1.0"
protocol_version: "{PV}"
roles:
  - name: custom
    extends: nonexistent
envelope_types: []
checkpoint_types: []
"#
    );
    let err = Taxonomy::load_yaml(&yaml, PV).unwrap_err();
    assert!(matches!(err, TaxonomyError::InvalidInheritance { .. }));
}

#[test]
fn empty_taxonomy_has_base_types_only() {
    let t = Taxonomy::empty(PV);
    assert_eq!(t.envelope_types.len(), 3);
    assert_eq!(t.checkpoint_types.len(), 2);
    assert_eq!(t.resolved_roles.len(), 0);
    assert_eq!(t.protocol_version, PV);
}

#[test]
fn minimal_yaml_loads_successfully() {
    let t = Taxonomy::load_yaml(&minimal_yaml(), PV).unwrap();
    assert!(t.is_valid_role("coordinator"));
    assert!(t.is_valid_role("worker"));
    assert!(t.is_valid_role("observer"));
    assert!(t.is_valid_envelope_type("directive"));
    assert!(!t.is_valid_role("custom"));
}

#[test]
fn resolve_role_returns_none_for_base() {
    let t = Taxonomy::empty(PV);
    assert!(t.resolve_role("coordinator").is_none());
    assert!(t.resolve_role("worker").is_none());
    assert!(t.resolve_role("observer").is_none());
    assert!(t.resolve_role("nonexistent").is_none());
}

// ── Phase 18b+: Additional taxonomy coverage ──

#[test]
fn is_valid_envelope_type_base() {
    let t = Taxonomy::empty(PV);
    assert!(t.is_valid_envelope_type("directive"));
    assert!(t.is_valid_envelope_type("feedback"));
    assert!(t.is_valid_envelope_type("query"));
}

#[test]
fn is_valid_envelope_type_unknown() {
    let t = Taxonomy::empty(PV);
    assert!(!t.is_valid_envelope_type("foo"));
    assert!(!t.is_valid_envelope_type(""));
    assert!(!t.is_valid_envelope_type("Directive")); // case-sensitive
}

#[test]
fn is_valid_checkpoint_type_base() {
    let t = Taxonomy::empty(PV);
    assert!(t.is_valid_checkpoint_type("artifact"));
    assert!(t.is_valid_checkpoint_type("observation"));
}

#[test]
fn is_valid_checkpoint_type_unknown() {
    let t = Taxonomy::empty(PV);
    assert!(!t.is_valid_checkpoint_type("foo"));
    assert!(!t.is_valid_checkpoint_type(""));
    assert!(!t.is_valid_checkpoint_type("Artifact")); // case-sensitive
}

#[test]
fn empty_taxonomy_version_matches() {
    let t = Taxonomy::empty("0.1");
    assert_eq!(t.protocol_version, "0.1");

    let t2 = Taxonomy::empty("2.0");
    assert_eq!(t2.protocol_version, "2.0");
}

#[test]
fn load_yaml_empty_lists() {
    let yaml = format!(
        r#"
id: empty-lists
version: "1.0"
protocol_version: "{PV}"
roles: []
envelope_types: []
checkpoint_types: []
"#
    );
    let t = Taxonomy::load_yaml(&yaml, PV).unwrap();
    // Should have base types only — no custom additions.
    assert!(t.is_valid_role("coordinator"));
    assert!(t.is_valid_role("worker"));
    assert!(t.is_valid_role("observer"));
    assert!(t.is_valid_envelope_type("directive"));
    assert!(t.is_valid_envelope_type("feedback"));
    assert!(t.is_valid_envelope_type("query"));
    assert!(t.is_valid_checkpoint_type("artifact"));
    assert!(t.is_valid_checkpoint_type("observation"));
    assert_eq!(t.resolved_roles.len(), 0, "no derived roles");
    assert_eq!(t.envelope_permissions.len(), 3, "only 3 base envelope permissions");
}

// ---------------------------------------------------------------------------
// VerticalManifest tests
// ---------------------------------------------------------------------------

use crate::vertical::{FieldType, ToolPolicyKind, VerticalManifest};

fn swe_manifest_yaml() -> &'static str {
    r#"
id: swe
name: Software Engineering
defining_constraint: "DAG validation — every workflow stage declares an explicit dependsOn list."
context_schema: {}
tool_policies: {}
checkpoint_types: {}
quality_criteria:
  - id: correctness
    name: Correctness
    description: Code does what the directive requires.
    weight: 1.0
task_types:
  - id: swe:debug
    name: Debug
    description: Diagnose and fix a defect.
    workflow_id: swe:fix-bug
    keywords:
      - fix
      - bug
workflows:
  - id: swe:fix-bug
    name: Fix Bug
    description: Isolate → patch → verify
    stage_count: 3
    gated_stage_count: 0
profiles:
  - role_id: swe:implementer
    autonomy: gated
tools:
  - name: code_edit
    description: Edit source files.
"#
}

fn finance_manifest_yaml() -> &'static str {
    r#"
id: finance
name: Finance
defining_constraint: "Regulatory pre-check + fiduciary duty."
context_schema:
  jurisdiction:
    type: enum
    required: true
    description: Regulatory jurisdiction.
    enum_values:
      - SEC
      - FINRA
tool_policies:
  trade_execute:
    kind: requires_checkpoint
    description: Requires a prior approved compliance_check.
    checkpoint_type: compliance_check
    matching_field: trade_id
    expires_after_ms: 300000
checkpoint_types:
  compliance_check:
    description: Pre-trade compliance verification.
    fields:
      - name: trade_id
        type: string
        description: Trade identifier.
      - name: status
        type: enum
        description: Compliance decision.
        enum_values:
          - approved
          - rejected
quality_criteria:
  - id: regulatory_compliance
    name: Regulatory Compliance
    description: All regulations cited and checked.
    weight: 1.0
task_types:
  - id: finance:trade
    name: Trade Execution
    description: Execute a buy or sell order.
    workflow_id: finance:trade-execution
    keywords:
      - buy
      - sell
workflows:
  - id: finance:trade-execution
    name: Trade Execution
    description: Analyze → compliance → execute → record
    stage_count: 4
    gated_stage_count: 2
profiles:
  - role_id: finance:analyst
    autonomy: gated
tools:
  - name: trade_execute
    description: Execute a trade.
"#
}

#[test]
fn vertical_manifest_swe_roundtrip() {
    let manifest = VerticalManifest::load_yaml(swe_manifest_yaml()).expect("parse");
    assert_eq!(manifest.id, "swe");
    assert_eq!(manifest.name, "Software Engineering");
    assert!(manifest.context_schema.is_empty(), "SWE has no context requirements");
    assert!(manifest.tool_policies.is_empty(), "SWE has no tool policies");
    assert!(manifest.checkpoint_types.is_empty(), "SWE has no custom checkpoint types");
    assert_eq!(manifest.quality_criteria.len(), 1);
    assert_eq!(manifest.quality_criteria[0].id, "correctness");
    assert_eq!(manifest.task_types.len(), 1);
    assert_eq!(manifest.task_types[0].id, "swe:debug");
    assert_eq!(manifest.task_types[0].workflow_id, "swe:fix-bug");
    assert_eq!(manifest.task_types[0].keywords, vec!["fix", "bug"]);
    assert_eq!(manifest.workflows.len(), 1);
    assert_eq!(manifest.workflows[0].stage_count, 3);
    assert_eq!(manifest.workflows[0].gated_stage_count, 0);
}

#[test]
fn vertical_manifest_finance_roundtrip() {
    let manifest = VerticalManifest::load_yaml(finance_manifest_yaml()).expect("parse");
    assert_eq!(manifest.id, "finance");

    // context_schema
    let jurisdiction = manifest.context_schema.get("jurisdiction").expect("jurisdiction field");
    assert_eq!(jurisdiction.field_type, FieldType::Enum);
    assert!(jurisdiction.required);
    let enum_vals = jurisdiction.enum_values.as_ref().expect("enum_values");
    assert!(enum_vals.contains(&"SEC".to_string()));

    // tool_policies
    let policy = manifest.tool_policies.get("trade_execute").expect("trade_execute policy");
    assert_eq!(policy.kind, ToolPolicyKind::RequiresCheckpoint);
    assert_eq!(policy.checkpoint_type.as_deref(), Some("compliance_check"));
    assert_eq!(policy.matching_field.as_deref(), Some("trade_id"));
    assert_eq!(policy.expires_after_ms, Some(300_000));

    // checkpoint_types
    let ct = manifest.checkpoint_types.get("compliance_check").expect("compliance_check type");
    assert_eq!(ct.fields.len(), 2);
    let status_field = ct.fields.iter().find(|f| f.name == "status").expect("status field");
    assert_eq!(status_field.field_type, FieldType::Enum);
    let status_vals = status_field.enum_values.as_ref().expect("status enum_values");
    assert!(status_vals.contains(&"approved".to_string()));

    // quality_criteria
    assert_eq!(manifest.quality_criteria.len(), 1);
    assert!((manifest.quality_criteria[0].weight - 1.0).abs() < f64::EPSILON);
}

#[test]
fn vertical_manifest_malformed_yaml_error() {
    let bad = "id: [not a string\nbroken yaml: {{{";
    let err = VerticalManifest::load_yaml(bad).unwrap_err();
    assert!(matches!(err, TaxonomyError::ParseError(_)));
}

#[test]
fn vertical_manifest_extra_fields_tolerated() {
    // Future manifests may add fields; existing parser must not reject them.
    let yaml = r#"
id: test
name: Test
defining_constraint: "none"
future_field_not_yet_known: 42
another_future: [a, b, c]
context_schema: {}
tool_policies: {}
checkpoint_types: {}
"#;
    let manifest = VerticalManifest::load_yaml(yaml).expect("should tolerate extra fields");
    assert_eq!(manifest.id, "test");
}
