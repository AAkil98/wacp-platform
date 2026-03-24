# WACP — Seed Context

> This file primes a fresh Claude session with the project's current state. Read this first.

---

## What WACP Is

WACP (Workspace Agent Coordination Protocol) is a formal protocol for coordinating autonomous agents. It defines how agents communicate, how work is organized, how progress is recorded, and how everything is audited. The protocol is complete — 20 constituent specs + PROTOCOL.md + TAXONOMY.md. The specification layer is complete — 10 implementation specs covering every protocol domain. The project is now entering the coding phase.

## Current State

**Specification: complete.** 20 protocol specs, 10 implementation specs. Zero unresolved coverage gaps (audit 2026-03-22, gaps resolved 2026-03-24). All three conformance levels (Level 1–3) have implementation guidance.

**Initial implementation: complete.** 28 coding tasks in `SPEC-STRATEGY.md` are done. 12 Rust crates compile, 225 Rust tests pass, 14 Python tests pass. The runtime binary starts a gRPC server, accepts agent and highway connections, and runs the coordinator event loop.

**Phase: coding against full spec coverage.** The initial implementation covers the core runtime. The next work is coding against the Phase 2–3 implementation specs — deployment, migration, topology operations, task scheduling, integration, and the highway UI.

## Repository Map

```
wacp/
├── IMPLEMENTATION.md        # Decision log, spec tracking, audit findings
├── SPEC-STRATEGY.md         # Phased coding plan — 28 tasks (all complete)
├── SEED-CONTEXT.md          # This file
├── Cargo.toml               # Workspace manifest — 12 crates
├── .github/workflows/ci.yml # GitHub Actions: build, clippy, test, fmt, proto check
├── .gitignore
│
├── protocol/                # The specification layer (20 specs)
│   ├── PROTOCOL.md          # Authoritative protocol spec (976 lines)
│   ├── TAXONOMY.md          # Extension registry (385 lines)
│   ├── primitives/          # 8 specs: workspace, envelope, signal, checkpoint, task, trail, identity, user
│   ├── foundations/          # 2 specs: clock, roles
│   ├── mechanisms/          # 4 specs: integration, recovery, security, human-highway
│   └── topology/            # 6 specs: tree, graph, visibility, ownership, causation, channels
│
├── impl/                    # Implementation specs (10 specs, all complete)
│   ├── runtime.md           # Phase 1 — state machine, permissions, trail, clock, recovery, concurrency
│   ├── storage.md           # Phase 1 — trail backend, checkpoints, snapshots, tiered storage
│   ├── protocol-interface.md # Phase 1 — protobuf, gRPC services, transport trait, authentication
│   ├── sdk-agent.md         # Phase 1 — Python + Rust SDK surface, LLM agent mapping
│   ├── highway-ui.md        # Phase 2 — TypeScript SPA, gRPC-Web, gates, escalations
│   ├── deployment.md        # Phase 2 — config (YAML), CLI, TLS, auth, logging, metrics, Docker, systemd
│   ├── migration.md         # Phase 2 — coordinator procedure, snapshot, unbind/bind, rollback
│   ├── topology.md          # Phase 3 — workspace tree, task graph, visibility, ownership, causation, port rights
│   ├── task-scheduling.md   # Phase 3 — task lifecycle, gates, dispatch, resource allocation, retry
│   └── integration.md       # Phase 3 — merge strategies, conflict detection/resolution, salvage
│
├── proto/                   # 4 protobuf definitions (shared contract)
│   ├── primitives.proto     # Enums, core messages (Envelope, Signal, Checkpoint, Task, TrailEntry)
│   ├── agent.proto          # AgentService — 8 RPCs (6 unary, 2 streaming)
│   ├── highway.proto        # HighwayService — 12 RPCs (8 unary, 4 streaming)
│   └── taxonomy.proto       # Taxonomy configuration messages
│
├── specs/coding/            # 28 coding specs (one per task, all status: complete)
│
├── crates/                  # Rust implementation (12 crates, 225 tests)
│   ├── wacp-types/          # Protocol enums (19), identifier newtypes (8), structs (12) — 11 tests
│   ├── wacp-clock/          # HLC: Timestamp, Clock<TimeSource>, ManualTimeSource — 14 tests
│   ├── wacp-fsm/            # StateMachine trait + workspace/envelope/task FSMs — 41 tests
│   ├── wacp-taxonomy/       # YAML/JSON loader, 11 validation checks, role resolution — 22 tests
│   ├── wacp-permissions/    # Permission matrix, checkpoint table, port rights, default-deny — 20 tests
│   ├── wacp-trail/          # Storage traits, in-memory + filesystem backends, hash chain, SQLite index — 48 tests
│   ├── wacp-workspace/      # Workspace actor: 9 components, biased select loop, envelope/checkpoint — 17 tests
│   ├── wacp-coordinator/    # Workspace tree, task DAG, orchestration, integration engine — 28 tests
│   ├── wacp-transport/      # Transport trait, InProcessTransport, gRPC (tonic codegen + services) — 8 tests
│   ├── wacp-recovery/       # Trail integrity check, state reconstruction, clock recovery — 6 tests
│   ├── wacp-runtime/        # Binary: init sequence, gRPC server, event loop, shutdown — 7 tests
│   └── wacp-sdk/            # Rust agent SDK: Agent, builders, streams — 3 tests
│
└── sdk-python/              # Python agent SDK (14 tests)
    ├── pyproject.toml
    ├── src/wacp/
    │   ├── __init__.py      # Package: Agent, Signal, CheckpointStatus, Confidence, Priority
    │   ├── agent.py         # Agent class: connect, signal, checkpoint, send_envelope, inbox, commands
    │   ├── types.py         # Protocol constants with proto enum mapping
    │   └── proto/v1.py      # betterproto-generated types from .proto files
    └── tests/
```

