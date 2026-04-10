import { sweToolDefinitions, executeSweTools } from "./tools/swe-tools.js";
import { SWE_PROFILES } from "./profiles/profiles.js";
import { SWE_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";

export { SWE_ROLES, SWE_TASK_TYPES, getRole, getTaskType, type SweRole, type SweTaskType } from "./taxonomy.js";
export { sweToolDefinitions, executeSweTools, type ToolDefinition } from "./tools/swe-tools.js";
export { SWE_PROFILES, getProfile, allProfiles, type AgentProfile } from "./profiles/profiles.js";
export { SWE_WORKFLOWS, getWorkflow, allWorkflows, validateWorkflow, type Workflow, type WorkflowStage } from "./workflows/workflows.js";
export { SWE_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type QualityDimension, type QualityLevel, type QualityReport, type EvaluationContext } from "./quality/quality.js";
export { detectTaskType, type DetectedTaskType } from "./detect.js";

const SWE_TOOL_OPERATIONS: Record<string, string> = {
  code_search: "file_read",
  code_edit: "file_write",
  test_run: "shell_exec",
  type_check: "shell_exec",
  lint_check: "shell_exec",
  git_branch: "git_write",
  git_commit: "git_write",
  dependency_check: "shell_exec",
};

/** Vertical descriptor consumed by the CLI ecosystem loader. */
export const SWE_VERTICAL = {
  id: "swe",
  name: "Software Engineering",
  workflows: SWE_WORKFLOWS,
  profiles: SWE_PROFILES,
  toolDefinitions: sweToolDefinitions(),
  detectTaskType,
  executeTool: executeSweTools,
  toolOperation: (name: string): string | null => SWE_TOOL_OPERATIONS[name] ?? null,
};
