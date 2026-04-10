# WACP — Implementation Plan

```yaml
created: 2026-04-01
revised: 2026-04-02
status: active
authors:
  - Akil Abderrahim
  - Claude Opus 4.6
```

---

## Baseline

**Phases 0–19 + T1–T5: complete.** Protocol runtime fully implemented and tested. 1,192 tests across 3 ecosystems (947 Rust, 181 TypeScript, 64 Python). 12 Rust crates, Python agent SDK, TypeScript Highway UI. See `SEED-CONTEXT.md` for full state.

```
Ecosystem    (domain verticals — parameterize the platform)
─────────────────────────────────────────────────────── ecosystem boundary
Applications (CLI, SDK, API, IDE, dashboard, bridge)
─────────────────────────────────────────────────────── application boundary
Middleware   (7 frameworks — contracts for building on the runtime)
─────────────────────────────────────────────────────── middleware boundary
WACP Runtime (12 Rust crates + proto + protocol specs)     ← DONE
```

---

## Phases 20–26: Status + Gaps

Middleware, applications, and ecosystem work. Tests: 663 across Rust + TypeScript. **Structurally incomplete.** The application layer (CLI + SWE) bypasses the protocol — no workspaces, no signals, no trail, no coordinator. The middleware is partially stubbed. The vertical's workflows are dead data.

### What was built (correct and complete)

| Phase | Crate / Package | Tests | Status |
|-------|----------------|-------|--------|
| 20 | `wacp-tools` — descriptor, handler, package, registry, execution, resilience, sandbox, discovery | 124 | **Complete** |
| 21 | `wacp-llm` — adapter trait, types, result, error, stream, retry, rate_limit, cost, providers (Anthropic + OpenAI) | 134 | **Complete** |
| 22 (partial) | `wacp-sdk` — AgentContext wrapping Agent + ToolRegistry | 58 | **Complete** |
| 22 (partial) | `wacp-coordinator-sdk` — CoordinatorContext client + `coordinator.proto` (15 RPCs) | 11 | **Client only** |
| 23 (partial) | `wacp-security` — ContentFilter, SecretStore, AuditEvent | 45 | **Complete** |
| 23 (partial) | `wacp-transport` — ApiKeyAuthenticator, SessionTokenAuthenticator | 91 | **Partial** |
| 24 (partial) | `@wacp/local` — session, autonomy, interaction, resources, context | 73 | **No orchestration** |
| 25 | `@wacp/cli` — config, tools, commands, display, repl, agent loop, SSE streaming | 70 | **No protocol use** |
| 26 | `@wacp/swe` — taxonomy, tools, profiles, workflows, quality | 57 | **Unused by CLI** |

### What was NOT built (gaps)

| # | Gap | Where | What's missing |
|---|-----|-------|----------------|
| **G1** | CoordinatorService server | `wacp-transport` | Server-side handlers bridging 15 coordinator RPCs to `wacp-coordinator` internals. The proto and client SDK exist but nothing serves the RPCs. |
| **G2** | Self-orchestration | `@wacp/local` | `orchestrator.ts` — root agent dispatching child workspaces via CoordinatorContext, consuming workflow definitions, switching profiles per stage. |
| **G3** | Protocol-aware CLI | `@wacp/cli` | The agent loop must create workspaces, emit signals, record trail, create checkpoints — not raw-fetch an LLM and call shell commands. |
| **G4** | Workflow execution | `@wacp/swe` + `@wacp/cli` | SWE workflows define task DAGs with role assignments. Nothing executes them. The CLI should decompose goals into workflow stages and execute each with the correct profile. |
| **G5** | REST gateway handlers | `wacp-transport` | 25 endpoint handlers return 501. Must wire to gRPC clients. |
| **G6** | WebSocket binding | `wacp-transport` | JSON-RPC over WebSocket. Not implemented. |
| **G7** | Python bindings | `sdk-python` | tools, llm, coordinator, local packages — all missing. |
| **G8** | OAuth authenticator | `wacp-transport` | OIDC/JWT validation — not implemented. |

---

## Phase 26R — Remediation

**No Phase 27 until every gap is closed.** This phase makes the system architecturally correct: the CLI uses the WACP protocol, the SWE vertical's workflows execute through the coordinator, workspaces exist, signals flow, trail records.

