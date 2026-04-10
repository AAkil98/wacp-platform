import { describe, it, expect } from "vitest";
import { FINANCE_PROFILES, getProfile, allProfiles } from "../src/profiles/profiles.js";

describe("Finance Profiles", () => {
  it("defines 5 profiles (one per role)", () => {
    expect(FINANCE_PROFILES).toHaveLength(5);
  });

  it("all profiles have non-empty system prompts", () => {
    for (const profile of FINANCE_PROFILES) {
      expect(profile.systemPrompt.length).toBeGreaterThan(50);
    }
  });

  it("all profiles have at least one tool", () => {
    for (const profile of FINANCE_PROFILES) {
      expect(profile.tools.length).toBeGreaterThan(0);
    }
  });

  it("analyst has market_data_fetch and financial_model_build", () => {
    const analyst = getProfile("finance:analyst")!;
    expect(analyst.tools).toContain("market_data_fetch");
    expect(analyst.tools).toContain("financial_model_build");
  });

  it("analyst does not have trade_execute (analysis != execution)", () => {
    const analyst = getProfile("finance:analyst")!;
    expect(analyst.tools).not.toContain("trade_execute");
  });

  it("portfolio_manager has trade_execute and portfolio_rebalance", () => {
    const pm = getProfile("finance:portfolio_manager")!;
    expect(pm.tools).toContain("trade_execute");
    expect(pm.tools).toContain("portfolio_rebalance");
  });

  it("compliance_officer has compliance_check and kyc_screen", () => {
    const co = getProfile("finance:compliance_officer")!;
    expect(co.tools).toContain("compliance_check");
    expect(co.tools).toContain("kyc_screen");
  });

  it("compliance_officer does not have trade_execute (separation of duties)", () => {
    const co = getProfile("finance:compliance_officer")!;
    expect(co.tools).not.toContain("trade_execute");
  });

  it("risk_officer has risk_calc and not trade_execute", () => {
    const ro = getProfile("finance:risk_officer")!;
    expect(ro.tools).toContain("risk_calc");
    expect(ro.tools).not.toContain("trade_execute");
  });

  it("auditor is autonomous and read-only (no execution tools)", () => {
    const auditor = getProfile("finance:auditor")!;
    expect(auditor.autonomy).toBe("autonomous");
    expect(auditor.tools).not.toContain("trade_execute");
    expect(auditor.tools).not.toContain("portfolio_rebalance");
    expect(auditor.tools).not.toContain("compliance_check");
    expect(auditor.tools).toContain("audit_trail_export");
  });

  it("workers are gated", () => {
    const workers = [
      "finance:analyst",
      "finance:portfolio_manager",
      "finance:risk_officer",
      "finance:compliance_officer",
    ];
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
    expect(FINANCE_PROFILES).toHaveLength(5);
  });
});
