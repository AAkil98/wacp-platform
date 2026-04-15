import { describe, it, expect } from "vitest";
import { DATASCI_WORKFLOWS, getWorkflow, validateWorkflow, allWorkflows } from "../src/workflows/workflows.js";
import { DATASCI_TASK_TYPES } from "../src/taxonomy.js";

describe("Data Science Workflows", () => {
  it("defines 4 workflows", () => {
    expect(DATASCI_WORKFLOWS).toHaveLength(4);
  });

  it("all workflows have unique IDs", () => {
    const ids = DATASCI_WORKFLOWS.map((w) => w.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("full-analysis has 5 stages", () => {
    const wf = getWorkflow("datasci:full-analysis")!;
    expect(wf.stages).toHaveLength(5);
    expect(wf.stages.map((s) => s.id)).toEqual(["explore", "hypothesize", "test", "interpret", "review"]);
  });

  it("hypothesis-test has 3 stages: declare → test → interpret", () => {
    const wf = getWorkflow("datasci:hypothesis-test")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["declare", "test", "interpret"]);
  });

  it("model-build has 4 stages: explore → feature → fit → validate", () => {
    const wf = getWorkflow("datasci:model-build")!;
    expect(wf.stages).toHaveLength(4);
    expect(wf.stages.map((s) => s.id)).toEqual(["explore", "feature", "fit", "validate"]);
  });

  it("exploration has 2 stages: profile → visualize", () => {
    const wf = getWorkflow("datasci:exploration")!;
    expect(wf.stages).toHaveLength(2);
  });

  it("first stage of each workflow has no dependencies", () => {
    for (const wf of DATASCI_WORKFLOWS) {
      expect(wf.stages[0].dependsOn).toHaveLength(0);
    }
  });

  it("test stage is gated in hypothesis-test (prevent post-hoc fishing)", () => {
    const wf = getWorkflow("datasci:hypothesis-test")!;
    expect(wf.stages.find((s) => s.id === "test")!.gated).toBe(true);
  });

  it("test stage is gated in full-analysis", () => {
    const wf = getWorkflow("datasci:full-analysis")!;
    expect(wf.stages.find((s) => s.id === "test")!.gated).toBe(true);
  });

  it("review stage is gated in full-analysis", () => {
    const wf = getWorkflow("datasci:full-analysis")!;
    expect(wf.stages.find((s) => s.id === "review")!.gated).toBe(true);
  });

  it("fit stage is gated in model-build", () => {
    const wf = getWorkflow("datasci:model-build")!;
    expect(wf.stages.find((s) => s.id === "fit")!.gated).toBe(true);
  });

  it("exploration stages are not gated (EDA is read-only)", () => {
    const wf = getWorkflow("datasci:exploration")!;
    for (const stage of wf.stages) {
      expect(stage.gated).toBe(false);
    }
  });

  it("all workflows validate (no cycles, deps exist)", () => {
    for (const wf of DATASCI_WORKFLOWS) {
      const errors = validateWorkflow(wf);
      expect(errors).toHaveLength(0);
    }
  });

  it("validateWorkflow catches missing dependency", () => {
    const bad = {
      id: "bad",
      name: "Bad",
      description: "broken",
      stages: [
        { id: "a", name: "A", roleId: "x", dependsOn: ["nonexistent"], gated: false },
      ],
    };
    const errors = validateWorkflow(bad);
    expect(errors.length).toBeGreaterThan(0);
  });

  it("every task type references a defined workflow or direct", () => {
    const workflowIds = new Set(DATASCI_WORKFLOWS.map((w) => w.id));
    const directWorkflows = new Set([
      "datasci:feature-only",
      "datasci:validate-only",
      "datasci:interpret-only",
      "datasci:review-only",
    ]);
    for (const tt of DATASCI_TASK_TYPES) {
      const found = workflowIds.has(tt.defaultWorkflow) || directWorkflows.has(tt.defaultWorkflow);
      expect(found, `Task type ${tt.id} references unknown workflow ${tt.defaultWorkflow}`).toBe(true);
    }
  });

  it("getWorkflow returns undefined for unknown", () => {
    expect(getWorkflow("unknown")).toBeUndefined();
  });
});
