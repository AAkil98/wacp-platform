//! Fixture vertical manifests for testing.
//!
//! Two fixtures, built from the real `wacp_taxonomy::VerticalManifest` type:
//! - `fixture-simple`: SWE-like baseline — no context schema, no tool policies.
//! - `fixture-complex`: Finance-like — required context fields, tool policies,
//!   checkpoint types, quality criteria.

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
}
