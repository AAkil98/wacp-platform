import { describe, it, expect } from "vitest";
import { HEALTHCARE_ROLES, HEALTHCARE_TASK_TYPES, getRole, getTaskType } from "../src/taxonomy.js";

describe("Healthcare Taxonomy", () => {
  it("defines 5 roles", () => {
    expect(HEALTHCARE_ROLES).toHaveLength(5);
  });

  it("all roles have unique IDs", () => {
    const ids = HEALTHCARE_ROLES.map((r) => r.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("clinician extends worker with clinical access", () => {
    const clinician = getRole("health:clinician")!;
    expect(clinician.extends).toBe("worker");
    expect(clinician.toolAccess).toBe("clinical");
    expect(clinician.autonomy).toBe("gated");
  });

  it("researcher extends worker with search-deidentify access", () => {
    const researcher = getRole("health:researcher")!;
    expect(researcher.extends).toBe("worker");
    expect(researcher.toolAccess).toBe("search-deidentify");
  });

  it("analyst extends worker with deidentified-analytics access", () => {
    const analyst = getRole("health:analyst")!;
    expect(analyst.extends).toBe("worker");
    expect(analyst.toolAccess).toBe("deidentified-analytics");
  });

  it("compliance extends observer with autonomous access", () => {
    const compliance = getRole("health:compliance")!;
    expect(compliance.extends).toBe("observer");
    expect(compliance.toolAccess).toBe("read-audit");
    expect(compliance.autonomy).toBe("autonomous");
  });

  it("coordinator extends worker with coordination-education access", () => {
    const coordinator = getRole("health:coordinator")!;
    expect(coordinator.extends).toBe("worker");
    expect(coordinator.toolAccess).toBe("coordination-education");
  });

  it("defines 8 task types", () => {
    expect(HEALTHCARE_TASK_TYPES).toHaveLength(8);
  });

  it("all task types have unique IDs", () => {
    const ids = HEALTHCARE_TASK_TYPES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("assess, diagnose_support, and report share patient-assessment workflow", () => {
    const assess = getTaskType("health:assess")!;
    const ddx = getTaskType("health:diagnose_support")!;
    const report = getTaskType("health:report")!;
    expect(assess.defaultWorkflow).toBe("health:patient-assessment");
    expect(ddx.defaultWorkflow).toBe("health:patient-assessment");
    expect(report.defaultWorkflow).toBe("health:patient-assessment");
  });

  it("audit task uses phi-audit workflow", () => {
    const audit = getTaskType("health:audit")!;
    expect(audit.defaultWorkflow).toBe("health:phi-audit");
    expect(audit.roles).toContain("health:compliance");
  });

  it("educate task uses patient-education workflow", () => {
    const educate = getTaskType("health:educate")!;
    expect(educate.defaultWorkflow).toBe("health:patient-education");
    expect(educate.roles).toContain("health:coordinator");
  });

  it("getRole returns undefined for unknown", () => {
    expect(getRole("unknown")).toBeUndefined();
  });

  it("getTaskType returns undefined for unknown", () => {
    expect(getTaskType("unknown")).toBeUndefined();
  });
});