## Implementation Spec Index

| # | Spec | Sections | Key content |
|---|------|----------|-------------|
| 1 | `runtime.md` | 17 | FSM engine, permission engine, trail write-ahead, HLC clock, workspace isolation, recovery, concurrency model, crate structure |
| 2 | `storage.md` | 12 | Trail backend (custom append-only log), checkpoint content-addressing, snapshots, tiered storage, retention, durability guarantees |
| 3 | `protocol-interface.md` | 11 | Protobuf types, agent/highway gRPC services, serialization rules, authenticator trait, transport trait |
| 4 | `sdk-agent.md` | 11 | Python + Rust SDK surface, connection lifecycle, LLM agent mapping, tool mounting, testing |
| 5 | `highway-ui.md` | 17 | TypeScript SPA, gRPC-Web, trail streaming, gate management, escalation, injection, autonomy presets |
| 6 | `deployment.md` | 13 | YAML config (47 fields), CLI (`serve`/`validate`/`defaults`), TLS, auth providers (PSK/external), logging, metrics (30 gauges/counters), health, Docker, systemd |
| 7 | `migration.md` | 12 | 7-step coordinator procedure, workspace snapshot (5 live components), unbind/bind, atomic rollback, resource meter continuity |
| 8 | `topology.md` | 10 | 6 topologies (tree, task graph, visibility, ownership, causation, port rights), compound operations, trail-driven recovery |
| 9 | `task-scheduling.md` | 11 | Task lifecycle (8 states, 10 triggers), gate enforcement, dispatch policy, resource allocation, retry/cancellation, progressive decomposition |
| 10 | `integration.md` | 11 | 3 merge strategies, 4 conflict types, 3 resolution strategies, salvage (3 guardrails), integration ordering, 22 invariants |

## Key Decisions (from IMPLEMENTATION.md)