### 26R.1 — CoordinatorService Server (G1)

Implement server-side handlers in `crates/wacp-transport/` that bridge all 15 `CoordinatorService` RPCs to `wacp-coordinator` internals.

| # | Task | Deliverable |
|---|------|-------------|
| 26R.1.1 | `grpc_coordinator.rs` | `CoordinatorServiceImpl` struct holding coordinator state. Implement all 15 RPC handlers: SubmitGoal → create root task, Decompose → task_graph.add_tasks, GetReadyTasks → task_graph.ready_tasks, CancelTask → cascade cancel, Dispatch → coordinator.dispatch, Abort/Suspend/Resume → workspace commands, SendDirective/SendFeedback → inject envelope, TriggerIntegration → integration_queue, GetAllocatable → budget_enforcer, StreamSignals → event_bus.subscribe. |
| 26R.1.2 | Wire into `grpc_server.rs` | Register `CoordinatorServiceServer` alongside Agent + Highway. New port config (default 9092). |
| 26R.1.3 | Integration tests | Each RPC: call client → server processes → verify coordinator state changed. Use `InProcessTransport` pattern. |

**Exit criteria:** `CoordinatorContext` client connects to `CoordinatorServiceImpl` server. All 15 RPCs execute against a real coordinator. Tests verify state changes.

### 26R.2 — Self-Orchestration (G2)

Implement `orchestrator.ts` in `@wacp/local` that executes workflow stages through the protocol.

| # | Task | Deliverable |
|---|------|-------------|
| 26R.2.1 | `orchestrator.ts` | `WorkflowExecutor` class. Takes a `Workflow` + `AgentProfile[]` + `LocalResources` + LLM config. Executes stages sequentially: for each stage, set the profile (system prompt + tool whitelist), run the agent loop, capture output, pass as context to next stage. Gate check between gated stages. |
| 26R.2.2 | Protocol integration | Each stage creates a logical workspace context (workspace ID, trail entries, checkpoints). Signals emitted at stage boundaries (started, complete, failed). Stage output recorded as checkpoints. |
| 26R.2.3 | Wire into `LocalSession` | `session.executeWorkflow(workflow, profiles, goal)` dispatches to `WorkflowExecutor`. Single-agent sequential execution (not parallel — that requires the runtime). |
| 26R.2.4 | Tests | Workflow with 2 stages: planner → implementer. Verify: both profiles used, stage output flows, signals emitted, checkpoints recorded. Gated stage pauses for approval. |

**Exit criteria:** `LocalSession.executeWorkflow()` runs a multi-stage SWE workflow. Each stage uses its profile. Output flows between stages. Trail records stage transitions.

### 26R.3 — Protocol-Aware CLI (G3)

Rewrite the CLI agent loop to use WACP protocol concepts.

| # | Task | Deliverable |
|---|------|-------------|
| 26R.3.1 | Goal → workflow routing | When user submits a goal, classify it as a task type (swe:implement, swe:debug, etc.). Look up the default workflow. Execute via `session.executeWorkflow()`. |
| 26R.3.2 | Stage display | Print current stage name and role before each agent loop iteration. Show stage transitions. Display checkpoint summaries. |
| 26R.3.3 | Profile switching | Each stage uses the SWE profile's system prompt and tool whitelist. The agent loop receives different tools per stage (planner: read-only, implementer: read-write). |
| 26R.3.4 | Quality evaluation | After the last stage, run `evaluateQuality()` from the SWE vertical. Print the quality report (pass/warn/fail per dimension). |
| 26R.3.5 | Tests | Submit "implement feature" goal → decomposes into 4-stage workflow → each stage runs with correct profile → quality report at end. |

**Exit criteria:** `wacp> implement a login page` executes: plan (read-only) → implement (read-write, gated) → test (test_run) → review (read-only, autonomous). Quality report printed. Trail records all stages.

### 26R.4 — SWE Vertical Integration (G4)

Wire the SWE vertical into the CLI so workflows are the execution model, not decoration.

