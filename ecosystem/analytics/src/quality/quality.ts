/** Quality evaluation result for a single dimension. */
export type QualityLevel = "pass" | "warn" | "fail";

/** A quality dimension. */
export interface QualityDimension {
  id: string;
  name: string;
  description: string;
  evaluate: (context: EvaluationContext) => QualityLevel;
}

/** Context for Analytics quality evaluation. */
export interface EvaluationContext {
  reconciliationDiffPercent: number; // 0–1
  dataAgeHours: number;
  slaHours: number;
  queryHashPresent: boolean;
  snapshotIdPresent: boolean;
  requiredSectionsPresent: boolean;
  sourceCitationCount: number;
  narrativePresent: boolean;
  queryDurationSeconds: number;
  softQueryThresholdSeconds: number;
  hardQueryThresholdSeconds: number;
}

/** Evaluation result across all dimensions. */
export interface QualityReport {
  dimensions: { id: string; name: string; level: QualityLevel }[];
  overall: QualityLevel;
}

export const ANALYTICS_QUALITY_DIMENSIONS: QualityDimension[] = [
  {
    id: "accuracy",
    name: "Accuracy",
    description: "Numbers reconcile with source systems.",
    evaluate: (ctx) => {
      if (ctx.reconciliationDiffPercent > 0.01) return "fail";
      if (ctx.reconciliationDiffPercent > 0.001) return "warn";
      return "pass";
    },
  },
  {
    id: "data_freshness",
    name: "Data Freshness",
    description: "Underlying data within SLA window.",
    evaluate: (ctx) => {
      if (ctx.slaHours === 0) return "pass"; // no SLA set
      if (ctx.dataAgeHours > ctx.slaHours) return "fail";
      if (ctx.dataAgeHours > ctx.slaHours * 0.75) return "warn";
      return "pass";
    },
  },
  {
    id: "reproducibility",
    name: "Reproducibility",
    description: "Queries re-executable with same result.",
    evaluate: (ctx) => {
      if (!ctx.queryHashPresent || !ctx.snapshotIdPresent) return "fail";
      return "pass";
    },
  },
  {
    id: "completeness",
    name: "Completeness",
    description: "Report answers the original question.",
    evaluate: (ctx) => ctx.requiredSectionsPresent ? "pass" : "fail",
  },
  {
    id: "clarity",
    name: "Clarity",
    description: "Results understandable to target audience.",
    evaluate: (ctx) => {
      if (ctx.sourceCitationCount === 0) return "fail";
      if (!ctx.narrativePresent) return "warn";
      return "pass";
    },
  },
  {
    id: "performance",
    name: "Performance",
    description: "Queries execute within budget.",
    evaluate: (ctx) => {
      if (ctx.queryDurationSeconds > ctx.hardQueryThresholdSeconds) return "fail";
      if (ctx.queryDurationSeconds > ctx.softQueryThresholdSeconds) return "warn";
      return "pass";
    },
  },
];

/** Evaluate all quality dimensions and produce a report. */
export function evaluateQuality(context: EvaluationContext): QualityReport {
  const dimensions = ANALYTICS_QUALITY_DIMENSIONS.map((dim) => ({
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
  return ANALYTICS_QUALITY_DIMENSIONS.find((d) => d.id === id);
}
