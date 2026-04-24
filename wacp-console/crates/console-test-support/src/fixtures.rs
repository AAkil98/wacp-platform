//! Fixture vertical manifests for testing.
//!
//! Built from the real `wacp_taxonomy::VerticalManifest` type:
//! - `fixture-simple`: SWE-like baseline — no context schema, no tool policies.
//! - `fixture-complex`: Finance-like — required context fields, tool policies,
//!   checkpoint types, quality criteria.
//! - `fixture-evolution` trio (v1, v2_breaking, v2_additive): a paired
//!   baseline + two evolved variants used by schema-evolution integration
//!   tests (`session_lifecycle_with_schema_change`). All three share
//!   `id = "fixture-evolution"` so they hot-swap in the mock REST fixture
//!   in place; only `context_schema` differs across the three.

use std::collections::HashMap;
use wacp_taxonomy::VerticalManifest;
use wacp_taxonomy::{
    CheckpointField, CheckpointSchema, ContextField, FieldType, ProfileSummary, QualityCriterion,
    TaskTypeDescriptor, ToolPolicy, ToolPolicyKind, ToolSummary, WorkflowSummary,
};

/// SWE-like vertical: minimal configuration, no policies, no required context.
pub fn fixture_simple() -> VerticalManifest {
    VerticalManifest {
        id: "fixture-simple".into(),
        name: "Fixture Simple (SWE)".into(),
        defining_constraint: "Software engineering tasks with standard tooling".into(),
        context_schema: HashMap::new(),
        tool_policies: HashMap::new(),
        checkpoint_types: HashMap::new(),
        quality_criteria: vec![],
        task_types: vec![
            TaskTypeDescriptor {
                id: "implementation".into(),
                name: "Implementation".into(),
                description: "Write or modify source code".into(),
                workflow_id: "standard-dev".into(),
                keywords: vec!["code".into(), "implement".into(), "build".into()],
            },
            TaskTypeDescriptor {
                id: "bug-fix".into(),
                name: "Bug Fix".into(),
                description: "Diagnose and fix a defect".into(),
                workflow_id: "standard-dev".into(),
                keywords: vec!["bug".into(), "fix".into(), "defect".into()],
            },
        ],
        workflows: vec![WorkflowSummary {
            id: "standard-dev".into(),
            name: "Standard Development".into(),
            description: "Plan → implement → test → review".into(),
            stage_count: 4,
            gated_stage_count: 1,
        }],
        profiles: vec![
            ProfileSummary {
                role_id: "developer".into(),
                autonomy: "autonomous".into(),
            },
            ProfileSummary {
                role_id: "reviewer".into(),
                autonomy: "assisted".into(),
            },
        ],
        tools: vec![
            ToolSummary {
                name: "file_read".into(),
                description: "Read file contents".into(),
            },
            ToolSummary {
                name: "file_write".into(),
                description: "Write file contents".into(),
            },
            ToolSummary {
                name: "shell_exec".into(),
                description: "Execute shell commands".into(),
            },
            ToolSummary {
                name: "search".into(),
                description: "Search codebase".into(),
            },
        ],
    }
}

