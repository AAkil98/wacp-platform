import { financeToolDefinitions, executeFinanceTools } from "./tools/finance-tools.js";
import { FINANCE_PROFILES } from "./profiles/profiles.js";
import { FINANCE_WORKFLOWS } from "./workflows/workflows.js";
import { detectTaskType } from "./detect.js";
import { FINANCE_QUALITY_DIMENSIONS } from "./quality/quality.js";

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
  defining_constraint:
    "Regulatory pre-check + fiduciary duty — trade_execute refuses without an approved compliance_check checkpoint for the same trade_id (expires after 5 minutes); classifyForbiddenPattern() hard-blocks insider/wash/spoofing/layering/front-running/churning/painting-the-tape.",
  context_schema: {
    compliance_scope: {
      type: "string" as const,
      required: true,
      description: "Regulatory scope for trades in this session (e.g. equities, fixed-income, derivatives).",
    },
    jurisdiction: {
      type: "enum" as const,
      required: true,
      description: "Regulatory jurisdiction governing trades in this session.",
      enum_values: ["SEC", "FINRA", "MiFID II", "FCA", "other"] as const,
    },
  },
  tool_policies: {
    trade_execute: {
      kind: "requires_checkpoint" as const,
      description: "Refuses without an approved compliance_check checkpoint whose trade_id matches and that was created within the last 5 minutes.",
      checkpoint_type: "compliance_check",
      matching_field: "trade_id",
      expires_after_ms: 300_000,
    },
  },
  checkpoint_types: {
    compliance_check: {
      description: "Pre-trade compliance verification — regulatory check, KYC, and forbidden-pattern screen.",
      fields: [
        { name: "trade_id", type: "string" as const, description: "Unique identifier for the trade being checked." },
        { name: "instrument", type: "string" as const, description: "Financial instrument (ticker, ISIN, CUSIP)." },
        { name: "side", type: "enum" as const, description: "Trade direction.", enum_values: ["buy", "sell"] as const },
        { name: "quantity", type: "number" as const, description: "Trade quantity." },
        { name: "status", type: "enum" as const, description: "Compliance decision.", enum_values: ["approved", "rejected"] as const },
        { name: "regulation_cited", type: "string" as const, description: "Applicable regulation(s) checked." },
        { name: "forbidden_pattern_screened", type: "boolean" as const, description: "Whether the forbidden-pattern screen ran." },
        { name: "suitability_verified", type: "boolean" as const, description: "Whether suitability was verified for the client." },
        { name: "kyc_current", type: "boolean" as const, description: "Whether KYC is current for the counterparty." },
        { name: "expires_at", type: "number" as const, description: "Unix timestamp (ms) after which this check is stale." },
      ],
    },
  },
  quality_criteria: FINANCE_QUALITY_DIMENSIONS.map((d) => ({
    id: d.id,
    name: d.name,
    description: d.description,
    weight: 1.0,
  })),
  task_types: [
    { id: "finance:trade", name: "Trade Execution", description: "Execute a buy or sell order through compliance pre-check.", workflow_id: "finance:trade-execution", keywords: ["buy", "sell", "trade", "order", "execute"] },
    { id: "finance:rebalance", name: "Portfolio Rebalance", description: "Rebalance a portfolio to target weights.", workflow_id: "finance:portfolio-rebalance", keywords: ["rebalance", "reweight", "target weights", "allocation"] },
    { id: "finance:onboard", name: "Client Onboarding", description: "Onboard a new client including KYC/AML screening.", workflow_id: "finance:client-onboarding", keywords: ["kyc", "aml", "onboard client", "client onboarding", "sanctions screen"] },
    { id: "finance:model", name: "Financial Model", description: "Build a valuation or financial model.", workflow_id: "finance:model-only", keywords: ["dcf", "lbo", "valuation", "comparables", "financial model"] },
    { id: "finance:risk_assess", name: "Risk Assessment", description: "Quantify portfolio or trade risk.", workflow_id: "finance:risk-only", keywords: ["var", "cvar", "value at risk", "stress test", "greeks"] },
    { id: "finance:compliance_check", name: "Compliance Check", description: "Run a pre-trade or regulatory compliance check.", workflow_id: "finance:compliance-only", keywords: ["pre-trade", "compliance check", "regulatory check", "sec rule", "finra", "mifid"] },
    { id: "finance:audit", name: "Fiduciary Audit", description: "Audit trade history or portfolio for regulatory filings.", workflow_id: "finance:audit-only", keywords: ["audit trail", "fiduciary audit", "10-k", "10-q", "13f"] },
    { id: "finance:analyze", name: "Investment Analysis", description: "Produce equity research, credit analysis, or macro commentary.", workflow_id: "finance:analyze-only", keywords: ["equity research", "credit analysis", "macro analysis", "investment thesis"] },
    { id: "finance:report", name: "Performance Report", description: "Generate a quarterly, annual, or holdings report.", workflow_id: "finance:full-report", keywords: ["quarterly report", "annual report", "holdings report", "performance report"] },
  ],
};