| # | Task | Deliverable |
|---|------|-------------|
| 26R.4.1 | Task type detection | Classify user goals into SWE task types using LLM or heuristics. "fix the bug" → `swe:debug`. "add authentication" → `swe:implement`. "refactor the database module" → `swe:refactor`. |
| 26R.4.2 | Vertical loading | CLI loads `@wacp/swe` at boot. Registers SWE tools alongside built-in tools. Makes profiles available to the orchestrator. |
| 26R.4.3 | Integration test | Full E2E: boot CLI → submit SWE goal → detect task type → select workflow → execute stages with profiles → evaluate quality. |

**Exit criteria:** The SWE vertical is the execution engine, not a passive data package.

### 26R.5 — REST Gateway Wiring (G5)

Wire all 25 REST gateway handlers to their gRPC counterparts.

| # | Task | Deliverable |
|---|------|-------------|
| 26R.5.1 | Gateway holds gRPC clients | `GatewayState` holds `CoordinatorServiceClient`, `HighwayServiceClient`, `AgentServiceClient`. Constructed at startup. |
| 26R.5.2 | Wire handlers | Each handler: deserialize JSON → call gRPC client → serialize response → return HTTP status. Error mapping from gRPC status to HTTP status. |
| 26R.5.3 | SSE streaming handlers | Trail, gates, escalations, signals, workspaces: open gRPC stream → convert to SSE events → write to HTTP response. |
| 26R.5.4 | Tests | Each endpoint: mock gRPC client → call HTTP endpoint → verify response. SSE: verify event format. |

**Exit criteria:** Every REST endpoint returns real data from the runtime. No 501 stubs.

### 26R.6 — WebSocket Binding (G6)

| # | Task | Deliverable |
|---|------|-------------|
| 26R.6.1 | `websocket.rs` | WebSocket upgrade handler on `/v1/ws`. JSON-RPC 2.0 framing. Authenticate on connect. Subscribe to event streams. Ping/pong keepalive (30s). |
| 26R.6.2 | Tests | Connect + authenticate. Send method → receive result. Subscribe → receive server-push events. |

### 26R.7 — Python Bindings (G7)

| # | Task | Deliverable |
|---|------|-------------|
| 26R.7.1 | `sdk-python/src/wacp/tools/` | `ToolDescriptor`, `ToolPackage`, `@tool` decorator, `ToolContext`. |
| 26R.7.2 | `sdk-python/src/wacp/llm/` | `LlmAdapter` protocol, `AnthropicAdapter`, `OpenAiAdapter`. Async `httpx`. |
| 26R.7.3 | `sdk-python/src/wacp/coordinator.py` | `CoordinatorContext` async class wrapping gRPC client. |
| 26R.7.4 | `sdk-python/src/wacp/local/` | `LocalSession`, `AutonomyManager`, `LocalResources`. |
| 26R.7.5 | Tests | Each module: roundtrip tests, lifecycle tests. |

### 26R.8 — OAuth Authenticator (G8)

| # | Task | Deliverable |
|---|------|-------------|
| 26R.8.1 | `auth_oauth.rs` | `OAuthAuthenticator` implementing `Authenticator`. JWT validation (structure, iss, aud, exp). JWKS fetch + cache. |
| 26R.8.2 | Tests | Valid JWT accepted. Expired rejected. Wrong issuer rejected. Wrong audience rejected. |

---

## Phase 26R Exit Criteria

**Every item must pass before Phase 27 begins:**

- [ ] CoordinatorService serves all 15 RPCs against a real coordinator
- [ ] Self-orchestration executes multi-stage workflows with profile switching
- [ ] CLI decomposes goals into SWE workflows and executes them through the protocol
- [ ] SWE workflows are the execution model — not a data package
- [ ] REST gateway returns real data from runtime — no 501 stubs
- [ ] WebSocket binding handles JSON-RPC + server-push events
- [ ] Python bindings for tools, llm, coordinator, local — all tested
- [ ] OAuth authenticator validates JWTs
- [ ] All tests pass across all 3 ecosystems

---

## Phase 27 — Remaining Verticals (E2–E5)

**Rationale for reorder:** Build all verticals before the API server. Four verticals stress-test protocol generality across domains. The API + dashboard design benefits from knowing the full vertical surface — roles, task types, tools, workflows, quality dimensions — rather than being shaped by SWE alone.

