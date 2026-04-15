import { describe, it, expect } from "vitest";
import { ANALYTICS_WORKFLOWS, getWorkflow, validateWorkflow, allWorkflows } from "../src/workflows/workflows.js";
import { ANALYTICS_TASK_TYPES } from "../src/taxonomy.js";

describe("Analytics Workflows", () => {
  it("defines 4 workflows", () => {
    expect(ANALYTICS_WORKFLOWS).toHaveLength(4);
  });

  it("all workflows have unique IDs", () => {
    const ids = ANALYTICS_WORKFLOWS.map((w) => w.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("query-and-report has 3 stages: query → validate → report", () => {
    const wf = getWorkflow("analytics:query-and-report")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["query", "validate", "report"]);
  });

  it("build-dashboard has 3 stages: model → build → validate", () => {
    const wf = getWorkflow("analytics:build-dashboard")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["model", "build", "validate"]);
  });

  it("investigate has 3 stages: explore → reconcile → report", () => {
    const wf = getWorkflow("analytics:investigate")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["explore", "reconcile", "report"]);
  });

  it("model-data has 3 stages: explore → model → validate", () => {
    const wf = getWorkflow("analytics:model-data")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["explore", "model", "validate"]);
  });

  it("first stage of each workflow has no dependencies", () => {
    for (const wf of ANALYTICS_WORKFLOWS) {
      expect(wf.stages[0].dependsOn).toHaveLength(0);
    }
  });

  it("report stage in query-and-report is gated", () => {
    const wf = getWorkflow("analytics:query-and-report")!;
    expect(wf.stages.find((s) => s.id === "report")!.gated).toBe(true);
  });

  it("build stage in build-dashboard is gated", () => {
    const wf = getWorkflow("analytics:build-dashboard")!;
    expect(wf.stages.find((s) => s.id === "build")!.gated).toBe(true);
  });

  it("model stage in model-data is gated", () => {
    const wf = getWorkflow("analytics:model-data")!;
    expect(wf.stages.find((s) => s.id === "model")!.gated).toBe(true);
  });

  it("investigate stages are not gated (read-only throughout)", () => {
    const wf = getWorkflow("analytics:investigate")!;
    for (const stage of wf.stages) {
      expect(stage.gated).toBe(false);
    }
  });

  it("all workflows validate (no cycles, deps exist)", () => {
    for (const wf of ANALYTICS_WORKFLOWS) {
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
    const workflowIds = new Set(ANALYTICS_WORKFLOWS.map((w) => w.id));
    const directWorkflows = new Set(["analytics:validate-only", "analytics:monitor-only"]);
    for (const tt of ANALYTICS_TASK_TYPES) {
      const found = workflowIds.has(tt.defaultWorkflow) || directWorkflows.has(tt.defaultWorkflow);
      expect(found, `Task type ${tt.id} references unknown workflow ${tt.defaultWorkflow}`).toBe(true);
    }
  });

  it("getWorkflow returns undefined for unknown", () => {
    expect(getWorkflow("unknown")).toBeUndefined();
  });
});
