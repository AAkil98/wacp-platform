import { describe, it, expect } from "vitest";
import { mlopsToolDefinitions } from "../src/tools/mlops-tools.js";

describe("MLOps Tools", () => {
  const tools = mlopsToolDefinitions();

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

  it("dataset_validate requires path", () => {
    const dv = tools.find((t) => t.name === "dataset_validate")!;
    expect(dv.input_schema.required).toContain("path");
  });

  it("train_launch requires script", () => {
    const tl = tools.find((t) => t.name === "train_launch")!;
    expect(tl.input_schema.required).toContain("script");
  });

  it("eval_benchmark requires model_path and test_data", () => {
    const eb = tools.find((t) => t.name === "eval_benchmark")!;
    const required = eb.input_schema.required as string[];
    expect(required).toContain("model_path");
    expect(required).toContain("test_data");
  });

  it("model_register requires name and path", () => {
    const mr = tools.find((t) => t.name === "model_register")!;
    const required = mr.input_schema.required as string[];
    expect(required).toContain("name");
    expect(required).toContain("path");
  });

  it("model_deploy requires model_name and model_version", () => {
    const md = tools.find((t) => t.name === "model_deploy")!;
    const required = md.input_schema.required as string[];
    expect(required).toContain("model_name");
    expect(required).toContain("model_version");
  });

  it("compute_budget has optional fields only", () => {
    const cb = tools.find((t) => t.name === "compute_budget")!;
    expect(cb.input_schema.required).toBeUndefined();
  });
});