**Pattern:** Each vertical follows the SWE template (`ecosystem/swe/`). Each sub-phase produces: spec, taxonomy, tools, profiles, workflows, quality criteria, tests. Verticals are independent — sub-phases can overlap.

### 27A — DevOps Vertical (E2)

**Roles (5):** architect, deployer, monitor, responder, auditor.

**Task types (9):** provision, deploy, monitor, respond, audit, migrate, configure, secure, optimize.

**Key constraint:** Blast radius model. Environment-scaled gating — staging is auto-gated, production requires human approval. Rollback-aware workflows.

| # | Task | Deliverable |
|---|------|-------------|
| 27A.1 | Spec | `ecosystem/devops/DEVOPS.md` — roles, task types, tools, profiles, workflows, quality, gates |
| 27A.2 | Taxonomy | `taxonomy.ts` — 5 roles + 9 task types + lookup functions |
| 27A.3 | Tools | Domain tools: `infra_plan`, `deploy_execute`, `rollback`, `health_check`, `log_query`, `metric_query`, `alert_manage`, `config_validate`, `secret_rotate`, `compliance_scan` |
| 27A.4 | Profiles | 5 profiles: architect (read + plan), deployer (execute + rollback), monitor (read + alert), responder (execute, gated), auditor (read-only, autonomous) |
| 27A.5 | Workflows | `devops:provision` (3 stages), `devops:deploy` (4 stages: plan → validate → deploy → verify), `devops:respond` (3 stages: triage → mitigate → postmortem), `devops:audit` (2 stages) |
| 27A.6 | Quality | Dimensions: availability, security posture, compliance, blast radius, rollback readiness, documentation |
| 27A.7 | Tests | Target: ~55 tests matching SWE coverage pattern |

### 27B — MLOps Vertical (E3)

**Roles (5):** researcher, trainer, evaluator, deployer, monitor.

**Task types (9):** experiment, train, evaluate, deploy, monitor, optimize, data-prep, reproduce, audit.

**Key constraint:** Compute budget enforcement. Reproducibility — every experiment must be checkpointed with hyperparameters, data hash, model hash. Model lineage tracked through trail.

| # | Task | Deliverable |
|---|------|-------------|
| 27B.1 | Spec | `ecosystem/mlops/MLOPS.md` |
| 27B.2 | Taxonomy | `taxonomy.ts` — 5 roles + 9 task types |
| 27B.3 | Tools | Domain tools: `dataset_validate`, `experiment_track`, `train_launch`, `eval_benchmark`, `model_register`, `model_deploy`, `drift_detect`, `compute_budget`, `reproduce_check`, `data_lineage` |
| 27B.4 | Profiles | 5 profiles: researcher (explore + experiment), trainer (compute-gated), evaluator (read + benchmark), deployer (model registry, gated), monitor (read + alert) |
| 27B.5 | Workflows | `mlops:experiment` (4 stages: design → data-prep → train → evaluate), `mlops:deploy` (3 stages: validate → deploy → monitor), `mlops:reproduce` (2 stages), `mlops:optimize` (3 stages) |
| 27B.6 | Quality | Dimensions: metric performance, reproducibility, data quality, compute efficiency, model freshness, documentation |
| 27B.7 | Tests | Target: ~55 tests |

### 27C — Finance Vertical (E4)

**Roles (5):** analyst, risk-analyst, compliance-officer, reporter, portfolio-analyst.

**Task types (9):** analyze, valuate, assess-risk, check-compliance, report, review-portfolio, rebalance, stress-test, investigate.

**Key constraint:** Regulatory compliance as a first-class gate. Fiduciary model — all recommendations must include risk disclosure. Audit trail is legally required, not optional.

