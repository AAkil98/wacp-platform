# WACP — Spec-Driven Implementation Strategy

```yaml
created: 2026-03-19
status: active
authors:
  - Akil Abderrahim
  - Claude Opus 4.6
```

---

## Approach

Every crate, every `.proto` file, and every SDK module gets a **coding spec** before implementation. A coding spec is not an architecture document (those exist in `impl/`). It is a build plan: the types to define, the functions to write, the tests to pass, and the acceptance criteria that mark the task as done.

**Workflow per task:**
1. Draft the coding spec.
2. Review and approve.
3. Implement to spec.
4. Tests pass → task complete.

**Phases are sequential.** Each phase depends on the output of the previous phase. Within a phase, tasks may be parallel if they share no dependencies.

**Atomic deliverables.** Each task produces a crate, a proto file, or an SDK module that compiles, passes its tests, and can be depended on by subsequent tasks. No partial crates. No "we'll add tests later."

---

## Crate Dependency Graph

From runtime spec §16. This is the build order — you cannot compile a crate before its dependencies exist.

```
wacp-types          (leaf — no internal dependencies)
    │
    ├── wacp-clock
    ├── wacp-fsm
    ├── wacp-taxonomy
    │
    ├── wacp-trail ──────────── depends on: types, clock
    ├── wacp-permissions ────── depends on: types, taxonomy
    │
    ├── wacp-workspace ──────── depends on: types, clock, trail, fsm, permissions
    ├── wacp-transport ──────── depends on: types
    │
    ├── wacp-coordinator ────── depends on: types, clock, trail, fsm, permissions, workspace
    │
    ├── wacp-recovery ───────── depends on: types, trail, fsm, clock, workspace, coordinator
    │
    └── wacp-runtime ────────── depends on: all crates (the binary)
```

---

## Phases

### Phase 0 — Scaffold

Bootstrap the Cargo workspace, proto files, and CI.

| # | Task | Output | Spec |
|---|------|--------|------|
| 0.1 | Cargo workspace scaffold | `Cargo.toml` (workspace), 11 empty crate skeletons with `Cargo.toml` + `src/lib.rs` | `specs/coding/phase0-scaffold.md` |
| 0.2 | Protobuf definitions | `proto/primitives.proto`, `proto/agent.proto`, `proto/highway.proto`, `proto/taxonomy.proto` | `specs/coding/phase0-proto.md` |
| 0.3 | CI pipeline | GitHub Actions: build, test, clippy, proto codegen check | `specs/coding/phase0-ci.md` |

**Exit criteria:** `cargo build` succeeds on the empty workspace. Proto files compile. CI runs green.

---

### Phase 1 — Foundation Types

The leaf crates. No internal dependencies. Everything else builds on these.

| # | Task | Output | Spec |
|---|------|--------|------|
| 1.1 | `wacp-types` | Protocol enums, identifier types, all struct definitions (Envelope, Signal, Checkpoint, Task, TrailEntry, Workspace, etc.) | `specs/coding/phase1-types.md` |
| 1.2 | `wacp-clock` | HLC implementation: timestamp struct, generation rules, comparison, serialization (10-byte big-endian) | `specs/coding/phase1-clock.md` |
| 1.3 | `wacp-fsm` | Generic `StateMachine` trait + three instantiations (workspace, envelope, task). Exhaustive transition matching. | `specs/coding/phase1-fsm.md` |

**Exit criteria:** All three crates compile with tests. `wacp-fsm` has a test for every valid transition and every rejected transition across all three state machines.

---

### Phase 2 — Taxonomy and Permissions

The policy layer. Depends only on Phase 1.

| # | Task | Output | Spec |
|---|------|--------|------|
| 2.1 | `wacp-taxonomy` | YAML/JSON parser, 11 validation checks, derived role resolution, lookup table construction | `specs/coding/phase2-taxonomy.md` |
| 2.2 | `wacp-permissions` | Permission matrix, checkpoint type table, port rights table, evaluation logic, default-deny | `specs/coding/phase2-permissions.md` |

