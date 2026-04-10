import { datasciToolDefinitions, executeDatasciTools } from "./tools/datasci-tools.js";
import { DATASCI_PROFILES } from "./profiles/profiles.js";
import { DATASCI_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";

export { DATASCI_ROLES, DATASCI_TASK_TYPES, getRole, getTaskType, type DatasciRole, type DatasciTaskType } from "./taxonomy.js";
export { datasciToolDefinitions, executeDatasciTools, validateHypothesisDeclaration, type ToolDefinition, type HypothesisDeclaration } from "./tools/datasci-tools.js";
export { DATASCI_PROFILES, getProfile, allProfiles, type AgentProfile } from "./profiles/profiles.js";
export { DATASCI_WORKFLOWS, getWorkflow, allWorkflows, validateWorkflow, type Workflow, type WorkflowStage } from "./workflows/workflows.js";
export { DATASCI_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type QualityDimension, type QualityLevel, type QualityReport, type EvaluationContext } from "./quality/quality.js";
export { detectTaskType, type DetectedTaskType } from "./detect.js";

const DATASCI_TOOL_OPERATIONS: Record<string, string> = {
  stat_summary: "data_read",
  correlation_analysis: "data_read",
  hypothesis_test: "data_read",
  feature_extract: "data_read",
  feature_transform: "data_write",
  model_fit: "compute_exec",
  diagnostic_plots: "file_write",
  bootstrap_sample: "compute_exec",
  causal_inference: "compute_exec",
  interpretation: "data_read",
};

/** Vertical descriptor consumed by the CLI ecosystem loader. */
export const DATASCI_VERTICAL = {
  id: "datasci",
  name: "Data Science",
  workflows: DATASCI_WORKFLOWS,
  profiles: DATASCI_PROFILES,
  toolDefinitions: datasciToolDefinitions(),
  detectTaskType,
  executeTool: executeDatasciTools,
  toolOperation: (name: string): string | null => DATASCI_TOOL_OPERATIONS[name] ?? null,
};
