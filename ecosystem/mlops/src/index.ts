import { mlopsToolDefinitions, executeMlopsTools } from "./tools/mlops-tools.js";
import { MLOPS_PROFILES } from "./profiles/profiles.js";
import { MLOPS_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";

export { MLOPS_ROLES, MLOPS_TASK_TYPES, getRole, getTaskType, type MlopsRole, type MlopsTaskType } from "./taxonomy.js";
export { mlopsToolDefinitions, executeMlopsTools, type ToolDefinition } from "./tools/mlops-tools.js";
export { MLOPS_PROFILES, getProfile, allProfiles, type AgentProfile } from "./profiles/profiles.js";
export { MLOPS_WORKFLOWS, getWorkflow, allWorkflows, validateWorkflow, type Workflow, type WorkflowStage } from "./workflows/workflows.js";
export { MLOPS_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type QualityDimension, type QualityLevel, type QualityReport, type EvaluationContext } from "./quality/quality.js";
export { detectTaskType, type DetectedTaskType } from "./detect.js";

const MLOPS_TOOL_OPERATIONS: Record<string, string> = {
  dataset_validate: "data_read",
  experiment_track: "data_write",
  train_launch: "compute_exec",
  eval_benchmark: "compute_exec",
  model_register: "data_write",
  model_deploy: "deploy_exec",
  drift_detect: "data_read",
  compute_budget: "data_read",
  reproduce_check: "compute_exec",
  data_lineage: "data_read",
};

/** Vertical descriptor consumed by the CLI ecosystem loader. */
export const MLOPS_VERTICAL = {
  id: "mlops",
  name: "MLOps",
  workflows: MLOPS_WORKFLOWS,
  profiles: MLOPS_PROFILES,
  toolDefinitions: mlopsToolDefinitions(),
  detectTaskType,
  executeTool: executeMlopsTools,
  toolOperation: (name: string): string | null => MLOPS_TOOL_OPERATIONS[name] ?? null,
};
