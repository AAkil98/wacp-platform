/** Quality evaluation result for a single dimension. */
export type QualityLevel = "pass" | "warn" | "fail";

/** A quality dimension. */
export interface QualityDimension {
  id: string;
  name: string;
  description: string;
  evaluate: (context: EvaluationContext) => QualityLevel;
}

/** Context for quality evaluation — results from tools/tests. */
export interface EvaluationContext {
  testsPassed: number;
  testsFailed: number;
  typeCheckPassed: boolean;
  lintPassed: boolean;
  changedFiles: number;
  plannedFiles: number;
  reviewVerdict?: "approve" | "request_changes";
}

/** Evaluation result across all dimensions. */
export interface QualityReport {
  dimensions: { id: string; name: string; level: QualityLevel }[];
  overall: QualityLevel;
}

export const SWE_QUALITY_DIMENSIONS: QualityDimension[] = [
  {
    id: "correctness",
    name: "Correctness",
    description: "Code does what the directive requires. No logic errors.",
    evaluate: (ctx) => {
      if (ctx.testsFailed > 0) return "fail";
      if (ctx.testsPassed === 0) return "warn"; // no tests at all
      return "pass";
    },
  },
  {
    id: "type_safety",
    name: "Type Safety",
    description: "Type errors eliminated. Compiler/checker passes.",
    evaluate: (ctx) => ctx.typeCheckPassed ? "pass" : "fail",
  },
  {
    id: "style",
    name: "Style",
    description: "Follows project conventions. Lint passes.",
    evaluate: (ctx) => ctx.lintPassed ? "pass" : "warn",
  },
  {
    id: "coverage",
    name: "Coverage",
    description: "Tests cover the changed code.",
    evaluate: (ctx) => {
      if (ctx.testsPassed === 0 && ctx.testsFailed === 0) return "warn";
      return "pass";
    },
  },
  {
    id: "scope",
    name: "Scope",
    description: "Changes limited to declared scope.",
    evaluate: (ctx) => {
      if (ctx.plannedFiles === 0) return "pass"; // no plan to compare
      const ratio = ctx.changedFiles / ctx.plannedFiles;
      if (ratio > 2.0) return "warn"; // changed much more than planned
      return "pass";
    },
  },
  {
    id: "design",
    name: "Design",
    description: "Architecture decisions sound. Maintainability preserved.",
    evaluate: (ctx) => {
      if (!ctx.reviewVerdict) return "pass"; // no review happened
      return ctx.reviewVerdict === "approve" ? "pass" : "fail";
    },
  },
];

/** Evaluate all quality dimensions and produce a report. */
export function evaluateQuality(context: EvaluationContext): QualityReport {
  const dimensions = SWE_QUALITY_DIMENSIONS.map((dim) => ({
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
  return SWE_QUALITY_DIMENSIONS.find((d) => d.id === id);
}
