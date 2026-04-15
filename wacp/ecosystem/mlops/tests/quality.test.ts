import { describe, it, expect } from "vitest";
import { MLOPS_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type EvaluationContext } from "../src/quality/quality.js";

describe("MLOps Quality", () => {
  const allGood: EvaluationContext = {
    targetMetricMet: true,
    metricImproved: true,
    reproducible: true,
    dataValidationPassed: true,
    computeBudgetExceeded: false,
    computeUtilization: 0.5,
    modelAgeDays: 10,
    freshnessThresholdDays: 30,
    experimentLogged: true,
    modelCardPresent: true,
  };

  it("defines 6 quality dimensions", () => {
    expect(MLOPS_QUALITY_DIMENSIONS).toHaveLength(6);
  });

  it("all dimensions have unique IDs", () => {
    const ids = MLOPS_QUALITY_DIMENSIONS.map((d) => d.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("all-pass context → overall pass", () => {
    const report = evaluateQuality(allGood);
    expect(report.overall).toBe("pass");
    for (const dim of report.dimensions) {
      expect(dim.level).toBe("pass");
    }
  });

  it("target metric not met and no improvement → metric_performance fail", () => {
    const ctx = { ...allGood, targetMetricMet: false, metricImproved: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "metric_performance")!.level).toBe("fail");
    expect(report.overall).toBe("fail");
  });

  it("target not met but improved → metric_performance warn", () => {
    const ctx = { ...allGood, targetMetricMet: false, metricImproved: true };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "metric_performance")!.level).toBe("warn");
  });

  it("not reproducible → reproducibility fail", () => {
    const ctx = { ...allGood, reproducible: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "reproducibility")!.level).toBe("fail");
  });

  it("data validation failed → data_quality fail", () => {
    const ctx = { ...allGood, dataValidationPassed: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "data_quality")!.level).toBe("fail");
  });

  it("budget exceeded → compute_efficiency fail", () => {
    const ctx = { ...allGood, computeBudgetExceeded: true };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "compute_efficiency")!.level).toBe("fail");
  });

  it("high utilization → compute_efficiency warn", () => {
    const ctx = { ...allGood, computeUtilization: 0.85 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "compute_efficiency")!.level).toBe("warn");
  });

  it("model stale → model_freshness fail", () => {
    const ctx = { ...allGood, modelAgeDays: 60, freshnessThresholdDays: 30 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "model_freshness")!.level).toBe("fail");
  });

  it("model aging → model_freshness warn", () => {
    const ctx = { ...allGood, modelAgeDays: 25, freshnessThresholdDays: 30 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "model_freshness")!.level).toBe("warn");
  });

  it("no freshness threshold → model_freshness pass", () => {
    const ctx = { ...allGood, modelAgeDays: 999, freshnessThresholdDays: 0 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "model_freshness")!.level).toBe("pass");
  });

  it("experiment not logged → documentation fail", () => {
    const ctx = { ...allGood, experimentLogged: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "documentation")!.level).toBe("fail");
  });

  it("no model card → documentation warn", () => {
    const ctx = { ...allGood, modelCardPresent: false };
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
