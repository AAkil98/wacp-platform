import { describe, it, expect } from "vitest";
import { ANALYTICS_PROFILES, getProfile, allProfiles } from "../src/profiles/profiles.js";

describe("Analytics Profiles", () => {
  it("defines 5 profiles (one per role)", () => {
    expect(ANALYTICS_PROFILES).toHaveLength(5);
  });

  it("all profiles have non-empty system prompts", () => {
    for (const profile of ANALYTICS_PROFILES) {
      expect(profile.systemPrompt.length).toBeGreaterThan(50);
    }
  });

  it("all profiles have at least one tool", () => {
    for (const profile of ANALYTICS_PROFILES) {
      expect(profile.tools.length).toBeGreaterThan(0);
    }
  });

  it("analyst has sql_query and profile tools", () => {
    const analyst = getProfile("analytics:analyst")!;
    expect(analyst.tools).toContain("sql_query");
    expect(analyst.tools).toContain("data_profile");
    expect(analyst.tools).toContain("schema_explore");
  });

  it("analyst does not have report or dashboard tools", () => {
    const analyst = getProfile("analytics:analyst")!;
    expect(analyst.tools).not.toContain("report_generate");
    expect(analyst.tools).not.toContain("dashboard_build");
  });

  it("modeler has metric_define", () => {
    const modeler = getProfile("analytics:modeler")!;
    expect(modeler.tools).toContain("metric_define");
    expect(modeler.tools).toContain("schema_explore");
  });

  it("reporter has report_generate and viz_create", () => {
    const reporter = getProfile("analytics:reporter")!;
    expect(reporter.tools).toContain("report_generate");
    expect(reporter.tools).toContain("viz_create");
    expect(reporter.tools).toContain("dashboard_build");
  });

  it("validator is autonomous", () => {
    const validator = getProfile("analytics:validator")!;
    expect(validator.autonomy).toBe("autonomous");
  });

  it("insights is autonomous", () => {
    const insights = getProfile("analytics:insights")!;
    expect(insights.autonomy).toBe("autonomous");
  });

  it("workers are gated", () => {
    const workers = ["analytics:analyst", "analytics:modeler", "analytics:reporter"];
    for (const roleId of workers) {
      const profile = getProfile(roleId)!;
      expect(profile.autonomy).toBe("gated");
    }
  });

  it("getProfile returns undefined for unknown", () => {
    expect(getProfile("unknown")).toBeUndefined();
  });

  it("allProfiles returns a copy", () => {
    const profiles = allProfiles();
    profiles.push({} as any);
    expect(ANALYTICS_PROFILES).toHaveLength(5);
  });
});
