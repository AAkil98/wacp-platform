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
