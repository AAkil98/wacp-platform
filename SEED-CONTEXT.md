# WACP — Seed Context

> This file primes a fresh Claude session with the project's current state. Read this first.

---

## What WACP Is

WACP (Workspace Agent Coordination Protocol) is a formal protocol for coordinating autonomous agents. It defines how agents communicate, how work is organized, how progress is recorded, and how everything is audited. The protocol is complete — 20 constituent specs + PROTOCOL.md + TAXONOMY.md. The specification layer is complete — 10 implementation specs covering every protocol domain.

## Current State

**Specification: complete.** 20 protocol specs, 10 implementation specs. Zero unresolved coverage gaps (audit 2026-03-22, gaps resolved 2026-03-24). All three conformance levels (Level 1–3) have implementation guidance.

**Runtime (Phases 0–19 + T1–T5): complete.** 12 Rust crates, 1,192 runtime tests across 3 ecosystems (947 Rust, 181 TypeScript, 64 Python). The runtime binary starts a gRPC server with three services (AgentService, HighwayService, CoordinatorService), manages workspaces, enforces the protocol, and records everything in a hash-chained trail.

**Middleware (Phases 20–24): complete.** 7 frameworks implemented — tool framework, LLM adapters, agent SDK v2, coordinator SDK, local SDK, security, transport extensions. See details below.

**Applications (Phase 25 + 26R): complete.** CLI agent spawns the Rust runtime as a child process, connects via gRPC, and drives multi-stage SWE workflows through the protocol. Every workspace, signal, checkpoint, and trail entry is real.

**Ecosystem (Phase 26): complete.** SWE vertical — 4 roles, 7 task types, 14 tools, 4 agent profiles, 4 workflow DAGs, 6 quality dimensions. Workflows execute through the protocol (SubmitGoal → Decompose → Dispatch → Bind → Signal → Checkpoint per stage).

**Phase 26R (Remediation): complete.** Closed 8 architectural gaps — CoordinatorService server, self-orchestration, protocol-aware CLI, REST gateway wiring (no stubs), WebSocket binding, Python bindings, OAuth authenticator. No shortcuts remain.

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
│   └── wacp-cli/            # CLI agent: REPL, gRPC, workflows — 97 tests
│
├── ecosystem/
│   └── swe/                 # SWE vertical: roles, tools, profiles, workflows — 57 tests
│
└── sdk-python/              # Python SDK: agent, tools, llm, coordinator, local — 104 tests
```

## Architecture Summary

**Runtime (Rust):** Event-driven actor system on `tokio`. Three actor types: coordinator (singleton), workspace (per active workspace), transport (routes messages). No shared mutable state. Three gRPC services: AgentService (port 9400), HighwayService (port 9401), CoordinatorService (port 9402). REST gateway + WebSocket binding on the transport layer.

**CLI Agent (TypeScript):** Spawns `wacp-runtime serve` as child process. Connects via gRPC using `@grpc/grpc-js`. Loads SWE vertical (4 workflows, 4 profiles). Detects task type from goal → selects workflow → drives execution through CoordinatorService (SubmitGoal → Decompose → Dispatch) and AgentService (Bind → Signal → Checkpoint) per stage. LLM calls are raw HTTP (external to protocol); everything else goes through the runtime.

**Middleware:** 7 frameworks. Tool framework (Rust: descriptors, JSON Schema validation, execution engine, circuit breakers, sandboxing). LLM adapters (Rust: Anthropic + OpenAI providers, SSE streaming, microdollar cost tracking, retry with backoff). Agent SDK v2 (Rust: AgentContext wrapping Agent + ToolRegistry). Coordinator SDK (Rust: CoordinatorContext + 15 proto RPCs, client + server). Local SDK (TypeScript: session lifecycle, autonomy manager, WorkflowExecutor, local resources). Security (Rust: content filter with 7 PII rules, secret store, audit events). Transport (Rust: REST gateway with GatewayBackend trait, WebSocket JSON-RPC 2.0, API key + session token + OAuth authenticators).

**SWE Vertical:** 4 roles (planner, implementer, tester, reviewer). 7 task types. 14 tools (7 built-in + 7 SWE-specific). 4 workflow DAGs. 6 quality dimensions. Executes through the protocol — each stage is a real workspace with signals, checkpoints, and trail entries.

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

See `IMPLEMENTATION.md` for the full plan. Phase 26R remediation is complete. Remaining phases:

| Phase | Name | Status |
|-------|------|--------|
| 27 | API Server + Dashboard | Pending |
| 28 | IDE + Chat Bridge | Pending |
| 29 | Remaining Verticals (DevOps, MLOps, Finance, Healthcare) | Pending |

---

*Seed context for WACP — Akil Abderrahim and Claude Opus 4.6*
