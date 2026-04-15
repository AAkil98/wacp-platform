/** Quality evaluation result for a single dimension. */
export type QualityLevel = "pass" | "warn" | "fail";

/** A quality dimension. */
export interface QualityDimension {
  id: string;
  name: string;
  description: string;
  evaluate: (context: EvaluationContext) => QualityLevel;
}

/** Context for Finance quality evaluation. */
export interface EvaluationContext {
  tradesExecuted: number;
  tradesWithCompliance: number;
  tradesWithRejectedCompliance: number;
  forbiddenPatternDetected: boolean;
  kycCurrent: boolean;
  regulationCited: boolean;
  trailHashValid: boolean;
  trailEntriesPresent: boolean;
  trailTimestampsMonotonic: boolean;
  suitabilityVerified: boolean;
  conflictsDisclosed: boolean;
  recommendationConsistentWithRiskTolerance: boolean;
  materialRisksDisclosed: boolean;
  riskLanguageSpecific: boolean;
  sourcesCited: boolean;
  pricesTimestamped: boolean;
  modelInputsVersioned: boolean;
  methodologyDocumented: boolean;
  assumptionsExplicit: boolean;
}

/** Evaluation result across all dimensions. */
export interface QualityReport {
  dimensions: { id: string; name: string; level: QualityLevel }[];
  overall: QualityLevel;
}

export const FINANCE_QUALITY_DIMENSIONS: QualityDimension[] = [
  {
    id: "regulatory_compliance",
    name: "Regulatory Compliance",
    description: "All applicable regulations cited and checked.",
    evaluate: (ctx) => {
      if (ctx.tradesExecuted > 0 && ctx.tradesWithCompliance < ctx.tradesExecuted) return "fail";
      if (ctx.tradesWithRejectedCompliance > 0) return "fail";
      if (ctx.forbiddenPatternDetected) return "fail";
      if (!ctx.kycCurrent) return "fail";
      if (!ctx.regulationCited) return "warn";
      return "pass";
    },
  },
  {
    id: "audit_trail_integrity",
    name: "Audit Trail Integrity",
    description: "Hash chain intact, every action recorded, timestamps ordered.",
    evaluate: (ctx) => {
      if (!ctx.trailHashValid) return "fail";
      if (!ctx.trailEntriesPresent) return "fail";
      if (!ctx.trailTimestampsMonotonic) return "fail";
      return "pass";
    },
  },
  {
    id: "fiduciary_duty",
    name: "Fiduciary Duty",
    description: "Suitability verified, conflicts disclosed.",
    evaluate: (ctx) => {
      if (!ctx.suitabilityVerified) return "fail";
      if (!ctx.conflictsDisclosed) return "fail";
      if (!ctx.recommendationConsistentWithRiskTolerance) return "fail";
      return "pass";
    },
  },
  {
    id: "risk_disclosure",
    name: "Risk Disclosure",
    description: "Material risks disclosed with specificity.",
    evaluate: (ctx) => {
      if (!ctx.materialRisksDisclosed) return "fail";
      if (!ctx.riskLanguageSpecific) return "warn";
      return "pass";
    },
  },
  {
    id: "data_provenance",
    name: "Data Provenance",
    description: "Sources cited, prices timestamped, model inputs versioned.",
    evaluate: (ctx) => {
      if (!ctx.sourcesCited) return "fail";
      if (!ctx.pricesTimestamped) return "warn";
      if (!ctx.modelInputsVersioned) return "warn";
      return "pass";
    },
  },
  {
    id: "documentation",
    name: "Documentation",
    description: "Methodology, assumptions, valuations documented.",
    evaluate: (ctx) => {
      if (!ctx.methodologyDocumented) return "fail";
      if (!ctx.assumptionsExplicit) return "warn";
      return "pass";
    },
  },
];

/** Evaluate all quality dimensions and produce a report. */
export function evaluateQuality(context: EvaluationContext): QualityReport {
  const dimensions = FINANCE_QUALITY_DIMENSIONS.map((dim) => ({
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
  return FINANCE_QUALITY_DIMENSIONS.find((d) => d.id === id);
}
