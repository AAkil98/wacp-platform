import { describe, it, expect } from "vitest";
import { DEVOPS_WORKFLOWS, getWorkflow, validateWorkflow, allWorkflows } from "../src/workflows/workflows.js";
import { DEVOPS_TASK_TYPES } from "../src/taxonomy.js";

describe("DevOps Workflows", () => {
  it("defines 5 workflows", () => {
    expect(DEVOPS_WORKFLOWS).toHaveLength(5);
  });

  it("all workflows have unique IDs", () => {
    const ids = DEVOPS_WORKFLOWS.map((w) => w.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("deploy has 4 stages: plan → validate → execute → verify", () => {
    const wf = getWorkflow("devops:deploy")!;
    expect(wf.stages).toHaveLength(4);
    expect(wf.stages.map((s) => s.id)).toEqual(["plan", "validate", "execute", "verify"]);
  });

  it("provision has 3 stages: plan → execute → verify", () => {
    const wf = getWorkflow("devops:provision")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["plan", "execute", "verify"]);
  });

  it("respond has 3 stages: triage → mitigate → postmortem", () => {
    const wf = getWorkflow("devops:respond")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["triage", "mitigate", "postmortem"]);
  });

  it("secure has 3 stages: scan → remediate → verify", () => {
    const wf = getWorkflow("devops:secure")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["scan", "remediate", "verify"]);
  });

  it("optimize has 3 stages: analyze → propose → validate", () => {
    const wf = getWorkflow("devops:optimize")!;
    expect(wf.stages).toHaveLength(3);
    expect(wf.stages.map((s) => s.id)).toEqual(["analyze", "propose", "validate"]);
  });

  it("first stage of each workflow has no dependencies", () => {
    for (const wf of DEVOPS_WORKFLOWS) {
      expect(wf.stages[0].dependsOn).toHaveLength(0);
    }
  });

  it("execute stage in deploy depends on validate", () => {
    const wf = getWorkflow("devops:deploy")!;
    const execute = wf.stages.find((s) => s.id === "execute")!;
    expect(execute.dependsOn).toContain("validate");
  });

  it("execute stages are gated (mutation requires approval)", () => {
    const deploy = getWorkflow("devops:deploy")!;
    expect(deploy.stages.find((s) => s.id === "execute")!.gated).toBe(true);

    const provision = getWorkflow("devops:provision")!;
    expect(provision.stages.find((s) => s.id === "execute")!.gated).toBe(true);
  });

  it("mitigate stage is gated", () => {
    const respond = getWorkflow("devops:respond")!;
    expect(respond.stages.find((s) => s.id === "mitigate")!.gated).toBe(true);
  });

  it("remediate stage is gated", () => {
    const secure = getWorkflow("devops:secure")!;
    expect(secure.stages.find((s) => s.id === "remediate")!.gated).toBe(true);
  });

  it("all workflows validate (no cycles, deps exist)", () => {
    for (const wf of DEVOPS_WORKFLOWS) {
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
    const workflowIds = new Set(DEVOPS_WORKFLOWS.map((w) => w.id));
    const directWorkflows = new Set(["devops:monitor-only", "devops:audit"]);
    for (const tt of DEVOPS_TASK_TYPES) {
      const found = workflowIds.has(tt.defaultWorkflow) || directWorkflows.has(tt.defaultWorkflow);
      expect(found, `Task type ${tt.id} references unknown workflow ${tt.defaultWorkflow}`).toBe(true);
    }
  });

  it("getWorkflow returns undefined for unknown", () => {
    expect(getWorkflow("unknown")).toBeUndefined();
  });
});
