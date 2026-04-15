import { describe, it, expect } from "vitest";
import { DATASCI_ROLES, DATASCI_TASK_TYPES, getRole, getTaskType } from "../src/taxonomy.js";

describe("Data Science Taxonomy", () => {
  it("defines 5 roles", () => {
    expect(DATASCI_ROLES).toHaveLength(5);
  });

  it("all roles have unique IDs", () => {
    const ids = DATASCI_ROLES.map((r) => r.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("explorer extends worker", () => {
    const explorer = getRole("datasci:explorer")!;
    expect(explorer.extends).toBe("worker");
    expect(explorer.toolAccess).toBe("read-summary");
    expect(explorer.autonomy).toBe("gated");
  });

  it("statistician extends worker with test-inference access", () => {
    const statistician = getRole("datasci:statistician")!;
    expect(statistician.extends).toBe("worker");
    expect(statistician.toolAccess).toBe("test-inference");
  });

  it("feature_engineer extends worker", () => {
    const fe = getRole("datasci:feature_engineer")!;
    expect(fe.extends).toBe("worker");
    expect(fe.toolAccess).toBe("extract-transform");
  });

  it("modeler extends worker with fit-diagnostics access", () => {
    const modeler = getRole("datasci:modeler")!;
    expect(modeler.extends).toBe("worker");
    expect(modeler.toolAccess).toBe("fit-diagnostics");
  });

  it("reviewer extends observer with autonomous access", () => {
    const reviewer = getRole("datasci:reviewer")!;
    expect(reviewer.extends).toBe("observer");
    expect(reviewer.toolAccess).toBe("read-only");
    expect(reviewer.autonomy).toBe("autonomous");
  });

  it("defines 9 task types", () => {
    expect(DATASCI_TASK_TYPES).toHaveLength(9);
  });

  it("all task types have unique IDs", () => {
    const ids = DATASCI_TASK_TYPES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("hypothesize and test share the same workflow", () => {
    const hyp = getTaskType("datasci:hypothesize")!;
    const test = getTaskType("datasci:test")!;
    expect(hyp.defaultWorkflow).toBe(test.defaultWorkflow);
    expect(hyp.defaultWorkflow).toBe("datasci:hypothesis-test");
  });

  it("model task involves 4 roles", () => {
    const model = getTaskType("datasci:model")!;
    expect(model.roles).toHaveLength(4);
    expect(model.defaultWorkflow).toBe("datasci:model-build");
  });

  it("report task uses full-analysis workflow", () => {
    const report = getTaskType("datasci:report")!;
    expect(report.defaultWorkflow).toBe("datasci:full-analysis");
  });

  it("getRole returns undefined for unknown", () => {
    expect(getRole("unknown")).toBeUndefined();
  });

  it("getTaskType returns undefined for unknown", () => {
    expect(getTaskType("unknown")).toBeUndefined();
  });
});
