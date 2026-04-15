import { describe, it, expect } from "vitest";
import { FINANCE_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type EvaluationContext } from "../src/quality/quality.js";

describe("Finance Quality", () => {
  const allGood: EvaluationContext = {
    tradesExecuted: 1,
    tradesWithCompliance: 1,
    tradesWithRejectedCompliance: 0,
    forbiddenPatternDetected: false,
    kycCurrent: true,
    regulationCited: true,
    trailHashValid: true,
    trailEntriesPresent: true,
    trailTimestampsMonotonic: true,
    suitabilityVerified: true,
    conflictsDisclosed: true,
    recommendationConsistentWithRiskTolerance: true,
    materialRisksDisclosed: true,
    riskLanguageSpecific: true,
    sourcesCited: true,
    pricesTimestamped: true,
    modelInputsVersioned: true,
    methodologyDocumented: true,
    assumptionsExplicit: true,
  };

  it("defines 6 quality dimensions", () => {
    expect(FINANCE_QUALITY_DIMENSIONS).toHaveLength(6);
  });

  it("all dimensions have unique IDs", () => {
    const ids = FINANCE_QUALITY_DIMENSIONS.map((d) => d.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("all-pass context → overall pass", () => {
    const report = evaluateQuality(allGood);
    expect(report.overall).toBe("pass");
    for (const dim of report.dimensions) {
      expect(dim.level).toBe("pass");
    }
  });

  it("trade without compliance → regulatory_compliance fail", () => {
    const ctx = { ...allGood, tradesExecuted: 1, tradesWithCompliance: 0 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "regulatory_compliance")!.level).toBe("fail");
    expect(report.overall).toBe("fail");
  });

  it("rejected compliance → regulatory_compliance fail", () => {
    const ctx = { ...allGood, tradesWithRejectedCompliance: 1 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "regulatory_compliance")!.level).toBe("fail");
  });

  it("forbidden pattern detected → regulatory_compliance fail", () => {
    const ctx = { ...allGood, forbiddenPatternDetected: true };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "regulatory_compliance")!.level).toBe("fail");
  });

  it("KYC not current → regulatory_compliance fail", () => {
    const ctx = { ...allGood, kycCurrent: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "regulatory_compliance")!.level).toBe("fail");
  });

  it("regulation not cited → regulatory_compliance warn", () => {
    const ctx = { ...allGood, regulationCited: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "regulatory_compliance")!.level).toBe("warn");
  });

  it("trail hash invalid → audit_trail_integrity fail", () => {
    const ctx = { ...allGood, trailHashValid: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "audit_trail_integrity")!.level).toBe("fail");
  });

  it("trail timestamps not monotonic → audit_trail_integrity fail", () => {
    const ctx = { ...allGood, trailTimestampsMonotonic: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "audit_trail_integrity")!.level).toBe("fail");
  });

  it("suitability not verified → fiduciary_duty fail", () => {
    const ctx = { ...allGood, suitabilityVerified: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "fiduciary_duty")!.level).toBe("fail");
  });

  it("conflicts undisclosed → fiduciary_duty fail", () => {
    const ctx = { ...allGood, conflictsDisclosed: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "fiduciary_duty")!.level).toBe("fail");
  });

  it("recommendation contradicts risk tolerance → fiduciary_duty fail", () => {
    const ctx = { ...allGood, recommendationConsistentWithRiskTolerance: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "fiduciary_duty")!.level).toBe("fail");
  });

  it("material risks not disclosed → risk_disclosure fail", () => {
    const ctx = { ...allGood, materialRisksDisclosed: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "risk_disclosure")!.level).toBe("fail");
  });

  it("boilerplate risk language → risk_disclosure warn", () => {
    const ctx = { ...allGood, riskLanguageSpecific: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "risk_disclosure")!.level).toBe("warn");
  });

  it("sources not cited → data_provenance fail", () => {
    const ctx = { ...allGood, sourcesCited: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "data_provenance")!.level).toBe("fail");
  });

  it("prices not timestamped → data_provenance warn", () => {
    const ctx = { ...allGood, pricesTimestamped: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "data_provenance")!.level).toBe("warn");
  });

  it("methodology undocumented → documentation fail", () => {
    const ctx = { ...allGood, methodologyDocumented: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "documentation")!.level).toBe("fail");
  });

  it("assumptions implicit → documentation warn", () => {
    const ctx = { ...allGood, assumptionsExplicit: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "documentation")!.level).toBe("warn");
  });

  it("getDimension returns undefined for unknown", () => {
    expect(getDimension("unknown")).toBeUndefined();
  });
});
