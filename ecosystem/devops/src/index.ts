import { devopsToolDefinitions, executeDevopsTools } from "./tools/devops-tools.js";
import { DEVOPS_PROFILES } from "./profiles/profiles.js";
import { DEVOPS_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";

export { DEVOPS_ROLES, DEVOPS_TASK_TYPES, ENVIRONMENT_TIERS, getRole, getTaskType, requiresHumanApproval, type DevopsRole, type DevopsTaskType, type EnvironmentTier } from "./taxonomy.js";
export { devopsToolDefinitions, executeDevopsTools, type ToolDefinition } from "./tools/devops-tools.js";
export { DEVOPS_PROFILES, getProfile, allProfiles, type AgentProfile } from "./profiles/profiles.js";
export { DEVOPS_WORKFLOWS, getWorkflow, allWorkflows, validateWorkflow, type Workflow, type WorkflowStage } from "./workflows/workflows.js";
export { DEVOPS_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type QualityDimension, type QualityLevel, type QualityReport, type EvaluationContext } from "./quality/quality.js";
export { detectTaskType, type DetectedTaskType } from "./detect.js";

const DEVOPS_TOOL_OPERATIONS: Record<string, string> = {
  infra_plan: "compute_exec",
  deploy_execute: "deploy_exec",
  rollback: "deploy_exec",
  health_check: "data_read",
  log_query: "data_read",
  metric_query: "data_read",
  alert_manage: "data_write",
  config_validate: "compute_exec",
  secret_rotate: "secret_write",
  compliance_scan: "data_read",
};

/** Vertical descriptor consumed by the CLI ecosystem loader. */
export const DEVOPS_VERTICAL = {
  id: "devops",
  name: "DevOps",
  workflows: DEVOPS_WORKFLOWS,
  profiles: DEVOPS_PROFILES,
  toolDefinitions: devopsToolDefinitions(),
  detectTaskType,
  executeTool: executeDevopsTools,
  toolOperation: (name: string): string | null => DEVOPS_TOOL_OPERATIONS[name] ?? null,
};
