# WACP — Implementation Journal

```yaml
created: 2026-03-17
revised: 2026-03-19
status: active
authors:
  - Akil Abderrahim
  - Claude Opus 4.6
```

---

## Decision Log

Decisions are numbered, dated, and final. Each records the choice, the alternatives considered, and the reasoning.

### D-001: Language Strategy — Multi-Language (2026-03-17)

**Decision:** WACP is a multi-language project. Three languages, one per trust boundary.

| Component | Language | Role |
|-----------|----------|------|
| Runtime | Rust | Trust root. All protocol enforcement. |
| Agent SDKs | Python + Rust | Thin clients. Python for LLM agents, Rust for system agents. |
| Highway UI | TypeScript | Human-facing interface. Streaming trail, gates, injection. |

**Alternatives considered:**

| Language | Verdict | Reason |
|----------|---------|--------|
| Rust (single-language) | Rejected | Excellent for runtime, poor fit for LLM agent ecosystem and UI |
| Go | Rejected | Type system cannot encode WACP's closed sets at compile time. Immutability is convention, not enforced. Weakest fit for the protocol's precision. |
| TypeScript (single-language) | Rejected | Runtime type checking only. Cannot serve as trust root where the compiler must enforce invariants. Viable for highway UI only. |
| Python (single-language) | Rejected | GIL limits concurrency. Type enforcement is advisory. Acceptable for agent SDK only. |
| C++ | Rejected | Rust-equivalent performance without memory safety. Runtime is the trust root — use-after-free in the runtime compromises all guarantees. |
| JavaScript | Rejected | TypeScript without the type system. No compile-time checking for closed sets. |

**Reasoning:** The protocol defines three participants (runtime, agents, humans) communicating through serialized messages. These are natural process boundaries. Each language sits where its strengths matter: Rust where correctness is enforced, Python where the LLM ecosystem lives, TypeScript where UI is built. No FFI — process isolation with message passing, consistent with the protocol's "messages over mutations" principle (§3.1).

---

### D-002: Correctness Over Velocity (2026-03-17)

**Decision:** Correctness is the primary design constraint. Development velocity is explicitly deprioritized.

**Implications:**
- Closed sets (11 signals, 9 states, 4 conflict types) are compile-time types, not runtime strings.
- State machine transitions are exhaustively matched — illegal transitions do not compile.
- Immutability is structural (Rust ownership), not conventional.
- Write-ahead trail uses synchronous durable writes, not async batching.
- Full spec coverage before code — no "prototype first, fix later."

---

### D-003: Process Isolation Over FFI (2026-03-17)

**Decision:** Components communicate via IPC (gRPC over the network or Unix sockets), not FFI (PyO3, napi-rs, C bindings).

**Reasoning:**
- Process isolation reinforces workspace isolation — an agent crash cannot corrupt runtime state.
- Agent migration (§6.9) becomes connection management, not in-process surgery.
- Consistent with the protocol's own communication model (envelopes are messages, not function calls).
- Debuggable — every cross-boundary interaction is a serialized message visible in the trail.
- No shared mutable state across languages. The runtime owns all mutable state.

**Cost:** Three build systems (Cargo, pip/poetry, npm/pnpm). Integration testing across languages. Protobuf code generation pipeline. Coordinated versioning.

---

### D-004: Protobuf as Interface Definition Language (2026-03-17)

**Decision:** Protocol messages crossing language boundaries are defined in `.proto` files. Protobuf is the single source of truth for message shapes.

**Reasoning:**
- Code generation in all three languages from one source.
- Binary efficiency for trail writes and envelope delivery.
- Schema enforcement at the serialization boundary — malformed messages cannot be constructed.
- Backward compatibility for taxonomy evolution across runs.
- Enums encode the protocol's closed sets (signal types, workspace states, envelope priorities).

**What protobuf does NOT encode:** State transition validity, permission checks, hash chain integrity. These remain Rust runtime concerns. The IDL defines message shapes; the runtime defines rules.

**Code generation targets:**
- Rust: `prost` + `tonic` (gRPC)
- Python: `betterproto`
- TypeScript: `ts-proto`

---

### D-005: gRPC as Transport (2026-03-17, confirmed 2026-03-19)

**Decision:** gRPC is the default transport. Confirmed after implementation — both InProcessTransport (testing) and GrpcTransport (production) are implemented behind the transport trait.

