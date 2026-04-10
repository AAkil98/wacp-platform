# WACP — Seed Context

> This file primes a fresh Claude session with the project's current state. Read this first.

---

## What WACP Is

WACP (Workspace Agent Coordination Protocol) is a formal protocol for coordinating autonomous agents. It defines how agents communicate, how work is organized, how progress is recorded, and how everything is audited. The protocol is complete — 20 constituent specs + PROTOCOL.md + TAXONOMY.md. The specification layer is complete — 10 implementation specs covering every protocol domain.

## Current State

**Specification: complete.** 20 protocol specs, 10 implementation specs, 6 ecosystem specs (SWE, DevOps, MLOps, Finance, Healthcare, Analytics, Data Science). Zero unresolved coverage gaps (audit 2026-03-22, gaps resolved 2026-03-24). All three conformance levels (Level 1–3) have implementation guidance.

**Runtime (Phases 0–19 + T1–T5): complete.** 12 Rust crates, 1,192 runtime tests across 3 ecosystems (947 Rust, 181 TypeScript, 64 Python). The runtime binary starts a gRPC server with three services (AgentService, HighwayService, CoordinatorService), manages workspaces, enforces the protocol, and records everything in a hash-chained trail.

**Middleware (Phases 20–24): complete.** 7 frameworks implemented — tool framework, LLM adapters, agent SDK v2, coordinator SDK, local SDK, security, transport extensions. See details below.

**Applications (Phase 25 + 26R): complete.** CLI agent spawns the Rust runtime as a child process, connects via gRPC, and drives multi-stage SWE workflows through the protocol. Every workspace, signal, checkpoint, and trail entry is real.

**Ecosystem (Phase 26): complete.** SWE vertical — 4 roles, 7 task types, 14 tools, 4 agent profiles, 4 workflow DAGs, 6 quality dimensions. Workflows execute through the protocol (SubmitGoal → Decompose → Dispatch → Bind → Signal → Checkpoint per stage).

**Phase 26R (Remediation): complete.** Closed 8 architectural gaps — CoordinatorService server, self-orchestration, protocol-aware CLI, REST gateway wiring (no stubs), WebSocket binding, Python bindings, OAuth authenticator. No shortcuts remain.

**Phase 27 (Remaining Verticals): complete.** Phase order swapped — verticals before API server so API design is informed by the full domain spectrum. All 6 verticals complete: DevOps (27A), MLOps (27B), Finance (27C), Healthcare (27D), Data Analytics (27F), Data Science (27G). Each vertical has a distinct enforceable constraint baked into its tool layer: blast radius / env-scaled gating (DevOps), compute budget + reproducibility (MLOps), regulatory compliance pre-check + forbidden-pattern screen (Finance), PHI access grant (consent or de-identification basis) gating clinical tools (Healthcare), SQL safety classification + query reproducibility (Analytics), hypothesis-declaration contract (Data Science). **459 new tests** added across the six verticals.

**Phase 27R (Vertical Wiring Remediation): complete.** Discovered after 27D that the 6 new verticals were well-tested in isolation but architecturally orphaned: the CLI only loaded SWE, `detectTaskType()` only matched SWE keywords, the tool registry didn't include vertical tools, and constraint enforcement was unreachable. 27R closed all 7 wiring gaps: each vertical now exports `detectTaskType` + a `<UPPER>_VERTICAL` descriptor; the CLI's new `ecosystem.ts` loader composes all 7 via `loadEcosystem()`; `routeGoal()` dispatches across all loaded detectors; `buildToolDefinitionsForEcosystem` composes 7 built-in + 68 vertical tools; `executeTool()` dispatches to the owning vertical's executor; constraint enforcement reaches the CLI path end-to-end (Finance `trade_execute` blocked without compliance, Healthcare `clinical_report_generate` blocked without PHI grant, Data Science `hypothesis_test` blocked without declaration — all verified). The SWE inlining in `vertical.ts` is deleted — `@wacp/swe` is now the canonical source. **35 new cross-vertical integration tests** in `packages/wacp-cli/tests/ecosystem.test.ts`.

## Repository Map