| # | Task | Deliverable |
|---|------|-------------|
| 27C.1 | Spec | `ecosystem/finance/FINANCE.md` |
| 27C.2 | Taxonomy | `taxonomy.ts` — 5 roles + 9 task types |
| 27C.3 | Tools | Domain tools: `market_data`, `valuation_model`, `risk_calculator`, `compliance_check`, `report_generate`, `portfolio_analyze`, `stress_test`, `regulatory_lookup`, `audit_export`, `disclosure_attach` |
| 27C.4 | Profiles | 5 profiles: analyst (data access + models), risk-analyst (risk tools, autonomous), compliance-officer (regulatory lookup, gated), reporter (read + generate), portfolio-analyst (full analysis, gated) |
| 27C.5 | Workflows | `finance:analyze` (3 stages: data → model → report), `finance:risk-assess` (4 stages: identify → quantify → mitigate → report), `finance:compliance` (3 stages: check → remediate → certify), `finance:rebalance` (4 stages: analyze → propose → compliance-check → execute) |
| 27C.6 | Quality | Dimensions: accuracy, regulatory compliance, risk disclosure, timeliness, auditability, reproducibility |
| 27C.7 | Tests | Target: ~55 tests |

### 27D — Healthcare Vertical (E5)

**Roles (5):** clinician, researcher, analyst, compliance, coordinator.

**Task types (8):** assess, diagnose-support, research, analyze, monitor, report, audit, educate.

**Key constraint:** PHI/HIPAA compliance. All data handling must go through the security framework's content filter. Clinical validation gates — no output reaches patients without human sign-off. De-identification enforced at the tool level.

| # | Task | Deliverable |
|---|------|-------------|
| 27D.1 | Spec | `ecosystem/healthcare/HEALTHCARE.md` |
| 27D.2 | Taxonomy | `taxonomy.ts` — 5 roles + 8 task types |
| 27D.3 | Tools | Domain tools: `clinical_search`, `protocol_lookup`, `lab_interpret`, `risk_score`, `phi_filter`, `consent_verify`, `report_generate`, `audit_export`, `education_material`, `de_identify` |
| 27D.4 | Profiles | 5 profiles: clinician (clinical tools, always gated), researcher (search + analyze, gated), analyst (data + models, de-identified only), compliance (audit + verify, autonomous), coordinator (orchestration, gated) |
| 27D.5 | Workflows | `health:assess` (3 stages: gather → analyze → report), `health:research` (4 stages: question → search → synthesize → review), `health:audit` (2 stages: scan → report), `health:educate` (2 stages: assess-level → generate) |
| 27D.6 | Quality | Dimensions: clinical accuracy, PHI compliance, evidence basis, completeness, readability, regulatory adherence |
| 27D.7 | Tests | Target: ~55 tests |

### 27F — Data Analytics Vertical

**Roles (5):** analyst, modeler, validator, reporter, insights.

**Task types (8):** query, report, dashboard, model, analyze, validate, monitor, investigate.

**Key constraint:** Query reproducibility and data integrity. Every query result must be reproducible with the same query text and source data snapshot. Destructive SQL operations (DROP, TRUNCATE, DELETE without WHERE) are hard-gated. All reports cite their source queries and data freshness timestamps.

| # | Task | Deliverable |
|---|------|-------------|
| 27F.1 | Spec | `ecosystem/analytics/ANALYTICS.md` |
| 27F.2 | Taxonomy | `taxonomy.ts` — 5 roles + 8 task types |
| 27F.3 | Tools | Domain tools: `sql_query`, `dashboard_build`, `data_profile`, `kpi_calculate`, `report_generate`, `viz_create`, `data_reconcile`, `schema_explore`, `query_optimize`, `metric_define` |
| 27F.4 | Profiles | 5 profiles: analyst (query + profile, gated), modeler (schema + metric, gated), validator (reconcile + profile, autonomous), reporter (generate + viz, gated), insights (synthesis, autonomous) |
| 27F.5 | Workflows | `analytics:query-and-report` (3 stages: query → validate → report), `analytics:build-dashboard` (3 stages: model → build → validate), `analytics:investigate` (3 stages: explore → reconcile → report), `analytics:model-data` (3 stages: explore → model → validate) |
| 27F.6 | Quality | Dimensions: accuracy, data_freshness, reproducibility, completeness, clarity, performance |
| 27F.7 | Tests | Target: ~60 tests |

### 27G — Data Science Vertical

**Roles (5):** explorer, statistician, feature_engineer, modeler, reviewer.

**Task types (9):** explore, hypothesize, test, model, feature, validate, interpret, report, review.

