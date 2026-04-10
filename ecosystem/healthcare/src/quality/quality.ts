/** Quality evaluation result for a single dimension. */
export type QualityLevel = "pass" | "warn" | "fail";

/** A quality dimension. */
export interface QualityDimension {
  id: string;
  name: string;
  description: string;
  evaluate: (context: EvaluationContext) => QualityLevel;
}

/** Context for Healthcare quality evaluation. */
export interface EvaluationContext {
  clinicalToolInvocations: number;
  invocationsWithGrant: number;
  grantScopeMatches: boolean;
  grantNotExpired: boolean;
  phiDetectedInArtifact: boolean;
  recommendationContradictsGuideline: boolean;
  guidelineCited: boolean;
  offLabelDisclosed: boolean;
  literatureCitationsPresent: boolean;
  evidenceLevelGraded: boolean;
  requiredClinicalContextPresent: boolean;
  readingLevelGradeDelta: number;
  hipaaBreachIndicator: boolean;
  hipaaNoticeReferenced: boolean;
  fdaDisclaimerWhenRequired: boolean;
}

/** Evaluation result across all dimensions. */
export interface QualityReport {
  dimensions: { id: string; name: string; level: QualityLevel }[];
  overall: QualityLevel;
}

export const HEALTHCARE_QUALITY_DIMENSIONS: QualityDimension[] = [
  {
    id: "clinical_accuracy",
    name: "Clinical Accuracy",
    description: "Recommendations align with current clinical guidelines.",
    evaluate: (ctx) => {
      if (ctx.recommendationContradictsGuideline) return "fail";
      if (!ctx.guidelineCited) return "warn";
      if (!ctx.offLabelDisclosed) return "warn";
      return "pass";
    },
  },
  {
    id: "phi_compliance",
    name: "PHI Compliance",
    description: "No PHI exposed without a valid access grant.",
    evaluate: (ctx) => {
      if (ctx.clinicalToolInvocations > 0 && ctx.invocationsWithGrant < ctx.clinicalToolInvocations) return "fail";
      if (ctx.phiDetectedInArtifact) return "fail";
      if (!ctx.grantScopeMatches) return "fail";
      if (!ctx.grantNotExpired) return "fail";
      return "pass";
    },
  },
  {
    id: "evidence_basis",
    name: "Evidence Basis",
    description: "Citations to clinical literature, evidence levels graded.",
    evaluate: (ctx) => {
      if (!ctx.literatureCitationsPresent) return "fail";
      if (!ctx.evidenceLevelGraded) return "warn";
      return "pass";
    },
  },
  {
    id: "completeness",
    name: "Completeness",
    description: "All relevant clinical context included.",
    evaluate: (ctx) => {
      if (!ctx.requiredClinicalContextPresent) return "fail";
      return "pass";
    },
  },
  {
    id: "readability",
    name: "Readability",
    description: "Appropriate reading level for the audience.",
    evaluate: (ctx) => {
      if (ctx.readingLevelGradeDelta > 2) return "fail";
      if (ctx.readingLevelGradeDelta > 0) return "warn";
      return "pass";
    },
  },
  {
    id: "regulatory_adherence",
    name: "Regulatory Adherence",
    description: "HIPAA, GDPR, FDA where applicable.",
    evaluate: (ctx) => {
      if (ctx.hipaaBreachIndicator) return "fail";
      if (!ctx.fdaDisclaimerWhenRequired) return "fail";
      if (!ctx.hipaaNoticeReferenced) return "warn";
      return "pass";
    },
  },
];

/** Evaluate all quality dimensions and produce a report. */
export function evaluateQuality(context: EvaluationContext): QualityReport {
  const dimensions = HEALTHCARE_QUALITY_DIMENSIONS.map((dim) => ({
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
  return HEALTHCARE_QUALITY_DIMENSIONS.find((d) => d.id === id);
}
