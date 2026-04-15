/** Quality evaluation result for a single dimension. */
export type QualityLevel = "pass" | "warn" | "fail";

/** A quality dimension. */
export interface QualityDimension {
  id: string;
  name: string;
  description: string;
  evaluate: (context: EvaluationContext) => QualityLevel;
}

/** Context for Data Science quality evaluation. */
export interface EvaluationContext {
  hypothesisDeclared: boolean;
  testsRun: number;
  multipleTestingCorrected: boolean;
  assumptionsChecked: boolean;
  assumptionViolationsAddressed: boolean;
  snapshotHashPresent: boolean;
  randomSeedPresent: boolean;
  codeVersionPresent: boolean;
  interpretationCausalOnObservational: boolean;
  interpretationOverreaches: boolean;
  effectSizeReported: boolean;
  confidenceIntervalReported: boolean;
  methodologyDocumented: boolean;
}

/** Evaluation result across all dimensions. */
export interface QualityReport {
  dimensions: { id: string; name: string; level: QualityLevel }[];
  overall: QualityLevel;
}

export const DATASCI_QUALITY_DIMENSIONS: QualityDimension[] = [
  {
    id: "statistical_rigor",
    name: "Statistical Rigor",
    description: "Hypotheses declared, corrections applied.",
    evaluate: (ctx) => {
      if (!ctx.hypothesisDeclared) return "fail";
      if (ctx.testsRun > 1 && !ctx.multipleTestingCorrected) return "fail";
      if (ctx.testsRun > 1 && ctx.multipleTestingCorrected) return "pass";
      return "pass";
    },
  },
  {
    id: "reproducibility",
    name: "Reproducibility",
    description: "Analysis re-executable from recorded artifacts.",
    evaluate: (ctx) => {
      if (!ctx.snapshotHashPresent) return "fail";
      if (!ctx.codeVersionPresent) return "fail";
      if (!ctx.randomSeedPresent) return "warn";
      return "pass";
    },
  },
  {
    id: "interpretation_validity",
    name: "Interpretation Validity",
    description: "Interpretation matches the actual results.",
    evaluate: (ctx) => {
      if (ctx.interpretationCausalOnObservational) return "fail";
      if (ctx.interpretationOverreaches) return "warn";
      return "pass";
    },
  },
  {
    id: "assumptions_checked",
    name: "Assumptions Checked",
    description: "Parametric assumptions verified before tests.",
    evaluate: (ctx) => {
      if (!ctx.assumptionsChecked) return "fail";
      if (!ctx.assumptionViolationsAddressed) return "fail";
      return "pass";
    },
  },
  {
    id: "effect_size",
    name: "Effect Size",
    description: "Effect sizes and confidence intervals reported.",
    evaluate: (ctx) => {
      if (!ctx.effectSizeReported) return "fail";
      if (!ctx.confidenceIntervalReported) return "fail";
      return "pass";
    },
  },
  {
    id: "documentation",
    name: "Documentation",
    description: "Methodology documented, decisions justified.",
    evaluate: (ctx) => ctx.methodologyDocumented ? "pass" : "fail",
  },
];

/** Evaluate all quality dimensions and produce a report. */
export function evaluateQuality(context: EvaluationContext): QualityReport {
  const dimensions = DATASCI_QUALITY_DIMENSIONS.map((dim) => ({
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
  return DATASCI_QUALITY_DIMENSIONS.find((d) => d.id === id);
}
