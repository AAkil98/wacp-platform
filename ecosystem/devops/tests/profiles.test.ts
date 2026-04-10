import { describe, it, expect } from "vitest";
import { DEVOPS_PROFILES, getProfile, allProfiles } from "../src/profiles/profiles.js";

describe("DevOps Profiles", () => {
  it("defines 5 profiles (one per role)", () => {
    expect(DEVOPS_PROFILES).toHaveLength(5);
  });

  it("all profiles have non-empty system prompts", () => {
    for (const profile of DEVOPS_PROFILES) {
      expect(profile.systemPrompt.length).toBeGreaterThan(50);
    }
  });

  it("all profiles have at least one tool", () => {
    for (const profile of DEVOPS_PROFILES) {
      expect(profile.tools.length).toBeGreaterThan(0);
    }
  });

  it("architect has no execute tools", () => {
    const architect = getProfile("devops:architect")!;
    expect(architect.tools).not.toContain("deploy_execute");
    expect(architect.tools).not.toContain("rollback");
  });

  it("architect has plan tools", () => {
    const architect = getProfile("devops:architect")!;
    expect(architect.tools).toContain("infra_plan");
    expect(architect.tools).toContain("config_validate");
    expect(architect.tools).toContain("compliance_scan");
  });

  it("deployer has execute and rollback", () => {
    const deployer = getProfile("devops:deployer")!;
    expect(deployer.tools).toContain("deploy_execute");
    expect(deployer.tools).toContain("rollback");
    expect(deployer.tools).toContain("health_check");
  });

  it("monitor is autonomous", () => {
    const monitor = getProfile("devops:monitor")!;
    expect(monitor.autonomy).toBe("autonomous");
  });

  it("auditor is autonomous", () => {
    const auditor = getProfile("devops:auditor")!;
    expect(auditor.autonomy).toBe("autonomous");
  });

  it("workers are gated", () => {
    const workers = ["devops:architect", "devops:deployer", "devops:responder"];
    for (const roleId of workers) {
      const profile = getProfile(roleId)!;
      expect(profile.autonomy).toBe("gated");
    }
  });

  it("responder has both query and execute tools", () => {
    const responder = getProfile("devops:responder")!;
    expect(responder.tools).toContain("log_query");
    expect(responder.tools).toContain("metric_query");
    expect(responder.tools).toContain("deploy_execute");
    expect(responder.tools).toContain("rollback");
  });

  it("auditor has scan but no execute", () => {
    const auditor = getProfile("devops:auditor")!;
    expect(auditor.tools).toContain("compliance_scan");
    expect(auditor.tools).not.toContain("deploy_execute");
    expect(auditor.tools).not.toContain("rollback");
  });

  it("getProfile returns undefined for unknown", () => {
    expect(getProfile("unknown")).toBeUndefined();
  });

  it("allProfiles returns a copy", () => {
    const profiles = allProfiles();
    profiles.push({} as any);
    expect(DEVOPS_PROFILES).toHaveLength(5); // original unchanged
  });
});
