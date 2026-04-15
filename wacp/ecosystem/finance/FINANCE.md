# WACP Ecosystem: Finance Vertical

```yaml
id: wacp-eco-finance
type: ecosystem-spec
status: draft
created: 2026-04-10
lineage: IMPLEMENTATION.md (27C)
depends_on:
  - wacp-impl-cli-agent
  - wacp-impl-tool-framework
  - wacp-impl-local-sdk
  - wacp-impl-coordinator-sdk
  - wacp-impl-runtime
  - wacp-eco-swe
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, ecosystem, finance, regulatory, compliance, fiduciary, audit, vertical, multi-agent, workflows]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Role Taxonomy](#2-role-taxonomy)
3. [Task Taxonomy](#3-task-taxonomy)
4. [Tool Catalog](#4-tool-catalog)
5. [Agent Profiles](#5-agent-profiles)
6. [Workflows](#6-workflows)
7. [Execution Model](#7-execution-model)
8. [Quality Criteria](#8-quality-criteria)
9. [Gate Policies](#9-gate-policies)
10. [Package Structure](#10-package-structure)
11. [Test Requirements](#11-test-requirements)
12. [References](#12-references)

---

## 1. Purpose

This spec defines the Finance ecosystem vertical. It answers "how does the platform behave when the task is investment analysis, trading, portfolio management, or regulated financial activity" — not "how does the platform work" (that's the runtime + middleware).

**What the vertical provides:** Roles (who does what), task types (what kinds of work exist), tools (what capabilities are available), profiles (how each role behaves), workflows (how work decomposes into protocol-level workspaces), quality criteria (how to evaluate output), gate policies (when to ask the human).

**Key constraint — regulatory compliance and fiduciary duty:** Every trade or portfolio action must pass a compliance check **before execution**. The `trade_execute` tool refuses execution unless an approved `compliance_check` checkpoint exists in the trail for the proposed trade. Compliance checks classify the trade against a forbidden-pattern list (insider trading, wash trades, churning, front-running, layering, spoofing) and verify suitability against the client's risk profile. Every report and recommendation must declare a fiduciary stance (conflicts disclosed, suitability verified). The audit trail is hash-chained at the runtime layer — Finance vertical leverages it directly: every tool execution becomes an immutable, ordered record. This is distinct from MLOps (compute-budget) and Analytics (SQL-safety): here, the constraint is **legal** — actions that would violate securities law are blocked at the tool layer, not deferred to a downstream review.

**Execution model:** The CLI agent loads the Finance vertical at boot. When a goal is submitted, the CLI detects the task type, selects the matching workflow, and executes it through the WACP runtime — each stage is a real workspace with its own role profile, tool whitelist, signals, and checkpoints. The vertical defines the decomposition; the protocol enforces it.

---

## 2. Role Taxonomy

Five derived roles, each with a specific concern:

| Role | Extends | Concern | Tool access | Autonomy |
|------|---------|---------|-------------|----------|
| `finance:analyst` | worker | Market analysis, financial modeling, valuation | Read + model | Gated |
| `finance:portfolio_manager` | worker | Portfolio construction, allocation, rebalancing decisions | Allocate + rebalance | Gated |
| `finance:risk_officer` | worker | Risk measurement, exposure analysis, limit enforcement | Risk + read | Gated |
| `finance:compliance_officer` | worker | Regulatory checks, KYC/AML, trade pre-approval | Compliance + KYC | Gated |
| `finance:auditor` | observer | Audit trail review, fiduciary verification, filing review | Read-only | Autonomous |

**Protocol mapping:** Each role maps to a workspace role at dispatch time. The coordinator creates a workspace with the role's tool whitelist and the profile's system prompt as the directive. The agent binds to the workspace and operates within its permissions.

**Compliance context:** Every workspace in a trade or rebalance workflow carries a `compliance_check` checkpoint that must precede any `trade_execute` tool call. The tool refuses execution if no prior approval exists in the trail or if the approval is stale (older than the policy window).

---

## 3. Task Taxonomy

Nine task types with default decomposition:

| Type | Description | Default workflow | Roles involved |
|------|-------------|------------------|----------------|
| `finance:analyze` | Equity, credit, or macro analysis | Direct (1 stage) | 1 (analyst) |
| `finance:model` | Build a financial model — DCF, LBO, comparables | Direct (1 stage) | 1 (analyst) |
| `finance:trade` | Execute a trade | `finance:trade-execution` (4 stages) | 3 (analyst, compliance_officer, portfolio_manager) |
| `finance:rebalance` | Rebalance a portfolio toward target weights | `finance:portfolio-rebalance` (4 stages) | 3 (portfolio_manager, risk_officer, compliance_officer) |
| `finance:risk_assess` | Compute risk metrics — VaR, CVaR, exposure | Direct (1 stage) | 1 (risk_officer) |
| `finance:compliance_check` | Pre-trade or pre-publication compliance review | Direct (1 stage) | 1 (compliance_officer) |
| `finance:audit` | Audit trail review and verification | Direct (1 stage) | 1 (auditor) |
| `finance:report` | Produce a financial report — research, holdings, performance | `finance:full-report` (5 stages) | 4 (analyst, risk_officer, compliance_officer, auditor) |
| `finance:onboard` | Client onboarding — KYC, AML, suitability | `finance:client-onboarding` (3 stages) | 2 (compliance_officer, portfolio_manager) |

**Detection:** The CLI classifies user goals into task types via keyword heuristics: "trade"/"buy"/"sell"/"order" → `finance:trade`, "rebalance"/"reweight" → `finance:rebalance`, "model"/"DCF"/"LBO"/"valuation" → `finance:model`, "risk"/"VaR"/"exposure" → `finance:risk_assess`, "compliance"/"pre-trade" → `finance:compliance_check`, "audit"/"trail" → `finance:audit`, "report"/"research" → `finance:report`, "KYC"/"AML"/"onboard" → `finance:onboard`, default → `finance:analyze`.

---

## 4. Tool Catalog

Finance-specific tools beyond the CLI's built-in 7. All 17 tools (7 built-in + 10 finance) are registered at boot and filtered per stage by the profile's whitelist.

| Tool | Description | Operation type |
|------|-------------|----------------|
| `market_data_fetch` | Fetch market data — quotes, fundamentals, historical prices, corporate actions | `data_read` |
| `financial_model_build` | Build a financial model — DCF, LBO, comparables, sum-of-the-parts | `compute_exec` |
| `risk_calc` | Compute risk metrics — VaR, CVaR, beta, Greeks, scenario stress | `compute_exec` |
| `compliance_check` | Pre-trade compliance check — classifies trade, screens against forbidden patterns, verifies suitability | `data_read` |
| `kyc_screen` | KYC/AML/sanctions screen — identity, PEP, OFAC/SDN, adverse media | `data_read` |
| `trade_execute` | Execute a trade order. REQUIRES a prior approved `compliance_check` checkpoint. | `data_write` |
| `portfolio_rebalance` | Rebalance a portfolio toward target weights, generating a trade list | `data_write` |
| `audit_trail_export` | Export the hash-chained audit trail for a workspace, with cryptographic verification | `data_read` |
| `regulatory_filing_prepare` | Prepare a regulatory filing — 10-K, 10-Q, 13F, ADV — from structured data | `file_write` |
| `disclosure_review` | Review disclosure language for material risks, conflicts of interest, required statements | `data_read` |

Tool executors auto-detect the project's market data source (Bloomberg, Refinitiv, Polygon, public APIs) and the regulatory jurisdiction (SEC, FINRA, MiFID II, FCA) from the workspace config.

**Compliance enforcement:** `trade_execute` checks the trail for a prior `compliance_check` checkpoint with `status: "approved"` and a `trade_id` that matches the proposed trade. If absent, stale (older than the policy window — default 5 minutes), or rejected, it refuses execution with `COMPLIANCE_NOT_APPROVED`. The compliance check itself classifies the trade against the forbidden-pattern list — `insider_trading`, `wash_trade`, `churning`, `front_running`, `layering`, `spoofing`, `painting_the_tape` — and rejects matches outright. Suitability is verified against the client's recorded risk tolerance and investment objectives.

---

## 5. Agent Profiles

One profile per role. Each profile provides: system prompt, tool whitelist, autonomy level.

**`finance:analyst`** — Read + model. System prompt instructs: cite every data source. Document model assumptions explicitly. Distinguish forecast from observation. Never recommend a trade — that's the portfolio manager's call. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `market_data_fetch`, `financial_model_build`, `disclosure_review`, `git_status`.

**`finance:portfolio_manager`** — Allocate + rebalance. System prompt instructs: every allocation decision must reference the client's investment policy statement. Run the portfolio_rebalance tool to generate the trade list, then route through compliance before execution. Never call `trade_execute` directly without a compliance checkpoint. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `market_data_fetch`, `portfolio_rebalance`, `risk_calc`, `trade_execute`.

**`finance:risk_officer`** — Risk + read. System prompt instructs: report risk metrics with confidence intervals. Flag exposure breaches against limits. Run scenario stress alongside point estimates. Document the risk model used (parametric, historical, Monte Carlo). Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `market_data_fetch`, `risk_calc`, `disclosure_review`.

**`finance:compliance_officer`** — Compliance + KYC. System prompt instructs: classify every trade against the forbidden-pattern list before approving. Verify the client's KYC and suitability are current. Never approve a trade that you would not personally defend in front of a regulator. Document the regulation cited (SEC, FINRA, MiFID II) for every decision. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `compliance_check`, `kyc_screen`, `disclosure_review`, `audit_trail_export`.

**`finance:auditor`** — Read-only, autonomous. System prompt instructs: verify the audit trail hash chain is intact. Confirm every trade has a compliance checkpoint. Confirm every report has a fiduciary disclosure. Verify regulatory filings match the underlying data. Produce an audit report with pass/warn/fail per dimension. Tools: `read_file`, `list_dir`, `search_files`, `audit_trail_export`, `disclosure_review`, `market_data_fetch`.

---

## 6. Workflows

Four workflow DAGs plus direct task types. Each defines stages with role assignments, dependencies, and gate policies.

**`finance:trade-execution`** (4 stages):
```
analyze (analyst) ──→ compliance (compliance_officer) ──→ execute (portfolio_manager) ──→ record (auditor)
                          [gated]                              [gated]
