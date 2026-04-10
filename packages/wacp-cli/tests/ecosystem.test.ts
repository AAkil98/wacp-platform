import { describe, it, expect, beforeEach } from "vitest";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as os from "node:os";

import { AutonomyManager, LocalResources } from "@wacp/local";
import {
  loadEcosystem,
  routeGoal,
  composeToolDefinitions,
  knownVerticalIds,
  DEFAULT_LOAD_ORDER,
  type LoadedEcosystem,
} from "../src/ecosystem.js";
import {
  buildToolDefinitionsForEcosystem,
  builtinToolDefinitions,
  executeTool,
} from "../src/tools.js";

describe("Ecosystem Loader", () => {
  it("knows all 7 verticals", () => {
    const ids = knownVerticalIds();
    expect(ids).toContain("swe");
    expect(ids).toContain("devops");
    expect(ids).toContain("mlops");
    expect(ids).toContain("finance");
    expect(ids).toContain("healthcare");
    expect(ids).toContain("analytics");
    expect(ids).toContain("datasci");
    expect(ids).toHaveLength(7);
  });

  it("default load order has all 7 with swe last", () => {
    expect(DEFAULT_LOAD_ORDER).toHaveLength(7);
    expect(DEFAULT_LOAD_ORDER[DEFAULT_LOAD_ORDER.length - 1]).toBe("swe");
  });

  it("loadEcosystem with no args loads all 7 verticals", () => {
    const eco = loadEcosystem();
    expect(eco.verticals).toHaveLength(7);
    const ids = eco.verticals.map((v) => v.id);
    expect(ids).toEqual([...DEFAULT_LOAD_ORDER]);
  });

  it("loadEcosystem with explicit subset loads only those", () => {
    const eco = loadEcosystem(["finance", "healthcare"]);
    expect(eco.verticals).toHaveLength(2);
    expect(eco.verticals.map((v) => v.id)).toEqual(["finance", "healthcare"]);
  });

  it("loadEcosystem skips unknown IDs without throwing", () => {
    const eco = loadEcosystem(["finance", "nonexistent", "swe"]);
    expect(eco.verticals.map((v) => v.id)).toEqual(["finance", "swe"]);
  });

  it("each vertical has workflows, profiles, tools, detector, executor, toolOperation", () => {
    const eco = loadEcosystem();
    for (const v of eco.verticals) {
      expect(v.workflows.length).toBeGreaterThan(0);
      expect(v.profiles.length).toBeGreaterThan(0);
      expect(v.toolDefinitions.length).toBeGreaterThan(0);
      expect(typeof v.detectTaskType).toBe("function");
      expect(typeof v.executeTool).toBe("function");
      expect(typeof v.toolOperation).toBe("function");
    }
  });

  it("toolByName indexes every vertical tool with no collisions", () => {
    const eco = loadEcosystem();
    let total = 0;
    for (const v of eco.verticals) {
      total += v.toolDefinitions.length;
    }
    expect(eco.toolByName.size).toBe(total);
  });

  it("each tool name maps back to its owning vertical", () => {
    const eco = loadEcosystem();
    expect(eco.toolByName.get("trade_execute")?.id).toBe("finance");
    expect(eco.toolByName.get("clinical_report_generate")?.id).toBe("healthcare");
    expect(eco.toolByName.get("hypothesis_test")?.id).toBe("datasci");
    expect(eco.toolByName.get("deploy_execute")?.id).toBe("devops");
    expect(eco.toolByName.get("train_launch")?.id).toBe("mlops");
    expect(eco.toolByName.get("sql_query")?.id).toBe("analytics");
    expect(eco.toolByName.get("code_search")?.id).toBe("swe");
  });
});

