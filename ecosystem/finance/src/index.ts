import { financeToolDefinitions, executeFinanceTools } from "./tools/finance-tools.js";
import { FINANCE_PROFILES } from "./profiles/profiles.js";
import { FINANCE_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";

export { FINANCE_ROLES, FINANCE_TASK_TYPES, getRole, getTaskType, type FinanceRole, type FinanceTaskType } from "./taxonomy.js";
export {
  financeToolDefinitions,
  executeFinanceTools,
  validateComplianceCheck,
  classifyForbiddenPattern,
  isComplianceFresh,
  FORBIDDEN_PATTERNS,
  type ToolDefinition,
  type ComplianceCheck,
  type ForbiddenPattern,
} from "./tools/finance-tools.js";
export { FINANCE_PROFILES, getProfile, allProfiles, type AgentProfile } from "./profiles/profiles.js";
export { FINANCE_WORKFLOWS, getWorkflow, allWorkflows, validateWorkflow, type Workflow, type WorkflowStage } from "./workflows/workflows.js";
export { FINANCE_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type QualityDimension, type QualityLevel, type QualityReport, type EvaluationContext } from "./quality/quality.js";
export { detectTaskType, type DetectedTaskType } from "./detect.js";

const FINANCE_TOOL_OPERATIONS: Record<string, string> = {
  market_data_fetch: "data_read",
  financial_model_build: "compute_exec",
  risk_calc: "compute_exec",
  compliance_check: "data_read",
  kyc_screen: "data_read",
  trade_execute: "trade_exec",
  portfolio_rebalance: "data_write",
  audit_trail_export: "data_read",
  regulatory_filing_prepare: "file_write",
  disclosure_review: "data_read",
};

/** Vertical descriptor consumed by the CLI ecosystem loader. */
export const FINANCE_VERTICAL = {
  id: "finance",
  name: "Finance",
  workflows: FINANCE_WORKFLOWS,
  profiles: FINANCE_PROFILES,
  toolDefinitions: financeToolDefinitions(),
  detectTaskType,
  executeTool: executeFinanceTools,
  toolOperation: (name: string): string | null => FINANCE_TOOL_OPERATIONS[name] ?? null,
};