```
Used by: `finance:trade`. Analyze produces the trade rationale. Compliance is gated — human approves the compliance verdict before execution. Execute is gated — final human approval before the order goes to market. Record verifies the audit trail entry.

**`finance:portfolio-rebalance`** (4 stages):
```
assess (risk_officer) ──→ propose (portfolio_manager) ──→ compliance (compliance_officer) ──→ execute (portfolio_manager)
                                                              [gated]                              [gated]
```
Used by: `finance:rebalance`. Assess measures current exposure. Propose generates the rebalance trade list. Compliance is gated — every trade in the list is pre-checked. Execute is gated — final approval to send orders.

**`finance:full-report`** (5 stages):
```
collect (analyst) ──→ analyze (analyst) ──→ risk (risk_officer) ──→ compliance (compliance_officer) ──→ publish (auditor)
                                                                          [gated]                            [gated]
```
Used by: `finance:report`. Collect gathers data. Analyze produces findings. Risk adds risk metrics. Compliance is gated — disclosure language reviewed. Publish is gated — auditor signs off on the final document.

**`finance:client-onboarding`** (3 stages):
```
kyc (compliance_officer) ──→ suitability (compliance_officer) ──→ approve (portfolio_manager)
       [gated]                       [gated]                              [gated]
```
Used by: `finance:onboard`. KYC is gated — identity verification confirmed. Suitability is gated — risk tolerance and investment objectives recorded. Approve is gated — portfolio manager accepts the client.

**Direct execution:** `finance:analyze` (analyst), `finance:model` (analyst), `finance:risk_assess` (risk_officer), `finance:compliance_check` (compliance_officer), `finance:audit` (auditor).

**DAG validation:** `validateWorkflow()` checks that all dependencies exist and no cycles are present.

---

## 7. Execution Model

The Finance vertical executes through the WACP protocol — not as a simulation.

**Goal submission:**
```
1. User submits goal: "buy 10,000 shares of MSFT for the growth fund"
2. CLI detects task type: finance:trade
3. CLI selects workflow: finance:trade-execution (4 stages)
4. CoordinatorService.SubmitGoal → runtime creates root workspace
5. CoordinatorService.Decompose → runtime creates task graph (4 tasks)
```

**Per-stage execution:**
```
1. CoordinatorService.Dispatch(task, role, tools) → runtime creates child workspace
2. AgentService.Bind(workspace) → agent connects to workspace
3. AgentService.EmitSignal(STARTED) → trail records stage start
4. LLM loop:
   a. Call LLM with stage profile (system prompt + filtered tools)
   b. Stream tokens to terminal
   c. For each tool call:
      - Autonomy gate check
      - Compliance check (for trade_execute: verify prior approved compliance_check checkpoint exists and is fresh)
      - Execute tool via LocalResources
      - AgentService.CreateCheckpoint(observation, tool result)
   d. Feed tool results back to LLM
