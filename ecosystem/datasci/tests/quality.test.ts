import { describe, it, expect } from "vitest";
import { DATASCI_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type EvaluationContext } from "../src/quality/quality.js";

describe("Data Science Quality", () => {
  const allGood: EvaluationContext = {
    hypothesisDeclared: true,
    testsRun: 1,
    multipleTestingCorrected: true,
    assumptionsChecked: true,
    assumptionViolationsAddressed: true,
    snapshotHashPresent: true,
    randomSeedPresent: true,
    codeVersionPresent: true,
    interpretationCausalOnObservational: false,
    interpretationOverreaches: false,
    effectSizeReported: true,
    confidenceIntervalReported: true,
    methodologyDocumented: true,
  };

  it("defines 6 quality dimensions", () => {
    expect(DATASCI_QUALITY_DIMENSIONS).toHaveLength(6);
  });

  it("all dimensions have unique IDs", () => {
    const ids = DATASCI_QUALITY_DIMENSIONS.map((d) => d.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("all-pass context → overall pass", () => {
    const report = evaluateQuality(allGood);
    expect(report.overall).toBe("pass");
    for (const dim of report.dimensions) {
      expect(dim.level).toBe("pass");
    }
  });

  it("no hypothesis declared → rigor fail", () => {
    const ctx = { ...allGood, hypothesisDeclared: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "statistical_rigor")!.level).toBe("fail");
    expect(report.overall).toBe("fail");
  });

  it("multiple tests without correction → rigor fail", () => {
    const ctx = { ...allGood, testsRun: 5, multipleTestingCorrected: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "statistical_rigor")!.level).toBe("fail");
  });

  it("single test needs no correction → rigor pass", () => {
    const ctx = { ...allGood, testsRun: 1, multipleTestingCorrected: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "statistical_rigor")!.level).toBe("pass");
  });

  it("no snapshot hash → reproducibility fail", () => {
    const ctx = { ...allGood, snapshotHashPresent: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "reproducibility")!.level).toBe("fail");
  });

  it("no code version → reproducibility fail", () => {
    const ctx = { ...allGood, codeVersionPresent: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "reproducibility")!.level).toBe("fail");
  });

  it("no random seed → reproducibility warn", () => {
    const ctx = { ...allGood, randomSeedPresent: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "reproducibility")!.level).toBe("warn");
  });

  it("causal claim on observational → interpretation fail", () => {
    const ctx = { ...allGood, interpretationCausalOnObservational: true };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "interpretation_validity")!.level).toBe("fail");
  });

  it("interpretation overreach → interpretation warn", () => {
    const ctx = { ...allGood, interpretationOverreaches: true };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "interpretation_validity")!.level).toBe("warn");
  });

  it("assumptions not checked → assumptions_checked fail", () => {
    const ctx = { ...allGood, assumptionsChecked: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "assumptions_checked")!.level).toBe("fail");
  });

  it("assumption violations unaddressed → assumptions_checked fail", () => {
    const ctx = { ...allGood, assumptionViolationsAddressed: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "assumptions_checked")!.level).toBe("fail");
  });

  it("no effect size → effect_size fail", () => {
    const ctx = { ...allGood, effectSizeReported: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "effect_size")!.level).toBe("fail");
  });

  it("no confidence interval → effect_size fail", () => {
    const ctx = { ...allGood, confidenceIntervalReported: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "effect_size")!.level).toBe("fail");
  });

  it("no methodology → documentation fail", () => {
    const ctx = { ...allGood, methodologyDocumented: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "documentation")!.level).toBe("fail");
  });

  it("getDimension returns undefined for unknown", () => {
    expect(getDimension("unknown")).toBeUndefined();
  });
});