**Reasoning:** Natural fit with protobuf. Bidirectional streaming for trail observation and signal propagation. Mature libraries in all three languages. The runtime exposes gRPC services; SDKs and highway UI are gRPC clients.

**Resolved:** Additional transports (in-process channels for testing) were implemented from the start behind the transport abstraction.

---

## Correctness Layering

Three layers prevent protocol violations across language boundaries:

| Layer | What it catches | Mechanism |
|-------|----------------|-----------|
| Protobuf schema | Structural errors (missing fields, invalid enum values, type mismatches) | Code generation — malformed messages cannot be constructed |
| Rust runtime | Semantic errors (permission violations, illegal transitions, budget exceeded) | Runtime validation on every operation — untrusted clients |
| Trail | Everything, for audit | Write-ahead recording — every accepted and rejected operation is logged |

---

## Implementation Specs

### Phase 1 — Complete (pre-code specs for core runtime)

Four specs answered "how does this become code" for the runtime, storage, interface, and SDKs. All four are complete and have been implemented.

| # | Spec | Path | Scope | Status |
|---|------|------|-------|--------|
| 1 | Runtime Architecture | `impl/runtime.md` | State machine engine, permission engine, trail write-ahead path, clock, recovery engine, concurrency model, workspace isolation | **Complete — implemented** |
| 2 | Storage Architecture | `impl/storage.md` | Trail backend (custom append-only log), checkpoint content-addressing, workspace state persistence, snapshots, tiered storage, retention | **Complete — core implemented, tiered storage pending** |
| 3 | Protocol Interface | `impl/protocol-interface.md` | `.proto` definitions, gRPC service contracts (agent + highway), serialization rules, transport trait, authentication, versioning | **Complete — implemented** |
| 4 | Agent SDK Design | `impl/sdk-agent.md` | Python + Rust SDK surface area, connection lifecycle, LLM agent mapping, tool mounting, error handling, testing, packaging | **Complete — implemented** |

### Phase 2 — Complete (specs for remaining domains)

Three specs covering the remaining domains not addressed by Phase 1.