```
wacp/
├── IMPLEMENTATION.md        # Forward plan — Phases 20–29 + 26R remediation
├── LAYER-MAPPING.md         # Architectural mapping: mada-os layers → WACP equivalents
├── TEST-STRATEGY.md         # Comprehensive test strategy
├── SEED-CONTEXT.md          # This file
├── Cargo.toml               # Workspace manifest — 15 crates
│
├── protocol/                # The specification layer (20 specs)
│   ├── PROTOCOL.md          # Authoritative protocol spec (976 lines)
│   ├── TAXONOMY.md          # Extension registry (385 lines)
│   ├── primitives/          # 8 specs
│   ├── foundations/          # 2 specs
│   ├── mechanisms/          # 4 specs
│   └── topology/            # 6 specs
│
├── impl/                    # Implementation specs (17 total)
│   ├── runtime.md           # State machine, permissions, trail, clock
│   ├── storage.md           # Trail backend, checkpoints, snapshots
│   ├── protocol-interface.md # Protobuf, gRPC, transport trait
│   ├── sdk-agent.md         # Python + Rust SDK surface
│   ├── highway-ui.md        # TypeScript SPA, gRPC-Web
│   ├── deployment.md        # Config, CLI, TLS, logging, metrics
│   ├── migration.md         # Coordinator procedure, snapshot, rollback
│   ├── topology.md          # Workspace tree, task graph, visibility
│   ├── task-scheduling.md   # Task lifecycle, gates, dispatch
│   ├── integration.md       # Merge strategies, conflict resolution
│   ├── tool-framework.md    # Descriptors, execution, sandboxing, resilience
│   ├── llm-adapters.md      # Adapter trait, providers, streaming, cost
│   ├── agent-sdk-v2.md      # AgentContext wrapping Agent + ToolRegistry
│   ├── coordinator-sdk.md   # CoordinatorContext + 15 RPCs
│   ├── security.md          # Content filter, secret store, audit events
│   ├── transport-ext.md     # REST gateway, WebSocket, auth providers
│   ├── local-sdk.md         # Session, autonomy, orchestrator
│   └── cli-agent.md         # CLI spawns runtime, drives gRPC, workflows
│
├── proto/                   # 5 protobuf definitions
│   ├── primitives.proto     # Enums, core messages
│   ├── agent.proto          # AgentService — 8 RPCs
│   ├── highway.proto        # HighwayService — 12 RPCs
│   ├── coordinator.proto    # CoordinatorService — 15 RPCs
│   └── taxonomy.proto       # Taxonomy configuration
│
├── crates/                  # Rust implementation (15 crates)
│   ├── wacp-types/          # Protocol enums, newtypes, structs — 45 tests
│   ├── wacp-clock/          # HLC timestamps — 33 tests
│   ├── wacp-fsm/            # Workspace/envelope/task FSMs — 55 tests
│   ├── wacp-taxonomy/       # YAML/JSON loader, validation — 42 tests
│   ├── wacp-permissions/    # Permission matrix, port rights — 45 tests
│   ├── wacp-trail/          # Storage, hash chain, snapshots, tiered — 90 tests
│   ├── wacp-workspace/      # Workspace actor, 9 components — 60 tests
│   ├── wacp-coordinator/    # Decision engine, migration — 282 tests
│   ├── wacp-transport/      # gRPC (3 services), REST gateway, WebSocket, 4 auth providers — 125 tests
│   ├── wacp-recovery/       # Trail replay, snapshot recovery — 25 tests
│   ├── wacp-runtime/        # Binary: config, CLI, TLS, metrics, health — 85 tests
│   ├── wacp-sdk/            # Rust agent SDK: Agent, AgentContext — 58 tests
│   ├── wacp-coordinator-sdk/# Coordinator client SDK — 11 tests
│   ├── wacp-tools/          # Tool framework: registry, execution, resilience — 124 tests
│   ├── wacp-llm/            # LLM adapters: Anthropic, OpenAI, streaming — 134 tests
│   └── wacp-security/       # Content filter, secrets, audit — 45 tests
│
├── tests/                   # Cross-crate integration + E2E tests (65 tests)
│
├── highway-ui/              # Highway UI — TypeScript SPA (181 tests)
│
├── packages/                # TypeScript packages
│   ├── wacp-local/          # Local SDK: session, autonomy, orchestrator — 86 tests
│   └── wacp-cli/            # CLI agent: REPL, gRPC, ecosystem loader, multi-vertical router — 132 tests
│
├── ecosystem/
│   ├── swe/                 # SWE vertical — 57 tests
│   ├── devops/              # DevOps vertical: blast radius / env gating — 73 tests
│   ├── mlops/               # MLOps vertical: compute budget / reproducibility — 67 tests
│   ├── finance/             # Finance vertical: regulatory compliance / forbidden-pattern screen — 83 tests
│   ├── healthcare/          # Healthcare vertical: PHI access grant / HIPAA Safe Harbor — 90 tests
│   ├── analytics/           # Data Analytics vertical: SQL safety / query reproducibility — 73 tests
│   └── datasci/             # Data Science vertical: hypothesis declaration / statistical rigor — 73 tests
│
└── sdk-python/              # Python SDK: agent, tools, llm, coordinator, local — 104 tests
```