/// Finance-like vertical: required context fields, 4 tool policies,
/// 3 checkpoint types, quality criteria.
pub fn fixture_complex() -> VerticalManifest {
    VerticalManifest {
        id: "fixture-complex".into(),
        name: "Fixture Complex (Finance)".into(),
        defining_constraint: "Financial data processing with regulatory compliance".into(),
        context_schema: {
            let mut schema = HashMap::new();
            schema.insert(
                "portfolio_id".into(),
                ContextField {
                    field_type: FieldType::String,
                    required: true,
                    description: "Target portfolio identifier".into(),
                    enum_values: None,
                    default: None,
                },
            );
            schema.insert(
                "risk_level".into(),
                ContextField {
                    field_type: FieldType::Enum,
                    required: true,
                    description: "Risk classification for this session".into(),
                    enum_values: Some(vec![
                        "low".into(),
                        "medium".into(),
                        "high".into(),
                        "critical".into(),
                    ]),
                    default: Some(serde_json::Value::String("medium".into())),
                },
            );
            schema.insert(
                "dry_run".into(),
                ContextField {
                    field_type: FieldType::Boolean,
                    required: false,
                    description: "If true, no mutations are committed".into(),
                    enum_values: None,
                    default: Some(serde_json::Value::Bool(true)),
                },
            );
            schema.insert(
                "max_trade_value".into(),
                ContextField {
                    field_type: FieldType::Number,
                    required: false,
                    description: "Maximum single trade value in USD".into(),
                    enum_values: None,
                    default: Some(serde_json::json!(100_000)),
                },
            );
            schema
        },
        tool_policies: {
            let mut policies = HashMap::new();
            policies.insert(
                "trade_execute".into(),
                ToolPolicy {
                    kind: ToolPolicyKind::RequiresGate,
                    description: "All trade executions require human approval".into(),
                    checkpoint_type: None,
                    matching_field: None,
                    expires_after_ms: None,
                    gate_condition: Some("always".into()),
                    budget_field: None,
                    blocked_classifications: None,
                    override_flag: None,
                },
            );
            policies.insert(
                "data_export".into(),
                ToolPolicy {
                    kind: ToolPolicyKind::RequiresCheckpoint,
                    description: "Data exports must record a compliance checkpoint".into(),
                    checkpoint_type: Some("compliance-review".into()),
                    matching_field: None,
                    expires_after_ms: None,
                    gate_condition: None,
                    budget_field: None,
                    blocked_classifications: None,
                    override_flag: None,
                },
            );
            policies.insert(
                "market_query".into(),
                ToolPolicy {
                    kind: ToolPolicyKind::BudgetLimited,
                    description: "Market data queries consume API budget".into(),
                    checkpoint_type: None,
                    matching_field: None,
                    expires_after_ms: None,
                    gate_condition: None,
                    budget_field: Some("api_calls".into()),
                    blocked_classifications: None,
                    override_flag: None,
                },
            );
            policies.insert(
                "pii_access".into(),
                ToolPolicy {
                    kind: ToolPolicyKind::ClassificationGated,
                    description: "PII access blocked unless classification allows".into(),
                    checkpoint_type: None,
                    matching_field: None,
                    expires_after_ms: None,
                    gate_condition: None,
                    budget_field: None,
                    blocked_classifications: Some(vec!["public".into(), "internal".into()]),
                    override_flag: Some("pii_authorized".into()),
                },
            );
            policies
        },
        checkpoint_types: {
            let mut types = HashMap::new();
            types.insert(
                "compliance-review".into(),
                CheckpointSchema {
                    description: "Compliance review before data export".into(),
                    fields: vec![
                        CheckpointField {
                            name: "reviewer".into(),
                            field_type: FieldType::String,
                            description: "Who reviewed".into(),
                            enum_values: None,
                        },
                        CheckpointField {
                            name: "approved".into(),
                            field_type: FieldType::Boolean,
                            description: "Whether approved".into(),
                            enum_values: None,
                        },
                    ],
                },
            );
            types.insert(
                "risk-assessment".into(),
                CheckpointSchema {
                    description: "Risk assessment checkpoint".into(),
                    fields: vec![
                        CheckpointField {
                            name: "risk_score".into(),
                            field_type: FieldType::Number,
                            description: "Computed risk score (0-100)".into(),
                            enum_values: None,
                        },
                        CheckpointField {
                            name: "category".into(),
                            field_type: FieldType::Enum,
                            description: "Risk category".into(),
                            enum_values: Some(vec!["low".into(), "medium".into(), "high".into()]),
                        },
                    ],
                },
            );
            types.insert(
                "audit-trail".into(),
                CheckpointSchema {
                    description: "Audit trail entry for regulatory compliance".into(),
                    fields: vec![CheckpointField {
                        name: "action_summary".into(),
                        field_type: FieldType::String,
                        description: "Summary of the auditable action".into(),
                        enum_values: None,
                    }],
                },
            );
            types
        },
        quality_criteria: vec![
            QualityCriterion {
                id: "regulatory-compliance".into(),
                name: "Regulatory Compliance".into(),
                description: "All outputs meet financial regulatory requirements".into(),
                weight: 0.4,
            },
            QualityCriterion {
                id: "data-accuracy".into(),
                name: "Data Accuracy".into(),
                description: "Numerical results are verified against source data".into(),
                weight: 0.35,
            },
            QualityCriterion {
                id: "audit-completeness".into(),
                name: "Audit Completeness".into(),
                description: "All material actions have audit trail entries".into(),
                weight: 0.25,
            },
        ],
        task_types: vec![
            TaskTypeDescriptor {
                id: "portfolio-analysis".into(),
                name: "Portfolio Analysis".into(),
                description: "Analyze portfolio performance and risk".into(),
                workflow_id: "analysis-review".into(),
                keywords: vec!["portfolio".into(), "analysis".into(), "risk".into()],
            },
            TaskTypeDescriptor {
                id: "trade-execution".into(),
                name: "Trade Execution".into(),
                description: "Execute trades with compliance checks".into(),
                workflow_id: "gated-execution".into(),
                keywords: vec!["trade".into(), "execute".into(), "order".into()],
            },
        ],
        workflows: vec![
            WorkflowSummary {
                id: "analysis-review".into(),
                name: "Analysis & Review".into(),
                description: "Gather data → analyze → checkpoint → report".into(),
                stage_count: 4,
                gated_stage_count: 2,
            },
            WorkflowSummary {
                id: "gated-execution".into(),
                name: "Gated Execution".into(),
                description: "Validate → gate → execute → audit".into(),
                stage_count: 4,
                gated_stage_count: 3,
            },
        ],
        profiles: vec![
            ProfileSummary {
                role_id: "analyst".into(),
                autonomy: "assisted".into(),
            },
            ProfileSummary {
                role_id: "trader".into(),
                autonomy: "supervised".into(),
            },
            ProfileSummary {
                role_id: "compliance-officer".into(),
                autonomy: "supervised".into(),
            },
        ],
        tools: vec![
            ToolSummary {
                name: "trade_execute".into(),
                description: "Execute a trade order".into(),
            },
            ToolSummary {
                name: "data_export".into(),
                description: "Export data to external systems".into(),
            },
            ToolSummary {
                name: "market_query".into(),
                description: "Query market data feeds".into(),
            },
            ToolSummary {
                name: "pii_access".into(),
                description: "Access personally identifiable information".into(),
            },
            ToolSummary {
                name: "portfolio_read".into(),
                description: "Read portfolio state".into(),
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Schema-evolution fixtures (paired — all share `id = "fixture-evolution"`)
// ---------------------------------------------------------------------------

/// Shared skeleton for the evolution-fixture trio. Task types, workflows,
/// profiles, and tools are identical across v1/v2_breaking/v2_additive — only
/// `context_schema` differs, so tests isolate the schema-evolution signal
/// without noise from unrelated manifest fields.
fn evolution_skeleton() -> VerticalManifest {
    VerticalManifest {
        id: "fixture-evolution".into(),
        name: "Fixture Evolution".into(),
        defining_constraint: "Schema-evolution tests — keeps task/workflow/profile surface \
                              stable so only context_schema varies across the v1/v2 trio"
            .into(),
        context_schema: HashMap::new(),
        tool_policies: HashMap::new(),
        checkpoint_types: HashMap::new(),
        quality_criteria: vec![],
        task_types: vec![TaskTypeDescriptor {
            id: "evolve-task".into(),
            name: "Evolution Task".into(),
            description: "Schema-evolution harness task".into(),
            workflow_id: "evolve-flow".into(),
            keywords: vec!["evolve".into()],
        }],
        workflows: vec![WorkflowSummary {
            id: "evolve-flow".into(),
            name: "Evolution Flow".into(),
            description: "Baseline flow for evolution fixtures".into(),
            stage_count: 2,
            gated_stage_count: 0,
        }],
        profiles: vec![ProfileSummary {
            role_id: "evolver".into(),
            autonomy: "assisted".into(),
        }],
        tools: vec![ToolSummary {
            name: "read".into(),
            description: "Read a resource".into(),
        }],
    }
}

/// Baseline schema for schema-evolution tests. Two required fields:
/// `project_id: String` + `priority: Enum<low|medium|high>`.
///
/// Pairs with `fixture_context_v2_breaking()` (which narrows `priority` to
/// `Number` and adds a required `region`) and with `fixture_context_v2_additive()`
/// (which adds an optional `notes` field without touching v1's shape).
pub fn fixture_context_v1() -> VerticalManifest {
    let mut m = evolution_skeleton();
    m.context_schema.insert(
        "project_id".into(),
        ContextField {
            field_type: FieldType::String,
            required: true,
            description: "Target project identifier".into(),
            enum_values: None,
            default: None,
        },
    );
    m.context_schema.insert(
        "priority".into(),
        ContextField {
            field_type: FieldType::Enum,
            required: true,
            description: "Work priority".into(),
            enum_values: Some(vec!["low".into(), "medium".into(), "high".into()]),
            default: None,
        },
    );
    m
}

/// Breaking evolution of `fixture_context_v1`: narrows `priority` from
/// `Enum<low|medium|high>` to `Number`, and adds a required `region: String`.
///
/// Exercises two rejection paths in one pair:
/// - `MISSING_CONTEXT` (any v1-shaped context lacks `region`).
/// - `INVALID_CONTEXT` (a v1-shaped context supplies `priority` as a string,
///   but v2 expects a number).
pub fn fixture_context_v2_breaking() -> VerticalManifest {
    let mut m = evolution_skeleton();
    m.context_schema.insert(
        "project_id".into(),
        ContextField {
            field_type: FieldType::String,
            required: true,
            description: "Target project identifier".into(),
            enum_values: None,
            default: None,
        },
    );
    m.context_schema.insert(
        "priority".into(),
        ContextField {
            // Narrowed from Enum → Number.
            field_type: FieldType::Number,
            required: true,
            description: "Work priority (0–100 numeric scale)".into(),
            enum_values: None,
            default: None,
        },
    );
    m.context_schema.insert(
        "region".into(),
        ContextField {
            field_type: FieldType::String,
            required: true,
            description: "Geographic region code".into(),
            enum_values: None,
            default: None,
        },
    );
    m
}

/// Additive (safe) evolution of `fixture_context_v1`: keeps v1's two fields
/// exactly and adds a new *optional* field `notes: String`. Sessions shaped
/// against v1 remain valid under this schema — the evolution never rejects a
/// v1-conforming context.
pub fn fixture_context_v2_additive() -> VerticalManifest {
    let mut m = fixture_context_v1();
    m.context_schema.insert(
        "notes".into(),
        ContextField {
            field_type: FieldType::String,
            required: false,
            description: "Free-form notes (optional, added in v2)".into(),
            enum_values: None,
            default: None,
        },
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_simple_has_no_policies() {
        let m = fixture_simple();
        assert_eq!(m.id, "fixture-simple");
        assert!(m.context_schema.is_empty());
        assert!(m.tool_policies.is_empty());
        assert!(m.checkpoint_types.is_empty());
        assert!(m.quality_criteria.is_empty());
        assert!(!m.tools.is_empty());
        assert!(!m.task_types.is_empty());
    }

    #[test]
    fn fixture_complex_has_all_policy_kinds() {
        let m = fixture_complex();
        assert_eq!(m.id, "fixture-complex");
        assert_eq!(m.context_schema.len(), 4);
        assert_eq!(m.tool_policies.len(), 4);
        assert_eq!(m.checkpoint_types.len(), 3);
        assert_eq!(m.quality_criteria.len(), 3);

        // Verify all 4 policy kinds are represented
        let kinds: Vec<_> = m.tool_policies.values().map(|p| &p.kind).collect();
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, ToolPolicyKind::RequiresGate))
        );
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, ToolPolicyKind::RequiresCheckpoint))
        );
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, ToolPolicyKind::BudgetLimited))
        );
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, ToolPolicyKind::ClassificationGated))
        );
    }

    #[test]
    fn fixture_complex_has_required_context_fields() {
        let m = fixture_complex();
        let required: Vec<_> = m
            .context_schema
            .iter()
            .filter(|(_, f)| f.required)
            .map(|(k, _)| k.as_str())
            .collect();
        assert!(required.contains(&"portfolio_id"));
        assert!(required.contains(&"risk_level"));
    }

    #[test]
    fn evolution_fixtures_share_id_and_non_schema_fields() {
        // All three must hot-swap in place — any divergence outside
        // `context_schema` would conflate the evolution signal.
        let v1 = fixture_context_v1();
        let vb = fixture_context_v2_breaking();
        let va = fixture_context_v2_additive();
        for m in [&v1, &vb, &va] {
            assert_eq!(m.id, "fixture-evolution");
        }
        assert_eq!(v1.task_types.len(), vb.task_types.len());
        assert_eq!(v1.task_types.len(), va.task_types.len());
        assert_eq!(v1.workflows.len(), vb.workflows.len());
        assert_eq!(v1.workflows.len(), va.workflows.len());
    }

    #[test]
    fn evolution_v2_breaking_differs_from_v1_on_required_and_types() {
        let v1 = fixture_context_v1();
        let vb = fixture_context_v2_breaking();

        // v2_breaking has strictly more required fields than v1 (region added).
        let v1_required: std::collections::HashSet<_> = v1
            .context_schema
            .iter()
            .filter(|(_, f)| f.required)
            .map(|(k, _)| k.as_str())
            .collect();
        let vb_required: std::collections::HashSet<_> = vb
            .context_schema
            .iter()
            .filter(|(_, f)| f.required)
            .map(|(k, _)| k.as_str())
            .collect();
        assert!(
            vb_required.is_superset(&v1_required),
            "v2_breaking lost a v1 required field",
        );
        assert!(
            vb_required.contains("region"),
            "v2_breaking must add `region` as required",
        );

        // At least one field differs on type (priority: Enum → Number).
        let v1_prio = &v1.context_schema["priority"].field_type;
        let vb_prio = &vb.context_schema["priority"].field_type;
        assert_ne!(v1_prio, vb_prio, "priority field_type should differ");
    }

    #[test]
    fn evolution_v2_additive_is_superset_of_v1() {
        let v1 = fixture_context_v1();
        let va = fixture_context_v2_additive();

        // v2_additive contains every v1 field with identical type + required.
        for (name, v1_field) in &v1.context_schema {
            let va_field = va
                .context_schema
                .get(name)
                .unwrap_or_else(|| panic!("v2_additive dropped v1 field `{name}`"));
            assert_eq!(va_field.field_type, v1_field.field_type);
            assert_eq!(va_field.required, v1_field.required);
        }
        // New field `notes` is optional — every v1-shaped context is valid
        // under v2_additive.
        assert!(va.context_schema.contains_key("notes"));
        assert!(
            !va.context_schema["notes"].required,
            "notes must be optional to keep evolution additive",
        );
    }
}
