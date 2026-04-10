/** A detected Data Science task type with its target workflow. */
export interface DetectedTaskType {
  id: string;
  workflowId: string;
}

/**
 * Classify a user goal into a Data Science task type via keyword heuristics.
 * Returns null if the goal does not match the Data Science domain.
 */
export function detectTaskType(goal: string): DetectedTaskType | null {
  const lower = goal.toLowerCase();
  if (/\b(hypothesis\s+test|t[-\s]?test|chi[-\s]?square|anova|mann[-\s]?whitney|wilcoxon|p[-\s]?value)\b/.test(lower))
    return { id: "datasci:test", workflowId: "datasci:hypothesis-test" };
  if (/\b(test\s+if|test\s+whether|null\s+hypothesis|alternative\s+hypothesis)\b/.test(lower))
    return { id: "datasci:hypothesize", workflowId: "datasci:hypothesis-test" };
  if (/\b(causal\s+inference|propensity\s+score|instrumental\s+variable|difference[-\s]?in[-\s]?differences)\b/.test(lower))
    return { id: "datasci:test", workflowId: "datasci:hypothesis-test" };
  if (/\b(linear\s+regression|logistic\s+regression|glm|gam|mixed[-\s]?effect|survival\s+analysis|cox\s+model)\b/.test(lower))
    return { id: "datasci:model", workflowId: "datasci:model-build" };
  if (/\b(eda|exploratory\s+data\s+analysis|profile\s+(?:the\s+)?data|data\s+summary)\b/.test(lower))
    return { id: "datasci:explore", workflowId: "datasci:exploration" };
  if (/\b(feature\s+(?:extract|transform|engineer)|standardize|normalize|impute|pca)\b/.test(lower))
    return { id: "datasci:feature", workflowId: "datasci:feature-only" };
  if (/\b(interpret\s+(?:the\s+)?(?:results|coefficients|model|test)|effect\s+size|confidence\s+interval)\b/.test(lower))
    return { id: "datasci:interpret", workflowId: "datasci:interpret-only" };
  if (/\b(bootstrap|resampl)/.test(lower))
    return { id: "datasci:test", workflowId: "datasci:hypothesis-test" };
  if (/\b(statistical\s+(?:report|findings|analysis)|inference\s+(?:report|findings))\b/.test(lower))
    return { id: "datasci:report", workflowId: "datasci:full-analysis" };
  return null;
}