## Architecture Summary

**Runtime (Rust):** Event-driven actor system on `tokio`. Three actor types: coordinator (singleton), workspace (per active workspace), transport (routes messages). No shared mutable state. Three gRPC services: AgentService (port 9400), HighwayService (port 9401), CoordinatorService (port 9402). REST gateway + WebSocket binding on the transport layer.

**CLI Agent (TypeScript):** Spawns `wacp-runtime serve` as child process. Connects via gRPC using `@grpc/grpc-js`. Loads the **full ecosystem** at boot via `loadEcosystem()` — all 7 verticals (SWE + 6 domain) with their workflows, profiles, tool definitions, executors, and detectors. When a goal arrives, `routeGoal(goal, ecosystem)` tries each vertical's `detectTaskType` in load order (domain verticals before SWE catchall), selects a workflow, and drives execution through CoordinatorService (SubmitGoal → Decompose → Dispatch) and AgentService (Bind → Signal → Checkpoint) per stage. Tool execution dispatches via `ecosystem.toolByName` to the owning vertical's `executeTool` — so `compliance_check`/`trade_execute`/`clinical_report_generate`/`hypothesis_test` and the other 64 vertical tools all run their constraint enforcement on the CLI path. LLM calls are raw HTTP (external to protocol); everything else goes through the runtime.

**Middleware:** 7 frameworks. Tool framework (Rust: descriptors, JSON Schema validation, execution engine, circuit breakers, sandboxing). LLM adapters (Rust: Anthropic + OpenAI providers, SSE streaming, microdollar cost tracking, retry with backoff). Agent SDK v2 (Rust: AgentContext wrapping Agent + ToolRegistry). Coordinator SDK (Rust: CoordinatorContext + 15 proto RPCs, client + server). Local SDK (TypeScript: session lifecycle, autonomy manager, WorkflowExecutor, local resources). Security (Rust: content filter with 7 PII rules, secret store, audit events). Transport (Rust: REST gateway with GatewayBackend trait, WebSocket JSON-RPC 2.0, API key + session token + OAuth authenticators).

**SWE Vertical:** 4 roles (planner, implementer, tester, reviewer). 7 task types. 14 tools (7 built-in + 7 SWE-specific). 4 workflow DAGs. 6 quality dimensions. Executes through the protocol — each stage is a real workspace with signals, checkpoints, and trail entries.

**Additional Verticals (6):** Each follows the SWE template (taxonomy → tools → profiles → workflows → quality) but carries its own hard constraint enforced at the tool layer:

| Vertical | Roles | Task types | Tools | Workflows | Key constraint |
|---|---|---|---|---|---|
| DevOps (27A) | 5 | 9 | 10 | 5 | Environment-scaled gating — production mutations require human approval; `deploy_execute`/`rollback`/`secret_rotate` are env-aware |
| MLOps (27B) | 5 | 9 | 10 | 4 | Compute-budget gating + reproducibility checkpoints (data hash, code version, random seed, hyperparameters) |
| Finance (27C) | 5 | 9 | 10 | 4 | `trade_execute` refuses without an approved `compliance_check` checkpoint (fresh + matching trade_id); `classifyForbiddenPattern()` hard-blocks insider/wash/spoofing/layering/front-running/churning/painting-the-tape |
| Healthcare (27D) | 5 | 8 | 10 | 4 | `clinical_report_generate`/`lab_interpret`/`risk_score` refuse without a valid `phi_access_grant` (consent or de-identification basis); 18 HIPAA Safe Harbor identifiers screened by `phi_filter`; patient-assessment workflow fully gated for clinician sign-off |
| Data Analytics (27F) | 5 | 8 | 10 | 4 | `classifySql()` hard-blocks DROP/TRUNCATE/unscoped UPDATE/DELETE; every report must cite source queries |
| Data Science (27G) | 5 | 9 | 10 | 4 | `hypothesis_test` refuses execution without prior declaration checkpoint; CIs required on all point estimates |

All 6 verticals share the same package structure as SWE and depend only on `@wacp/local`. Tests: 73 + 67 + 83 + 90 + 73 + 73 = 459 added.

