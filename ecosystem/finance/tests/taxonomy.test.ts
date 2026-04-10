import { describe, it, expect } from "vitest";
import { FINANCE_ROLES, FINANCE_TASK_TYPES, getRole, getTaskType } from "../src/taxonomy.js";

describe("Finance Taxonomy", () => {
  it("defines 5 roles", () => {
    expect(FINANCE_ROLES).toHaveLength(5);
  });

  it("all roles have unique IDs", () => {
    const ids = FINANCE_ROLES.map((r) => r.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("analyst extends worker with read-model access", () => {
    const analyst = getRole("finance:analyst")!;
    expect(analyst.extends).toBe("worker");
    expect(analyst.toolAccess).toBe("read-model");
    expect(analyst.autonomy).toBe("gated");
  });

  it("portfolio_manager extends worker with allocate-rebalance access", () => {
    const pm = getRole("finance:portfolio_manager")!;
    expect(pm.extends).toBe("worker");
    expect(pm.toolAccess).toBe("allocate-rebalance");
  });

  it("risk_officer extends worker with risk-read access", () => {
    const ro = getRole("finance:risk_officer")!;
    expect(ro.extends).toBe("worker");
    expect(ro.toolAccess).toBe("risk-read");
  });

  it("compliance_officer extends worker with compliance-kyc access", () => {
    const co = getRole("finance:compliance_officer")!;
    expect(co.extends).toBe("worker");
    expect(co.toolAccess).toBe("compliance-kyc");
  });

  it("auditor extends observer with autonomous access", () => {
    const auditor = getRole("finance:auditor")!;
    expect(auditor.extends).toBe("observer");
    expect(auditor.toolAccess).toBe("read-only");
    expect(auditor.autonomy).toBe("autonomous");
  });

  it("defines 9 task types", () => {
    expect(FINANCE_TASK_TYPES).toHaveLength(9);
  });

  it("all task types have unique IDs", () => {
    const ids = FINANCE_TASK_TYPES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("trade and rebalance use distinct workflows", () => {
    const trade = getTaskType("finance:trade")!;
    const rebalance = getTaskType("finance:rebalance")!;
    expect(trade.defaultWorkflow).not.toBe(rebalance.defaultWorkflow);
    expect(trade.defaultWorkflow).toBe("finance:trade-execution");
    expect(rebalance.defaultWorkflow).toBe("finance:portfolio-rebalance");
  });

  it("report task involves 4 roles", () => {
    const report = getTaskType("finance:report")!;
    expect(report.roles).toHaveLength(4);
    expect(report.defaultWorkflow).toBe("finance:full-report");
  });

  it("onboard task uses client-onboarding workflow", () => {
    const onboard = getTaskType("finance:onboard")!;
    expect(onboard.defaultWorkflow).toBe("finance:client-onboarding");
  });

  it("getRole returns undefined for unknown", () => {
    expect(getRole("unknown")).toBeUndefined();
  });

  it("getTaskType returns undefined for unknown", () => {
    expect(getTaskType("unknown")).toBeUndefined();
  });
});
