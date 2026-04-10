import { describe, it, expect } from "vitest";
import { HEALTHCARE_WORKFLOWS, getWorkflow, validateWorkflow, allWorkflows } from "../src/workflows/workflows.js";
import { HEALTHCARE_TASK_TYPES } from "../src/taxonomy.js";

describe("Healthcare Workflows", () => {
  it("defines 4 workflows", () => {
    expect(HEALTHCARE_WORKFLOWS).toHaveLength(4);
  });

  it("all workflows have unique IDs", () => {
    const ids = HEALTHCARE_WORKFLOWS.map((w) => w.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("patient-assessment has 4 stages: consent → gather → analyze → report", () => {
    const wf = getWorkflow("health:patient-assessment")!;
    expect(wf.stages).toHaveLength(4);
    expect(wf.stages.map((s) => s.id)).toEqual(["consent", "gather", "analyze", "report"]);
  });

  it("literature-research has 4 stages: question → search → synthesize → review", () => {
    const wf = getWorkflow("health:literature-research")!;
    expect(wf.stages).toHaveLength(4);
    expect(wf.stages.map((s) => s.id)).toEqual(["question", "search", "synthesize", "review"]);
  });

  it("phi-audit has 2 stages: scan → report", () => {
    const wf = getWorkflow("health:phi-audit")!;
    expect(wf.stages).toHaveLength(2);
    expect(wf.stages.map((s) => s.id)).toEqual(["scan", "report"]);
  });

  it("patient-education has 2 stages: assess_level → generate", () => {
    const wf = getWorkflow("health:patient-education")!;
    expect(wf.stages).toHaveLength(2);
    expect(wf.stages.map((s) => s.id)).toEqual(["assess_level", "generate"]);
  });

  it("first stage of each workflow has no dependencies", () => {
    for (const wf of HEALTHCARE_WORKFLOWS) {
      expect(wf.stages[0].dependsOn).toHaveLength(0);
    }
  });

  it("all stages of patient-assessment are gated (clinician sign-off discipline)", () => {
    const wf = getWorkflow("health:patient-assessment")!;
    for (const stage of wf.stages) {
      expect(stage.gated).toBe(true);
    }
  });

  it("literature-research review stage is gated (compliance check for PHI leakage)", () => {
    const wf = getWorkflow("health:literature-research")!;
    expect(wf.stages.find((s) => s.id === "review")!.gated).toBe(true);
  });

  it("literature-research search and synthesize are not gated (public sources)", () => {
    const wf = getWorkflow("health:literature-research")!;
    expect(wf.stages.find((s) => s.id === "search")!.gated).toBe(false);
    expect(wf.stages.find((s) => s.id === "synthesize")!.gated).toBe(false);
  });

  it("phi-audit stages are not gated (compliance is observer-only and autonomous)", () => {
    const wf = getWorkflow("health:phi-audit")!;
    for (const stage of wf.stages) {
      expect(stage.gated).toBe(false);
    }
  });

  it("patient-education stages are all gated (patient-facing content)", () => {
    const wf = getWorkflow("health:patient-education")!;
    for (const stage of wf.stages) {
      expect(stage.gated).toBe(true);
    }
  });

  it("all workflows validate (no cycles, deps exist)", () => {
    for (const wf of HEALTHCARE_WORKFLOWS) {
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
    const workflowIds = new Set(HEALTHCARE_WORKFLOWS.map((w) => w.id));
    const directWorkflows = new Set([
      "health:analyze-only",
      "health:monitor-only",
    ]);
    for (const tt of HEALTHCARE_TASK_TYPES) {
      const found = workflowIds.has(tt.defaultWorkflow) || directWorkflows.has(tt.defaultWorkflow);
      expect(found, `Task type ${tt.id} references unknown workflow ${tt.defaultWorkflow}`).toBe(true);
    }
  });

  it("getWorkflow returns undefined for unknown", () => {
    expect(getWorkflow("unknown")).toBeUndefined();
  });
});
