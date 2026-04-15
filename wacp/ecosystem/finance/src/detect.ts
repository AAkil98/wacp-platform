/** A detected Finance task type with its target workflow. */
export interface DetectedTaskType {
  id: string;
  workflowId: string;
}

/**
 * Classify a user goal into a Finance task type via keyword heuristics.
 * Returns null if the goal does not match the Finance domain.
 */
export function detectTaskType(goal: string): DetectedTaskType | null {
  const lower = goal.toLowerCase();
  if (/\b(buy|sell|trade|order|execute\s+(?:the\s+)?(?:order|trade))\b/.test(lower))
    return { id: "finance:trade", workflowId: "finance:trade-execution" };
  if (/\b(rebalance|reweight|rebalance\s+(?:the\s+)?portfolio|target\s+weights?)\b/.test(lower))
    return { id: "finance:rebalance", workflowId: "finance:portfolio-rebalance" };
  if (/\b(kyc|aml|onboard\s+(?:the\s+)?client|client\s+onboarding|sanctions\s+screen)\b/.test(lower))
    return { id: "finance:onboard", workflowId: "finance:client-onboarding" };
  if (/\b(dcf|lbo|valuation|comparables|sum[-\s]?of[-\s]?the[-\s]?parts|sotp|financial\s+model)\b/.test(lower))
    return { id: "finance:model", workflowId: "finance:model-only" };
  if (/\b(var\b|cvar|value\s+at\s+risk|exposure|stress\s+test|greeks?)\b/.test(lower))
    return { id: "finance:risk_assess", workflowId: "finance:risk-only" };
  if (/\b(pre[-\s]?trade|compliance\s+check|regulatory\s+(?:check|review)|sec\s+rule|finra|mifid)\b/.test(lower))
    return { id: "finance:compliance_check", workflowId: "finance:compliance-only" };
  if (/\b(audit\s+(?:trade|portfolio|trail)|fiduciary\s+audit|10[-\s]?k|10[-\s]?q|13f)\b/.test(lower))
    return { id: "finance:audit", workflowId: "finance:audit-only" };
  if (/\b(equity\s+research|credit\s+analysis|macro\s+analysis|investment\s+thesis)\b/.test(lower))
    return { id: "finance:analyze", workflowId: "finance:analyze-only" };
  if (/\b(quarterly\s+report|annual\s+report|holdings\s+report|performance\s+report)\b/.test(lower))
    return { id: "finance:report", workflowId: "finance:full-report" };
  return null;
}
