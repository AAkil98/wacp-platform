# WACP — Seed Context

> This file primes a fresh Claude session with the project's current state. Read this first.

---

## What WACP Is

WACP (Workspace Agent Coordination Protocol) is a formal protocol for coordinating autonomous agents. It defines how agents communicate, how work is organized, how progress is recorded, and how everything is audited. The protocol is complete — 20 constituent specs + PROTOCOL.md + TAXONOMY.md. The specification layer is complete — 10 implementation specs covering every protocol domain.

## Current State

**Specification: complete.** 20 protocol specs, 10 implementation specs. Zero unresolved coverage gaps (audit 2026-03-22, gaps resolved 2026-03-24). All three conformance levels (Level 1–3) have implementation guidance.

**Initial implementation (Phases 0–8): complete.** 28 coding tasks in `SPEC-STRATEGY.md` are done. 12 Rust crates compile, 14 Python tests pass. The runtime binary starts a gRPC server, accepts agent and highway connections, and runs the coordinator event loop.

**Phases 9–13: complete.** Coordinator decision engine fully implemented. 214 tests in `wacp-coordinator` (was 28). See details below.

**Phase 14: complete.** Deployment infrastructure — full RuntimeConfig (9 sections, 47 fields), clap CLI (serve/validate/defaults), tracing-based structured logging (JSON/pretty), rustls TLS with mTLS, PSK authenticator with rate limiter, Prometheus metrics endpoint, HTTP health checks (Starting/Ready/Draining). 40 tests in `wacp-runtime` (was 7). See details below.

**Phase: 15 next.** Storage enhancements (system snapshots, tiered storage, retention). See `IMPLEMENTATION.md` for the full 42-task plan (Phases 9–18).

## Repository Map

