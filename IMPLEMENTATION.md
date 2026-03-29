# WACP — Implementation Plan

```yaml
created: 2026-03-28
status: active
authors:
  - Akil Abderrahim
  - Claude Opus 4.6
```

---

## Current State

**Phases 0–18 complete. Phase 19.1 complete.** 12 Rust crates, 687 Rust tests, 14 Python tests, 21 TypeScript tests. Runtime binary, coordinator decision engine, agent migration, Dockerfile, systemd unit. Highway UI scaffold with gRPC-Web transport, Zustand store, and all panel components. All coding specs archived.

Test counts by crate:

| Crate | Tests | Modules | Role |
|-------|-------|---------|------|
| wacp-types | 39 | 11 | Protocol enums, ID newtypes, structs |
| wacp-clock | 28 | 4 | HLC timestamps |
| wacp-fsm | 50 | 4 | Workspace/envelope/task FSMs |
| wacp-taxonomy | 36 | 5 | YAML/JSON taxonomy loader |
| wacp-permissions | 38 | 2 | Permission matrix, port rights |
| wacp-trail | 78 | 12 | Trail storage, snapshots, tiered storage |
| wacp-workspace | 44 | 3 | Workspace actor, 9 components |
| wacp-coordinator | 279 | 15 | Decision engine, migration, E2E |
| wacp-transport | 25 | 10 | gRPC, auth, rate limiter |
| wacp-recovery | 14 | 2 | Trail replay, snapshot recovery |
| wacp-runtime | 53 | 9 | Config, CLI, TLS, metrics, health |
| wacp-sdk | 3 | 6 | Rust agent SDK |
| **Total** | **687** | **83** | |

---

## Coverage Audit (2026-03-28)

Systematic review of all 83 modules against their test suites. Gaps ranked by risk.

### Critical — Low coverage, high complexity

| Crate | Gap | Risk |
|-------|-----|------|
| wacp-types | 12 struct types lack serde roundtrip tests; TrailEntry, GateEvent, EscalationEvent untested | Serialization bugs propagate to every crate |
| wacp-workspace | 10 of 16 CoordinatorCommand handlers untested in actor context (Suspend, Resume, GrantVisibility, UpdateBudget, GracefulTermination, all 5 integration/conflict commands) | Coordinator sends commands the actor has never been tested to handle |
| wacp-transport | PskAuthenticator.authenticate_agent/human — zero tests; AuthRateLimiter — zero tests; gRPC handlers untested | Auth bypass or rate limiter failure in production |
| wacp-recovery | recover_with_snapshot() — only empty trail tested; parse_workspace_state/parse_task_status — untested string matching; corrupted snapshot fallback not verified | Incorrect recovery after crash |

### Important — Moderate coverage, missing edge cases

| Crate | Gap | Risk |
|-------|-----|------|
| wacp-trail | TierManager transitions (hot→warm, warm→cold) error paths; FileSnapshotStorage corruption edge cases; compaction module untested | Data loss on tier transition or compaction |
| wacp-coordinator | Handler error paths (PermissionDenied, ValidationFailed); integration merge error paths; migration rollback with concurrent abort | Incorrect error handling under load |
| wacp-permissions | Derived role inheritance chain; coordinator-only capability enforcement; human-origin bypass scope | Permission escalation |
| wacp-clock | Physical overflow at u64::MAX; logical overflow in recv(); SystemTimeSource error path | Clock regression after long uptime |

### Acceptable — Good coverage, minor gaps

| Crate | Gap | Risk |
|-------|-----|------|
| wacp-fsm | All transitions tested; missing concurrent transition stress test | Low — FSM is pure functions |
| wacp-taxonomy | Version mismatch handling; reserved name collision edge cases | Low — validated at startup |

---

## Phase 18 — Coverage Hardening

Goal: bring every crate to near-complete branch coverage. Two sub-phases by crate boundary.

### Phase 18a — Core Crates

Pure logic crates with no IO dependencies. Tests are fast and deterministic.

| # | Task | Crate | Tests to add | Target |
|---|------|-------|-------------|--------|
| 18a.1 | Type serde roundtrips | wacp-types | Serde roundtrip for all 12 struct types (Envelope, Checkpoint, Signal, TrailEntry, GateEvent, EscalationEvent, ProtocolError, ResourceUsage, ResourceBudget, Task, PortRight, Originator). Default value verification. Display/Debug impl coverage. Empty/zero/max boundary values. | Every public type has a serialize→deserialize test |
| 18a.2 | FSM exhaustive coverage | wacp-fsm | Exhaustive transition table: every (state, trigger) pair tested — valid transitions return correct state, invalid transitions return IllegalTransition. Property: terminal states reject all triggers. Property: no transition produces an undefined state. | 100% of the transition table |
| 18a.3 | Clock edge cases | wacp-clock | Physical overflow at u64::MAX. Logical overflow at u16::MAX in recv(). Timestamp::ZERO successor. Byte encoding at boundary values (0, MAX). send() explicit test. | All arithmetic edge cases covered |
| 18a.4 | Permission hardening | wacp-permissions | Derived role inheritance (extends + add/remove capabilities). Coordinator-only capability enforcement (no derived role acquires create_workspace, perform_integration). Human-origin bypass scope (does NOT bypass signal/checkpoint permissions). Port rights: SendOnce consumption prevents reuse. | All permission paths exercised |
| 18a.5 | Taxonomy edge cases | wacp-taxonomy | Version mismatch (taxonomy.protocol_version ≠ runtime). Duplicate envelope type name. Reserved name collision in derived roles. Empty taxonomy (no custom types). Malformed YAML/JSON (syntax errors). | All validation checks tested |
| 18a.6 | Workspace command coverage | wacp-workspace | Actor tests for: Suspend → Suspended, Resume → Active, GrantVisibility (verify additive), UpdateBudget (verify replacement), GracefulTermination (placeholder behavior). IntegrationSucceeded/IntegrationFailed/ConflictDetected/ConflictResolved/ConflictUnresolvable actor transitions. Snapshot roundtrip: capture → serialize → deserialize → restore → verify all fields. pop_inbox on empty. Archive from Closed (not just Failed). | Every CoordinatorCommand variant tested in actor context |
| 18a.7 | Coordinator error paths | wacp-coordinator | Handler: bind to non-existent workspace. send_envelope with revoked port right. emit_signal for non-existent workspace. create_checkpoint for non-existent workspace. inject_envelope to non-existent target. gate_response for unknown gate. Migration: start with non-existent workspace in tree. bind with correct identity but workspace not in Migrating state. Topology: create_workspace with duplicate ID. terminate_workspace cascade with deep tree (3+ levels). Port rights: transfer expired right. consume non-SendOnce. | All error branches exercised |

