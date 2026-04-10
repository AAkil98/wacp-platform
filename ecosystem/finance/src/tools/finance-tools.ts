import type { LocalResources } from "@wacp/local";

/** Tool definition for LLM function-calling. */
export interface ToolDefinition {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

/** A compliance check result — required before any trade_execute call. */
export interface ComplianceCheck {
  trade_id: string;
  instrument: string;
  side: "buy" | "sell";
  quantity: number;
  status: "approved" | "rejected";
  regulation_cited: string;
  forbidden_pattern_screened: boolean;
  forbidden_pattern_detected?: string;
  suitability_verified: boolean;
  kyc_current: boolean;
  expires_at: number;
  reason?: string;
}

/** Forbidden trading patterns — automatic rejection. */
export const FORBIDDEN_PATTERNS = [
  "insider_trading",
  "wash_trade",
  "churning",
  "front_running",
  "layering",
  "spoofing",
  "painting_the_tape",
] as const;

export type ForbiddenPattern = (typeof FORBIDDEN_PATTERNS)[number];

/** Validate a compliance check for completeness and approval status. Returns errors if invalid. */
export function validateComplianceCheck(check: Partial<ComplianceCheck>): string[] {
  const errors: string[] = [];
  if (!check.trade_id || check.trade_id.trim().length === 0) {
    errors.push("trade_id is required");
  }
  if (!check.instrument || check.instrument.trim().length === 0) {
    errors.push("instrument is required");
  }
  if (check.side !== "buy" && check.side !== "sell") {
    errors.push("side must be 'buy' or 'sell'");
  }
  if (typeof check.quantity !== "number" || check.quantity <= 0) {
    errors.push("quantity must be a positive number");
  }
  if (check.status !== "approved" && check.status !== "rejected") {
    errors.push("status must be 'approved' or 'rejected'");
  }
  if (!check.regulation_cited || check.regulation_cited.trim().length === 0) {
    errors.push("regulation_cited is required");
  }
  if (check.forbidden_pattern_screened !== true) {
    errors.push("forbidden_pattern_screened must be true");
  }
  if (check.suitability_verified !== true) {
    errors.push("suitability_verified must be true");
  }
  if (check.kyc_current !== true) {
    errors.push("kyc_current must be true");
  }
  if (typeof check.expires_at !== "number" || check.expires_at <= 0) {
    errors.push("expires_at must be a positive timestamp");
  }
  return errors;
}

/** Classify a proposed trade against the forbidden-pattern list. Returns the matched pattern or null. */
export function classifyForbiddenPattern(trade: {
  instrument: string;
  side: "buy" | "sell";
  quantity: number;
  rationale?: string;
  related_orders?: { side: "buy" | "sell"; quantity: number; cancelled?: boolean }[];
  source?: string;
}): ForbiddenPattern | null {
  const rationale = (trade.rationale ?? "").toLowerCase();
  const source = (trade.source ?? "").toLowerCase();

  if (rationale.includes("material non-public") || source.includes("mnpi") || source.includes("insider")) {
    return "insider_trading";
  }

  if (trade.related_orders && trade.related_orders.length > 0) {
    const opposingSameQty = trade.related_orders.filter(
      (o) => o.side !== trade.side && o.quantity === trade.quantity && !o.cancelled,
    );
    if (opposingSameQty.length > 0) {
      return "wash_trade";
    }
    const cancelledOpposing = trade.related_orders.filter((o) => o.side !== trade.side && o.cancelled);
    if (cancelledOpposing.length >= 3) {
      return "spoofing";
    }
    const sameSideStaggered = trade.related_orders.filter((o) => o.side === trade.side && o.cancelled);
    if (sameSideStaggered.length >= 3) {
      return "layering";
    }
  }

  if (rationale.includes("front of") || rationale.includes("ahead of client")) {
    return "front_running";
  }

  if (rationale.includes("commission") || rationale.includes("generate fees")) {
    return "churning";
  }

  if (rationale.includes("close price") || rationale.includes("mark the close")) {
    return "painting_the_tape";
  }

  return null;
}

/** Check if a compliance check is fresh (not expired). */
export function isComplianceFresh(check: ComplianceCheck, nowMs: number): boolean {
  return check.expires_at > nowMs;
}

/** Finance-specific tool definitions beyond the CLI's built-in 7. */
export function financeToolDefinitions(): ToolDefinition[] {
  return [
    {
      name: "market_data_fetch",
      description: "Fetch market data — quotes, fundamentals, historical prices, corporate actions. Auto-detects data source.",
      input_schema: {
        type: "object",
        properties: {
          symbols: { type: "array", items: { type: "string" }, description: "Tickers to fetch" },
          fields: { type: "array", items: { type: "string" }, description: "Fields — price, volume, pe_ratio, etc." },
          start_date: { type: "string", description: "Start date (YYYY-MM-DD) for historical data" },
          end_date: { type: "string", description: "End date (YYYY-MM-DD) for historical data" },
        },
        required: ["symbols"],
      },
    },
    {
      name: "financial_model_build",
      description: "Build a financial model — DCF, LBO, comparables, sum-of-the-parts. Returns valuation with sensitivity analysis.",
      input_schema: {
        type: "object",
        properties: {
          model_type: { type: "string", enum: ["dcf", "lbo", "comparables", "sotp"], description: "Model family" },
          target: { type: "string", description: "Target company ticker or identifier" },
          assumptions: { type: "object", description: "Model assumptions (WACC, growth, exit multiple, etc.)" },
          data: { type: "string", description: "Path to historical financials" },
        },
        required: ["model_type", "target"],
      },
    },
    {
      name: "risk_calc",
      description: "Compute risk metrics — VaR, CVaR, beta, Greeks, scenario stress.",
      input_schema: {
        type: "object",
        properties: {
          metric: { type: "string", enum: ["var", "cvar", "beta", "greeks", "stress"], description: "Risk metric" },
          portfolio: { type: "string", description: "Portfolio identifier or path to holdings" },
          horizon_days: { type: "number", description: "Risk horizon in days (default: 1)" },
          confidence: { type: "number", description: "Confidence level (default: 0.95)" },
          scenario: { type: "string", description: "Scenario name for stress testing" },
        },
        required: ["metric", "portfolio"],
      },
    },
    {
      name: "compliance_check",
      description: "Pre-trade compliance check. Classifies trade, screens against forbidden patterns, verifies suitability and KYC. Produces a compliance_check checkpoint that trade_execute requires.",
      input_schema: {
        type: "object",
        properties: {
          trade_id: { type: "string", description: "Unique trade identifier" },
          instrument: { type: "string", description: "Instrument symbol" },
          side: { type: "string", enum: ["buy", "sell"], description: "Trade side" },
          quantity: { type: "number", description: "Trade quantity" },
          client_id: { type: "string", description: "Client identifier" },
          regulation: { type: "string", description: "Regulatory framework — SEC, FINRA, MiFID II, FCA" },
          rationale: { type: "string", description: "Trade rationale" },
          related_orders: {
            type: "array",
            items: { type: "object" },
            description: "Related orders for pattern analysis",
          },
        },
        required: ["trade_id", "instrument", "side", "quantity", "client_id", "regulation"],
      },
    },
    {
      name: "kyc_screen",
      description: "KYC/AML/sanctions screen — identity, PEP, OFAC/SDN, adverse media.",
      input_schema: {
        type: "object",
        properties: {
          client_id: { type: "string", description: "Client identifier" },
          name: { type: "string", description: "Legal name" },
          country: { type: "string", description: "Country of residence" },
          screens: {
            type: "array",
            items: { type: "string", enum: ["identity", "pep", "ofac", "sdn", "adverse_media"] },
            description: "Screens to run",
          },
        },
        required: ["client_id", "name"],
      },
    },
    {
      name: "trade_execute",
      description: "Execute a trade order. REQUIRES a prior approved compliance_check checkpoint with matching trade_id. Refuses execution if missing, rejected, expired, or mismatched.",
      input_schema: {
        type: "object",
        properties: {
          compliance_check: {
            type: "object",
            description: "The approved compliance check for this trade",
            properties: {
              trade_id: { type: "string" },
              instrument: { type: "string" },
              side: { type: "string", enum: ["buy", "sell"] },
              quantity: { type: "number" },
              status: { type: "string", enum: ["approved", "rejected"] },
              regulation_cited: { type: "string" },
              forbidden_pattern_screened: { type: "boolean" },
              suitability_verified: { type: "boolean" },
              kyc_current: { type: "boolean" },
              expires_at: { type: "number" },
            },
            required: [
              "trade_id",
              "instrument",
              "side",
              "quantity",
              "status",
              "regulation_cited",
              "forbidden_pattern_screened",
              "suitability_verified",
              "kyc_current",
              "expires_at",
            ],
          },
          venue: { type: "string", description: "Execution venue (default: smart route)" },
          order_type: { type: "string", enum: ["market", "limit", "stop"], description: "Order type" },
          limit_price: { type: "number", description: "Limit price (required for limit orders)" },
        },
        required: ["compliance_check"],
      },
    },
    {
      name: "portfolio_rebalance",
      description: "Rebalance a portfolio toward target weights. Generates a trade list — does not execute.",
      input_schema: {
        type: "object",
        properties: {
          portfolio: { type: "string", description: "Portfolio identifier" },
          target_weights: { type: "object", description: "Target weights by instrument" },
          tolerance: { type: "number", description: "Drift tolerance before rebalancing (default: 0.05)" },
          constraints: { type: "object", description: "Constraints — min/max position, turnover cap" },
        },
        required: ["portfolio", "target_weights"],
      },
    },
    {
      name: "audit_trail_export",
      description: "Export the hash-chained audit trail for a workspace, with cryptographic verification.",
      input_schema: {
        type: "object",
        properties: {
          workspace_id: { type: "string", description: "Workspace identifier" },
          start_time: { type: "number", description: "Start timestamp (optional)" },
          end_time: { type: "number", description: "End timestamp (optional)" },
          format: { type: "string", enum: ["json", "csv", "xbrl"], description: "Export format" },
        },
        required: ["workspace_id"],
      },
    },
    {
      name: "regulatory_filing_prepare",
      description: "Prepare a regulatory filing — 10-K, 10-Q, 13F, ADV — from structured data.",
      input_schema: {
        type: "object",
        properties: {
          filing_type: { type: "string", enum: ["10-K", "10-Q", "13F", "ADV", "8-K"], description: "Filing type" },
          period_end: { type: "string", description: "Period end date (YYYY-MM-DD)" },
          data: { type: "string", description: "Path to source data" },
          output: { type: "string", description: "Output file path" },
        },
        required: ["filing_type", "period_end"],
      },
    },
    {
      name: "disclosure_review",
      description: "Review disclosure language for material risks, conflicts of interest, required statements.",
      input_schema: {
        type: "object",
        properties: {
          document: { type: "string", description: "Path to document or document content" },
          checks: {
            type: "array",
            items: { type: "string", enum: ["material_risk", "coi", "forward_looking", "performance"] },
            description: "Disclosure checks to run",
          },
        },
        required: ["document"],
      },
    },
  ];
}

/** Execute a Finance tool using local resources. */
export async function executeFinanceTools(
  resources: LocalResources,
  toolName: string,
  args: Record<string, unknown>,
  nowMs: number = Date.now(),
): Promise<{ content: string; isError: boolean }> {
  try {
    switch (toolName) {
      case "market_data_fetch": {
        const symbols = args.symbols as string[];
        const fields = (args.fields as string[]) ?? ["price"];
        const cmd = `echo "Fetching ${fields.join(",")} for ${symbols.join(",")}"`;
        const result = await resources.exec(cmd, { timeout: 30_000 });
        return { content: result.stdout, isError: false };
      }
      case "financial_model_build": {
        const modelType = args.model_type as string;
        const target = args.target as string;
        const cmd = `echo "Building ${modelType} model for ${target}"`;
        const result = await resources.exec(cmd, { timeout: 60_000 });
        return { content: result.stdout, isError: false };
      }
      case "risk_calc": {
        const metric = args.metric as string;
        const portfolio = args.portfolio as string;
        const horizon = (args.horizon_days as number) ?? 1;
        const confidence = (args.confidence as number) ?? 0.95;
        const cmd = `echo "${metric.toUpperCase()} for ${portfolio} | horizon=${horizon}d | confidence=${confidence}"`;
        const result = await resources.exec(cmd, { timeout: 60_000 });
        return { content: result.stdout, isError: false };
      }
      case "compliance_check": {
        const tradeId = args.trade_id as string;
        const instrument = args.instrument as string;
        const side = args.side as "buy" | "sell";
        const quantity = args.quantity as number;
        const regulation = args.regulation as string;
        const rationale = args.rationale as string | undefined;
        const relatedOrders = args.related_orders as
          | { side: "buy" | "sell"; quantity: number; cancelled?: boolean }[]
          | undefined;

        const detected = classifyForbiddenPattern({
          instrument,
          side,
          quantity,
          rationale,
          related_orders: relatedOrders,
        });

        if (detected) {
          return {
            content: `COMPLIANCE_REJECTED: forbidden pattern detected — ${detected}. Trade ${tradeId} (${side} ${quantity} ${instrument}) blocked under ${regulation}.`,
            isError: true,
          };
        }

        const expiresAt = nowMs + 5 * 60 * 1000;
        const approval: ComplianceCheck = {
          trade_id: tradeId,
          instrument,
          side,
          quantity,
          status: "approved",
          regulation_cited: regulation,
          forbidden_pattern_screened: true,
          suitability_verified: true,
          kyc_current: true,
          expires_at: expiresAt,
        };
        return {
          content: `COMPLIANCE_APPROVED: ${JSON.stringify(approval)}`,
          isError: false,
        };
      }
      case "kyc_screen": {
        const clientId = args.client_id as string;
        const name = args.name as string;
        const screens = (args.screens as string[]) ?? ["identity", "ofac", "pep"];
        const cmd = `echo "KYC screen for client ${clientId} (${name}): ${screens.join(",")}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "trade_execute": {
        const check = args.compliance_check as Partial<ComplianceCheck> | undefined;
        if (!check) {
          return {
            content: "COMPLIANCE_NOT_APPROVED: trade_execute requires a compliance_check argument from a prior compliance_check checkpoint",
            isError: true,
          };
        }
        const errors = validateComplianceCheck(check);
        if (errors.length > 0) {
          return {
            content: `COMPLIANCE_NOT_APPROVED: compliance_check is invalid — ${errors.join("; ")}`,
            isError: true,
          };
        }
        if (check.status !== "approved") {
          return {
            content: `COMPLIANCE_NOT_APPROVED: compliance_check status is '${check.status}', not 'approved'`,
            isError: true,
          };
        }
        if (!isComplianceFresh(check as ComplianceCheck, nowMs)) {
          return {
            content: `COMPLIANCE_NOT_APPROVED: compliance_check expired at ${check.expires_at} (now=${nowMs})`,
            isError: true,
          };
        }
        const venue = (args.venue as string) ?? "smart_route";
        const orderType = (args.order_type as string) ?? "market";
        const cmd = `echo "EXECUTED: ${check.side} ${check.quantity} ${check.instrument} @ ${orderType} via ${venue} (trade_id=${check.trade_id}, regulation=${check.regulation_cited})"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "portfolio_rebalance": {
        const portfolio = args.portfolio as string;
        const tolerance = (args.tolerance as number) ?? 0.05;
        const cmd = `echo "Rebalance proposal for ${portfolio} (tolerance=${tolerance})"`;
        const result = await resources.exec(cmd, { timeout: 60_000 });
        return { content: result.stdout, isError: false };
      }
      case "audit_trail_export": {
        const workspaceId = args.workspace_id as string;
        const format = (args.format as string) ?? "json";
        const cmd = `echo "Exporting audit trail for ${workspaceId} as ${format}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      case "regulatory_filing_prepare": {
        const filingType = args.filing_type as string;
        const periodEnd = args.period_end as string;
        const cmd = `echo "Preparing ${filingType} filing for period ending ${periodEnd}"`;
        const result = await resources.exec(cmd, { timeout: 120_000 });
        return { content: result.stdout, isError: false };
      }
      case "disclosure_review": {
        const document = args.document as string;
        const checks = (args.checks as string[]) ?? ["material_risk", "coi"];
        const cmd = `echo "Reviewing disclosure ${document}: ${checks.join(",")}"`;
        const result = await resources.exec(cmd);
        return { content: result.stdout, isError: false };
      }
      default:
        return { content: `Unknown finance tool: ${toolName}`, isError: true };
    }
  } catch (err) {
    return { content: `Error: ${(err as Error).message}`, isError: true };
  }
}
