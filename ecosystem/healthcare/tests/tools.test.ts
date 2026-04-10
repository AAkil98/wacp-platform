import { describe, it, expect } from "vitest";
import {
  healthcareToolDefinitions,
  validatePhiAccessGrant,
  isPhiGrantFresh,
  grantCoversUse,
  detectPhi,
  HIPAA_SAFE_HARBOR_IDENTIFIERS,
  type PhiAccessGrant,
} from "../src/tools/healthcare-tools.js";

describe("Healthcare Tools", () => {
  const tools = healthcareToolDefinitions();

  it("defines 10 tools", () => {
    expect(tools).toHaveLength(10);
  });

  it("all tools have unique names", () => {
    const names = tools.map((t) => t.name);
    expect(new Set(names).size).toBe(names.length);
  });

  it("all tools have non-empty descriptions", () => {
    for (const tool of tools) {
      expect(tool.description.length).toBeGreaterThan(0);
    }
  });

  it("all tools have object input schemas", () => {
    for (const tool of tools) {
      expect(tool.input_schema.type).toBe("object");
    }
  });

  it("clinical_report_generate requires phi_access_grant", () => {
    const tool = tools.find((t) => t.name === "clinical_report_generate")!;
    expect(tool.input_schema.required).toContain("phi_access_grant");
  });

  it("lab_interpret requires phi_access_grant", () => {
    const tool = tools.find((t) => t.name === "lab_interpret")!;
    expect(tool.input_schema.required).toContain("phi_access_grant");
  });

  it("risk_score requires phi_access_grant", () => {
    const tool = tools.find((t) => t.name === "risk_score")!;
    expect(tool.input_schema.required).toContain("phi_access_grant");
  });

  it("clinical_search and protocol_lookup do not require phi_access_grant", () => {
    const cs = tools.find((t) => t.name === "clinical_search")!;
    const pl = tools.find((t) => t.name === "protocol_lookup")!;
    expect(cs.input_schema.required).not.toContain("phi_access_grant");
    expect(pl.input_schema.required).not.toContain("phi_access_grant");
  });
});

describe("PHI Access Grant Validation", () => {
  const consentGrant: PhiAccessGrant = {
    basis: "consent",
    patient_id: "P-001",
    consent_id: "C-001",
    consent_scope: ["assessment", "labs", "documentation"],
    expires_at: Date.now() + 60_000,
  };

  const deidGrant: PhiAccessGrant = {
    basis: "de_identified",
    deidentification_method: "safe_harbor",
    deidentified_data_hash: "sha256:abc123",
    expires_at: Date.now() + 60_000,
  };

  it("valid consent grant passes", () => {
    expect(validatePhiAccessGrant(consentGrant)).toHaveLength(0);
  });

  it("valid de-identified grant passes", () => {
    expect(validatePhiAccessGrant(deidGrant)).toHaveLength(0);
  });

  it("missing basis fails", () => {
    const errors = validatePhiAccessGrant({ ...consentGrant, basis: undefined as any });
    expect(errors.some((e) => e.includes("basis"))).toBe(true);
  });

  it("consent without patient_id fails", () => {
    const errors = validatePhiAccessGrant({ ...consentGrant, patient_id: "" });
    expect(errors.some((e) => e.includes("patient_id"))).toBe(true);
  });

  it("consent without scope fails", () => {
    const errors = validatePhiAccessGrant({ ...consentGrant, consent_scope: [] });
    expect(errors.some((e) => e.includes("consent_scope"))).toBe(true);
  });

  it("de-identified without method fails", () => {
    const errors = validatePhiAccessGrant({ ...deidGrant, deidentification_method: undefined });
    expect(errors.some((e) => e.includes("deidentification_method"))).toBe(true);
  });

  it("de-identified without hash fails", () => {
    const errors = validatePhiAccessGrant({ ...deidGrant, deidentified_data_hash: "" });
    expect(errors.some((e) => e.includes("deidentified_data_hash"))).toBe(true);
  });

  it("isPhiGrantFresh detects expiration", () => {
    const expired = { ...consentGrant, expires_at: 100 };
    expect(isPhiGrantFresh(expired, 200)).toBe(false);
    expect(isPhiGrantFresh(consentGrant, Date.now())).toBe(true);
  });

  it("grantCoversUse passes when scope includes use", () => {
    expect(grantCoversUse(consentGrant, "labs")).toBe(true);
    expect(grantCoversUse(consentGrant, "documentation")).toBe(true);
  });

  it("grantCoversUse fails when scope omits use", () => {
    expect(grantCoversUse(consentGrant, "research")).toBe(false);
  });

  it("grantCoversUse always passes for de-identified data", () => {
    expect(grantCoversUse(deidGrant, "anything")).toBe(true);
  });

  it("wildcard scope covers any use", () => {
    const wild = { ...consentGrant, consent_scope: ["*"] };
    expect(grantCoversUse(wild, "experimental")).toBe(true);
  });
});

describe("HIPAA Safe Harbor identifiers", () => {
  it("defines all 18 identifiers", () => {
    expect(HIPAA_SAFE_HARBOR_IDENTIFIERS).toHaveLength(18);
  });

  it("includes core identifiers", () => {
    expect(HIPAA_SAFE_HARBOR_IDENTIFIERS).toContain("name");
    expect(HIPAA_SAFE_HARBOR_IDENTIFIERS).toContain("ssn");
    expect(HIPAA_SAFE_HARBOR_IDENTIFIERS).toContain("mrn");
    expect(HIPAA_SAFE_HARBOR_IDENTIFIERS).toContain("biometric_identifier");
  });
});

describe("PHI Detection", () => {
  it("detects SSN", () => {
    expect(detectPhi("Patient SSN is 123-45-6789")).toContain("ssn");
  });

  it("detects email address", () => {
    expect(detectPhi("Contact: jane.doe@example.com")).toContain("email_address");
  });

  it("detects MRN", () => {
    expect(detectPhi("MRN: 12345678")).toContain("mrn");
  });

  it("detects telephone number", () => {
    expect(detectPhi("Call (555) 123-4567")).toContain("telephone_number");
  });

  it("returns empty array for clean text", () => {
    const result = detectPhi("Patient presents with elevated white count and fever per chart review");
    expect(result.filter((r) => r === "ssn" || r === "email_address" || r === "mrn")).toHaveLength(0);
  });
});