5. AgentService.CreateCheckpoint(artifact, FINAL, stage output)
6. AgentService.EmitSignal(COMPLETE)
7. Stage output flows as context to next stage
```

**Compliance check contract:** The compliance stage in `finance:trade-execution` produces a checkpoint with fields: `trade_id`, `instrument`, `side`, `quantity`, `price`, `status` (`approved` | `rejected`), `regulation_cited`, `forbidden_pattern_screened`, `suitability_verified`, `kyc_current`, `expires_at`. The `trade_execute` tool reads the most recent matching `compliance_check` checkpoint from the current workspace and rejects execution if any of: missing, status != approved, expired, trade_id mismatch.

**Forbidden-pattern screen:** `compliance_check` runs the proposed trade through a classifier that flags `insider_trading`, `wash_trade`, `churning`, `front_running`, `layering`, `spoofing`, `painting_the_tape`. A match is an automatic rejection — no human override at the tool layer.

**Audit trail integrity:** The runtime's hash-chained trail is the legal record. The Finance vertical does not maintain a parallel ledger. Every tool execution is a checkpoint; every checkpoint is signed by its parent. The `audit_trail_export` tool produces a verifiable export — auditors can re-validate the hash chain offline.

---

## 8. Quality Criteria

Six dimensions for evaluating Finance output:

| Dimension | Definition | Evaluation |
|-----------|-----------|------------|
| **Regulatory compliance** | All applicable regulations cited and checked | Compliance checkpoint present and approved |
| **Audit trail integrity** | Hash chain intact, every action recorded | All trades have matching trail entries |
| **Fiduciary duty** | Suitability verified, conflicts disclosed | Suitability checkpoint + COI disclosure present |
| **Risk disclosure** | Material risks disclosed with specificity | Risk language present and specific |
| **Data provenance** | Sources cited, prices timestamped, model inputs versioned | Provenance metadata present on all data |
| **Documentation** | Methodology, assumptions, valuations documented | Methodology checkpoint present |

**Evaluation function:** Each dimension returns `pass`, `warn`, or `fail`. Rules:
- Trade executed without compliance checkpoint → `regulatory_compliance` = `fail`
- Compliance checkpoint rejected → `regulatory_compliance` = `fail`
- Forbidden pattern detected → `regulatory_compliance` = `fail`
- KYC missing or expired → `regulatory_compliance` = `fail`
- Regulation not cited → `regulatory_compliance` = `warn`
- Audit trail hash invalid → `audit_trail_integrity` = `fail`
- Trade missing trail entry → `audit_trail_integrity` = `fail`
- Trail entry timestamps out of order → `audit_trail_integrity` = `fail`
- Suitability not verified → `fiduciary_duty` = `fail`
- Conflict of interest undisclosed → `fiduciary_duty` = `fail`
- Recommendation contradicts client risk tolerance → `fiduciary_duty` = `fail`
- Material risk undisclosed → `risk_disclosure` = `fail`
- Generic boilerplate risk language → `risk_disclosure` = `warn`
- Source citation missing → `data_provenance` = `fail`
- Price timestamp missing → `data_provenance` = `warn`
- Model input version missing → `data_provenance` = `warn`
- Methodology undocumented → `documentation` = `fail`
- Assumptions implicit → `documentation` = `warn`

Overall: `pass` if all pass, `warn` if any warn and none fail, `fail` if any fail.

---

## 9. Gate Policies

### Compliance Gating

The defining constraint of the Finance vertical. Gates enforce regulatory pre-approval and fiduciary duty — no trade without compliance, no execution without human sign-off, no publication without disclosure review.

| Transition | Gate | Rationale |
|-----------|------|-----------|
| Goal → analyze | None | Analysis is read-only |
| Analyze → compliance | **Human approval** | Compliance officer must independently review the analyst's rationale |
| Compliance → execute | **Human approval** | Final human approval before any order reaches the market |
| Execute → record | Auto | Recording is observational |
| Assess → propose | None | Proposal is internal |
| Propose → compliance | **Human approval** | Every trade in a rebalance batch is pre-checked |
| Compliance → execute (rebalance) | **Human approval** | Final approval to send orders |
| Collect → analyze (report) | None | Aggregation is read-only |
| Risk → compliance (report) | **Human approval** | Disclosure language reviewed before publication |
| Compliance → publish (report) | **Human approval** | Final auditor sign-off |
| KYC → suitability | **Human approval** | Identity confirmed before suitability questions |
| Suitability → approve | **Human approval** | Risk tolerance recorded before account opens |
| Approve → trade (onboarding) | **Human approval** | Portfolio manager personally accepts the client |
| Any stage → trade_execute tool | **Compliance check** | Tool refuses without approved compliance_check checkpoint |

---

## 10. Package Structure

```
ecosystem/finance/
├── FINANCE.md             # This spec
├── package.json           # @wacp/finance
├── tsconfig.json
├── .gitignore
├── src/
│   ├── index.ts           # Public exports
│   ├── taxonomy.ts        # 5 roles + 9 task types with lookup functions
│   ├── tools/
│   │   └── finance-tools.ts    # 10 tool definitions + executors (compliance enforcement)
│   ├── profiles/
│   │   └── profiles.ts         # 5 profiles with system prompts + tool whitelists
│   ├── workflows/
│   │   └── workflows.ts        # 4 workflow DAGs + validation
│   └── quality/
│       └── quality.ts          # 6 dimensions + evaluateQuality() → QualityReport
└── tests/
    ├── taxonomy.test.ts        # 14 tests
    ├── tools.test.ts           # 14 tests
    ├── profiles.test.ts        # 12 tests
    ├── workflows.test.ts       # 15 tests
    └── quality.test.ts         # 17 tests