| # | Spec | Path | Scope | Status | Depends On |
|---|------|------|-------|--------|------------|
| 5 | Highway UI | `impl/highway-ui.md` | TypeScript architecture, component model, real-time trail streaming via gRPC-Web, gate approval UX, escalation handling, injection interface, `ts-proto` codegen, framework choice, state management | **Complete** | Protocol Interface (#3) |
| 6 | Deployment | `impl/deployment.md` | Configuration file format (YAML), CLI interface, TLS setup, authenticator provider configuration, structured logging via `tracing`, metrics endpoint, Docker image, systemd unit, health checks | **Complete** | Runtime (#1) |
| 7 | Agent Migration | `impl/migration.md` | Migration coordinator procedure, workspace state snapshot for handoff, agent unbind sequence, new agent bind with state restore, atomic rollback on failure, connection handoff mechanics, resource meter continuity across migration | **Complete** | Runtime (#1), Storage (#2) |

### Phase 3 — Complete (coordinator decision logic)

Audit (2026-03-22) identified three protocol domains covered by the protocol specs but lacking implementation specs. All three completed 2026-03-24.

| # | Spec | Path | Scope | Status | Depends On |
|---|------|------|-------|--------|------------|
| 8 | Coordinator Integration | `impl/integration.md` | Merge strategies (`direct`/`layered`/`evaluated`), checkpoint evaluation, conflict detection (4 types), conflict resolution (3 strategies), salvage integration (3 guardrails) | **Complete** | Runtime (#1), Storage (#2), Topology (#10) |
| 9 | Task Scheduling | `impl/task-scheduling.md` | Task lifecycle (8 states), gate enforcement, dispatch policy, resource allocation, context assembly, retry/cancellation, progressive decomposition, task-workspace coupling | **Complete** | Runtime (#1), Topology (#10) |
| 10 | Topology Operations | `impl/topology.md` | Workspace tree (traversals, cascade, reparent), task graph (DAG, readiness), visibility graph, ownership domains, causation, port rights graph, compound operations, recovery | **Complete** | Runtime (#1) |

**Protocol sources:**

| Impl spec | Protocol sections | Constituent specs |
|-----------|-------------------|-------------------|
| Integration | §7.4–§7.9 | `mechanisms/integration.md` |
| Task Scheduling | §4.6 | `primitives/task.md` |
| Topology | §6.3, §6.8 | `topology/tree.md`, `topology/graph.md`, `topology/visibility.md`, `topology/ownership.md`, `topology/causation.md`, `topology/channels.md` |

### Spec Audit (2026-03-22)

**Internal consistency: clean.** All cross-references resolve. No trait conflicts. No struct/enum inconsistencies. The `Authenticator` trait appears in both protocol-interface.md §7 (defines) and deployment.md §5.1 (implements) — intentional and consistent.

**Protocol coverage gaps by severity (updated 2026-03-24):**

| Severity | Gap | Protocol source | Status |
|----------|-----|----------------|--------|
| ~~Blocking~~ | ~~Integration & conflict resolution~~ | §7.4–§7.9 | **Resolved** — integration.md |
| ~~Blocking~~ | ~~Task scheduling & DAG operations~~ | §4.6, task spec | **Resolved** — task-scheduling.md |
| ~~Blocking~~ | ~~Topology operations (tree, graph, visibility)~~ | 6 topology specs | **Resolved** — topology.md |
| Important | Graceful termination enforcement detail | §6.10 | Partially covered in deployment §3.4 and protocol-interface `GracefulTermination` message |
| Important | Trail signing | §11.5 | Explicitly deferred to Level 3 |
| ~~Important~~ | ~~Salvage integration guardrails~~ | §7.9 | **Resolved** — integration.md §7 |
| Deferrable | Ownership transfer | §4.8 | Covered in topology.md §5.3 |

**All blocking gaps resolved.** Zero protocol domains without implementation spec coverage.

**Fully covered protocol domains:**

| Domain | Impl spec(s) |
|--------|-------------|
| Runtime internals (FSM, permissions, clock, recovery, concurrency) | runtime.md |
| Storage (trail, checkpoints, snapshots, tiering) | storage.md |
| Wire format (protobuf, gRPC, serialization, transport trait) | protocol-interface.md |
| Agent SDKs (Python, Rust) | sdk-agent.md |
| Highway UI (TypeScript, gRPC-Web, gates, escalations) | highway-ui.md |
| Deployment (config, CLI, TLS, auth providers, logging, metrics, health, Docker, systemd) | deployment.md |
| Agent migration (coordinator procedure, snapshot, unbind/bind, rollback, resource continuity) | migration.md |
| Topology (tree, task graph, visibility, ownership, causation, port rights) | topology.md |
| Task scheduling (lifecycle, gates, dispatch, resource allocation, retry, decomposition) | task-scheduling.md |
| Integration (merge strategies, conflict detection/resolution, salvage, ordering) | integration.md |

### Coverage matrix — "What's Next" vs. specs

| Work item | Specced? | Spec source | Phase | Tasks |
|-----------|----------|-------------|-------|-------|
| Topology operations | Yes | topology.md §2–10 | 9 | 9.1–9.5 |
| Task scheduling | Yes | task-scheduling.md §2–11 | 10 | 10.1–10.4 |
| Coordinator integration | Yes | integration.md §2–11 | 11 | 11.1–11.4 |
| Timeout/budget enforcement | Yes | runtime.md §12 | 12 | 12.1–12.3 |
| Full gRPC request routing | Yes | protocol-interface.md §4–5, §8–9 | 13 | 13.1–13.4 |
| Production deployment | Yes | deployment.md §1–13 | 14 | 14.1–14.6 |
| Tiered storage + snapshots | Yes | storage.md §7–9 | 15 | 15.1–15.4 |
| Agent migration | Yes | migration.md §2–12 | 16 | 16.1–16.4 |
| End-to-end testing + packaging | Yes | all impl specs | 17 | 17.1–17.4 |
| Highway UI | Yes | highway-ui.md | 18 | 18.1–18.4 |

---

## Implementation Progress

### Phase 0 — Scaffold (complete)

| Task | Output | Status |
|------|--------|--------|
| 0.1 | Cargo workspace — 12 crates | Done |
| 0.2 | Protobuf — 4 `.proto` files | Done |
| 0.3 | CI pipeline — GitHub Actions | Done |

### Phases 1–8 — All 28 tasks complete

12 Rust crates, 225 Rust tests, 14 Python tests. See `SPEC-STRATEGY.md` for the full task list and `specs/coding/` for individual coding specs.

---

### Phase 9 — Topology Operations

Complete the six topology structures in the coordinator. Currently only the workspace tree and task graph have basic implementations. The topology spec defines visibility, ownership, causation, and port rights — all owned by the coordinator actor, all recoverable from the trail.

| # | Task | Output | Spec source | Crate |
|---|------|--------|-------------|-------|
| 9.1 | Workspace tree indices | `originator_index`, `owner_index` on `WorkspaceTree`; causal traversal (filter by originator within subtree); O(1) lookup for ownership and originator queries | topology.md §2.1–2.2 | wacp-coordinator |
| 9.2 | Visibility graph | `VisibilityGraph` — forward/reverse `HashSet` per node, grant insertion with containment enforcement, visibility query, default grant on workspace creation | topology.md §4 | wacp-coordinator |
| 9.3 | Ownership domains + causation | `OwnershipDomains` partition by `owner` field with transfer operation; `CausalForest` partition by `originator` with causal impact queries | topology.md §5–6 | wacp-coordinator |
| 9.4 | Port rights graph | `PortRightsGraph` with 3 indices (by holder, by target, by right id); create/transfer/revoke/consume lifecycle; `send_once` consumption on delivery | topology.md §7 | wacp-coordinator |
| 9.5 | Compound operations | Workspace creation spanning all 6 topologies (tree insert + visibility grant + ownership register + causation register + default port rights); failure cascade update spanning tree + ownership boundaries | topology.md §8 | wacp-coordinator |

**Depends on:** Phases 1–8.
**Exit criteria:** All six topology structures compile with tests. Workspace creation atomically updates all six. Failure cascade respects ownership boundaries and produces correct reparent/fail partitions. Port rights lifecycle (create → transfer → consume for `send_once`) passes end-to-end. All topology state recoverable from trail entries.

---

### Phase 10 — Task Scheduling

Policy layer on top of topology. The coordinator decides what to dispatch, where, with what budget, and what to do on failure.

| # | Task | Output | Spec source | Crate |
|---|------|--------|-------------|-------|
| 10.1 | Task graph enhancement | Readiness counters (decrement on dependency completion, zero = dispatchable); task-workspace binding (`bind_task_to_workspace` / `unbind`); status propagation from workspace signals to task status | task-scheduling.md §2, §9 | wacp-coordinator |
| 10.2 | Gate enforcement | `draft → pending` approval gate; gate event generation for highway; timeout auto-approve (configurable); gate response handling (approve / reject / modify) | task-scheduling.md §3 | wacp-coordinator |
| 10.3 | Dispatch + resource allocation | Ready task selection algorithm (priority, dependency depth, creation order); workspace budget derivation from task resource estimates; workspace creation with allocated resources and role | task-scheduling.md §4–5 | wacp-coordinator |
| 10.4 | Context assembly + retry + decomposition | Dependency output collection into workspace context (checkpoint payloads from completed dependencies); retry on failure (task → pending with attempt counter, max retries); cancellation cascade to dependent tasks; progressive subtask insertion into DAG mid-execution | task-scheduling.md §6–8 | wacp-coordinator |

**Depends on:** Phase 9 (topology structures).
**Exit criteria:** A task can move through the full lifecycle: `draft → pending → assigned → in_progress → completed → integrated`. Gate enforcement blocks `draft → pending` until approval or auto-approve timeout. Dispatch creates a workspace with correct budget and context. Retry resets a failed task to `pending` with incremented attempt counter. Subtask insertion maintains DAG acyclicity.

---

### Phase 11 — Integration Engine

Replace the stub merge strategies with full integration logic. The coordinator reads checkpoints, detects conflicts, resolves them, and merges output into the parent context.

| # | Task | Output | Spec source | Crate |
|---|------|--------|-------------|-------|
| 11.1 | Integration pipeline + ordering | End-to-end procedure: `complete` signal → workspace `integrating` → coordinator decision (accept/revise/reject) → merge → workspace `closed` or `failed`; integration queue; sibling ordering (dependency order, then creation order) | integration.md §2–3, §8 | wacp-coordinator |
| 11.2 | Merge strategies | `direct` (pass-through, no conflict check), `layered` (append to parent context with overlap detection), `evaluated` (coordinator reads + assesses checkpoint content); strategy selection criteria per workspace role and checkpoint type | integration.md §4 | wacp-coordinator |
| 11.3 | Conflict detection + resolution | 4 conflict types (`content_overlap`, `semantic_contradiction`, `dependency_violation`, `constraint_breach`); detection per merge strategy; 3 resolution strategies (`coordinator_resolve`, `escalate` to highway, `agent_rework` with feedback) | integration.md §5–6 | wacp-coordinator |
| 11.4 | Salvage integration | Failed workspace partial recovery — extract usable checkpoints, apply 3 guardrails (provisional-only, originator-scoped, explicit trail marking); trail events for all integration outcomes (22 invariants from spec) | integration.md §7, §9–10 | wacp-coordinator |

**Depends on:** Phase 10 (task lifecycle drives integration entry).
**Exit criteria:** Integration pipeline runs end-to-end: workspace completes → coordinator selects strategy → merge executes → conflicts detected and resolved → workspace closed and task marked `integrated`. Salvage extracts provisional checkpoints from a failed workspace. All 22 invariants from integration.md §10 hold. Trail events recorded for every integration step.

---

### Phase 12 — Resource Enforcement

Timeout, budget, and liveness enforcement. Independent timer-based mechanisms in the coordinator and workspace actors.

| # | Task | Output | Spec source | Crate |
|---|------|--------|-------------|-------|
| 12.1 | Timeout enforcement | Per-workspace timers via `FuturesUnordered` in coordinator; start/resume on `Active`/`Blocked`/`Conflicted`; pause on `Suspended`/`Migrating`; abort on expiry (`Failed` with `reason: timeout`); additive extensions only | runtime.md §12 | wacp-coordinator, wacp-workspace |
| 12.2 | Budget enforcement | Resource meter tracking (tokens, wall time, storage, network, cost); warning at configurable threshold (default 80%); feedback envelope to agent on warning; hard failure at limit (`Failed` with `reason: budget_exceeded`); additive budget increases | runtime.md §12 | wacp-workspace, wacp-coordinator |
| 12.3 | Liveness monitoring | Most-recent trail entry timestamp tracking per active workspace; `liveness_warning` trail entry when no activity within configured interval; coordinator notification (advisory — coordinator decides response) | runtime.md §12 | wacp-coordinator |

**Depends on:** Phases 9–11 (coordinator orchestration must be functional).
**Exit criteria:** A workspace that exceeds its timeout transitions to `Failed`. A workspace that exceeds any budget dimension transitions to `Failed` with warning emitted at the threshold. Liveness warning fires when a workspace has no trail activity within the interval. All three mechanisms operate independently and produce correct trail entries.

---

### Phase 13 — gRPC Request Routing

Wire the transport layer to the coordinator. Replace all `unimplemented!()` stubs with full request handling.

| # | Task | Output | Spec source | Crate |
|---|------|--------|-------------|-------|
| 13.1 | Agent service wiring | `Bind` returns full workspace state (role, directive, context, visibility, authority, budget); `SendEnvelope` routes through the delivery pipeline (permission check → trail write → deliver → ack); `EmitSignal` routes to workspace actor → coordinator; `CreateCheckpoint` creates checkpoint + writes to store; `QueryTrail` with access control | protocol-interface.md §4 | wacp-transport, wacp-runtime |
| 13.2 | Highway service wiring | `Authenticate` dispatches to configured auth provider; `InjectEnvelope` bypasses permission matrix, validates structure/target/type; `GetWorkspace`/`GetTaskGraph`/`GetCheckpoint` read from coordinator state; `QueryTrail` on global trail | protocol-interface.md §5 | wacp-transport, wacp-runtime |
| 13.3 | Gate + escalation routing | `RespondToGate` delivers gate response to coordinator (approve/reject/modify); `RespondToEscalation` delivers resolution; gate/escalation event flow from coordinator → highway transport → connected clients; escalation queuing when no client connected | protocol-interface.md §5; task-scheduling.md §3 | wacp-transport, wacp-coordinator |
| 13.4 | Streaming RPCs | `ReceiveEnvelopes` (workspace inbox stream); `ReceiveCommands` (coordinator commands to agent); `StreamTrail` (real-time trail entries); `StreamGates` (gate events); `StreamEscalations` (escalation events); `StreamWorkspaceState` (state changes) | protocol-interface.md §4–5 | wacp-transport |

**Depends on:** Phases 9–12 (coordinator must have real logic to route to).
**Exit criteria:** An agent can connect via gRPC, bind, send envelopes, emit signals, create checkpoints, query the trail, and receive streaming updates. A highway client can authenticate, inject envelopes, respond to gates and escalations, query the trail, and receive streaming events. All RPCs produce correct trail entries.

---

### Phase 14 — Deployment Infrastructure

Production operation surface: configuration, CLI, TLS, auth providers, logging, metrics, health.

| # | Task | Output | Spec source | Crate |
|---|------|--------|-------------|-------|
| 14.1 | Configuration | `RuntimeConfig` struct hierarchy (9 sections, 47 fields); `serde_yaml` with `deny_unknown_fields`; all defaults; 9 validation checks (TLS completeness, auth completeness, taxonomy readability, data dir, address uniqueness, numeric constraints, enum fields, log file path, cold retention parse); env var overrides | deployment.md §2, §11 | wacp-runtime |
| 14.2 | CLI interface | `clap` derive: `serve` (default), `validate`, `defaults` subcommands; `--config` and `--version` global options; exit codes (0–4, 101); signal handling (`SIGTERM` graceful, `SIGINT` immediate, double-signal escalation) | deployment.md §3 | wacp-runtime |
| 14.3 | Structured logging | `tracing` subscriber setup; JSON format (one JSON object per line: timestamp, level, target, message, span) and pretty format; stderr or file output; log level filtering; unconditional TLS-disabled warning | deployment.md §6 | wacp-runtime |
| 14.4 | TLS | `rustls` `ServerTlsConfig` for both endpoints; PEM cert/key loading; mTLS via `client_ca_file`; min version enforcement (1.2 or 1.3); plaintext mode when disabled; certificate expiry exposed as metric | deployment.md §4 | wacp-runtime, wacp-transport |
| 14.5 | Authentication providers | PSK provider: token gen (`ring::rand`), register/revoke per workspace, revoke on terminal state; external HTTP provider: POST to auth URL, response validation, timeout; rate limiting: sliding window per source IP, `authentication_rate_limited` trail entry | deployment.md §5 | wacp-transport, wacp-runtime |
| 14.6 | Observability | Prometheus metrics endpoint (gauges: active workspaces, active connections, trail size, queue depths; counters: envelopes delivered/rejected, signals emitted, checkpoints created, auth failures; histogram: request latency); HTTP health checks (liveness: process alive; readiness: trail writable + coordinator running) | deployment.md §7–8 | wacp-runtime |

**Depends on:** Phase 13 (gRPC routing must exist for TLS and auth to wrap).
**Exit criteria:** `wacp-runtime serve` starts from a YAML config file with all tunables. `wacp-runtime validate` catches every misconfiguration at startup. TLS-encrypted gRPC works end-to-end. Both auth providers authenticate agents and humans correctly. Metrics endpoint serves Prometheus-scrapable output. Health endpoint returns correct liveness and readiness status.

---

### Phase 15 — Storage Enhancements

System snapshots, snapshot-accelerated recovery, tiered storage, and retention.

| # | Task | Output | Spec source | Crate |
|---|------|--------|-------------|-------|
| 15.1 | System snapshots | Coordinator snapshot procedure: record `snapshot_started`, collect workspace states + coordinator state (tree, task graph, port rights) + clock + resource meters, write combined snapshot file, record `snapshot_completed`; configurable schedule (entry count or time interval); retention (keep N most recent) | storage.md §7 | wacp-coordinator, wacp-trail |
| 15.2 | Snapshot-accelerated recovery | Load latest valid system snapshot (verify internal checksum); reconstruct coordinator + workspace states from snapshot; replay trail from `snapshot_sequence + 1`; fallback to full replay on missing/corrupt/incompatible snapshot | storage.md §7; runtime.md §13 | wacp-recovery |
| 15.3 | Tiered storage | Hot tier (active + N recent sealed segments, uncompressed); warm tier (`zstd`-compressed sealed segments); cold tier (optional external path); tier transition logic: hot → warm on segment count threshold, warm → cold on age threshold; hot tier recovery invariant (all segments from latest snapshot onward stay hot) | storage.md §8 | wacp-trail |
| 15.4 | Retention + compaction | Background compaction task (configurable interval); warm segment merging; checkpoint payload cleanup (unreferenced payloads); `trail_tier_transition` and `trail_segment_deleted` trail entries; no-silent-deletion invariant | storage.md §9 | wacp-trail |

**Depends on:** Phase 12 (resource enforcement in coordinator for snapshot triggers).
**Exit criteria:** System snapshots are taken on schedule and at clean shutdown. Recovery from a snapshot + trail delta produces the same state as full trail replay. Warm segments are compressed with `zstd`. Cold tier moves segments to the configured destination. Compaction runs in the background without affecting the write path. Every deletion is trail-recorded before execution.

---

### Phase 16 — Agent Migration

Replace a workspace's agent while preserving all nine workspace components. Atomic — no partial migration state.

| # | Task | Output | Spec source | Crate |
|---|------|--------|-------------|-------|
| 16.1 | Migration procedure | 7-step linear sequence: record pre-migration state → transition to `migrating` → drain in-flight ops → snapshot workspace → unbind old agent → bind new agent → transition to pre-migration state; precondition checks (workspace in `active` or `blocked`, not mid-write) | migration.md §2 | wacp-coordinator |
| 16.2 | State snapshot + restore | Serialize all 9 workspace components; transfer to new agent via `Bind` response (full state: directive, context, checkpoint register, working memory, resource meter, local trail, visibility, authority); validation on restore | migration.md §3, §5 | wacp-workspace, wacp-transport |
| 16.3 | Unbind/bind + rollback | Old agent connection teardown (drain signals, final state capture); new agent connection establishment; atomic rollback on failure at any step (restore pre-migration state, workspace → `failed` if rollback impossible) | migration.md §4, §6–7 | wacp-transport, wacp-coordinator |
| 16.4 | Resource continuity | Budget meter transferred to new agent (no reset); timeout timer continuous across migration (pause during `migrating`, resume after); inbox preserved (unprocessed envelopes delivered to new agent); trail events (`migration_started`, `migration_completed` or `migration_failed`) | migration.md §8–10 | wacp-coordinator, wacp-workspace |

**Depends on:** Phase 13 (gRPC routing for bind/unbind), Phase 14 (auth for new agent token).
**Exit criteria:** Migration replaces an agent without losing workspace state. The new agent receives the full workspace via `Bind`. Resource meter and timeout are continuous. Inbox envelopes are preserved. Failure at any step triggers rollback. Trail records the complete migration lifecycle.

---

### Phase 17 — End-to-End Testing + Packaging

Validate the complete system. Package for deployment.

| # | Task | Output | Spec source | Crate |
|---|------|--------|-------------|-------|
| 17.1 | E2E: full lifecycle | Single-worker: task dispatch → agent bind → work → checkpoint → complete → integration → closed. Multi-worker: parallel dispatch, independent completion, ordered integration. Delegation: worker creates subtasks, subtree completes, parent integrates | All impl specs | wacp-runtime (integration tests) |
| 17.2 | E2E: failure scenarios | Timeout expiry → workspace failed. Budget exceeded → workspace failed. Failure cascade with ownership boundaries. Conflict detection → escalation → human resolution. Crash → recovery → resumed state matches pre-crash | runtime.md §12–13; integration.md §5–6 | wacp-runtime (integration tests) |
| 17.3 | E2E: highway integration | Gate approval flow: task draft → gate event to highway → human approves → task dispatched. Escalation flow: agent escalates → highway notified → human responds. Envelope injection: human injects directive via highway | protocol-interface.md §5; task-scheduling.md §3 | wacp-runtime (integration tests) |
| 17.4 | Docker image + systemd | Multi-stage Dockerfile (`rust:slim` build, `debian:slim` runtime); systemd unit file with `Type=notify`, `WatchdogSec`, `Restart=on-failure`; health check integration (Docker `HEALTHCHECK`, systemd watchdog) | deployment.md §9–10 | wacp-runtime (packaging) |

**Depends on:** Phases 9–16 (all runtime functionality).
**Exit criteria:** All E2E scenarios pass. The Docker image builds and runs the runtime with a production config. The systemd unit file starts, monitors, and restarts the runtime correctly.

---

### Phase 18 — Highway UI

TypeScript SPA for human-in-the-loop interaction. Separate project, gRPC-Web client.

| # | Task | Output | Spec source | Crate |
|---|------|--------|-------------|-------|
| 18.1 | TypeScript scaffold | Project structure (Vite + framework from highway-ui.md); `ts-proto` codegen from `.proto` files; gRPC-Web transport; dev server with proxy to runtime; build pipeline producing static files | highway-ui.md §2–4 | new: highway-ui/ |
| 18.2 | Trail viewer + workspace tree | Real-time trail streaming via `StreamTrail`; filtering by workspace, event type, time range; workspace tree visualization; workspace detail view (state, role, directive, checkpoint register, resource meter) | highway-ui.md §5–8 | highway-ui/ |
| 18.3 | Gate + escalation management | Gate event stream via `StreamGates`; approval/reject/modify UI; escalation event stream via `StreamEscalations`; escalation response UI; notification system for pending actions | highway-ui.md §9–11 | highway-ui/ |
| 18.4 | Envelope injection + autonomy | Injection form: target workspace, envelope type, payload, priority; validation against workspace existence and type registration; autonomy presets (full-auto, supervised, manual); preset switching at run-time | highway-ui.md §12–14 | highway-ui/ |

**Depends on:** Phase 13 (highway gRPC must be wired).
**Exit criteria:** The UI connects to the runtime via gRPC-Web, streams the trail in real time, displays the workspace tree, handles gate approvals and escalations, and allows envelope injection. Static build deployable independently of the runtime.

---

### Summary — Phases 9–18

| Phase | Name | Tasks | Depends on |
|-------|------|-------|------------|
| 9 | Topology Operations | 5 | Phases 1–8 |
| 10 | Task Scheduling | 4 | Phase 9 |
| 11 | Integration Engine | 4 | Phase 10 |
| 12 | Resource Enforcement | 3 | Phases 9–11 |
| 13 | gRPC Request Routing | 4 | Phases 9–12 |
| 14 | Deployment Infrastructure | 6 | Phase 13 |
| 15 | Storage Enhancements | 4 | Phase 12 |
| 16 | Agent Migration | 4 | Phases 13–14 |
| 17 | End-to-End Testing + Packaging | 4 | Phases 9–16 |
| 18 | Highway UI | 4 | Phase 13 |
| | **Total** | **42 tasks** | |

**42 coding specs. 42 tasks. 10 phases.** Combined with the initial 28 tasks (Phases 0–8): **70 tasks total across 19 phases.**

Each task follows the same workflow as Phases 0–8: coding spec → review → implement → tests pass → commit. Coding specs live in `specs/coding/`.

---

## Repository Structure (Actual)

```
wacp/
├── IMPLEMENTATION.md           # This file — decisions and tracking
├── SPEC-STRATEGY.md            # Phased plan — 28 tasks, all complete
├── SEED-CONTEXT.md             # Session primer
├── Cargo.toml                  # Workspace manifest — 12 crates
├── .github/workflows/ci.yml    # CI: build, clippy, test, fmt, proto
├── .gitignore
│
├── protocol/                    # The specification layer
│   ├── PROTOCOL.md             # Authoritative protocol spec
│   ├── TAXONOMY.md             # Extension registry
│   ├── primitives/             # 8 constituent specs
│   ├── foundations/             # 2 constituent specs
│   ├── mechanisms/              # 4 constituent specs
│   └── topology/                # 6 constituent specs
│
├── impl/                        # Implementation specs
│   ├── runtime.md              # Complete — implemented
│   ├── storage.md              # Complete — core implemented
│   ├── protocol-interface.md   # Complete — implemented
│   ├── sdk-agent.md            # Complete — implemented
│   ├── highway-ui.md           # Complete
│   ├── deployment.md           # Complete
│   ├── migration.md            # Complete
│   ├── topology.md             # Complete
│   ├── task-scheduling.md      # Complete
│   └── integration.md          # Complete
│
├── proto/                       # Protobuf definitions (shared contract)
│   ├── primitives.proto
│   ├── agent.proto
│   ├── highway.proto
│   └── taxonomy.proto
│
├── specs/coding/                # 28 coding specs (all complete)
│
├── crates/                      # Rust implementation (12 crates)
│   ├── wacp-types/
│   ├── wacp-clock/
│   ├── wacp-fsm/
│   ├── wacp-taxonomy/
│   ├── wacp-permissions/
│   ├── wacp-trail/
│   ├── wacp-workspace/
│   ├── wacp-coordinator/
│   ├── wacp-transport/
│   ├── wacp-recovery/
│   ├── wacp-runtime/
│   └── wacp-sdk/
│
└── sdk-python/                  # Python agent SDK
    ├── pyproject.toml
    ├── src/wacp/
    └── tests/
```

---

*WACP implementation journal — Akil Abderrahim and Claude Opus 4.6*
