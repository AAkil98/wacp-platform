import { describe, it, expect } from "vitest";
import { ANALYTICS_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type EvaluationContext } from "../src/quality/quality.js";

describe("Analytics Quality", () => {
  const allGood: EvaluationContext = {
    reconciliationDiffPercent: 0.0,
    dataAgeHours: 2,
    slaHours: 24,
    queryHashPresent: true,
    snapshotIdPresent: true,
    requiredSectionsPresent: true,
    sourceCitationCount: 3,
    narrativePresent: true,
    queryDurationSeconds: 5,
    softQueryThresholdSeconds: 30,
    hardQueryThresholdSeconds: 120,
  };

  it("defines 6 quality dimensions", () => {
    expect(ANALYTICS_QUALITY_DIMENSIONS).toHaveLength(6);
  });

  it("all dimensions have unique IDs", () => {
    const ids = ANALYTICS_QUALITY_DIMENSIONS.map((d) => d.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("all-pass context → overall pass", () => {
    const report = evaluateQuality(allGood);
    expect(report.overall).toBe("pass");
    for (const dim of report.dimensions) {
      expect(dim.level).toBe("pass");
    }
  });

  it("large reconciliation mismatch → accuracy fail", () => {
    const ctx = { ...allGood, reconciliationDiffPercent: 0.05 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "accuracy")!.level).toBe("fail");
  });

  it("small reconciliation mismatch → accuracy warn", () => {
    const ctx = { ...allGood, reconciliationDiffPercent: 0.005 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "accuracy")!.level).toBe("warn");
  });

  it("data older than SLA → data_freshness fail", () => {
    const ctx = { ...allGood, dataAgeHours: 48, slaHours: 24 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "data_freshness")!.level).toBe("fail");
  });

  it("data aging → data_freshness warn", () => {
    const ctx = { ...allGood, dataAgeHours: 20, slaHours: 24 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "data_freshness")!.level).toBe("warn");
  });

  it("no SLA → data_freshness pass", () => {
    const ctx = { ...allGood, dataAgeHours: 999, slaHours: 0 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "data_freshness")!.level).toBe("pass");
  });

  it("missing query hash → reproducibility fail", () => {
    const ctx = { ...allGood, queryHashPresent: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "reproducibility")!.level).toBe("fail");
  });

  it("missing snapshot ID → reproducibility fail", () => {
    const ctx = { ...allGood, snapshotIdPresent: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "reproducibility")!.level).toBe("fail");
  });

  it("missing required sections → completeness fail", () => {
    const ctx = { ...allGood, requiredSectionsPresent: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "completeness")!.level).toBe("fail");
  });

  it("no source citations → clarity fail", () => {
    const ctx = { ...allGood, sourceCitationCount: 0 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "clarity")!.level).toBe("fail");
  });

  it("no narrative → clarity warn", () => {
    const ctx = { ...allGood, narrativePresent: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "clarity")!.level).toBe("warn");
  });

  it("slow query beyond hard threshold → performance fail", () => {
    const ctx = { ...allGood, queryDurationSeconds: 150 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "performance")!.level).toBe("fail");
  });

  it("slow query beyond soft threshold → performance warn", () => {
    const ctx = { ...allGood, queryDurationSeconds: 60 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "performance")!.level).toBe("warn");
  });

  it("getDimension returns undefined for unknown", () => {
    expect(getDimension("unknown")).toBeUndefined();
  });
});