**Key constraint:** Statistical rigor. Every hypothesis test must declare alternative hypothesis, significance level, and multiple-testing correction strategy before execution. All point estimates must be accompanied by confidence intervals. Assumption checks (normality, independence, homoscedasticity) are required before parametric tests.

| # | Task | Deliverable |
|---|------|-------------|
| 27G.1 | Spec | `ecosystem/datasci/DATASCI.md` |
| 27G.2 | Taxonomy | `taxonomy.ts` — 5 roles + 9 task types |
| 27G.3 | Tools | Domain tools: `stat_summary`, `correlation_analysis`, `hypothesis_test`, `feature_extract`, `feature_transform`, `model_fit`, `diagnostic_plots`, `bootstrap_sample`, `causal_inference`, `interpretation` |
| 27G.4 | Profiles | 5 profiles: explorer (EDA tools, gated), statistician (tests + inference, gated), feature_engineer (extract + transform, gated), modeler (fit + diagnostics, gated), reviewer (read-only, autonomous) |
| 27G.5 | Workflows | `datasci:full-analysis` (5 stages: explore → hypothesize → test → interpret → review), `datasci:hypothesis-test` (3 stages: declare → test → interpret), `datasci:model-build` (4 stages: explore → feature → fit → validate), `datasci:exploration` (2 stages: profile → visualize) |
| 27G.6 | Quality | Dimensions: statistical_rigor, reproducibility, interpretation_validity, assumptions_checked, effect_size, documentation |
| 27G.7 | Tests | Target: ~60 tests |

### Phase 27 Exit Criteria

- [ ] 6 vertical specs written and reviewed (DevOps, MLOps, Finance, Healthcare, Data Analytics, Data Science)
- [ ] 6 verticals implemented following the SWE template
- [ ] Each vertical: taxonomy, tools, profiles, workflows, quality — all tested
- [ ] Each vertical's workflows execute through the protocol (SubmitGoal → Decompose → Dispatch → Bind → Signal → Checkpoint)
- [ ] ~350 new tests across 6 verticals
- [ ] CLI can load any vertical at boot and route goals to its workflows
- [ ] Protocol generalizes: no SWE-specific assumptions in runtime or middleware

---

## Phase 28 — IDE + Chat Bridge

| # | Component | Scope |
|---|-----------|-------|
| 28.1 | IDE extension | VS Code + JetBrains — workspace panel, signal stream, inline checkpoints |
| 28.2 | Chat bridge | Slack/Discord integration — goal submission, status, approvals via chat |

Depends on: 26R (runtime + middleware). Independent of verticals.

---

## Phase 29 — API Server + Dashboard

**Rationale for deferral:** With 5 verticals complete, the API surface is designed against the full domain spectrum — not just SWE.

| # | Component | Scope |
|---|-----------|-------|
| 29.1 | API server | HTTP API wrapping the 3 gRPC services. Auth (OAuth + API key). Rate limiting. OpenAPI spec. Serves all verticals uniformly. |
| 29.2 | Dashboard | Real-time workspace visualization, trail explorer, quality reports, cross-vertical workflow monitor. Consumes API server. |

Depends on: 27 (all verticals), 26R.

---

## Phase Summary

| Phase | Name | Status | Depends on |
|-------|------|--------|------------|
| 0–19, T1–T5 | Runtime | **Complete** | — |
| 20–24 | Middleware | **Complete** | Runtime |
| 25 | CLI Agent | **Complete** | Middleware |
| 26 | SWE Vertical | **Complete** | CLI |
| 26R | Remediation | **Complete** | 20–26 |
| **27A** | **DevOps Vertical** | **Complete** | 26R |
| **27B** | **MLOps Vertical** | **Complete** | 26R |
| **27C** | **Finance Vertical** | **Complete** | 26R |
| **27D** | **Healthcare Vertical** | **Complete** | 26R |
| **27F** | **Data Analytics Vertical** | **Complete** | 26R |
| **27G** | **Data Science Vertical** | **Complete** | 26R |
| 28 | IDE + Chat Bridge | Pending | 26R |
| **29** | **API Server + Dashboard** | **Pending** | 27 |

---

*WACP implementation plan — Akil Abderrahim and Claude Opus 4.6*
