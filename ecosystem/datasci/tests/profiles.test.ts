import { describe, it, expect } from "vitest";
import { DATASCI_PROFILES, getProfile, allProfiles } from "../src/profiles/profiles.js";

describe("Data Science Profiles", () => {
  it("defines 5 profiles (one per role)", () => {
    expect(DATASCI_PROFILES).toHaveLength(5);
  });

  it("all profiles have non-empty system prompts", () => {
    for (const profile of DATASCI_PROFILES) {
      expect(profile.systemPrompt.length).toBeGreaterThan(50);
    }
  });

  it("all profiles have at least one tool", () => {
    for (const profile of DATASCI_PROFILES) {
      expect(profile.tools.length).toBeGreaterThan(0);
    }
  });

  it("explorer has stat_summary and correlation_analysis", () => {
    const explorer = getProfile("datasci:explorer")!;
    expect(explorer.tools).toContain("stat_summary");
    expect(explorer.tools).toContain("correlation_analysis");
  });

  it("explorer does not have hypothesis_test (exploration != testing)", () => {
    const explorer = getProfile("datasci:explorer")!;
    expect(explorer.tools).not.toContain("hypothesis_test");
  });

  it("statistician has hypothesis_test", () => {
    const statistician = getProfile("datasci:statistician")!;
    expect(statistician.tools).toContain("hypothesis_test");
    expect(statistician.tools).toContain("bootstrap_sample");
    expect(statistician.tools).toContain("causal_inference");
  });

  it("feature_engineer has extract and transform tools", () => {
    const fe = getProfile("datasci:feature_engineer")!;
    expect(fe.tools).toContain("feature_extract");
    expect(fe.tools).toContain("feature_transform");
  });

  it("modeler has model_fit and diagnostic_plots", () => {
    const modeler = getProfile("datasci:modeler")!;
    expect(modeler.tools).toContain("model_fit");
    expect(modeler.tools).toContain("diagnostic_plots");
  });

  it("reviewer is autonomous and read-only", () => {
    const reviewer = getProfile("datasci:reviewer")!;
    expect(reviewer.autonomy).toBe("autonomous");
    expect(reviewer.tools).not.toContain("model_fit");
    expect(reviewer.tools).not.toContain("hypothesis_test");
    expect(reviewer.tools).not.toContain("feature_transform");
  });

  it("workers are gated", () => {
    const workers = ["datasci:explorer", "datasci:statistician", "datasci:feature_engineer", "datasci:modeler"];
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
    expect(DATASCI_PROFILES).toHaveLength(5);
  });
});
