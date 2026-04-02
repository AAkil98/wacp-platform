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

## Phase 27+ (unchanged, blocked by 26R)

| Phase | Name | Depends on |
|-------|------|------------|
| 27 | API Server + Dashboard | 26R |
| 28 | IDE + Chat Bridge | 26R |
| 29 | Remaining Verticals | 26R |

See `LAYER-MAPPING.md` for full architectural detail.

---

*WACP implementation plan — Akil Abderrahim and Claude Opus 4.6*
