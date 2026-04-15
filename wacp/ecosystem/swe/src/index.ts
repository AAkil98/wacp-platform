import { sweToolDefinitions, executeSweTools } from "./tools/swe-tools.js";
import { SWE_PROFILES } from "./profiles/profiles.js";
import { SWE_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";
import { SWE_QUALITY_DIMENSIONS } from "./quality/quality.js";

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
  defining_constraint:
    "DAG validation — every workflow stage declares an explicit dependsOn list; the stage graph must be a valid DAG with no cycles.",
  context_schema: {},
  tool_policies: {},
  checkpoint_types: {},
  quality_criteria: SWE_QUALITY_DIMENSIONS.map((d) => ({
    id: d.id,
    name: d.name,
    description: d.description,
    weight: 1.0,
  })),
  task_types: [
    { id: "swe:debug", name: "Debug", description: "Diagnose and fix a defect, crash, or error.", workflow_id: "swe:fix-bug", keywords: ["fix", "bug", "error", "crash", "debug"] },
    { id: "swe:refactor", name: "Refactor", description: "Restructure existing code without changing behaviour.", workflow_id: "swe:refactor", keywords: ["refactor", "restructure", "reorganize", "simplify", "clean up"] },
    { id: "swe:test", name: "Write Tests", description: "Add or improve test coverage.", workflow_id: "swe:write-tests", keywords: ["test", "tests", "coverage", "spec"] },
    { id: "swe:review", name: "Code Review", description: "Evaluate code quality without making changes.", workflow_id: "swe:review-only", keywords: ["review", "evaluate", "assess", "check quality"] },
    { id: "swe:document", name: "Document", description: "Write or improve documentation.", workflow_id: "swe:document-only", keywords: ["document", "docs", "readme", "comments"] },
    { id: "swe:investigate", name: "Investigate", description: "Research or understand a codebase area.", workflow_id: "swe:investigate-only", keywords: ["investigate", "research", "explore", "understand"] },
    { id: "swe:implement", name: "Implement Feature", description: "Build a new feature or capability (default catchall).", workflow_id: "swe:implement-feature", keywords: ["implement", "add", "build", "create", "develop"] },
  ],
};
