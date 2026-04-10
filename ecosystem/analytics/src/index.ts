import { analyticsToolDefinitions, executeAnalyticsTools } from "./tools/analytics-tools.js";
import { ANALYTICS_PROFILES } from "./profiles/profiles.js";
import { ANALYTICS_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";

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
};