**Exit criteria:** Taxonomy loads and validates the canonical reviewer example. Permission engine correctly allows/denies all base role actions. Port rights create/transfer/revoke/consume cycle works.

---

### Phase 3 — Trail and Storage

The persistence layer. The trail store is the runtime's commit point.

| # | Task | Output | Spec |
|---|------|--------|------|
| 3.1 | `wacp-trail` (storage trait + in-memory backend) | `TrailStorage` trait, `CheckpointStorage` trait, `SnapshotStorage` trait, in-memory implementations for all three | `specs/coding/phase3-trail-traits.md` |
| 3.2 | `wacp-trail` (filesystem backend) | Append-only segment log, segment rotation, fsync write path, crash-safe truncation | `specs/coding/phase3-trail-fs.md` |
| 3.3 | `wacp-trail` (index + queries) | SQLite index, async index writer, query API, access-controlled query results | `specs/coding/phase3-trail-index.md` |
| 3.4 | `wacp-trail` (checkpoint store) | Content-addressable filesystem store, deduplication, integrity verification | `specs/coding/phase3-checkpoint-store.md` |
| 3.5 | `wacp-trail` (hash chain) | SHA-256 chain computation, chain verification, write-ahead integration | `specs/coding/phase3-hash-chain.md` |

**Exit criteria:** Trail write path works end-to-end: entry → hash chain → fsync → index update. Checkpoint store round-trips payloads with integrity verification. In-memory backend passes all the same trait-level tests as the filesystem backend.

---

### Phase 4 — Workspace Actor

The core runtime unit. Each workspace is an actor owning its nine components.

| # | Task | Output | Spec |
|---|------|--------|------|
| 4.1 | `wacp-workspace` (state + components) | `WorkspaceState` struct, nine components, freezing rules, `ArchivedWorkspace` | `specs/coding/phase4-workspace-state.md` |
| 4.2 | `wacp-workspace` (actor loop) | Two-channel select loop (coordinator commands + agent messages), message handling, signal emission | `specs/coding/phase4-workspace-actor.md` |
| 4.3 | `wacp-workspace` (envelope processing) | Inbox management, priority queue, envelope validation delegation, delivery pipeline (workspace side) | `specs/coding/phase4-envelope-processing.md` |
| 4.4 | `wacp-workspace` (checkpoint creation) | Checkpoint chain validation, type-role check delegation, auto-signal emission, resource meter update | `specs/coding/phase4-checkpoint.md` |

**Exit criteria:** A workspace actor can be spawned, receive a directive, process envelopes, create checkpoints, emit signals, and transition through the full lifecycle to `Closed` or `Failed`. All with trail entries written.

---

### Phase 5 — Coordinator Actor

The orchestrator. Owns the workspace tree, task graph, and integration logic.

| # | Task | Output | Spec |
|---|------|--------|------|
| 5.1 | `wacp-coordinator` (workspace tree) | Tree data structure, create/abort/reparent operations, signal propagation routing, failure cascade | `specs/coding/phase5-workspace-tree.md` |
| 5.2 | `wacp-coordinator` (task graph) | DAG construction, dependency resolution, ready-task calculation, task lifecycle transitions | `specs/coding/phase5-task-graph.md` |
| 5.3 | `wacp-coordinator` (orchestration loop) | Signal handling, dispatch decisions, envelope routing, timeout/budget management (FuturesUnordered) | `specs/coding/phase5-orchestration.md` |
| 5.4 | `wacp-coordinator` (integration engine) | Three merge strategies, conflict detection, conflict resolution, salvage integration | `specs/coding/phase5-integration.md` |

**Exit criteria:** Coordinator can create workspaces, dispatch directives, receive signals, route envelopes, manage timeouts, and integrate completed workspaces. Full task lifecycle from `draft` through `integrated`.

---

### Phase 6 — Transport and Recovery

External boundaries and fault tolerance.

