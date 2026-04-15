import { describe, it, expect } from "vitest";
import { SWE_ROLES, SWE_TASK_TYPES, getRole, getTaskType } from "../src/taxonomy.js";

describe("SWE Taxonomy", () => {
  it("defines 4 roles", () => {
    expect(SWE_ROLES).toHaveLength(4);
  });

  it("all roles have unique IDs", () => {
    const ids = SWE_ROLES.map((r) => r.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("planner extends worker with read-only access", () => {
    const planner = getRole("swe:planner")!;
    expect(planner.extends).toBe("worker");
    expect(planner.toolAccess).toBe("read-only");
    expect(planner.autonomy).toBe("gated");
  });

  it("reviewer extends observer with autonomous access", () => {
    const reviewer = getRole("swe:reviewer")!;
    expect(reviewer.extends).toBe("observer");
    expect(reviewer.autonomy).toBe("autonomous");
  });

  it("implementer has read-write access", () => {
    expect(getRole("swe:implementer")!.toolAccess).toBe("read-write");
  });

  it("defines 7 task types", () => {
    expect(SWE_TASK_TYPES).toHaveLength(7);
  });

  it("all task types have unique IDs", () => {
    const ids = SWE_TASK_TYPES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("implement task involves all 4 roles", () => {
    const impl = getTaskType("swe:implement")!;
    expect(impl.roles).toHaveLength(4);
    expect(impl.defaultWorkflow).toBe("swe:implement-feature");
  });

  it("debug task excludes reviewer", () => {
    const debug = getTaskType("swe:debug")!;
    expect(debug.roles).not.toContain("swe:reviewer");
    expect(debug.roles).toHaveLength(3);
  });

  it("getRole returns undefined for unknown", () => {
    expect(getRole("unknown")).toBeUndefined();
  });

  it("getTaskType returns undefined for unknown", () => {
    expect(getTaskType("unknown")).toBeUndefined();
  });
});
