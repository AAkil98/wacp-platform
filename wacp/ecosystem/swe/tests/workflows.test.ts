import { describe, it, expect } from "vitest";
import { SWE_WORKFLOWS, getWorkflow, validateWorkflow, allWorkflows } from "../src/workflows/workflows.js";

describe("SWE Workflows", () => {
  it("defines 4 workflows", () => {
    expect(SWE_WORKFLOWS).toHaveLength(4);
  });

  it("all workflows have unique IDs", () => {
    const ids = SWE_WORKFLOWS.map((w) => w.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("implement-feature has 4 stages", () => {
    const wf = getWorkflow("swe:implement-feature")!;
    expect(wf.stages).toHaveLength(4);
    expect(wf.stages.map((s) => s.id)).toEqual(["plan", "implement", "test", "review"]);
  });

  it("fix-bug has 3 stages (no review)", () => {
    const wf = getWorkflow("swe:fix-bug")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).not.toContain("review");
  });

  it("write-tests has 2 stages", () => {
    const wf = getWorkflow("swe:write-tests")!;
    expect(wf.stages).toHaveLength(2);
  });

  it("plan stage has no dependencies", () => {
    for (const wf of SWE_WORKFLOWS) {
      const plan = wf.stages.find((s) => s.id === "plan");
      if (plan) {
        expect(plan.dependsOn).toHaveLength(0);
      }
    }
  });

  it("implement depends on plan", () => {
    const wf = getWorkflow("swe:implement-feature")!;
    const impl = wf.stages.find((s) => s.id === "implement")!;
    expect(impl.dependsOn).toContain("plan");
  });

  it("test depends on implement", () => {
    const wf = getWorkflow("swe:implement-feature")!;
    const test = wf.stages.find((s) => s.id === "test")!;
    expect(test.dependsOn).toContain("implement");
  });

  it("implement stage is gated (requires human approval)", () => {
    const wf = getWorkflow("swe:implement-feature")!;
    expect(wf.stages.find((s) => s.id === "implement")!.gated).toBe(true);
  });

  it("review stage is gated", () => {
    const wf = getWorkflow("swe:implement-feature")!;
    expect(wf.stages.find((s) => s.id === "review")!.gated).toBe(true);
  });

  it("plan stage is not gated", () => {
    const wf = getWorkflow("swe:implement-feature")!;
    expect(wf.stages.find((s) => s.id === "plan")!.gated).toBe(false);
  });

  it("all workflows validate (no cycles, deps exist)", () => {
    for (const wf of SWE_WORKFLOWS) {
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

  it("getWorkflow returns undefined for unknown", () => {
    expect(getWorkflow("unknown")).toBeUndefined();
  });
});
