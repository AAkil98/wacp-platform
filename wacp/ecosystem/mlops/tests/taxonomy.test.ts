import { describe, it, expect } from "vitest";
import { MLOPS_ROLES, MLOPS_TASK_TYPES, getRole, getTaskType } from "../src/taxonomy.js";

describe("MLOps Taxonomy", () => {
  it("defines 5 roles", () => {
    expect(MLOPS_ROLES).toHaveLength(5);
  });

  it("all roles have unique IDs", () => {
    const ids = MLOPS_ROLES.map((r) => r.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("researcher extends worker with read-experiment access", () => {
    const researcher = getRole("mlops:researcher")!;
    expect(researcher.extends).toBe("worker");
    expect(researcher.toolAccess).toBe("read-experiment");
    expect(researcher.autonomy).toBe("gated");
  });

  it("trainer extends worker with compute-exec access", () => {
    const trainer = getRole("mlops:trainer")!;
    expect(trainer.extends).toBe("worker");
    expect(trainer.toolAccess).toBe("compute-exec");
    expect(trainer.autonomy).toBe("gated");
  });

  it("evaluator extends observer with autonomous access", () => {
    const evaluator = getRole("mlops:evaluator")!;
    expect(evaluator.extends).toBe("observer");
    expect(evaluator.toolAccess).toBe("read-benchmark");
    expect(evaluator.autonomy).toBe("autonomous");
  });

  it("deployer extends worker with execute-registry access", () => {
    const deployer = getRole("mlops:deployer")!;
    expect(deployer.extends).toBe("worker");
    expect(deployer.toolAccess).toBe("execute-registry");
    expect(deployer.autonomy).toBe("gated");
  });

  it("monitor extends observer with autonomous access", () => {
    const monitor = getRole("mlops:monitor")!;
    expect(monitor.extends).toBe("observer");
    expect(monitor.toolAccess).toBe("read-monitor");
    expect(monitor.autonomy).toBe("autonomous");
  });

  it("defines 9 task types", () => {
    expect(MLOPS_TASK_TYPES).toHaveLength(9);
  });

  it("all task types have unique IDs", () => {
    const ids = MLOPS_TASK_TYPES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("experiment task involves 3 roles", () => {
    const experiment = getTaskType("mlops:experiment")!;
    expect(experiment.roles).toHaveLength(3);
    expect(experiment.defaultWorkflow).toBe("mlops:experiment");
  });

  it("train and optimize share the same workflow", () => {
    const train = getTaskType("mlops:train")!;
    const optimize = getTaskType("mlops:optimize")!;
    expect(train.defaultWorkflow).toBe(optimize.defaultWorkflow);
  });

  it("direct task types are single-role", () => {
    const directTypes = ["mlops:evaluate", "mlops:monitor", "mlops:data-prep", "mlops:audit"];
    for (const id of directTypes) {
      const tt = getTaskType(id)!;
      expect(tt.roles).toHaveLength(1);
    }
  });

  it("getRole returns undefined for unknown", () => {
    expect(getRole("unknown")).toBeUndefined();
  });

  it("getTaskType returns undefined for unknown", () => {
    expect(getTaskType("unknown")).toBeUndefined();
  });
});
