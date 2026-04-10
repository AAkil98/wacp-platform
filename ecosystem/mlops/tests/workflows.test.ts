import { describe, it, expect } from "vitest";
import { MLOPS_WORKFLOWS, getWorkflow, validateWorkflow, allWorkflows } from "../src/workflows/workflows.js";
import { MLOPS_TASK_TYPES } from "../src/taxonomy.js";

describe("MLOps Workflows", () => {
  it("defines 4 workflows", () => {
    expect(MLOPS_WORKFLOWS).toHaveLength(4);
  });

  it("all workflows have unique IDs", () => {
    const ids = MLOPS_WORKFLOWS.map((w) => w.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("experiment has 4 stages: design → data-prep → train → evaluate", () => {
    const wf = getWorkflow("mlops:experiment")!;
    expect(wf.stages).toHaveLength(4);
    expect(wf.stages.map((s) => s.id)).toEqual(["design", "data-prep", "train", "evaluate"]);
  });

  it("deploy has 3 stages: validate → deploy → monitor", () => {
    const wf = getWorkflow("mlops:deploy")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["validate", "deploy", "monitor"]);
  });

  it("reproduce has 2 stages: verify → execute", () => {
    const wf = getWorkflow("mlops:reproduce")!;
    expect(wf.stages).toHaveLength(2);
    expect(wf.stages.map((s) => s.id)).toEqual(["verify", "execute"]);
  });

  it("optimize has 3 stages: analyze → propose → train", () => {
    const wf = getWorkflow("mlops:optimize")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["analyze", "propose", "train"]);
  });

  it("first stage of each workflow has no dependencies", () => {
    for (const wf of MLOPS_WORKFLOWS) {
      expect(wf.stages[0].dependsOn).toHaveLength(0);
    }
  });

  it("train stage in experiment is gated (compute budget)", () => {
    const wf = getWorkflow("mlops:experiment")!;
    expect(wf.stages.find((s) => s.id === "train")!.gated).toBe(true);
  });

  it("train stage in optimize is gated", () => {
    const wf = getWorkflow("mlops:optimize")!;
    expect(wf.stages.find((s) => s.id === "train")!.gated).toBe(true);
  });

  it("deploy stage is gated (model promotion)", () => {
    const wf = getWorkflow("mlops:deploy")!;
    expect(wf.stages.find((s) => s.id === "deploy")!.gated).toBe(true);
  });

  it("reproduce execute stage is not gated", () => {
    const wf = getWorkflow("mlops:reproduce")!;
    expect(wf.stages.find((s) => s.id === "execute")!.gated).toBe(false);
  });

  it("all workflows validate (no cycles, deps exist)", () => {
    for (const wf of MLOPS_WORKFLOWS) {
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
    expect(errors[0]).toContain("non-existent");
  });

  it("every task type references a defined workflow or direct", () => {
    const workflowIds = new Set(MLOPS_WORKFLOWS.map((w) => w.id));
    const directWorkflows = new Set(["mlops:evaluate-only", "mlops:monitor-only", "mlops:data-prep-only", "mlops:audit-only"]);
    for (const tt of MLOPS_TASK_TYPES) {
      const found = workflowIds.has(tt.defaultWorkflow) || directWorkflows.has(tt.defaultWorkflow);
      expect(found, `Task type ${tt.id} references unknown workflow ${tt.defaultWorkflow}`).toBe(true);
    }
  });

  it("getWorkflow returns undefined for unknown", () => {
    expect(getWorkflow("unknown")).toBeUndefined();
  });
});
