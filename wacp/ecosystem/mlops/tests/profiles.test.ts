import { describe, it, expect } from "vitest";
import { MLOPS_PROFILES, getProfile, allProfiles } from "../src/profiles/profiles.js";

describe("MLOps Profiles", () => {
  it("defines 5 profiles (one per role)", () => {
    expect(MLOPS_PROFILES).toHaveLength(5);
  });

  it("all profiles have non-empty system prompts", () => {
    for (const profile of MLOPS_PROFILES) {
      expect(profile.systemPrompt.length).toBeGreaterThan(50);
    }
  });

  it("all profiles have at least one tool", () => {
    for (const profile of MLOPS_PROFILES) {
      expect(profile.tools.length).toBeGreaterThan(0);
    }
  });

  it("trainer has compute_budget tool", () => {
    const trainer = getProfile("mlops:trainer")!;
    expect(trainer.tools).toContain("compute_budget");
    expect(trainer.tools).toContain("train_launch");
  });

  it("researcher has experiment tools but no training", () => {
    const researcher = getProfile("mlops:researcher")!;
    expect(researcher.tools).toContain("dataset_validate");
    expect(researcher.tools).toContain("experiment_track");
    expect(researcher.tools).not.toContain("train_launch");
  });

  it("evaluator is autonomous", () => {
    const evaluator = getProfile("mlops:evaluator")!;
    expect(evaluator.autonomy).toBe("autonomous");
  });

  it("monitor is autonomous", () => {
    const monitor = getProfile("mlops:monitor")!;
    expect(monitor.autonomy).toBe("autonomous");
  });

  it("workers are gated", () => {
    const workers = ["mlops:researcher", "mlops:trainer", "mlops:deployer"];
    for (const roleId of workers) {
      const profile = getProfile(roleId)!;
      expect(profile.autonomy).toBe("gated");
    }
  });

  it("deployer has model_deploy and model_register", () => {
    const deployer = getProfile("mlops:deployer")!;
    expect(deployer.tools).toContain("model_deploy");
    expect(deployer.tools).toContain("model_register");
  });

  it("monitor has drift_detect", () => {
    const monitor = getProfile("mlops:monitor")!;
    expect(monitor.tools).toContain("drift_detect");
  });

  it("getProfile returns undefined for unknown", () => {
    expect(getProfile("unknown")).toBeUndefined();
  });

  it("allProfiles returns a copy", () => {
    const profiles = allProfiles();
    profiles.push({} as any);
    expect(MLOPS_PROFILES).toHaveLength(5);
  });
});
