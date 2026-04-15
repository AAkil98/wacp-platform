/** Quality evaluation result for a single dimension. */
export type QualityLevel = "pass" | "warn" | "fail";

/** A quality dimension. */
export interface QualityDimension {
  id: string;
  name: string;
  description: string;
  evaluate: (context: EvaluationContext) => QualityLevel;
}

/** Context for MLOps quality evaluation. */
export interface EvaluationContext {
  targetMetricMet: boolean;
  metricImproved: boolean;
  reproducible: boolean;
  dataValidationPassed: boolean;
  computeBudgetExceeded: boolean;
  computeUtilization: number; // 0–1
  modelAgeDays: number;
  freshnessThresholdDays: number;
  experimentLogged: boolean;
  modelCardPresent: boolean;
}

/** Evaluation result across all dimensions. */
export interface QualityReport {
  dimensions: { id: string; name: string; level: QualityLevel }[];
  overall: QualityLevel;
}

export const MLOPS_QUALITY_DIMENSIONS: QualityDimension[] = [
  {
    id: "metric_performance",
    name: "Metric Performance",
    description: "Model meets target metrics.",
    evaluate: (ctx) => {
      if (!ctx.targetMetricMet && !ctx.metricImproved) return "fail";
      if (!ctx.targetMetricMet && ctx.metricImproved) return "warn";
      return "pass";
    },
  },
  {
    id: "reproducibility",
    name: "Reproducibility",
    description: "Experiment can be reproduced within tolerance.",
    evaluate: (ctx) => ctx.reproducible ? "pass" : "fail",
  },
  {
    id: "data_quality",
    name: "Data Quality",
    description: "Dataset passes validation checks.",
    evaluate: (ctx) => ctx.dataValidationPassed ? "pass" : "fail",
  },
  {
    id: "compute_efficiency",
    name: "Compute Efficiency",
    description: "Training within budget.",
    evaluate: (ctx) => {
      if (ctx.computeBudgetExceeded) return "fail";
      if (ctx.computeUtilization > 0.8) return "warn";
      return "pass";
    },
  },
  {
    id: "model_freshness",
    name: "Model Freshness",
    description: "Model not stale — trained within acceptable timeframe.",
    evaluate: (ctx) => {
      if (ctx.freshnessThresholdDays === 0) return "pass"; // no threshold set
      if (ctx.modelAgeDays > ctx.freshnessThresholdDays) return "fail";
      if (ctx.modelAgeDays > ctx.freshnessThresholdDays * 0.75) return "warn";
      return "pass";
    },
  },
  {
    id: "documentation",
    name: "Documentation",
    description: "Experiment logged, model card present.",
    evaluate: (ctx) => {
      if (!ctx.experimentLogged) return "fail";
      if (!ctx.modelCardPresent) return "warn";
      return "pass";
    },
  },
];

/** Evaluate all quality dimensions and produce a report. */
export function evaluateQuality(context: EvaluationContext): QualityReport {
  const dimensions = MLOPS_QUALITY_DIMENSIONS.map((dim) => ({
    id: dim.id,
    name: dim.name,
    level: dim.evaluate(context),
  }));

  let overall: QualityLevel = "pass";
  for (const dim of dimensions) {
    if (dim.level === "fail") { overall = "fail"; break; }
    if (dim.level === "warn") overall = "warn";
  }

  return { dimensions, overall };
}

/** Get a dimension by ID. */
export function getDimension(id: string): QualityDimension | undefined {
  return MLOPS_QUALITY_DIMENSIONS.find((d) => d.id === id);
}