## Protocol Constants (must be exact)

- **11 signal types:** ready, started, blocked, checkpoint, complete, failed, integrate, acknowledged, escalation, suspend, migrate
- **9 workspace states:** idle, active, blocked, suspended, migrating, integrating, conflicted, closed, failed
- **2 terminal states:** closed, failed
- **3 base roles:** coordinator, worker, observer
- **3 base envelope types:** directive, feedback, query
- **5 envelope states:** created, validated, delivered, acknowledged, rejected
- **3 envelope priorities:** normal, urgent, blocking
- **8 task statuses:** draft, pending, assigned, in_progress, completed, failed, integrated, cancelled
- **2 checkpoint statuses:** provisional, final
- **3 confidence levels:** high, medium, low
- **2 base checkpoint types:** artifact, observation
- **3 merge strategies:** direct, layered, evaluated
- **4 conflict types:** content_overlap, semantic_contradiction, dependency_violation, constraint_breach
- **3 resolution strategies:** coordinator_resolve, escalate, agent_rework
- **6 gate types:** task_approval, workspace_create, envelope_delivery, integration, conflict_resolution, workspace_abort
- **3 port right types:** send, receive, send_once
- **Redelivery attempts:** 3 (4 total)

## User Preferences

- Correctness over velocity. Always.
- No stubs, no deferring, no cutting corners. Full implementations only.
- If the protocol defines it, the implementation must exercise it.
- Concise communication. No trailing summaries.
- Tidy tables for tracking progress.
- Incremental, atomic deliverables.
- Spec first, then code. No code without an approved spec.

## What's Next

See `IMPLEMENTATION.md` for the full plan. Phase 27 (verticals) and Phase 27R (wiring) are both **complete**. Next up: Phase 28 (IDE + Chat Bridge) and Phase 29 (API Server + Dashboard).

| Phase | Name | Status |
|-------|------|--------|
| 27A | DevOps Vertical | **Complete** (`c529f3e`) |
| 27B | MLOps Vertical | **Complete** (`4c40f7b`) |
| 27C | Finance Vertical | **Complete** |
| 27D | Healthcare Vertical | **Complete** |
| 27F | Data Analytics Vertical | **Complete** (`0ac589d`) |
| 27G | Data Science Vertical | **Complete** (`03c922a`) |
| **27R** | **Vertical Wiring Remediation** | **Complete** |
| 28 | IDE + Chat Bridge | Pending — **resume here** |
| 29 | API Server + Dashboard | Pending |

**Resumption notes for the next session:**
- All 7 verticals are now wired into the CLI through `packages/wacp-cli/src/ecosystem.ts`. `loadEcosystem()` returns a `LoadedEcosystem` with workflows, profiles, tool definitions, executors, and detectors from every vertical. `routeGoal()` dispatches across them. `executeTool()` routes tool calls to the owning vertical's executor. Constraint enforcement (compliance_check, phi_access_grant, hypothesis declaration, SQL safety, env tier, compute budget) is reachable end-to-end from the CLI path.
- The CLI's `vertical.ts` is now a thin backward-compat wrapper around `loadEcosystem(["swe"])`. The pre-27R inlined SWE definitions and "circular dependency" comment are gone — `@wacp/swe` is the canonical source.
- Per-vertical detectors live in `ecosystem/<id>/src/detect.ts`. They return `null` for non-matches (so the router can try the next vertical), except for SWE which always returns at least the catchall `swe:implement-feature`.
- Adding a new vertical (8th, etc.) requires: the standard package layout, exporting `<UPPER>_VERTICAL` from `index.ts`, and adding it to the `REGISTRY` map + `DEFAULT_LOAD_ORDER` array in `packages/wacp-cli/src/ecosystem.ts` plus `packages/wacp-cli/package.json` deps.
- Phase 28 (IDE + Chat Bridge) — see `IMPLEMENTATION.md`. Likely focus: VS Code / JetBrains extension that connects to a running runtime via the existing Highway gRPC service, plus a chat bridge (Slack/Discord/Teams) that maps incoming messages to `SubmitGoal` and streams signals back. Both reuse existing transport — no new protocol surface needed.
- Phase 29 (API Server + Dashboard) — REST + WebSocket gateway is already wired in `wacp-transport`; this phase is the dashboard frontend (web UI for trail browsing, workspace tree, signal stream) plus a stable public API surface contract.

---

*Seed context for WACP — Akil Abderrahim and Claude Opus 4.6*
