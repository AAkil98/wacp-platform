import { analyticsToolDefinitions, executeAnalyticsTools } from "./tools/analytics-tools.js";
import { ANALYTICS_PROFILES } from "./profiles/profiles.js";
import { ANALYTICS_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";
import { ANALYTICS_QUALITY_DIMENSIONS } from "./quality/quality.js";

export { ANALYTICS_ROLES, ANALYTICS_TASK_TYPES, getRole, getTaskType, type AnalyticsRole, type AnalyticsTaskType } from "./taxonomy.js";
export { analyticsToolDefinitions, executeAnalyticsTools, classifySql, type ToolDefinition, type SqlSafety } from "./tools/analytics-tools.js";
export { ANALYTICS_PROFILES, getProfile, allProfiles, type AgentProfile } from "./profiles/profiles.js";
export { ANALYTICS_WORKFLOWS, getWorkflow, allWorkflows, validateWorkflow, type Workflow, type WorkflowStage } from "./workflows/workflows.js";
export { ANALYTICS_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type QualityDimension, type QualityLevel, type QualityReport, type EvaluationContext } from "./quality/quality.js";
export { detectTaskType, type DetectedTaskType } from "./detect.js";

const ANALYTICS_TOOL_OPERATIONS: Record<string, string> = {
  sql_query: "data_read",
  dashboard_build: "data_write",
  data_profile: "data_read",
  kpi_calculate: "data_read",
  report_generate: "file_write",
  viz_create: "file_write",
  data_reconcile: "data_read",
  schema_explore: "data_read",
  query_optimize: "compute_exec",
  metric_define: "data_write",
};

/** Vertical descriptor consumed by the CLI ecosystem loader. */
export const ANALYTICS_VERTICAL = {
  id: "analytics",
  name: "Data Analytics",
  workflows: ANALYTICS_WORKFLOWS,
  profiles: ANALYTICS_PROFILES,
  toolDefinitions: analyticsToolDefinitions(),
  detectTaskType,
  executeTool: executeAnalyticsTools,
  toolOperation: (name: string): string | null => ANALYTICS_TOOL_OPERATIONS[name] ?? null,
  defining_constraint:
    "SQL safety + query reproducibility — classifySql() hard-blocks DROP/TRUNCATE and unscoped UPDATE/DELETE unless allow_destructive=true with gate clearance; every report must cite its source queries and a data_snapshot_id for reproducibility.",
  context_schema: {
    data_snapshot_id: {
      type: "string" as const,
      required: true,
      description: "Snapshot ID of the source data; all queries must target this snapshot for reproducibility.",
    },
  },
  tool_policies: {
    sql_query: {
      kind: "classification_gated" as const,
      description: "classifySql() blocks destructive and unscoped-mutation queries; set allow_destructive=true with gate clearance to proceed.",
      blocked_classifications: ["destructive", "unscoped_mutation"] as const,
      override_flag: "allow_destructive",
    },
  },
  checkpoint_types: {
    data_snapshot: {
      description: "Records the source data snapshot used for a query or report — required for reproducibility.",
      fields: [
        { name: "snapshot_id", type: "string" as const, description: "Unique snapshot identifier." },
        { name: "created_at", type: "number" as const, description: "Unix timestamp (ms) when the snapshot was taken." },
        { name: "row_hash", type: "string" as const, description: "SHA-256 of the canonical row order for this snapshot." },
        { name: "table_name", type: "string" as const, description: "Source table or view name." },
      ],
    },
  },
  quality_criteria: ANALYTICS_QUALITY_DIMENSIONS.map((d) => ({
    id: d.id,
    name: d.name,
    description: d.description,
    weight: 1.0,
  })),
  task_types: [
    { id: "analytics:dashboard", name: "Build Dashboard", description: "Design and build an analytics dashboard.", workflow_id: "analytics:build-dashboard", keywords: ["dashboard", "grafana", "tableau", "looker", "superset"] },
    { id: "analytics:query", name: "Run Query", description: "Write and execute a SQL query against source data.", workflow_id: "analytics:query-and-report", keywords: ["sql query", "run query", "select from", "where clause"] },
    { id: "analytics:report", name: "KPI Report", description: "Produce a KPI or metrics report.", workflow_id: "analytics:query-and-report", keywords: ["kpi", "key performance", "north star metric", "funnel"] },
    { id: "analytics:investigate", name: "Investigate Discrepancy", description: "Reconcile data discrepancies between systems.", workflow_id: "analytics:investigate", keywords: ["reconcile", "data reconciliation", "discrepancy", "mismatch"] },
    { id: "analytics:model", name: "Model Data", description: "Define or update a semantic data model.", workflow_id: "analytics:model-data", keywords: ["schema model", "data model", "star schema", "snowflake schema", "dbt"] },
    { id: "analytics:validate", name: "Validate / Optimize", description: "Validate query results or optimize slow queries.", workflow_id: "analytics:validate-only", keywords: ["query optimize", "tune query", "slow query", "explain plan"] },
    { id: "analytics:monitor", name: "Monitor Freshness", description: "Monitor data freshness and pipeline health.", workflow_id: "analytics:monitor-only", keywords: ["monitor dashboard", "data freshness", "staleness", "pipeline health"] },
  ],
};
