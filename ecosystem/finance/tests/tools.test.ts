import { describe, it, expect } from "vitest";
import {
  financeToolDefinitions,
  validateComplianceCheck,
  classifyForbiddenPattern,
  isComplianceFresh,
  FORBIDDEN_PATTERNS,
  type ComplianceCheck,
} from "../src/tools/finance-tools.js";

describe("Finance Tools", () => {
  const tools = financeToolDefinitions();

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

  it("trade_execute requires compliance_check", () => {
    const te = tools.find((t) => t.name === "trade_execute")!;
    expect(te.input_schema.required).toContain("compliance_check");
  });

  it("compliance_check requires trade_id, instrument, side, quantity", () => {
    const cc = tools.find((t) => t.name === "compliance_check")!;
    const required = cc.input_schema.required as string[];
    expect(required).toContain("trade_id");
    expect(required).toContain("instrument");
    expect(required).toContain("side");
    expect(required).toContain("quantity");
    expect(required).toContain("regulation");
  });

  it("kyc_screen requires client_id and name", () => {
    const ks = tools.find((t) => t.name === "kyc_screen")!;
    const required = ks.input_schema.required as string[];
    expect(required).toContain("client_id");
    expect(required).toContain("name");
  });
});

describe("Compliance Check Validation", () => {
  const validCheck: ComplianceCheck = {
    trade_id: "T-001",
    instrument: "MSFT",
    side: "buy",
    quantity: 100,
    status: "approved",
    regulation_cited: "SEC Rule 10b-5",
    forbidden_pattern_screened: true,
    suitability_verified: true,
    kyc_current: true,
    expires_at: Date.now() + 60_000,
  };

  it("valid check passes", () => {
    const errors = validateComplianceCheck(validCheck);
    expect(errors).toHaveLength(0);
  });

  it("missing trade_id fails", () => {
    const errors = validateComplianceCheck({ ...validCheck, trade_id: "" });
    expect(errors.some((e) => e.includes("trade_id"))).toBe(true);
  });

  it("missing regulation_cited fails", () => {
    const errors = validateComplianceCheck({ ...validCheck, regulation_cited: "" });
    expect(errors.some((e) => e.includes("regulation_cited"))).toBe(true);
  });

  it("forbidden_pattern_screened false fails", () => {
    const errors = validateComplianceCheck({ ...validCheck, forbidden_pattern_screened: false });
    expect(errors.some((e) => e.includes("forbidden_pattern_screened"))).toBe(true);
  });

  it("suitability_verified false fails", () => {
    const errors = validateComplianceCheck({ ...validCheck, suitability_verified: false });
    expect(errors.some((e) => e.includes("suitability_verified"))).toBe(true);
  });

  it("kyc_current false fails", () => {
    const errors = validateComplianceCheck({ ...validCheck, kyc_current: false });
    expect(errors.some((e) => e.includes("kyc_current"))).toBe(true);
  });

  it("isComplianceFresh detects expiration", () => {
    const expired = { ...validCheck, expires_at: 100 };
    expect(isComplianceFresh(expired, 200)).toBe(false);
    expect(isComplianceFresh(validCheck, Date.now())).toBe(true);
  });
});

describe("Forbidden Pattern Classification", () => {
  it("FORBIDDEN_PATTERNS contains 7 patterns", () => {
    expect(FORBIDDEN_PATTERNS).toHaveLength(7);
    expect(FORBIDDEN_PATTERNS).toContain("insider_trading");
    expect(FORBIDDEN_PATTERNS).toContain("wash_trade");
    expect(FORBIDDEN_PATTERNS).toContain("spoofing");
  });

  it("detects insider trading from rationale", () => {
    const result = classifyForbiddenPattern({
      instrument: "ACME",
      side: "buy",
      quantity: 1000,
      rationale: "based on material non-public information from CEO meeting",
    });
    expect(result).toBe("insider_trading");
  });

  it("detects wash trade from opposing same-quantity related order", () => {
    const result = classifyForbiddenPattern({
      instrument: "ACME",
      side: "buy",
      quantity: 500,
      related_orders: [{ side: "sell", quantity: 500 }],
    });
    expect(result).toBe("wash_trade");
  });

  it("detects spoofing from many cancelled opposing orders", () => {
    const result = classifyForbiddenPattern({
      instrument: "ACME",
      side: "buy",
      quantity: 100,
      related_orders: [
        { side: "sell", quantity: 1000, cancelled: true },
        { side: "sell", quantity: 2000, cancelled: true },
        { side: "sell", quantity: 3000, cancelled: true },
      ],
    });
    expect(result).toBe("spoofing");
  });

  it("detects front running from rationale", () => {
    const result = classifyForbiddenPattern({
      instrument: "ACME",
      side: "buy",
      quantity: 100,
      rationale: "executing ahead of client order to capture price move",
    });
    expect(result).toBe("front_running");
  });

  it("returns null for clean trade", () => {
    const result = classifyForbiddenPattern({
      instrument: "MSFT",
      side: "buy",
      quantity: 100,
      rationale: "increase technology sector exposure per IPS target",
    });
    expect(result).toBeNull();
  });
});
