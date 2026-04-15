import { describe, it, expect } from "vitest";
import { SWE_PROFILES, getProfile, allProfiles } from "../src/profiles/profiles.js";

describe("SWE Profiles", () => {
  it("defines 4 profiles (one per role)", () => {
    expect(SWE_PROFILES).toHaveLength(4);
  });

  it("all profiles have non-empty system prompts", () => {
    for (const profile of SWE_PROFILES) {
      expect(profile.systemPrompt.length).toBeGreaterThan(50);
    }
  });

  it("all profiles have at least one tool", () => {
    for (const profile of SWE_PROFILES) {
      expect(profile.tools.length).toBeGreaterThan(0);
    }
  });

  it("planner has no write tools", () => {
    const planner = getProfile("swe:planner")!;
    expect(planner.tools).not.toContain("write_file");
    expect(planner.tools).not.toContain("code_edit");
    expect(planner.tools).not.toContain("git_commit");
  });

  it("planner has read tools", () => {
    const planner = getProfile("swe:planner")!;
    expect(planner.tools).toContain("read_file");
    expect(planner.tools).toContain("code_search");
  });

  it("implementer has write tools", () => {
    const impl = getProfile("swe:implementer")!;
    expect(impl.tools).toContain("write_file");
    expect(impl.tools).toContain("code_edit");
    expect(impl.tools).toContain("git_commit");
  });

  it("tester has test_run", () => {
    const tester = getProfile("swe:tester")!;
    expect(tester.tools).toContain("test_run");
  });

  it("reviewer is autonomous", () => {
    const reviewer = getProfile("swe:reviewer")!;
    expect(reviewer.autonomy).toBe("autonomous");
  });

  it("all other roles are gated", () => {
    for (const profile of SWE_PROFILES) {
      if (profile.roleId !== "swe:reviewer") {
        expect(profile.autonomy).toBe("gated");
      }
    }
  });

  it("getProfile returns undefined for unknown", () => {
    expect(getProfile("unknown")).toBeUndefined();
  });

  it("allProfiles returns a copy", () => {
    const profiles = allProfiles();
    profiles.push({} as any);
    expect(SWE_PROFILES).toHaveLength(4); // original unchanged
  });
});