| Decision | Choice | Reasoning |
|----------|--------|-----------|
| Language strategy | Multi-language | Rust runtime, Python agent SDK, TypeScript highway UI |
| Runtime language | Rust | Structural immutability, exhaustive matching, compiler-enforced correctness |
| Correctness vs velocity | Correctness first | Closed sets are compile-time types, not runtime strings |
| Inter-process communication | gRPC over IPC, no FFI | Process isolation reinforces workspace isolation |
| Interface definition | Protobuf | Code generation in all 3 languages from single `.proto` files |
| Transport | gRPC | Bidirectional streaming, mature libraries, abstracted behind a trait |

## Architecture Summary

**Runtime (Rust):** Event-driven actor system on `tokio`. Three actor types: coordinator (singleton, owns workspace tree + task graph + all 6 topology structures), workspace (one per active workspace, owns nine components), transport (routes messages). No shared mutable state. Workspace actors use biased two-channel select (coordinator commands take priority over agent messages).

**State machines:** Generic `StateMachine` trait instantiated three times: workspace lifecycle (9 states), envelope lifecycle (5 states), task lifecycle (8 statuses). Exhaustive `match` — illegal transitions don't compile.

**Trail:** Custom append-only segment log with per-entry fsync. SHA-256 hash chain for tamper evidence. SQLite index for queries. Content-addressable checkpoint store with integrity verification on read.

**Clock:** Hybrid Logical Clock (HLC). 80-bit: 64-bit physical (microseconds since epoch) + 16-bit logical counter. 10-byte big-endian encoding for lexicographic storage ordering.

**Transport:** Trait-based abstraction. `InProcessTransport` (testing) and `GrpcTransport` (production, via tonic). Agent service on port 9090, highway service on port 9091. Proto codegen via `tonic-build` in `build.rs`.

**Topology:** 6 independent structures over the same node set — workspace tree (parent pointers + 3 indices), task graph (dual adjacency lists + readiness counters), visibility graph (forward/reverse `HashSet`), ownership domains (partition by `owner`), causal forest (partition by `originator`), port rights graph (3 indices by holder/target/id). All owned by coordinator actor, all recoverable from trail.

**Crate structure:** 12 crates in a Cargo workspace. Dependency order: `wacp-types` (leaf) → `wacp-clock`, `wacp-fsm`, `wacp-taxonomy` → `wacp-trail`, `wacp-permissions` → `wacp-workspace` → `wacp-coordinator` → `wacp-transport`, `wacp-recovery` → `wacp-runtime` (binary), `wacp-sdk` (agent SDK).

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
- Concise communication. No trailing summaries.
- Tidy tables for tracking progress.
- Incremental, atomic deliverables.
- Spec first, then code. No code without an approved spec.

## What's Next

All specs are complete. The next phase is coding against the full spec coverage. Priority work items (all fully specced):

| Work item | Spec source | Crate(s) affected |
|-----------|-------------|-------------------|
| Full gRPC request routing | protocol-interface.md §4–5, §8–9 | wacp-transport, wacp-coordinator |
| Topology operations (tree, graph, visibility, port rights) | topology.md §2–7 | wacp-coordinator |
| Task scheduling (dispatch, gates, retry) | task-scheduling.md §2–8 | wacp-coordinator |
| Integration engine (merge, conflict, salvage) | integration.md §2–7 | wacp-coordinator |
| Timeout/budget enforcement | runtime.md §12 | wacp-workspace, wacp-coordinator |
| Deployment infrastructure (config, CLI, TLS, auth, logging, metrics, health) | deployment.md §2–12 | wacp-runtime |
| Agent migration | migration.md §2–8 | wacp-coordinator, wacp-transport |
| Tiered storage (hot/warm/cold) | storage.md §8–9 | wacp-trail |
| System snapshots | storage.md §7 | wacp-recovery, wacp-coordinator |
| Highway UI (TypeScript SPA) | highway-ui.md | new: TypeScript project |
| Docker image + systemd unit | deployment.md §9–10 | wacp-runtime (packaging) |

---

*Seed context for WACP — Akil Abderrahim and Claude Opus 4.6*
