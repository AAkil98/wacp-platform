/** A detected MLOps task type with its target workflow. */
export interface DetectedTaskType {
  id: string;
  workflowId: string;
}

/**
 * Classify a user goal into an MLOps task type via keyword heuristics.
 * Returns null if the goal does not match the MLOps domain.
 */
export function detectTaskType(goal: string): DetectedTaskType | null {
  const lower = goal.toLowerCase();
  if (/\b(reproduce|reproducibility|rerun\s+experiment|recreate\s+(?:results|run))\b/.test(lower))
    return { id: "mlops:reproduce", workflowId: "mlops:reproduce" };
  if (/\b(train(?:ing)?|fine[-\s]?tune|pretrain|sweep|hyperparameter)\b/.test(lower))
    return { id: "mlops:train", workflowId: "mlops:experiment" };
  if (/\b(experiment|run\s+(?:a\s+)?study|ablation|grid\s+search)\b/.test(lower))
    return { id: "mlops:experiment", workflowId: "mlops:experiment" };
  if (/\b(deploy\s+(?:the\s+)?model|model\s+(?:registry|deployment|serving)|register\s+(?:the\s+)?model)\b/.test(lower))
    return { id: "mlops:deploy", workflowId: "mlops:deploy" };
  if (/\b(evaluate|benchmark|leaderboard|eval\s+set)\b/.test(lower))
    return { id: "mlops:evaluate", workflowId: "mlops:evaluate-only" };
  if (/\b(drift|distribution\s+shift|monitor\s+(?:the\s+)?model|production\s+monitoring)\b/.test(lower))
    return { id: "mlops:monitor", workflowId: "mlops:monitor-only" };
  if (/\b(optimize\s+(?:the\s+)?model|model\s+(?:compression|quantization|pruning)|onnx|tensorrt)\b/.test(lower))
    return { id: "mlops:optimize", workflowId: "mlops:optimize" };
  if (/\b(dataset|data\s+(?:prep|preparation|preprocess|clean)|labeling)\b/.test(lower))
    return { id: "mlops:data_prep", workflowId: "mlops:data-prep-only" };
  if (/\b(model\s+audit|data\s+lineage|provenance)\b/.test(lower))
    return { id: "mlops:audit", workflowId: "mlops:audit-only" };
  return null;
}
