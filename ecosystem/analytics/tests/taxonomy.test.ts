import { describe, it, expect } from "vitest";
import { ANALYTICS_ROLES, ANALYTICS_TASK_TYPES, getRole, getTaskType } from "../src/taxonomy.js";

describe("Analytics Taxonomy", () => {
  it("defines 5 roles", () => {
    expect(ANALYTICS_ROLES).toHaveLength(5);
  });

  it("all roles have unique IDs", () => {
    const ids = ANALYTICS_ROLES.map((r) => r.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("analyst extends worker with query-profile access", () => {
    const analyst = getRole("analytics:analyst")!;
    expect(analyst.extends).toBe("worker");
    expect(analyst.toolAccess).toBe("query-profile");
    expect(analyst.autonomy).toBe("gated");
  });

  it("modeler extends worker with schema-metric access", () => {
    const modeler = getRole("analytics:modeler")!;
    expect(modeler.extends).toBe("worker");
    expect(modeler.toolAccess).toBe("schema-metric");
  });

  it("validator extends observer with autonomous access", () => {
    const validator = getRole("analytics:validator")!;
    expect(validator.extends).toBe("observer");
    expect(validator.autonomy).toBe("autonomous");
  });

  it("insights extends observer with autonomous access", () => {
    const insights = getRole("analytics:insights")!;
    expect(insights.extends).toBe("observer");
    expect(insights.autonomy).toBe("autonomous");
  });

  it("defines 8 task types", () => {
    expect(ANALYTICS_TASK_TYPES).toHaveLength(8);
  });

  it("all task types have unique IDs", () => {
    const ids = ANALYTICS_TASK_TYPES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("query, report, and analyze share the same workflow", () => {
    const q = getTaskType("analytics:query")!;
    const r = getTaskType("analytics:report")!;
    const a = getTaskType("analytics:analyze")!;
    expect(q.defaultWorkflow).toBe(r.defaultWorkflow);
    expect(r.defaultWorkflow).toBe(a.defaultWorkflow);
  });

  it("investigate uses the investigate workflow", () => {
    const inv = getTaskType("analytics:investigate")!;
    expect(inv.defaultWorkflow).toBe("analytics:investigate");
    expect(inv.roles).toContain("analytics:insights");
  });

  it("validate and monitor are single-role tasks", () => {
    const v = getTaskType("analytics:validate")!;
    const m = getTaskType("analytics:monitor")!;
    expect(v.roles).toHaveLength(1);
    expect(m.roles).toHaveLength(1);
  });

  it("getRole returns undefined for unknown", () => {
    expect(getRole("unknown")).toBeUndefined();
  });

  it("getTaskType returns undefined for unknown", () => {
    expect(getTaskType("unknown")).toBeUndefined();
  });
});