```

---

## 11. Test Requirements

| Module | Tests | Count |
|--------|-------|-------|
| `taxonomy.ts` | 5 roles unique, correct extends/access/autonomy. 9 task types unique, correct workflow mapping. Lookup functions. Auditor is observer. Trade and rebalance use distinct workflows. | 14 |
| `tools/finance-tools.ts` | 10 definitions unique, valid schemas. Required fields present. Compliance check validation logic works. Forbidden pattern detection works. | 14 |
| `profiles/profiles.ts` | 5 profiles with non-empty prompts. Tool whitelist matches role. Compliance officer has compliance_check. Portfolio manager has trade_execute. Analyst lacks trade_execute. Auditor is read-only and autonomous. Workers are gated. | 12 |
| `workflows/workflows.ts` | 4 workflows unique, correct stage counts. Dependency order correct. Compliance and execute stages gated in trade-execution. Compliance and execute gated in rebalance. Onboarding stages all gated. DAG validation passes. Task type mapping complete. | 15 |
| `quality/quality.ts` | 6 dimensions unique. All-pass → pass. No compliance → fail. Forbidden pattern → fail. KYC missing → fail. No regulation cited → warn. Bad hash → audit fail. Suitability missing → fiduciary fail. COI undisclosed → fail. Risk undisclosed → fail. Boilerplate risk → warn. No source → fail. No timestamp → warn. No methodology → fail. Implicit assumptions → warn. | 17 |
| **Total** | | **72** |

---

## 12. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| SWE vertical spec | §1–12 | §1 | Pattern template — structure, execution model |
| DevOps vertical spec | §1 | §1 | Distinction: DevOps gates by environment, finance gates by regulation |
| MLOps vertical spec | §1 | §1 | Distinction: MLOps gates by compute budget, finance gates by legal admissibility |
| Analytics vertical spec | §1 | §1 | Distinction: analytics enforces SQL safety, finance enforces trade legality |
| Data Science vertical spec | §1 | §1 | Distinction: datasci enforces hypothesis declaration, finance enforces compliance pre-check |
| CLI agent spec | §6–7 | §7 | Workflow execution, stage agent loop |
| Coordinator SDK spec | §3–5 | §7 | SubmitGoal, Decompose, Dispatch RPCs |
| Agent SDK v2 spec | §3 | §7 | Bind, EmitSignal, CreateCheckpoint |
| Runtime spec | §3 (process model) | §7 | Workspace lifecycle, trail recording |
| Tool framework spec | §3 | §4 | ToolDefinition schema |
| IMPLEMENTATION.md | 27C | §1 | Finance vertical design |

---

*WACP ecosystem specification — authored by Akil Abderrahim and Claude Opus 4.6*