```
wacp/
├── IMPLEMENTATION.md        # Decision log, spec tracking, Phase 9–18 plan (42 tasks)
├── SPEC-STRATEGY.md         # Phased coding plan — 28 tasks (Phases 0–8, all complete)
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
├── specs/coding/            # Coding specs (one per task)
│   ├── phase0-*.md – phase8-*.md    # Phases 0–8 (28 specs, all complete)
│   ├── phase9-*.md                  # Phase 9: topology (5 specs, complete)
│   ├── phase10-*.md                 # Phase 10: scheduling (4 specs, complete)
│   ├── phase11-*.md                 # Phase 11: integration (3 specs, complete)
│   ├── phase13-*.md                 # Phase 13: request handler (1 spec, complete)
│   └── phase14-*.md                 # Phase 14: deployment (6 specs, complete)
│
├── crates/                  # Rust implementation (12 crates)
│   ├── wacp-types/          # Protocol enums (19), identifier newtypes (8), structs (12) — 11 tests
│   ├── wacp-clock/          # HLC: Timestamp, Clock<TimeSource>, ManualTimeSource — 14 tests
│   ├── wacp-fsm/            # StateMachine trait + workspace/envelope/task FSMs — 41 tests
│   ├── wacp-taxonomy/       # YAML/JSON loader, 11 validation checks, role resolution — 22 tests
│   ├── wacp-permissions/    # Permission matrix, checkpoint table, port rights, default-deny — 20 tests
│   ├── wacp-trail/          # Storage traits, in-memory + filesystem backends, hash chain, SQLite index — 48 tests
│   ├── wacp-workspace/      # Workspace actor: 9 components, biased select loop, envelope/checkpoint — 17 tests
│   ├── wacp-coordinator/    # Full coordinator decision engine — 214 tests (see below)
│   ├── wacp-transport/      # Transport trait, InProcessTransport, gRPC (tonic + TLS), Authenticator trait, PSK provider, rate limiter — 17 tests
│   ├── wacp-recovery/       # Trail integrity check, state reconstruction, clock recovery — 6 tests
│   ├── wacp-runtime/        # Binary: config (47 fields), clap CLI, tracing logging, TLS, metrics, health — 40 tests
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

## wacp-coordinator Modules (Phases 9–13)

The coordinator crate grew from 28 to 214 tests across Phases 9–13. It now contains the full decision engine:

| Module | Purpose | Phase |
|--------|---------|-------|
| `tree.rs` | Workspace tree with originator_index, owner_index, causal traversal, siblings, transfer_owner, cascade | 9 |
| `visibility.rs` | Directed visibility graph — forward/reverse HashSet, grant/grant_checked, can_see | 9 |
| `ownership.rs` | EscalationRouter, resolve_owner, resolve_originator | 9 |
| `port_rights.rs` | Port rights multigraph — 3 indices, create/transfer/consume/revoke/expire, validate_send | 9 |
| `topology.rs` | TopologySet — compound operations spanning all 4 structures (create/terminate/transfer) | 9 |
| `task_graph.rs` | DAG with readiness counters, forward edges, bidirectional task-workspace binding | 10 |
| `gate.rs` | GateController — approval gates, timeout fallback, first-response-wins | 10 |
| `dispatch.rs` | Dispatcher — task selection, budget allocation with margin, capacity limits | 10 |
| `scheduling.rs` | SchedulingOps — context assembly, retry policy, cancellation cascade, subtask decomposition | 10 |
| `integration.rs` | IntegrationQueue + Pipeline + MergeExecutor (direct/layered/evaluated) + ConflictResolver + SalvageIntegration | 11 |
| `resource.rs` | TimeoutTracker, BudgetEnforcer, LivenessMonitor — pure state tracking | 12 |
| `handler.rs` | RequestHandler — domain-level agent/highway/gate RPC handling (no tonic dep) | 13 |
| `events.rs` | EventBus — callback subscribers + buffering for streaming RPCs | 13 |
| `orchestrator.rs` | Original coordinator actor (dispatch, event handling, envelope routing) | 0–8 |

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

**Topology:** 6 independent structures over the same node set — workspace tree (parent pointers + 3 indices), task graph (dual adjacency lists + readiness counters), visibility graph (forward/reverse `HashSet`), ownership domains (partition by `owner` + EscalationRouter), causal forest (partition by `originator`), port rights graph (3 indices by holder/target/id). All owned by coordinator actor via `TopologySet`, all recoverable from trail.

**Scheduling:** GateController (approval gates with timeout fallback), Dispatcher (task selection + budget allocation + capacity limits), SchedulingOps (context assembly from dependency checkpoints, retry policy, cancellation cascade, progressive decomposition).

**Integration:** IntegrationQueue (sequential, one-at-a-time), IntegrationPipeline (find final checkpoint, decide accept/revise/reject, select strategy), MergeExecutor (direct/layered/evaluated with resource overlap detection), ConflictResolver (type-based strategy selection, escalation pauses, rework fails workspace), SalvageIntegration (3 guardrails).

**Resource enforcement:** TimeoutTracker (elapsed time in Active/Blocked/Conflicted, pause on Suspended/Migrating), BudgetEnforcer (5-dimension check with warning threshold), LivenessMonitor (configurable inactivity detection).

**Request handling:** RequestHandler (domain-level types, testable without gRPC — bind, send envelope, emit signal, create checkpoint, get workspace, get task graph, inject envelope, gate response). EventBus (callback subscribers + buffering for streaming RPCs).

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

Phases 9–14 complete (topology, scheduling, integration, resource enforcement, request routing, deployment infrastructure). The remaining work:

| Phase | Work item | Spec source | Status |
|-------|-----------|-------------|--------|
| 14 | Deployment infrastructure (config, CLI, TLS, auth, logging, metrics, health) | deployment.md §2–12 | **Complete** |
| 15 | Storage enhancements (system snapshots, tiered storage, retention) | storage.md §7–9 | **Next** |
| 16 | Agent migration (7-step procedure, snapshot, unbind/bind, rollback) | migration.md §2–8 | Pending |
| 17 | End-to-end testing + Docker/systemd packaging | all impl specs | Pending |
| 18 | Highway UI (TypeScript SPA) | highway-ui.md | Pending |

See `IMPLEMENTATION.md` for the full task breakdown within each phase.

---

*Seed context for WACP — Akil Abderrahim and Claude Opus 4.6*
