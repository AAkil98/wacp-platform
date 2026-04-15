/** A detected DevOps task type with its target workflow. */
export interface DetectedTaskType {
  id: string;
  workflowId: string;
}

/**
 * Classify a user goal into a DevOps task type via keyword heuristics.
 * Returns null if the goal does not match the DevOps domain.
 */
export function detectTaskType(goal: string): DetectedTaskType | null {
  const lower = goal.toLowerCase();
  if (/\b(rollback|revert\s+(?:the\s+)?(?:deploy|release|prod))\b/.test(lower))
    return { id: "devops:respond", workflowId: "devops:respond" };
  if (/\b(incident|outage|on[-\s]?call|page(?:r|d)?|sev[-\s]?\d|postmortem)\b/.test(lower))
    return { id: "devops:respond", workflowId: "devops:respond" };
  if (/\b(deploy|deployment|release\s+to|ship\s+to|push\s+to\s+(?:prod|staging|production))\b/.test(lower))
    return { id: "devops:deploy", workflowId: "devops:deploy" };
  if (/\b(provision|terraform|cloudformation|pulumi|infra(?:structure)?|spin\s+up|vpc|kubernetes\s+cluster)\b/.test(lower))
    return { id: "devops:provision", workflowId: "devops:provision" };
  if (/\b(harden|secure|secret\s+rotat|rotate\s+(?:secrets|keys|credentials)|cve)\b/.test(lower))
    return { id: "devops:secure", workflowId: "devops:secure" };
  if (/\b(optimize\s+(?:cost|infra|cluster|spend)|cost\s+reduction|right[-\s]?siz)/.test(lower))
    return { id: "devops:optimize", workflowId: "devops:optimize" };
  if (/\b(audit\s+(?:infra|cluster|prod|deploy)|compliance\s+scan|security\s+audit)\b/.test(lower))
    return { id: "devops:audit", workflowId: "devops:audit" };
  if (/\b(monitor|alert|metric|dashboard\s+(?:for|of)\s+(?:cpu|memory|latency))\b/.test(lower))
    return { id: "devops:monitor", workflowId: "devops:monitor-only" };
  return null;
}
