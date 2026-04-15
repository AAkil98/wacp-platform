import { describe, it, expect } from "vitest";
import { SWE_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type EvaluationContext } from "../src/quality/quality.js";

describe("SWE Quality", () => {
  const allGood: EvaluationContext = {
    testsPassed: 10,
    testsFailed: 0,
    typeCheckPassed: true,
    lintPassed: true,
    changedFiles: 3,
    plannedFiles: 3,
    reviewVerdict: "approve",
  };

  it("defines 6 quality dimensions", () => {
    expect(SWE_QUALITY_DIMENSIONS).toHaveLength(6);
  });

  it("all dimensions have unique IDs", () => {
    const ids = SWE_QUALITY_DIMENSIONS.map((d) => d.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("all-pass context → overall pass", () => {
    const report = evaluateQuality(allGood);
    expect(report.overall).toBe("pass");
    for (const dim of report.dimensions) {
      expect(dim.level).toBe("pass");
    }
  });

  it("failed tests → correctness fail → overall fail", () => {
    const ctx = { ...allGood, testsFailed: 2 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "correctness")!.level).toBe("fail");
    expect(report.overall).toBe("fail");
  });

  it("no tests → correctness warn → overall warn", () => {
    const ctx = { ...allGood, testsPassed: 0, testsFailed: 0 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "correctness")!.level).toBe("warn");
    expect(report.overall).toBe("warn");
  });

  it("type check failed → type_safety fail", () => {
    const ctx = { ...allGood, typeCheckPassed: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "type_safety")!.level).toBe("fail");
  });

  it("lint failed → style warn", () => {
    const ctx = { ...allGood, lintPassed: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "style")!.level).toBe("warn");
  });

  it("scope: changed much more than planned → warn", () => {
    const ctx = { ...allGood, changedFiles: 20, plannedFiles: 3 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "scope")!.level).toBe("warn");
  });

  it("scope: no plan → pass", () => {
    const ctx = { ...allGood, plannedFiles: 0 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "scope")!.level).toBe("pass");
  });

  it("review request_changes → design fail", () => {
    const ctx = { ...allGood, reviewVerdict: "request_changes" as const };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "design")!.level).toBe("fail");
  });

  it("no review → design pass", () => {
    const ctx = { ...allGood, reviewVerdict: undefined };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "design")!.level).toBe("pass");
  });

  it("getDimension returns undefined for unknown", () => {
    expect(getDimension("unknown")).toBeUndefined();
  });

  it("report includes all 6 dimensions", () => {
    const report = evaluateQuality(allGood);
    expect(report.dimensions).toHaveLength(6);
  });
});
