import { describe, it, expect } from "vitest";
import { FINANCE_WORKFLOWS, getWorkflow, validateWorkflow, allWorkflows } from "../src/workflows/workflows.js";
import { FINANCE_TASK_TYPES } from "../src/taxonomy.js";

describe("Finance Workflows", () => {
  it("defines 4 workflows", () => {
    expect(FINANCE_WORKFLOWS).toHaveLength(4);
  });

  it("all workflows have unique IDs", () => {
    const ids = FINANCE_WORKFLOWS.map((w) => w.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("trade-execution has 4 stages", () => {
    const wf = getWorkflow("finance:trade-execution")!;
    expect(wf.stages).toHaveLength(4);
    expect(wf.stages.map((s) => s.id)).toEqual(["analyze", "compliance", "execute", "record"]);
  });

  it("portfolio-rebalance has 4 stages: assess → propose → compliance → execute", () => {
    const wf = getWorkflow("finance:portfolio-rebalance")!;
    expect(wf.stages).toHaveLength(4);
    expect(wf.stages.map((s) => s.id)).toEqual(["assess", "propose", "compliance", "execute"]);
  });

  it("full-report has 5 stages", () => {
    const wf = getWorkflow("finance:full-report")!;
    expect(wf.stages).toHaveLength(5);
    expect(wf.stages.map((s) => s.id)).toEqual(["collect", "analyze", "risk", "compliance", "publish"]);
  });

  it("client-onboarding has 3 stages: kyc → suitability → approve", () => {
    const wf = getWorkflow("finance:client-onboarding")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["kyc", "suitability", "approve"]);
  });

  it("first stage of each workflow has no dependencies", () => {
    for (const wf of FINANCE_WORKFLOWS) {
      expect(wf.stages[0].dependsOn).toHaveLength(0);
    }
  });

  it("compliance stage is gated in trade-execution", () => {
    const wf = getWorkflow("finance:trade-execution")!;
    expect(wf.stages.find((s) => s.id === "compliance")!.gated).toBe(true);
  });

  it("execute stage is gated in trade-execution", () => {
    const wf = getWorkflow("finance:trade-execution")!;
    expect(wf.stages.find((s) => s.id === "execute")!.gated).toBe(true);
  });

  it("compliance and execute are gated in portfolio-rebalance", () => {
    const wf = getWorkflow("finance:portfolio-rebalance")!;
    expect(wf.stages.find((s) => s.id === "compliance")!.gated).toBe(true);
    expect(wf.stages.find((s) => s.id === "execute")!.gated).toBe(true);
  });

  it("full-report compliance and publish are gated", () => {
    const wf = getWorkflow("finance:full-report")!;
    expect(wf.stages.find((s) => s.id === "compliance")!.gated).toBe(true);
    expect(wf.stages.find((s) => s.id === "publish")!.gated).toBe(true);
  });

  it("client-onboarding all stages gated", () => {
    const wf = getWorkflow("finance:client-onboarding")!;
    for (const stage of wf.stages) {
      expect(stage.gated).toBe(true);
    }
  });

  it("all workflows validate (no cycles, deps exist)", () => {
    for (const wf of FINANCE_WORKFLOWS) {
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
    const workflowIds = new Set(FINANCE_WORKFLOWS.map((w) => w.id));
    const directWorkflows = new Set([
      "finance:analyze-only",
      "finance:model-only",
      "finance:risk-only",
      "finance:compliance-only",
      "finance:audit-only",
    ]);
    for (const tt of FINANCE_TASK_TYPES) {
      const found = workflowIds.has(tt.defaultWorkflow) || directWorkflows.has(tt.defaultWorkflow);
      expect(found, `Task type ${tt.id} references unknown workflow ${tt.defaultWorkflow}`).toBe(true);
    }
  });

  it("getWorkflow returns undefined for unknown", () => {
    expect(getWorkflow("unknown")).toBeUndefined();
  });
});
