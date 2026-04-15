import { describe, it, expect } from "vitest";
import { HEALTHCARE_QUALITY_DIMENSIONS, evaluateQuality, getDimension, type EvaluationContext } from "../src/quality/quality.js";

describe("Healthcare Quality", () => {
  const allGood: EvaluationContext = {
    clinicalToolInvocations: 1,
    invocationsWithGrant: 1,
    grantScopeMatches: true,
    grantNotExpired: true,
    phiDetectedInArtifact: false,
    recommendationContradictsGuideline: false,
    guidelineCited: true,
    offLabelDisclosed: true,
    literatureCitationsPresent: true,
    evidenceLevelGraded: true,
    requiredClinicalContextPresent: true,
    readingLevelGradeDelta: 0,
    hipaaBreachIndicator: false,
    hipaaNoticeReferenced: true,
    fdaDisclaimerWhenRequired: true,
  };

  it("defines 6 quality dimensions", () => {
    expect(HEALTHCARE_QUALITY_DIMENSIONS).toHaveLength(6);
  });

  it("all dimensions have unique IDs", () => {
    const ids = HEALTHCARE_QUALITY_DIMENSIONS.map((d) => d.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("all-pass context → overall pass", () => {
    const report = evaluateQuality(allGood);
    expect(report.overall).toBe("pass");
    for (const dim of report.dimensions) {
      expect(dim.level).toBe("pass");
    }
  });

  it("clinical tool without grant → phi_compliance fail", () => {
    const ctx = { ...allGood, clinicalToolInvocations: 1, invocationsWithGrant: 0 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "phi_compliance")!.level).toBe("fail");
    expect(report.overall).toBe("fail");
  });

  it("PHI detected in artifact → phi_compliance fail", () => {
    const ctx = { ...allGood, phiDetectedInArtifact: true };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "phi_compliance")!.level).toBe("fail");
  });

  it("grant scope mismatch → phi_compliance fail", () => {
    const ctx = { ...allGood, grantScopeMatches: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "phi_compliance")!.level).toBe("fail");
  });

  it("grant expired → phi_compliance fail", () => {
    const ctx = { ...allGood, grantNotExpired: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "phi_compliance")!.level).toBe("fail");
  });

  it("recommendation contradicts guideline → clinical_accuracy fail", () => {
    const ctx = { ...allGood, recommendationContradictsGuideline: true };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "clinical_accuracy")!.level).toBe("fail");
  });

  it("no guideline cited → clinical_accuracy warn", () => {
    const ctx = { ...allGood, guidelineCited: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "clinical_accuracy")!.level).toBe("warn");
  });

  it("off-label undisclosed → clinical_accuracy warn", () => {
    const ctx = { ...allGood, offLabelDisclosed: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "clinical_accuracy")!.level).toBe("warn");
  });

  it("no literature citation → evidence_basis fail", () => {
    const ctx = { ...allGood, literatureCitationsPresent: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "evidence_basis")!.level).toBe("fail");
  });

  it("evidence ungraded → evidence_basis warn", () => {
    const ctx = { ...allGood, evidenceLevelGraded: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "evidence_basis")!.level).toBe("warn");
  });

  it("missing clinical context → completeness fail", () => {
    const ctx = { ...allGood, requiredClinicalContextPresent: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "completeness")!.level).toBe("fail");
  });

  it("reading level 1 grade above target → readability warn", () => {
    const ctx = { ...allGood, readingLevelGradeDelta: 1 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "readability")!.level).toBe("warn");
  });

  it("reading level 3 grades above target → readability fail", () => {
    const ctx = { ...allGood, readingLevelGradeDelta: 3 };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "readability")!.level).toBe("fail");
  });

  it("HIPAA breach indicator → regulatory_adherence fail", () => {
    const ctx = { ...allGood, hipaaBreachIndicator: true };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "regulatory_adherence")!.level).toBe("fail");
  });

  it("missing FDA disclaimer → regulatory_adherence fail", () => {
    const ctx = { ...allGood, fdaDisclaimerWhenRequired: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "regulatory_adherence")!.level).toBe("fail");
  });

  it("HIPAA notice not referenced → regulatory_adherence warn", () => {
    const ctx = { ...allGood, hipaaNoticeReferenced: false };
    const report = evaluateQuality(ctx);
    expect(report.dimensions.find((d) => d.id === "regulatory_adherence")!.level).toBe("warn");
  });

  it("getDimension returns undefined for unknown", () => {
    expect(getDimension("unknown")).toBeUndefined();
  });
});
