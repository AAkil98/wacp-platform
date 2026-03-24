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

| Work item | Specced? | Spec source | Action |
|-----------|----------|-------------|--------|
| Full gRPC request routing | Yes | protocol-interface.md §4–5, §8–9; runtime.md §3, §9, §10 | Code — wire RPCs to coordinator |
| Timeout/budget enforcement | Yes | runtime.md §12 | Code — FuturesUnordered timers |
| Tiered storage (hot/warm/cold) | Yes | storage.md §8–9 | Code — compression, tier transitions |
| System snapshots | Yes | storage.md §7 | Code — snapshot procedure, recovery integration |
| Agent migration | Yes | migration.md §2–12, workspace spec §11, PROTOCOL.md §6.9 | Code — migration procedure |
| Highway UI | Yes | highway-ui.md | Code — TypeScript SPA |
| Production deployment | Yes | deployment.md §1–13 | Code — config, CLI, Docker, systemd |
| Coordinator integration | Yes | integration.md §2–11 | Code — merge strategies, conflict detection/resolution |
| Task scheduling | Yes | task-scheduling.md §2–11 | Code — dispatch, gates, retry, resource allocation |
| Topology operations | Yes | topology.md §2–10 | Code — tree, graph, visibility, ownership, causation, port rights |

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
