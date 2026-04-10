import { describe, it, expect } from "vitest";
import { DEVOPS_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type EvaluationContext } from "../src/quality/quality.js";

describe("DevOps Quality", () => {
  const allGood: EvaluationContext = {
    healthChecksPassed: 5,
    healthChecksFailed: 0,
    complianceScanPassed: true,
    newCriticalFindings: 0,
    newNonCriticalFindings: 0,
    affectedResources: 3,
    plannedResources: 3,
    rollbackCheckpointPresent: true,
    rollbackTested: true,
    changelogPresent: true,
  };

  it("defines 6 quality dimensions", () => {
    expect(DEVOPS_QUALITY_DIMENSIONS).toHaveLength(6);
  });

  it("all dimensions have unique IDs", () => {
    const ids = DEVOPS_QUALITY_DIMENSIONS.map((d) => d.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("all-pass context → overall pass", () => {
    const report = evaluateQuality(allGood);
    expect(report.overall).toBe("pass");
    for (const dim of report.dimensions) {
      expect(dim.level).toBe("pass");
    }
  });

  it("health check failure → availability fail → overall fail", () => {
    const ctx = { ...allGood, healthChecksFailed: 1 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "availability")!.level).toBe("fail");
    expect(report.overall).toBe("fail");
  });

  it("no health checks → availability warn", () => {
    const ctx = { ...allGood, healthChecksPassed: 0, healthChecksFailed: 0 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "availability")!.level).toBe("warn");
  });

  it("new critical finding → security_posture fail", () => {
    const ctx = { ...allGood, newCriticalFindings: 1 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "security_posture")!.level).toBe("fail");
  });

  it("non-critical finding → security_posture warn", () => {
    const ctx = { ...allGood, newNonCriticalFindings: 2 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "security_posture")!.level).toBe("warn");
  });

  it("affected > planned → blast_radius fail", () => {
    const ctx = { ...allGood, affectedResources: 10, plannedResources: 3 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "blast_radius")!.level).toBe("fail");
  });

  it("no plan → blast_radius pass", () => {
    const ctx = { ...allGood, plannedResources: 0 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "blast_radius")!.level).toBe("pass");
  });

  it("no rollback checkpoint → rollback_readiness fail", () => {
    const ctx = { ...allGood, rollbackCheckpointPresent: false, rollbackTested: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "rollback_readiness")!.level).toBe("fail");
  });

  it("untested rollback → rollback_readiness warn", () => {
    const ctx = { ...allGood, rollbackTested: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "rollback_readiness")!.level).toBe("warn");
  });

  it("compliance scan failed → compliance fail", () => {
    const ctx = { ...allGood, complianceScanPassed: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "compliance")!.level).toBe("fail");
  });

  it("no changelog → documentation warn", () => {
    const ctx = { ...allGood, changelogPresent: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "documentation")!.level).toBe("warn");
  });

  it("getDimension returns undefined for unknown", () => {
    expect(getDimension("unknown")).toBeUndefined();
  });

  it("report includes all 6 dimensions", () => {
    const report = evaluateQuality(allGood);
    expect(report.dimensions).toHaveLength(6);
  });
});