describe("Goal Router (multi-vertical detection)", () => {
  let ecosystem: LoadedEcosystem;
  beforeEach(() => {
    ecosystem = loadEcosystem();
  });

  it("routes finance trade goals to finance", () => {
    const r = routeGoal("buy 10000 shares of MSFT for the growth fund", ecosystem);
    expect(r.verticalId).toBe("finance");
    expect(r.taskType).toBe("finance:trade");
    expect(r.workflowId).toBe("finance:trade-execution");
    expect(r.workflow?.id).toBe("finance:trade-execution");
  });

  it("routes finance rebalance goals to finance", () => {
    const r = routeGoal("rebalance the portfolio toward target weights", ecosystem);
    expect(r.verticalId).toBe("finance");
    expect(r.taskType).toBe("finance:rebalance");
  });

  it("routes healthcare assessment goals to healthcare", () => {
    const r = routeGoal("do an admission H&P for the new patient in bed 4", ecosystem);
    expect(r.verticalId).toBe("healthcare");
    expect(r.workflow?.id).toBe("health:patient-assessment");
  });

  it("routes healthcare PHI audit goals to healthcare", () => {
    const r = routeGoal("audit the workspace for PHI compliance", ecosystem);
    expect(r.verticalId).toBe("healthcare");
    expect(r.workflowId).toBe("health:phi-audit");
  });

  it("routes devops deploy goals to devops", () => {
    const r = routeGoal("deploy the API to production", ecosystem);
    expect(r.verticalId).toBe("devops");
    expect(r.workflowId).toBe("devops:deploy");
  });

  it("routes devops rollback goals to devops respond workflow", () => {
    const r = routeGoal("rollback the prod release", ecosystem);
    expect(r.verticalId).toBe("devops");
    expect(r.workflowId).toBe("devops:respond");
  });

  it("routes mlops training goals to mlops experiment", () => {
    const r = routeGoal("train the model with hyperparameter sweep", ecosystem);
    expect(r.verticalId).toBe("mlops");
    expect(r.workflowId).toBe("mlops:experiment");
  });

  it("routes analytics dashboard goals to analytics", () => {
    const r = routeGoal("build a dashboard for weekly KPIs", ecosystem);
    expect(r.verticalId).toBe("analytics");
    expect(r.workflowId).toBe("analytics:build-dashboard");
  });

  it("routes datasci hypothesis goals to datasci", () => {
    const r = routeGoal("test if the new onboarding flow improves day-7 retention with a t-test", ecosystem);
    expect(r.verticalId).toBe("datasci");
    expect(r.workflowId).toBe("datasci:hypothesis-test");
  });

  it("routes datasci regression goals to datasci model build", () => {
    const r = routeGoal("fit a logistic regression model on the conversion data", ecosystem);
    expect(r.verticalId).toBe("datasci");
    expect(r.workflowId).toBe("datasci:model-build");
  });

  it("routes SWE goals to SWE", () => {
    const r = routeGoal("fix the auth bug in login.ts", ecosystem);
    expect(r.verticalId).toBe("swe");
    expect(r.workflowId).toBe("swe:fix-bug");
  });

  it("ambiguous goals fall back to SWE catchall", () => {
    const r = routeGoal("make the app faster", ecosystem);
    expect(r.verticalId).toBe("swe");
    expect(r.workflowId).toBe("swe:implement-feature");
  });

  it("domain keywords win over SWE keywords (mixed goal)", () => {
    // "implement" is a SWE keyword but "trade execution" is a finance signal
    const r = routeGoal("implement the trade execution flow for institutional clients", ecosystem);
    expect(r.verticalId).toBe("finance");
  });
});

describe("Tool Registry Composition", () => {
  it("buildToolDefinitionsForEcosystem returns 7 built-in + all vertical tools", () => {
    const eco = loadEcosystem();
    const tools = buildToolDefinitionsForEcosystem(eco);
    const builtin = builtinToolDefinitions();
    const verticalCount = eco.verticals.reduce((sum, v) => sum + v.toolDefinitions.length, 0);
    expect(tools.length).toBe(builtin.length + verticalCount);
  });

  it("composeToolDefinitions matches buildToolDefinitionsForEcosystem", () => {
    const eco = loadEcosystem();
    const a = composeToolDefinitions(builtinToolDefinitions(), eco);
    const b = buildToolDefinitionsForEcosystem(eco);
    expect(a.map((t) => t.name)).toEqual(b.map((t) => t.name));
  });

  it("composed registry contains tools from every vertical", () => {
    const eco = loadEcosystem();
    const tools = buildToolDefinitionsForEcosystem(eco);
    const names = new Set(tools.map((t) => t.name));
    // sample one tool from each vertical
    expect(names.has("read_file")).toBe(true);          // built-in
    expect(names.has("code_search")).toBe(true);        // swe
    expect(names.has("deploy_execute")).toBe(true);     // devops
    expect(names.has("train_launch")).toBe(true);       // mlops
    expect(names.has("trade_execute")).toBe(true);      // finance
    expect(names.has("clinical_report_generate")).toBe(true); // healthcare
    expect(names.has("sql_query")).toBe(true);          // analytics
    expect(names.has("hypothesis_test")).toBe(true);    // datasci
  });

  it("all composed tool names are unique", () => {
    const eco = loadEcosystem();
    const tools = buildToolDefinitionsForEcosystem(eco);
    const names = tools.map((t) => t.name);
    expect(new Set(names).size).toBe(names.length);
  });
});