| # | Task | Output | Spec |
|---|------|--------|------|
| 6.1 | `wacp-transport` (trait + in-process) | `Transport`, `AgentSession`, `HighwaySession` traits, `InProcessTransport` implementation | `specs/coding/phase6-transport-trait.md` |
| 6.2 | `wacp-transport` (gRPC) | `tonic` server, `AgentServiceServer`, `HighwayServiceServer`, protobuf ↔ Rust type conversion, error mapping | `specs/coding/phase6-grpc.md` |
| 6.3 | `wacp-recovery` | Trail integrity check, state reconstruction, in-flight recovery, timer reconstruction, clock recovery | `specs/coding/phase6-recovery.md` |

**Exit criteria:** An agent can connect via `InProcessTransport`, bind to a workspace, and execute the full protocol pipeline. gRPC transport passes the same tests. Recovery restores state from trail after simulated crash.

---

### Phase 7 — Runtime Binary

Wire everything together.

| # | Task | Output | Spec |
|---|------|--------|------|
| 7.1 | `wacp-runtime` (binary) | Initialization sequence, configuration, taxonomy loading, recovery, coordinator spawn, transport start, shutdown | `specs/coding/phase7-runtime.md` |
| 7.2 | Integration tests | End-to-end scenarios: single worker, multi-worker, delegation, conflict, timeout, budget, escalation, recovery | `specs/coding/phase7-integration.md` |

**Exit criteria:** The `wacp-runtime` binary starts, accepts agent connections, orchestrates work, and shuts down cleanly. Integration tests cover the protocol's conformance requirements at Level 1 (Core).

---

### Phase 8 — Agent SDKs

Client libraries.

| # | Task | Output | Spec |
|---|------|--------|------|
| 8.1 | `wacp-sdk` (Rust) | `Agent` struct, builders, connection lifecycle, streams, error types, `TestRuntime` | `specs/coding/phase8-sdk-rust.md` |
| 8.2 | `wacp` (Python) | `Agent` class, dataclasses, connection lifecycle, async iterators, error hierarchy, `MockRuntime` | `specs/coding/phase8-sdk-python.md` |

**Exit criteria:** Both SDKs connect to the runtime, execute a full agent lifecycle (bind → work → checkpoint → complete), and pass tests using their respective mock/test runtimes. Python SDK installable via `pip install -e .`. Rust SDK compiles as a standalone crate.

---

## Summary

| Phase | Name | Tasks | Depends on |
|-------|------|-------|------------|
| 0 | Scaffold | 3 | — |
| 1 | Foundation Types | 3 | Phase 0 |
| 2 | Taxonomy and Permissions | 2 | Phase 1 |
| 3 | Trail and Storage | 5 | Phase 1 |
| 4 | Workspace Actor | 4 | Phases 2, 3 |
| 5 | Coordinator Actor | 4 | Phase 4 |
| 6 | Transport and Recovery | 3 | Phase 5 |
| 7 | Runtime Binary | 2 | Phase 6 |
| 8 | Agent SDKs | 2 | Phase 7 |
| | **Total** | **28 tasks** | |

**28 coding specs. 28 tasks. 9 phases.**

Each coding spec lives in `specs/coding/`. Each follows a consistent format: scope, types, functions, tests, acceptance criteria. No spec, no code.

---

## Coding Spec Format

Every coding spec follows this template:

```markdown
# Task [N.M]: [Name]

## Scope
What this task produces. What it does NOT produce.

## Dependencies
Crates and tasks that must be complete before this task begins.

## Types
Structs, enums, and traits to define. Field-level detail.

## Functions
Public API. Signature, behavior, error cases.

## Internal Design
Non-obvious implementation decisions. Data structures. Algorithms.

## Tests
Specific test cases. Each test has a name and a one-line description of what it verifies.

## Acceptance Criteria
Bulleted checklist. All must be true for the task to be complete.
```

---

*WACP spec-driven implementation strategy — Akil Abderrahim and Claude Opus 4.6*
