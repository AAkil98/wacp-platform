# WACP — Seed Context

> This file primes a fresh Claude session with the project's current state. Read this first.

---

## What WACP Is

WACP (Workspace Agent Coordination Protocol) is a formal protocol for coordinating autonomous agents. It defines how agents communicate, how work is organized, how progress is recorded, and how everything is audited.

**This repository is the reference implementation** (Rust runtime + TypeScript CLI + Python SDK + 7 ecosystem verticals), licensed Apache-2.0. **The protocol specification itself lives in a sibling repository:** [`Madahub-dev/wacp-protocol`](https://github.com/Madahub-dev/wacp-protocol) under CC BY-SA 4.0 — 20 constituent specs + `PROTOCOL.md` + `TAXONOMY.md`. The protocol is complete at v0.1. The implementation-specs layer in this repo (`impl/`) is also complete — 10 implementation specs covering every protocol domain.

## Current State

**Specification: complete.** 20 protocol specs, 10 implementation specs, 6 ecosystem specs (SWE, DevOps, MLOps, Finance, Healthcare, Analytics, Data Science). Zero unresolved coverage gaps (audit 2026-03-22, gaps resolved 2026-03-24). All three conformance levels (Level 1–3) have implementation guidance.

**Runtime (Phases 0–19 + T1–T5): complete.** 12 Rust crates, 1,340 Rust runtime tests + 181 TypeScript + 104 Python. The runtime binary starts a gRPC server with three services (AgentService, HighwayService, CoordinatorService), manages workspaces, enforces the protocol, and records everything in a hash-chained trail.

**Middleware (Phases 20–24): complete.** 7 frameworks implemented — tool framework, LLM adapters, agent SDK v2, coordinator SDK, local SDK, security, transport extensions. See details below.

**Applications (Phase 25 + 26R): complete.** CLI agent spawns the Rust runtime as a child process, connects via gRPC, and drives multi-stage SWE workflows through the protocol. Every workspace, signal, checkpoint, and trail entry is real.

**Ecosystem (Phase 26): complete.** SWE vertical — 4 roles, 7 task types, 14 tools, 4 agent profiles, 4 workflow DAGs, 6 quality dimensions. Workflows execute through the protocol (SubmitGoal → Decompose → Dispatch → Bind → Signal → Checkpoint per stage).

**Phase 26R (Remediation): complete.** Closed 8 architectural gaps — CoordinatorService server, self-orchestration, protocol-aware CLI, REST gateway wiring (no stubs), WebSocket binding, Python bindings, OAuth authenticator. No shortcuts remain.

**Phase 27 (Remaining Verticals): complete.** Phase order swapped — verticals before API server so API design is informed by the full domain spectrum. All 6 verticals complete: DevOps (27A), MLOps (27B), Finance (27C), Healthcare (27D), Data Analytics (27F), Data Science (27G). Each vertical has a distinct enforceable constraint baked into its tool layer: blast radius / env-scaled gating (DevOps), compute budget + reproducibility (MLOps), regulatory compliance pre-check + forbidden-pattern screen (Finance), PHI access grant (consent or de-identification basis) gating clinical tools (Healthcare), SQL safety classification + query reproducibility (Analytics), hypothesis-declaration contract (Data Science). **459 new tests** added across the six verticals.

**Phase 27R (Vertical Wiring Remediation): complete.** Discovered after 27D that the 6 new verticals were well-tested in isolation but architecturally orphaned: the CLI only loaded SWE, `detectTaskType()` only matched SWE keywords, the tool registry didn't include vertical tools, and constraint enforcement was unreachable. 27R closed all 7 wiring gaps: each vertical now exports `detectTaskType` + a `<UPPER>_VERTICAL` descriptor; the CLI's new `ecosystem.ts` loader composes all 7 via `loadEcosystem()`; `routeGoal()` dispatches across all loaded detectors; `buildToolDefinitionsForEcosystem` composes 7 built-in + 68 vertical tools; `executeTool()` dispatches to the owning vertical's executor; constraint enforcement reaches the CLI path end-to-end (Finance `trade_execute` blocked without compliance, Healthcare `clinical_report_generate` blocked without PHI grant, Data Science `hypothesis_test` blocked without declaration — all verified). The SWE inlining in `vertical.ts` is deleted — `@wacp/swe` is now the canonical source. **35 new cross-vertical integration tests** in `packages/wacp-cli/tests/ecosystem.test.ts`.

**Phase 27S (Vertical Surfacing): complete.** Extended each `*_VERTICAL` descriptor with 6 new typed fields (`defining_constraint`, `context_schema`, `tool_policies`, `checkpoint_types`, `quality_criteria`, `task_types`) that surface domain knowledge previously embedded only in executable code. Added `packages/wacp-cli/scripts/generate-manifests.ts` which serializes each vertical to a deterministic `ecosystem/{id}/vertical.yaml`; all 7 manifests are committed. `wacp-taxonomy::VerticalManifest` (Rust) was extended to match. The runtime loads manifests from `taxonomy.verticals_dir` at startup and serves them via `GET /v1/verticals` + `GET /v1/verticals/{id}` through the REST gateway. **9 new tests** across `wacp-transport` + `wacp-taxonomy`. This phase unblocks `wacp-console` — the Console queries the runtime's REST endpoint for vertical manifests instead of filesystem-parsing per ADR-001 in its `SPEC_BUILD.md`.

**Forward strategy + protocol split (2026-04-11): complete.** Three repo-level changes post-27S:

1. **`IMPLEMENTATION.md` rewritten as a forward strategy** (commit `1b8d6e5`) — no longer a phase-by-phase historical record. Organized around Stream A (runtime productionization: port alignment, CI expansion, release pipeline, `wacp-taxonomy` stability, OpenAPI generation, REST surface audit, mock runtime binary) and Stream B (Phase 28 IDE + chat bridge). Phase 29.2 Dashboard is recognized as `wacp-console` sibling repo. Full task inventory in §8. Stale pre-Phase-20 planning docs (`SPEC-STRATEGY.md`, `TEST-STRATEGY.md`) deleted in the same commit.

2. **Protocol specs extracted to sibling repo** (commit `ef20421`) — the `protocol/` subtree (22 files, 20 specs + `PROTOCOL.md` + `TAXONOMY.md`) was extracted via `git filter-repo` into a new repo at [`github.com/Madahub-dev/wacp-protocol`](https://github.com/Madahub-dev/wacp-protocol) **licensed CC BY-SA 4.0**. This repo (`Madahub-dev/wacp`) is now uniformly **Apache-2.0**: root `LICENSE` replaced, `NOTICE` file added, 10 TypeScript `package.json` files given explicit `"license": "Apache-2.0"` (they previously declared nothing), 15 markdown cross-references in `impl/*.md` updated to absolute URLs in the sibling repo. Resolves a three-way license drift that predated this session. Local clone of the sibling repo is at `/home/aakil98/mada/wacp-protocol/`.

3. **Console Q1–Q6 folded into `IMPLEMENTATION.md`** (commit `199dee0`) — `wacp-console/TECH_STACK_PROPOSAL.md` §10 Q1–Q7 are all answered. Runtime-side impacts integrated: §3.2 notes Q2/Q3 compatibility (existing auth + TLS are sufficient), §4.2 commits Stream A to `cargo-dist` with 5 channels and Tier 1/Tier 2 matrix matching Console §5.2, §5 flags Q2's 7-Console-side-spec-revision blocker, new §5.1 table records all 7 Q decisions with runtime-side impact for future-session lookup.

**Stream A (runtime productionization, 2026-04-11 → 2026-04-12): complete.** 9 tasks (A1–A9) landed on `dev` in commits `7ed2db0`–`3c24743`. All 8 Console-blocking gaps (G1–G8 in `IMPLEMENTATION.md` §3.3) are resolved: canonical port map, CI matrix expansion, release pipeline, crates.io metadata for `wacp-types` + `wacp-taxonomy`, utoipa OpenAPI annotations + `gen_openapi` binary + CI drift check, `GET /v1/sessions/{id}/workspaces`, `subscribe_session_trail` on `/v1/ws`, and `wacp-mock-runtime` binary with fixture loader.

**Codebase health audit + CI cleanup (2026-04-12 → 2026-04-13): complete.** `AUDIT-2026-04-12.md` catalogued pre-existing CI debt. Resolved in 5 commits (`6bd0f9c`–`bd4f821`): `cargo fmt --all` (107 files), early-return fix for a cancellation race in `wacp-tools`, and 24 clippy warnings across 8 crates. The `dev` branch is **CI-clean**: `cargo fmt --check --all`, `cargo clippy --workspace -- -D warnings`, and `cargo test --workspace` (1,340 tests) all pass with zero failures.

## Repository Map

```
wacp/
├── IMPLEMENTATION.md        # Forward strategy — runtime productionization + Phase 28/29
├── LAYER-MAPPING.md         # Historical map: mada-os layers → WACP (referenced by impl specs)
├── SEED-CONTEXT.md          # This file
├── LICENSE                  # Apache-2.0 (specification is separately licensed CC BY-SA 4.0 in wacp-protocol)
├── NOTICE                   # Attribution + pointer to wacp-protocol
├── Cargo.toml               # Workspace manifest — 16 crates
│
# Protocol specification lives in the sibling repository:
#   github.com/Madahub-dev/wacp-protocol (CC BY-SA 4.0)
#   — PROTOCOL.md, TAXONOMY.md, primitives/, foundations/, mechanisms/, topology/
# This repo implements those specs; it does not define them.
│
├── impl/                    # Implementation specs (17 total — Rust/Tonic/SQLite-specific)
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
├── crates/                  # Rust implementation (16 crates)
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

**Runtime (Rust):** Event-driven actor system on `tokio`. Three actor types: coordinator (singleton), workspace (per active workspace), transport (routes messages). No shared mutable state. Canonical port map per `crates/wacp-runtime/src/config.rs`: AgentService `[::1]:9090`, HighwayService `[::1]:9091`, CoordinatorService `[::1]:9092`, REST gateway + WebSocket `[::1]:9093`, health `[::1]:9094`, metrics `[::1]:9095`. All six ports are contiguous and non-overlapping. `Dockerfile` and `deploy/wacp-runtime.service` bind on `0.0.0.0` with the same port numbers.

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

See `IMPLEMENTATION.md` — now the forward strategy, not a phase-by-phase plan. Organized around two parallel streams. Protocol spec + runtime + middleware + CLI + all 7 verticals are complete. No code blockers on either stream.

| Phase | Name | Status |
|-------|------|--------|
| 27A–G | All 6 domain verticals (DevOps, MLOps, Finance, Healthcare, Analytics, Data Science) | **Complete** |
| 27R | Vertical wiring (multi-vertical ecosystem loader, cross-vertical router, tool dispatch) | **Complete** |
| 27S | Vertical surfacing + `GET /v1/verticals[/{id}]` REST endpoint | **Complete** |
| — | Forward strategy rewrite + protocol split + Apache-2.0 relicense | **Complete** (`1b8d6e5`, `ef20421`, `199dee0`) |
| **Stream A** | Runtime productionization — ports, CI, release pipeline, OpenAPI, mock binary | **Complete** (A1–A9, `7ed2db0`–`3c24743`) |
| — | Codebase health audit + CI cleanup | **Complete** (`AUDIT-2026-04-12.md`, `6bd0f9c`–`bd4f821`) |
| **Stream B** | Phase 28 — IDE + chat bridge (parallel, no hard dependency) | **Pending — can start any time** |
| 29.2 | Dashboard (≡ `wacp-console` sibling repo, separate project) | **Pending** — blocked on Console-side Q2 spec revisions; upstream gaps resolved |

Stream A task list is in `IMPLEMENTATION.md` §8.1 (A1–A9, all done). Stream B task list is in §8.2 (B1–B6).

**Resumption notes for the next session:**

*Architecture (unchanged from Phase 27R/27S):*
- All 7 verticals are wired into the CLI through `packages/wacp-cli/src/ecosystem.ts`. `loadEcosystem()` returns a `LoadedEcosystem` with workflows, profiles, tool definitions, executors, and detectors from every vertical. `routeGoal()` dispatches across them. `executeTool()` routes tool calls to the owning vertical's executor. Constraint enforcement (`compliance_check`, `phi_access_grant`, hypothesis declaration, SQL safety, env tier, compute budget) is reachable end-to-end from the CLI path.
- Per-vertical detectors live in `ecosystem/<id>/src/detect.ts`. They return `null` for non-matches (so the router can try the next vertical), except for SWE which always returns at least the catchall `swe:implement-feature`.
- Each `*_VERTICAL` descriptor has 6 typed fields from 27S: `defining_constraint`, `context_schema`, `tool_policies`, `checkpoint_types`, `quality_criteria`, `task_types`. Manifest generator (`scripts/generate-manifests.ts`) reads these and writes `ecosystem/{id}/vertical.yaml`. The runtime loads them via `taxonomy.verticals_dir`.
- Adding an 8th vertical: standard package layout, export `<UPPER>_VERTICAL` from `index.ts`, add to `REGISTRY` + `DEFAULT_LOAD_ORDER` in `packages/wacp-cli/src/ecosystem.ts`, add dep in `packages/wacp-cli/package.json`.

*Repo state (as of end of session 2026-04-13):*
- `dev` is at `bd4f821` — 20 commits ahead of the Phase 27S baseline. Stream A (A1–A9) + audit cleanup all landed. The branch is CI-clean.
- `origin/main` on GitHub is still ancient at `7f6a330` (pre-Phase-20). **Deliberately not updated.** If future work should be on `main`, either `git checkout main && git merge dev && git push` or continue on `dev`.
- Sibling repo **`github.com/Madahub-dev/wacp-protocol`** (public, CC BY-SA 4.0) is live with `main` branch. Contains `PROTOCOL.md`, `TAXONOMY.md`, and 20 protocol specs in `primitives/`, `foundations/`, `mechanisms/`, `topology/`. Local clone at `/home/aakil98/mada/wacp-protocol/`. Cross-references in this repo's `impl/*.md` footers point at its GitHub URLs.
- Sibling project **`wacp-console`** at `/home/aakil98/mada/wacp-console/` is **uninitialized** (zero commits, no remote). Contents: `SPEC_BUILD.md` (spec map + ADR-001), `TECH_STACK_PROPOSAL.md` (Q1–Q7 all answered), `STATUS.md` (upstream state of affairs as of 2026-04-13), 11 draft specs in `specs/`. Not committed — the user will seed the first commit when ready.

*Where to start work:*
- **Stream A — complete.** All 9 tasks (A1–A9) done. CI-clean. No remaining runtime productionization work.
- **Stream B — ready to start.** Phase 28 (IDE + chat bridge) can begin. Depends only on a reachable runtime, which works locally via `cargo run --bin wacp-runtime` today. `IMPLEMENTATION.md` §8.2 has the task list (B1–B6). Recommended start: B4 (chat bridge scaffolding) — smaller scope, validates the runtime-as-service model.
- **`wacp-console` — blocked on spec revisions.** The 7 Console-side spec revisions driven by Q2 (multi-user auth in Phase 1) are the critical path: new `wcon-auth` spec + updates to `wcon-architecture` §8, `wcon-data-model` §5, `wcon-api`, `wcon-profiles`, `wcon-sessions`, `wcon-ui`. See `IMPLEMENTATION.md` §5 step 1 and `wacp-console/STATUS.md` for the full breakdown. All upstream gaps are resolved — this is purely Console-side spec maturation. First step: `/glossary` to establish canonical Console terminology.
- **`dev` → `main` merge** is available whenever the user decides. The branches have diverged significantly (20+ commits). A PR or direct merge-and-push would bring `main` current.

*Other known state (not blocking but worth knowing):*
- `LAYER-MAPPING.md` is a historical planning doc from early Phase 20 but is still referenced by 11 `impl/*.md` specs via `lineage:` frontmatter and References tables — left in place to avoid breaking cross-refs.
- 35 Rust source files have `(PROTOCOL.md §X.Y)` prose citations in doc comments. Those were NOT updated in the repo split — they're English prose references, not markdown links, and editing 35 files for cosmetic alignment would be churn. Readers mentally map them to the `wacp-protocol` sibling repo; the `NOTICE` file and README make that pointer explicit.
- `serde_yaml` 0.9 is in maintenance mode (dtolnay archived the repo). No CVEs, but worth migrating to `serde_yml` when convenient. See `AUDIT-2026-04-12.md` §6.

---

*Seed context for WACP — Akil Abderrahim and Claude Opus 4.6*