describe("Tool Execution Dispatch (constraint enforcement reaches CLI path)", () => {
  let tmpDir: string;
  let autonomy: AutonomyManager;
  let resources: LocalResources;
  let ecosystem: LoadedEcosystem;

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "wacp-eco-test-"));
    autonomy = new AutonomyManager("autonomous");
    resources = new LocalResources(tmpDir, autonomy);
    ecosystem = loadEcosystem();
  });

  it("Finance: trade_execute refuses without compliance_check (CLI dispatch path)", async () => {
    const result = await executeTool(
      resources,
      autonomy,
      "trade_execute",
      "call-1",
      { /* no compliance_check */ },
      ecosystem,
    );
    expect(result.isError).toBe(true);
    expect(result.content).toContain("COMPLIANCE_NOT_APPROVED");
  });

  it("Finance: trade_execute rejects expired compliance check", async () => {
    const result = await executeTool(
      resources,
      autonomy,
      "trade_execute",
      "call-2",
      {
        compliance_check: {
          trade_id: "T-001",
          instrument: "MSFT",
          side: "buy",
          quantity: 100,
          status: "approved",
          regulation_cited: "SEC Rule 10b-5",
          forbidden_pattern_screened: true,
          suitability_verified: true,
          kyc_current: true,
          expires_at: 1, // Expired
        },
      },
      ecosystem,
    );
    expect(result.isError).toBe(true);
    expect(result.content).toContain("COMPLIANCE_NOT_APPROVED");
    expect(result.content).toContain("expired");
  });

  it("Finance: compliance_check with forbidden pattern hard-blocks", async () => {
    const result = await executeTool(
      resources,
      autonomy,
      "compliance_check",
      "call-3",
      {
        trade_id: "T-002",
        instrument: "ACME",
        side: "buy",
        quantity: 1000,
        client_id: "C-1",
        regulation: "SEC Rule 10b-5",
        rationale: "based on material non-public information from CEO meeting",
      },
      ecosystem,
    );
    expect(result.isError).toBe(true);
    expect(result.content).toContain("COMPLIANCE_REJECTED");
    expect(result.content).toContain("insider_trading");
  });

  it("Healthcare: clinical_report_generate refuses without phi_access_grant", async () => {
    const result = await executeTool(
      resources,
      autonomy,
      "clinical_report_generate",
      "call-4",
      { report_type: "hp" /* no phi_access_grant */ },
      ecosystem,
    );
    expect(result.isError).toBe(true);
    expect(result.content).toContain("PHI_ACCESS_NOT_GRANTED");
  });

  it("Healthcare: lab_interpret refuses without phi_access_grant", async () => {
    const result = await executeTool(
      resources,
      autonomy,
      "lab_interpret",
      "call-5",
      { labs: [] /* no phi_access_grant */ },
      ecosystem,
    );
    expect(result.isError).toBe(true);
    expect(result.content).toContain("PHI_ACCESS_NOT_GRANTED");
  });

  it("Healthcare: clinical_report_generate accepts a valid de-identified grant", async () => {
    const result = await executeTool(
      resources,
      autonomy,
      "clinical_report_generate",
      "call-6",
      {
        report_type: "hp",
        phi_access_grant: {
          basis: "de_identified",
          deidentification_method: "safe_harbor",
          deidentified_data_hash: "sha256:abc123",
          expires_at: Date.now() + 60_000,
        },
      },
      ecosystem,
    );
    expect(result.isError).toBe(false);
    expect(result.content).toContain("Generated hp report");
  });

  it("Datasci: hypothesis_test enforces declaration contract via dispatch", async () => {
    const result = await executeTool(
      resources,
      autonomy,
      "hypothesis_test",
      "call-7",
      { /* no declaration */ data: "data.csv" },
      ecosystem,
    );
    expect(result.isError).toBe(true);
    expect(result.content).toContain("HYPOTHESIS_NOT_DECLARED");
  });

  it("dispatch reaches the analytics SQL classifier", async () => {
    const result = await executeTool(
      resources,
      autonomy,
      "sql_query",
      "call-8",
      { query: "SELECT * FROM users" },
      ecosystem,
    );
    expect(result.isError).toBe(false);
  });

  it("unknown tool returns structured error even with ecosystem", async () => {
    const result = await executeTool(
      resources,
      autonomy,
      "totally_made_up_tool",
      "call-9",
      {},
      ecosystem,
    );
    expect(result.isError).toBe(true);
    expect(result.content).toContain("Unknown tool");
  });

  it("built-in tools still work alongside ecosystem", async () => {
    await fs.writeFile(path.join(tmpDir, "test.txt"), "hello world");
    const result = await executeTool(
      resources,
      autonomy,
      "read_file",
      "call-10",
      { path: "test.txt" },
      ecosystem,
    );
    expect(result.isError).toBe(false);
    expect(result.content).toBe("hello world");
  });
});
