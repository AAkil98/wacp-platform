/** Finance role definition. */
export interface FinanceRole {
  id: string;
  name: string;
  extends: "worker" | "observer";
  concern: string;
  toolAccess: "read-model" | "allocate-rebalance" | "risk-read" | "compliance-kyc" | "read-only";
  autonomy: "gated" | "autonomous";
}

/** Finance task type definition. */
export interface FinanceTaskType {
  id: string;
  name: string;
  description: string;
  defaultWorkflow: string;
  roles: string[];
}

export const FINANCE_ROLES: FinanceRole[] = [
  {
    id: "finance:analyst",
    name: "Analyst",
    extends: "worker",
    concern: "Market analysis, financial modeling, valuation",
    toolAccess: "read-model",
    autonomy: "gated",
  },
  {
    id: "finance:portfolio_manager",
    name: "Portfolio Manager",
    extends: "worker",
    concern: "Portfolio construction, allocation, rebalancing decisions",
    toolAccess: "allocate-rebalance",
    autonomy: "gated",
  },
  {
    id: "finance:risk_officer",
    name: "Risk Officer",
    extends: "worker",
    concern: "Risk measurement, exposure analysis, limit enforcement",
    toolAccess: "risk-read",
    autonomy: "gated",
  },
  {
    id: "finance:compliance_officer",
    name: "Compliance Officer",
    extends: "worker",
    concern: "Regulatory checks, KYC/AML, trade pre-approval",
    toolAccess: "compliance-kyc",
    autonomy: "gated",
  },
  {
    id: "finance:auditor",
    name: "Auditor",
    extends: "observer",
    concern: "Audit trail review, fiduciary verification, filing review",
    toolAccess: "read-only",
    autonomy: "autonomous",
  },
];

export const FINANCE_TASK_TYPES: FinanceTaskType[] = [
  {
    id: "finance:analyze",
    name: "Analyze",
    description: "Equity, credit, or macro analysis",
    defaultWorkflow: "finance:analyze-only",
    roles: ["finance:analyst"],
  },
  {
    id: "finance:model",
    name: "Model",
    description: "Build a financial model — DCF, LBO, comparables",
    defaultWorkflow: "finance:model-only",
    roles: ["finance:analyst"],
  },
  {
    id: "finance:trade",
    name: "Trade",
    description: "Execute a trade",
    defaultWorkflow: "finance:trade-execution",
    roles: ["finance:analyst", "finance:compliance_officer", "finance:portfolio_manager"],
  },
  {
    id: "finance:rebalance",
    name: "Rebalance",
    description: "Rebalance a portfolio toward target weights",
    defaultWorkflow: "finance:portfolio-rebalance",
    roles: ["finance:portfolio_manager", "finance:risk_officer", "finance:compliance_officer"],
  },
  {
    id: "finance:risk_assess",
    name: "Risk Assess",
    description: "Compute risk metrics — VaR, CVaR, exposure",
    defaultWorkflow: "finance:risk-only",
    roles: ["finance:risk_officer"],
  },
  {
    id: "finance:compliance_check",
    name: "Compliance Check",
    description: "Pre-trade or pre-publication compliance review",
    defaultWorkflow: "finance:compliance-only",
    roles: ["finance:compliance_officer"],
  },
  {
    id: "finance:audit",
    name: "Audit",
    description: "Audit trail review and verification",
    defaultWorkflow: "finance:audit-only",
    roles: ["finance:auditor"],
  },
  {
    id: "finance:report",
    name: "Report",
    description: "Produce a financial report",
    defaultWorkflow: "finance:full-report",
    roles: ["finance:analyst", "finance:risk_officer", "finance:compliance_officer", "finance:auditor"],
  },
  {
    id: "finance:onboard",
    name: "Onboard",
    description: "Client onboarding — KYC, AML, suitability",
    defaultWorkflow: "finance:client-onboarding",
    roles: ["finance:compliance_officer", "finance:portfolio_manager"],
  },
];

/** Get a role by ID. */
export function getRole(id: string): FinanceRole | undefined {
  return FINANCE_ROLES.find((r) => r.id === id);
}

/** Get a task type by ID. */
export function getTaskType(id: string): FinanceTaskType | undefined {
  return FINANCE_TASK_TYPES.find((t) => t.id === id);
}
