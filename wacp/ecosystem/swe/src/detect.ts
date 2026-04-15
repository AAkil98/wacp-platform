/** A detected SWE task type with its target workflow. */
export interface DetectedTaskType {
  id: string;
  workflowId: string;
}

/**
 * Classify a user goal into a SWE task type via keyword heuristics.
 *
 * SWE is the universal catchall — it always returns a result (defaults to
 * `swe:implement` for ambiguous goals). Domain verticals (devops, finance,
 * etc.) return null for non-matches and the router tries them first.
 */
export function detectTaskType(goal: string): DetectedTaskType {
  const lower = goal.toLowerCase();
  if (/\b(fix|bug|error|crash|broken|issue|debug)\b/.test(lower))
    return { id: "swe:debug", workflowId: "swe:fix-bug" };
  if (/\b(refactor|restructure|reorganize|simplify|clean\s*up)\b/.test(lower))
    return { id: "swe:refactor", workflowId: "swe:refactor" };
  if (/\b(test|tests|coverage|spec|specs)\b/.test(lower) && !/\b(implement|add|build|create)\b/.test(lower))
    return { id: "swe:test", workflowId: "swe:write-tests" };
  if (/\b(review|evaluate|assess|check\s+quality)\b/.test(lower))
    return { id: "swe:review", workflowId: "swe:review-only" };
  if (/\b(document|docs|readme|comments)\b/.test(lower))
    return { id: "swe:document", workflowId: "swe:document-only" };
  if (/\b(investigate|research|explore|understand|how\s+does)\b/.test(lower))
    return { id: "swe:investigate", workflowId: "swe:investigate-only" };
  return { id: "swe:implement", workflowId: "swe:implement-feature" };
}
