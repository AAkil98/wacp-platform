/** A detected Data Analytics task type with its target workflow. */
export interface DetectedTaskType {
  id: string;
  workflowId: string;
}

/**
 * Classify a user goal into a Data Analytics task type via keyword heuristics.
 * Returns null if the goal does not match the Analytics domain.
 */
export function detectTaskType(goal: string): DetectedTaskType | null {
  const lower = goal.toLowerCase();
  if (/\b(dashboard|grafana|tableau|looker|superset|metabase)\b/.test(lower))
    return { id: "analytics:dashboard", workflowId: "analytics:build-dashboard" };
  if (/\b(sql\s+query|run\s+(?:a\s+)?query|select\s+\*?\s*from|where\s+clause|join\s+on)\b/.test(lower))
    return { id: "analytics:query", workflowId: "analytics:query-and-report" };
  if (/\b(kpi|key\s+performance|north\s+star\s+metric|metric\s+definition|funnel\s+analysis)\b/.test(lower))
    return { id: "analytics:report", workflowId: "analytics:query-and-report" };
  if (/\b(reconcile|data\s+(?:reconciliation|integrity)|investigate\s+(?:the\s+)?(?:discrepan|drop|spike))\b/.test(lower))
    return { id: "analytics:investigate", workflowId: "analytics:investigate" };
  if (/\b(schema\s+model|data\s+model|dimension\s+table|fact\s+table|star\s+schema|snowflake\s+schema)\b/.test(lower))
    return { id: "analytics:model", workflowId: "analytics:model-data" };
  if (/\b(weekly\s+report|monthly\s+report|business\s+report|exec\s+report|ad[-\s]?hoc\s+analysis)\b/.test(lower))
    return { id: "analytics:report", workflowId: "analytics:query-and-report" };
  if (/\b(query\s+optim|tune\s+(?:the\s+)?query|slow\s+query|explain\s+plan)\b/.test(lower))
    return { id: "analytics:validate", workflowId: "analytics:validate-only" };
  if (/\b(monitor\s+(?:dashboard|kpi|metric)|data\s+freshness|staleness)\b/.test(lower))
    return { id: "analytics:monitor", workflowId: "analytics:monitor-only" };
  return null;
}
