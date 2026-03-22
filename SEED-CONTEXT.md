# WACP — Seed Context

> This file primes a fresh Claude session with the project's current state. Read this first.

---

## What WACP Is

WACP (Workspace Agent Coordination Protocol) is a formal protocol for coordinating autonomous agents. It defines how agents communicate, how work is organized, how progress is recorded, and how everything is audited. The protocol is complete — 20 constituent specs + PROTOCOL.md + TAXONOMY.md. The implementation is complete — 28 coding tasks across 9 phases, all done.

## Current State

**Implementation: complete.** All 28 tasks in `SPEC-STRATEGY.md` are done. 12 Rust crates compile, 225 Rust tests pass, 14 Python tests pass. The runtime binary starts a gRPC server, accepts agent and highway connections, and runs the coordinator event loop.

## Repository Map

```
wacp/
├── IMPLEMENTATION.md        # Decision log: language, architecture, strategy
├── SPEC-STRATEGY.md         # Phased implementation plan — 28 tasks across 9 phases (all complete)
├── SEED-CONTEXT.md          # This file
├── Cargo.toml               # Workspace manifest — 12 crates
├── .github/workflows/ci.yml # GitHub Actions: build, clippy, test, fmt, proto check
├── .gitignore
│
├── protocol/                # The specification layer
│   ├── PROTOCOL.md          # Authoritative protocol spec (976 lines)
│   ├── TAXONOMY.md          # Extension registry (385 lines)
│   ├── primitives/          # 8 constituent specs
│   ├── foundations/          # 2 constituent specs
│   ├── mechanisms/          # 4 constituent specs
│   └── topology/            # 6 constituent specs
│
├── impl/                    # Implementation specs (bridge between protocol and code)
│   ├── runtime.md           # Complete — implemented
│   ├── storage.md           # Complete — core implemented
│   ├── protocol-interface.md # Complete — implemented
│   └── sdk-agent.md         # Complete — implemented
│
├── proto/                   # 4 protobuf definitions (shared contract)
│   ├── primitives.proto     # Enums, core messages (Envelope, Signal, Checkpoint, Task, TrailEntry)
│   ├── agent.proto          # AgentService — 8 RPCs (6 unary, 2 streaming)
│   ├── highway.proto        # HighwayService — 12 RPCs (8 unary, 4 streaming)
│   └── taxonomy.proto       # Taxonomy configuration messages
│
├── specs/coding/            # 28 coding specs (one per task, all status: complete)
│
├── crates/                  # Rust implementation
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
└── sdk-python/              # Python agent SDK
    ├── pyproject.toml
    ├── src/wacp/
    │   ├── __init__.py      # Package: Agent, Signal, CheckpointStatus, Confidence, Priority
    │   ├── agent.py         # Agent class: connect, signal, checkpoint, send_envelope, inbox, commands
    │   ├── types.py         # Protocol constants with proto enum mapping
    │   └── proto/v1.py      # betterproto-generated types from .proto files
    └── tests/               # 14 tests
```

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

**Runtime (Rust):** Event-driven actor system on `tokio`. Three actor types: coordinator (singleton, owns workspace tree + task graph), workspace (one per active workspace, owns nine components), transport (routes messages). No shared mutable state. Workspace actors use biased two-channel select (coordinator commands take priority over agent messages).

**State machines:** Generic `StateMachine` trait instantiated three times: workspace lifecycle (9 states), envelope lifecycle (5 states), task lifecycle (8 statuses). Exhaustive `match` — illegal transitions don't compile.

**Trail:** Custom append-only segment log with per-entry fsync. SHA-256 hash chain for tamper evidence. SQLite index for queries. Content-addressable checkpoint store with integrity verification on read.

**Clock:** Hybrid Logical Clock (HLC). 80-bit: 64-bit physical (microseconds since epoch) + 16-bit logical counter. 10-byte big-endian encoding for lexicographic storage ordering.

**Transport:** Trait-based abstraction. `InProcessTransport` (testing) and `GrpcTransport` (production, via tonic). Agent service on port 9400, highway service on port 9401. Proto codegen via `tonic-build` in `build.rs`.

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

The core implementation is complete. Potential next work:
- Highway UI (TypeScript) — the human-facing interface
- Full gRPC request routing (wire agent/highway RPCs to coordinator actions end-to-end)
- Timeout/budget enforcement with `FuturesUnordered` timers
- Tiered storage (hot/warm/cold) and retention policies
- System snapshots for recovery acceleration
- Agent migration implementation
- Production deployment configuration (TLS, auth, logging)

---

*Seed context for WACP — Akil Abderrahim and Claude Opus 4.6*