**Depends on:** Nothing — all pure logic.
**Exit criteria:** `cargo test` for wacp-types, wacp-clock, wacp-fsm, wacp-taxonomy, wacp-permissions, wacp-workspace, wacp-coordinator all pass. Every public method has at least one test. Every error variant is triggered by at least one test.

### Phase 18b — Boundary Crates

Crates with IO, network, or filesystem dependencies. Some require protoc.

| # | Task | Crate | Tests to add | Target |
|---|------|-------|-------------|--------|
| 18b.1 | Trail error paths | wacp-trail | FileTrailStorage: append to read-only dir → error. Corrupted segment header → detected on scan. Segment rotation at exact max size. FileCheckpointStorage: read nonexistent hash → None. Store duplicate hash → idempotent. FileSnapshotStorage: partial file (< 32 bytes) → error. Zero-byte file → error. TierManager: hot→warm with recovery window anchor blocking. Compaction: merge two warm segments. Delete with trail entry recorded first. | All error paths return correct errors |
| 18b.2 | Auth + rate limiter | wacp-transport | PskAuthenticator: register_agent → authenticate → success. Invalid token → InvalidToken. Valid token, wrong workspace → WorkspaceMismatch. revoke_agent → authenticate → InvalidToken. register_human → authenticate → success. AuthRateLimiter: under limit → Ok. At limit → RateLimited. Window expiry → Ok again. Disabled (0) → always Ok. | Auth and rate limiter fully tested |
| 18b.3 | Recovery paths | wacp-recovery | recover_with_snapshot: valid snapshot + trail delta → correct state. Corrupted snapshot JSON → fallback to full replay (verify same result). Snapshot sequence in middle of trail → replay from anchor. Empty trail + valid snapshot → snapshot state used. Multiple workspace state changes → last state wins. Migration interrupted (migration_started, no migration_completed) → workspace Failed. | All recovery scenarios produce correct state |
| 18b.4 | Runtime config + CLI | wacp-runtime | RuntimeConfig: all 47 fields have defaults. Environment variable overrides (WACP_SERVER__AGENT_LISTEN). Unknown field → deny_unknown_fields error. Validation: TLS enabled but cert missing → error. CLI: `serve` default subcommand. `validate` with valid config → exit 0. `validate` with invalid config → exit 1. `defaults` prints YAML. | Config validation complete, CLI paths tested |

**Depends on:** Phase 18a. 18b.2 and 18b.4 require protoc for compilation.
**Exit criteria:** All tests pass. Error paths return correct error types. No silent failure — every IO error is either handled or propagated.

---

## Phase 19 — Highway UI

TypeScript SPA for human-in-the-loop interaction. Separate project, gRPC-Web client.

| # | Task | Output | Spec source |
|---|------|--------|-------------|
| 19.1 | TypeScript scaffold | Vite + React 19, `@bufbuild/protobuf` + `@connectrpc/connect-web` codegen from `.proto` files, gRPC-Web transport layer, Zustand store (6 slices), Tailwind CSS, Vitest (21 tests), dev server with proxy, production build (static files). All panel components with initial implementations. | highway-ui.md §2–4 |
| 19.2 | Trail viewer + workspace tree | Real-time trail streaming via `StreamTrail`, filtering by workspace/event type/time range, workspace tree visualization, workspace detail view (state, role, directive, checkpoint register, resource meter) | highway-ui.md §5–8 |
| 19.3 | Gate + escalation management | Gate event stream via `StreamGates`, approval/reject/modify UI, escalation event stream, escalation response UI, notification system for pending actions | highway-ui.md §9–11 |
| 19.4 | Envelope injection + autonomy | Injection form (target workspace, envelope type, payload, priority), validation, autonomy presets (full-auto, supervised, manual), preset switching at run-time | highway-ui.md §12–14 |

**Depends on:** Phase 13 (highway gRPC). Independent of Phase 18 (testing).
**Exit criteria:** UI connects to runtime via gRPC-Web, streams trail in real time, displays workspace tree, handles gate approvals and escalations, allows envelope injection. Static build deployable independently.

---

## Summary

| Phase | Name | Tasks | Depends on | Status |
|-------|------|-------|------------|--------|
| 18a | Coverage: Core Crates | 7 | — | **Complete** |
| 18b | Coverage: Boundary Crates | 4 | 18a | **Complete** |
| 19 | Highway UI | 4 | Phase 13 | **19.1 complete**, 19.2–19.4 next |
| | **Total** | **15** | | |

---

*WACP implementation plan — Akil Abderrahim and Claude Opus 4.6*
