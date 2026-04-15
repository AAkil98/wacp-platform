import { describe, it, expect } from "vitest";
import { datasciToolDefinitions, validateHypothesisDeclaration } from "../src/tools/datasci-tools.js";

describe("Data Science Tools", () => {
  const tools = datasciToolDefinitions();

  it("defines 10 tools", () => {
    expect(tools).toHaveLength(10);
  });

  it("all tools have unique names", () => {
    const names = tools.map((t) => t.name);
    expect(new Set(names).size).toBe(names.length);
  });

  it("all tools have non-empty descriptions", () => {
    for (const tool of tools) {
      expect(tool.description.length).toBeGreaterThan(0);
    }
  });

  it("all tools have object input schemas", () => {
    for (const tool of tools) {
      expect(tool.input_schema.type).toBe("object");
    }
  });

  it("hypothesis_test requires declaration", () => {
    const ht = tools.find((t) => t.name === "hypothesis_test")!;
    expect(ht.input_schema.required).toContain("declaration");
  });

  it("model_fit requires model_type, formula, data", () => {
    const mf = tools.find((t) => t.name === "model_fit")!;
    const required = mf.input_schema.required as string[];
    expect(required).toContain("model_type");
    expect(required).toContain("formula");
    expect(required).toContain("data");
  });

  it("causal_inference requires method and variables", () => {
    const ci = tools.find((t) => t.name === "causal_inference")!;
    const required = ci.input_schema.required as string[];
    expect(required).toContain("method");
    expect(required).toContain("treatment");
    expect(required).toContain("outcome");
  });
});

describe("Hypothesis Declaration Validation", () => {
  it("valid declaration passes", () => {
    const errors = validateHypothesisDeclaration({
      null_hypothesis: "mean = 0",
      alternative_hypothesis: "mean != 0",
      alpha: 0.05,
      test_type: "t-test",
    });
    expect(errors).toHaveLength(0);
  });

  it("missing null_hypothesis fails", () => {
    const errors = validateHypothesisDeclaration({
      alternative_hypothesis: "mean != 0",
      alpha: 0.05,
      test_type: "t-test",
    });
    expect(errors.some((e) => e.includes("null_hypothesis"))).toBe(true);
  });

  it("missing alternative_hypothesis fails", () => {
    const errors = validateHypothesisDeclaration({
      null_hypothesis: "mean = 0",
      alpha: 0.05,
      test_type: "t-test",
    });
    expect(errors.some((e) => e.includes("alternative_hypothesis"))).toBe(true);
  });

  it("invalid alpha fails (zero)", () => {
    const errors = validateHypothesisDeclaration({
      null_hypothesis: "H0",
      alternative_hypothesis: "H1",
      alpha: 0,
      test_type: "t-test",
    });
    expect(errors.some((e) => e.includes("alpha"))).toBe(true);
  });

  it("invalid alpha fails (>= 1)", () => {
    const errors = validateHypothesisDeclaration({
      null_hypothesis: "H0",
      alternative_hypothesis: "H1",
      alpha: 1.0,
      test_type: "t-test",
    });
    expect(errors.some((e) => e.includes("alpha"))).toBe(true);
  });

  it("missing test_type fails", () => {
    const errors = validateHypothesisDeclaration({
      null_hypothesis: "H0",
      alternative_hypothesis: "H1",
      alpha: 0.05,
    });
    expect(errors.some((e) => e.includes("test_type"))).toBe(true);
  });

  it("empty null_hypothesis fails", () => {
    const errors = validateHypothesisDeclaration({
      null_hypothesis: "   ",
      alternative_hypothesis: "H1",
      alpha: 0.05,
      test_type: "t-test",
    });
    expect(errors.length).toBeGreaterThan(0);
  });
});
