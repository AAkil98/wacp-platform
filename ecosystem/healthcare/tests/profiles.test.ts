import { describe, it, expect } from "vitest";
import { HEALTHCARE_PROFILES, getProfile, allProfiles } from "../src/profiles/profiles.js";

describe("Healthcare Profiles", () => {
  it("defines 5 profiles (one per role)", () => {
    expect(HEALTHCARE_PROFILES).toHaveLength(5);
  });

  it("all profiles have non-empty system prompts", () => {
    for (const profile of HEALTHCARE_PROFILES) {
      expect(profile.systemPrompt.length).toBeGreaterThan(50);
    }
  });

  it("all profiles have at least one tool", () => {
    for (const profile of HEALTHCARE_PROFILES) {
      expect(profile.tools.length).toBeGreaterThan(0);
    }
  });

  it("clinician has clinical_report_generate, lab_interpret, risk_score", () => {
    const clinician = getProfile("health:clinician")!;
    expect(clinician.tools).toContain("clinical_report_generate");
    expect(clinician.tools).toContain("lab_interpret");
    expect(clinician.tools).toContain("risk_score");
    expect(clinician.tools).toContain("consent_verify");
  });

  it("researcher does not have clinical_report_generate (clinician's tool)", () => {
    const researcher = getProfile("health:researcher")!;
    expect(researcher.tools).not.toContain("clinical_report_generate");
    expect(researcher.tools).not.toContain("lab_interpret");
  });

  it("researcher has de_identify and clinical_search", () => {
    const researcher = getProfile("health:researcher")!;
    expect(researcher.tools).toContain("de_identify");
    expect(researcher.tools).toContain("clinical_search");
  });

  it("analyst has only de-identified analytics tools", () => {
    const analyst = getProfile("health:analyst")!;
    expect(analyst.tools).toContain("de_identify");
    expect(analyst.tools).toContain("phi_filter");
    expect(analyst.tools).toContain("risk_score");
    expect(analyst.tools).not.toContain("clinical_report_generate");
    expect(analyst.tools).not.toContain("consent_verify");
  });

  it("compliance is autonomous and has audit tools only", () => {
    const compliance = getProfile("health:compliance")!;
    expect(compliance.autonomy).toBe("autonomous");
    expect(compliance.tools).toContain("audit_export");
    expect(compliance.tools).toContain("phi_filter");
    expect(compliance.tools).not.toContain("clinical_report_generate");
    expect(compliance.tools).not.toContain("de_identify");
  });

  it("coordinator has education_material and consent_verify", () => {
    const coordinator = getProfile("health:coordinator")!;
    expect(coordinator.tools).toContain("education_material");
    expect(coordinator.tools).toContain("consent_verify");
    expect(coordinator.tools).not.toContain("clinical_report_generate");
  });

  it("workers are gated", () => {
    const workers = ["health:clinician", "health:researcher", "health:analyst", "health:coordinator"];
    for (const roleId of workers) {
      const profile = getProfile(roleId)!;
      expect(profile.autonomy).toBe("gated");
    }
  });

  it("only clinician has clinical_report_generate", () => {
    const withReport = HEALTHCARE_PROFILES.filter((p) => p.tools.includes("clinical_report_generate"));
    expect(withReport).toHaveLength(1);
    expect(withReport[0].roleId).toBe("health:clinician");
  });

  it("only clinician has lab_interpret", () => {
    const withLab = HEALTHCARE_PROFILES.filter((p) => p.tools.includes("lab_interpret"));
    expect(withLab).toHaveLength(1);
    expect(withLab[0].roleId).toBe("health:clinician");
  });

  it("getProfile returns undefined for unknown", () => {
    expect(getProfile("unknown")).toBeUndefined();
  });

  it("allProfiles returns a copy", () => {
    const profiles = allProfiles();
    profiles.push({} as any);
    expect(HEALTHCARE_PROFILES).toHaveLength(5);
  });
});
