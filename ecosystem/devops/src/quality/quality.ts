/** Quality evaluation result for a single dimension. */
export type QualityLevel = "pass" | "warn" | "fail";

/** A quality dimension. */
export interface QualityDimension {
  id: string;
  name: string;
  description: string;
  evaluate: (context: EvaluationContext) => QualityLevel;
}

/** Context for DevOps quality evaluation. */
export interface EvaluationContext {
  healthChecksPassed: number;
  healthChecksFailed: number;
  complianceScanPassed: boolean;
  newCriticalFindings: number;
  newNonCriticalFindings: number;
  affectedResources: number;
  plannedResources: number;
  rollbackCheckpointPresent: boolean;
  rollbackTested: boolean;
  changelogPresent: boolean;
}

/** Evaluation result across all dimensions. */
export interface QualityReport {
  dimensions: { id: string; name: string; level: QualityLevel }[];
  overall: QualityLevel;
}

export const DEVOPS_QUALITY_DIMENSIONS: QualityDimension[] = [
  {
    id: "availability",
    name: "Availability",
    description: "Services remain healthy after changes.",
    evaluate: (ctx) => {
      if (ctx.healthChecksFailed > 0) return "fail";
      if (ctx.healthChecksPassed === 0) return "warn"; // no checks ran
      return "pass";
    },
  },
  {
    id: "security_posture",
    name: "Security Posture",
    description: "No new vulnerabilities introduced.",
    evaluate: (ctx) => {
      if (ctx.newCriticalFindings > 0) return "fail";
      if (ctx.newNonCriticalFindings > 0) return "warn";
      return "pass";
    },
  },
  {
    id: "blast_radius",
    name: "Blast Radius",
    description: "Changes scoped to declared target.",
    evaluate: (ctx) => {
      if (ctx.plannedResources === 0) return "pass"; // no plan to compare
      if (ctx.affectedResources > ctx.plannedResources) return "fail";
      return "pass";
    },
  },
  {
    id: "rollback_readiness",
    name: "Rollback Readiness",
    description: "Rollback strategy exists and is executable.",
    evaluate: (ctx) => {
      if (!ctx.rollbackCheckpointPresent) return "fail";
      if (!ctx.rollbackTested) return "warn";
      return "pass";
    },
  },
  {
    id: "compliance",
    name: "Compliance",
    description: "Changes meet regulatory/policy requirements.",
    evaluate: (ctx) => ctx.complianceScanPassed ? "pass" : "fail",
  },
  {
    id: "documentation",
    name: "Documentation",
    description: "Runbook/playbook updated, changes documented.",
    evaluate: (ctx) => ctx.changelogPresent ? "pass" : "warn",
  },
];

/** Evaluate all quality dimensions and produce a report. */
export function evaluateQuality(context: EvaluationContext): QualityReport {
  const dimensions = DEVOPS_QUALITY_DIMENSIONS.map((dim) => ({
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
  return DEVOPS_QUALITY_DIMENSIONS.find((d) => d.id === id);
}
