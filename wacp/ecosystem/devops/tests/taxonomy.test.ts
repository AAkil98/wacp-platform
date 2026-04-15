import { describe, it, expect } from "vitest";
import { DEVOPS_ROLES, DEVOPS_TASK_TYPES, ENVIRONMENT_TIERS, getRole, getTaskType, requiresHumanApproval } from "../src/taxonomy.js";

describe("DevOps Taxonomy", () => {
  it("defines 5 roles", () => {
    expect(DEVOPS_ROLES).toHaveLength(5);
  });

  it("all roles have unique IDs", () => {
    const ids = DEVOPS_ROLES.map((r) => r.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("architect extends worker with read-plan access", () => {
    const architect = getRole("devops:architect")!;
    expect(architect.extends).toBe("worker");
    expect(architect.toolAccess).toBe("read-plan");
    expect(architect.autonomy).toBe("gated");
  });

  it("deployer extends worker with execute access", () => {
    const deployer = getRole("devops:deployer")!;
    expect(deployer.extends).toBe("worker");
    expect(deployer.toolAccess).toBe("execute");
    expect(deployer.autonomy).toBe("gated");
  });

  it("monitor extends observer with autonomous access", () => {
    const monitor = getRole("devops:monitor")!;
    expect(monitor.extends).toBe("observer");
    expect(monitor.toolAccess).toBe("read-query");
    expect(monitor.autonomy).toBe("autonomous");
  });

  it("responder extends worker with execute-query access", () => {
    const responder = getRole("devops:responder")!;
    expect(responder.extends).toBe("worker");
    expect(responder.toolAccess).toBe("execute-query");
    expect(responder.autonomy).toBe("gated");
  });

  it("auditor extends observer with autonomous access", () => {
    const auditor = getRole("devops:auditor")!;
    expect(auditor.extends).toBe("observer");
    expect(auditor.toolAccess).toBe("read-scan");
    expect(auditor.autonomy).toBe("autonomous");
  });

  it("defines 9 task types", () => {
    expect(DEVOPS_TASK_TYPES).toHaveLength(9);
  });

  it("all task types have unique IDs", () => {
    const ids = DEVOPS_TASK_TYPES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("deploy task involves all 4 distinct roles", () => {
    const deploy = getTaskType("devops:deploy")!;
    const uniqueRoles = new Set(deploy.roles);
    expect(uniqueRoles.size).toBe(4);
    expect(deploy.defaultWorkflow).toBe("devops:deploy");
  });

  it("deploy and migrate share the same workflow", () => {
    const deploy = getTaskType("devops:deploy")!;
    const migrate = getTaskType("devops:migrate")!;
    expect(deploy.defaultWorkflow).toBe(migrate.defaultWorkflow);
  });

  it("provision and configure share the same workflow", () => {
    const provision = getTaskType("devops:provision")!;
    const configure = getTaskType("devops:configure")!;
    expect(provision.defaultWorkflow).toBe(configure.defaultWorkflow);
  });

  it("monitor task is single-role", () => {
    const monitor = getTaskType("devops:monitor")!;
    expect(monitor.roles).toHaveLength(1);
    expect(monitor.roles[0]).toBe("devops:monitor");
  });

  it("getRole returns undefined for unknown", () => {
    expect(getRole("unknown")).toBeUndefined();
  });

  it("getTaskType returns undefined for unknown", () => {
    expect(getTaskType("unknown")).toBeUndefined();
  });
});

describe("Environment Tiers", () => {
  it("defines 3 tiers", () => {
    expect(ENVIRONMENT_TIERS).toHaveLength(3);
    expect(ENVIRONMENT_TIERS).toContain("dev");
    expect(ENVIRONMENT_TIERS).toContain("staging");
    expect(ENVIRONMENT_TIERS).toContain("production");
  });

  it("production requires human approval", () => {
    expect(requiresHumanApproval("production")).toBe(true);
  });

  it("dev does not require human approval", () => {
    expect(requiresHumanApproval("dev")).toBe(false);
  });

  it("staging does not require human approval", () => {
    expect(requiresHumanApproval("staging")).toBe(false);
  });
});
