# WACP — Test Strategy

```yaml
created: 2026-03-30
status: active
authors:
  - Akil Abderrahim
  - Claude Opus 4.6
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Current State Audit](#2-current-state-audit)
3. [Testing Principles](#3-testing-principles)
4. [Unit Test Specification — Rust](#4-unit-test-specification--rust)
5. [Unit Test Specification — TypeScript](#5-unit-test-specification--typescript)
6. [Unit Test Specification — Python](#6-unit-test-specification--python)
7. [Integration Test Specification](#7-integration-test-specification)
8. [End-to-End Test Specification](#8-end-to-end-test-specification)
9. [CI Pipeline](#9-ci-pipeline)
10. [Phased Execution Plan](#10-phased-execution-plan)

---

## 1. Purpose

This document defines the comprehensive testing strategy for WACP. It specifies what to test, at what level, and why — across all three language ecosystems (Rust runtime, TypeScript highway UI, Python agent SDK), all 12 Rust crates, all cross-crate boundaries, and all cross-language interfaces. The goal is near-complete branch coverage for every module and verified correctness for every protocol interaction.

**Scope.** Every public function, every trait implementation, every RPC handler, every React component, every Python SDK method. Unit tests for isolated logic. Integration tests for cross-boundary interactions. End-to-end tests for full-system protocol flows.

**Not in scope.** Performance benchmarks, load testing, security penetration testing, UI visual regression testing. These require dedicated infrastructure and are separate efforts.

---

## 2. Current State Audit

### 2.1 Test Counts by Ecosystem

| Ecosystem | Module | Tests | Status |
|-----------|--------|-------|--------|
| **Rust** | wacp-types | 39 | Good |
| | wacp-clock | 28 | Good |
| | wacp-fsm | 50 | Good |
| | wacp-taxonomy | 36 | Good |
| | wacp-permissions | 38 | Good |
| | wacp-trail | 78 | Good |
| | wacp-workspace | 44 | Moderate |
| | wacp-coordinator | 279 | Good |
| | wacp-transport | 25 | **Poor** |
| | wacp-recovery | 14 | Moderate |
| | wacp-runtime | 53 | **Poor** |
| | wacp-sdk | 3 | **Critical** |
| | **Subtotal** | **687** | |
| **TypeScript** | store | 25 | Good |
| | transport (errors, streams) | 18 | Moderate |
| | transport (client, session, rpcs) | 0 | **Critical** |
| | notifications | 4 | Moderate |
| | components (trail, gates, escalations, workspaces, injection, settings) | 58 | Good |
| | components (layout, tasks, checkpoint) | 0 | **Poor** |
| | **Subtotal** | **105** | |
| **Python** | types | 13 | Good |
| | agent | 1 | **Critical** |
| | **Subtotal** | **14** | |
| | **Grand total** | **806** | |

### 2.2 Coverage Gaps Ranked by Risk

| Priority | Module | Gap | Risk |
|----------|--------|-----|------|
| **P0** | wacp-sdk | 99% of Agent API untested — all builder methods, all async methods, all stream types | Agents are the primary consumers; a broken SDK blocks all users |
| **P0** | wacp-transport | gRPC RPC handlers untested — AgentServiceImpl (8 RPCs), HighwayServiceImpl (12 RPCs), TLS, auth | Runtime is unreachable if transport is broken |
| **P0** | sdk-python | Agent methods untested — signal, checkpoint, envelope, inbox, commands, disconnect | Python agents are the primary integration path |
| **P0** | CI pipeline | TypeScript and Python tests absent from CI | Tests exist but don't run; regressions go undetected |
| **P1** | wacp-runtime | RuntimeConfig (47 fields) parsing/validation untested; health, metrics, TLS init untested | Configuration errors crash production |
| **P1** | highway-ui transport | SessionManager, client init, RPC wrappers — 0 tests | Session drops, reconnection failures go undetected |
| **P1** | highway-ui layout | MainLayout, LoginScreen, Sidebar, ConnectionBanner — 0 tests | Broken navigation or login blocks all UI access |
| **P2** | wacp-coordinator | RequestHandler signal/envelope handling, port rights enforcement, migration coordination | Incorrect routing or permission bypass under specific conditions |
| **P2** | wacp-trail | Compaction lifecycle, tiered storage fallback, concurrent writes | Data loss on long-running deployments |
| **P2** | wacp-recovery | Snapshot corruption fallback, partial trail corruption, large trail replay | Recovery fails after crash |
| **P3** | wacp-workspace | WorkspaceActor async behavior, concurrent envelope processing | Race conditions under load |
| **P3** | wacp-clock | TimeSource edge cases, clock at u64::MAX | Extremely rare boundary conditions |

### 2.3 Interface Boundaries Without Integration Tests

No workspace-level `tests/` directory exists. All tests are crate-internal. The following cross-crate boundaries have no dedicated integration test:

| Boundary | Provider → Consumer | Interface |
|----------|---------------------|-----------|
| Transport ↔ Runtime | wacp-transport → wacp-runtime | gRPC channels (AgentRequest, HighwayRequest) |
| Runtime ↔ Coordinator | wacp-coordinator → wacp-runtime | RequestHandler dispatch |
| Coordinator ↔ Workspace | wacp-workspace → wacp-coordinator | MPSC channels (CoordinatorCommand, WorkspaceEvent) |
| Recovery ↔ Trail | wacp-trail → wacp-recovery | TrailStorage + SnapshotStorage read |
| Permissions ↔ Taxonomy | wacp-taxonomy → wacp-permissions | Taxonomy → PermissionEngine construction |
| SDK ↔ Transport | wacp-transport → wacp-sdk | gRPC client ↔ server |
| Highway UI ↔ Runtime | TypeScript → Rust | gRPC-Web ↔ HighwayService |
| Python SDK ↔ Runtime | Python → Rust | gRPC ↔ AgentService |

---

## 3. Testing Principles

**T-1. Every public symbol has at least one test.** Public functions, trait implementations, struct methods, enum variants, error types. If it's `pub`, it's tested.

**T-2. Every error path is exercised.** Every `Err` variant, every `None` return, every validation rejection. Tests don't just verify the happy path — they verify the system says no when it should.

**T-3. Tests are deterministic.** No reliance on wall-clock time, network availability, or filesystem ordering. Use `ManualTimeSource` for clocks, `InMemoryTrailStorage` for storage, `InProcessTransport` for networking. Flaky tests are bugs.

**T-4. Tests are fast.** Unit tests complete in <1ms each. Integration tests complete in <100ms each. E2E tests complete in <5s each. Slow tests get optimized or moved to a separate long-running suite.

**T-5. Test what the protocol promises.** The protocol spec defines invariants (PROTOCOL.md). Tests verify those invariants, not implementation details. When the implementation changes, protocol-level tests should still pass.

**T-6. Three test levels with clear boundaries.**

| Level | Scope | Dependencies | Runs in CI |
|-------|-------|-------------|------------|
| Unit | Single module, single function | None (mocks/stubs for external deps) | Yes, every push |
| Integration | Cross-boundary interaction | Real implementations across 2+ crates | Yes, every push |
| E2E | Full system, all three languages | Running runtime + gRPC | Yes, merge to main |

**T-7. Coverage targets.**

| Level | Target |
|-------|--------|
| Unit — core crates (types, fsm, clock, permissions, taxonomy) | 95% branch coverage |
| Unit — actor crates (workspace, coordinator) | 90% branch coverage |
| Unit — boundary crates (trail, transport, recovery, runtime) | 85% branch coverage |
| Unit — SDK crates (sdk, sdk-python) | 90% method coverage |
| Unit — highway-ui | 90% component coverage |
| Integration | Every cross-crate boundary exercised |
| E2E | Every protocol flow exercised |

---

## 4. Unit Test Specification — Rust

### 4.1 wacp-types (39 existing → target: 45)

| Module | Tests to add | Target |
|--------|-------------|--------|
| Display/Debug impls | Verify formatting for all ID types, all enum Display | 3 tests |
| Default impls | Verify default values for all structs that derive Default | 3 tests |

### 4.2 wacp-clock (28 existing → target: 33)

| Module | Tests to add | Target |
|--------|-------------|--------|
| `Clock::last()` | Direct accessor test | 1 test |
| `SystemTimeSource` | Test that `now_micros()` returns non-zero, monotonic | 2 tests |
| Clock at max time | Initialize at u64::MAX - 1, advance, verify saturation | 2 tests |

### 4.3 wacp-fsm (50 existing → target: 55)

| Module | Tests to add | Target |
|--------|-------------|--------|
| TransitionError Display | Verify error message formatting | 1 test |
| Exhaustive (state, trigger) matrix | Property test: every pair is either valid or returns IllegalTransition | 4 tests (one per FSM + combined) |

### 4.4 wacp-taxonomy (36 existing → target: 42)

| Module | Tests to add | Target |
|--------|-------------|--------|
| `is_valid_envelope_type()` | Registered type → true, unregistered → false | 2 tests |
| `is_valid_checkpoint_type()` | Same pattern | 2 tests |
| Error paths | Duplicate role name, duplicate envelope type, invalid base role | 2 tests |

### 4.5 wacp-permissions (38 existing → target: 45)

| Module | Tests to add | Target |
|--------|-------------|--------|
| Port right expiration | Grant with TTL, check after expiry | 2 tests |
| Coordinator-only capabilities | Derived role cannot acquire create_workspace, perform_integration | 2 tests |
| Human-origin bypass scope | Bypass applies to envelope delivery, NOT to signal/checkpoint permissions | 2 tests |
| Negative port right path | `has_send_right` on never-granted workspace pair → false | 1 test |

### 4.6 wacp-trail (78 existing → target: 90)

| Module | Tests to add | Target |
|--------|-------------|--------|
| Compaction | Merge two warm segments, verify entries preserved | 3 tests |
| Tiered storage fallback | Hot→warm with recovery window blocking, verify data accessible | 3 tests |
| Index rebuild | Corrupt index, trigger rebuild, verify queries work | 2 tests |
| Concurrent append | Two writers (tokio tasks), verify no data loss | 2 tests |
| Large payload | 10 MB checkpoint store + read, verify hash | 1 test |
| Zero-byte file corruption | Snapshot storage reads zero-byte file → error | 1 test |

### 4.7 wacp-workspace (44 existing → target: 60)

| Module | Tests to add | Target |
|--------|-------------|--------|
| WorkspaceActor command handlers | Suspend→Suspended, Resume→Active, GrantVisibility, UpdateBudget, GracefulTermination — in async actor context | 5 tests |
| Integration commands | IntegrationSucceeded, IntegrationFailed, ConflictDetected, ConflictResolved, ConflictUnresolvable — actor transitions | 5 tests |
| Resource budget enforcement | Budget exhaustion → workspace fails | 2 tests |
| Concurrent envelope delivery | Two envelopes delivered simultaneously, both appear in inbox | 2 tests |
| Archive from Closed | Workspace in Closed state → archive produces ArchivedWorkspace | 1 test |
| Context immutability | Attempt to modify context after checkpoint → rejected | 1 test |

### 4.8 wacp-coordinator (279 existing → target: 310)

| Module | Tests to add | Target |
|--------|-------------|--------|
| `RequestHandler.handle_signal()` | Signal routing for all 11 signal types | 5 tests |
| `RequestHandler.handle_send_envelope()` | Permission check, routing, port rights enforcement | 5 tests |
| Port rights cross-workspace | Send with valid right → delivered; send with revoked right → denied; send_once consumed → second fails | 3 tests |
| Migration lifecycle | Start → snapshot → complete (happy); start → timeout → fail (sad) | 4 tests |
| Integration queue | Queue 3 integrations, process sequentially, verify order | 3 tests |
| Dispatch error paths | Dispatch to non-existent task, dispatch with exhausted budget | 2 tests |
| Deep cascade | 4-level tree, root fails, verify all descendants fail in order | 2 tests |
| Gate timeout | Gate created, timeout fires, fallback executes | 2 tests |
| Concurrent workspace creation | 10 workspaces spawned simultaneously, all get unique IDs | 2 tests |
| EventBus | Subscribe, receive event, unsubscribe, verify no more events | 3 tests |

### 4.9 wacp-transport (25 existing → target: 70)

| Module | Tests to add | Target |
|--------|-------------|--------|
| `AgentServiceImpl` — Bind | Valid token → success; invalid token → UNAUTHENTICATED; wrong workspace → NOT_FOUND | 3 tests |
| `AgentServiceImpl` — SendEnvelope | Valid → envelope_id; revoked port → PERMISSION_DENIED | 2 tests |
| `AgentServiceImpl` — EmitSignal | Valid signal → ok; invalid type → INVALID_ARGUMENT | 2 tests |
| `AgentServiceImpl` — CreateCheckpoint | Valid → checkpoint_id + content_hash | 2 tests |
| `AgentServiceImpl` — QueryTrail | With filters, empty result, limit enforcement | 3 tests |
| `AgentServiceImpl` — ReceiveEnvelopes | Stream delivers envelopes, closes on workspace close | 2 tests |
| `AgentServiceImpl` — ReceiveCommands | Stream delivers commands, closes on disconnect | 2 tests |
| `HighwayServiceImpl` — Authenticate | Valid → user_id + capabilities; invalid → UNAUTHENTICATED | 2 tests |
| `HighwayServiceImpl` — InjectEnvelope | Valid → envelope_id; terminal workspace → FAILED_PRECONDITION | 2 tests |
| `HighwayServiceImpl` — RespondToGate | Approve → applied; already resolved → not applied | 2 tests |
| `HighwayServiceImpl` — RespondToEscalation | Feedback, abort, delegate — all three action types | 3 tests |
| `HighwayServiceImpl` — GetWorkspace | Existing → view; non-existent → NOT_FOUND | 2 tests |
| `HighwayServiceImpl` — GetTaskGraph | Returns current graph | 1 test |
| `HighwayServiceImpl` — GetCheckpoint | Existing → payload; non-existent → NOT_FOUND | 2 tests |
| `HighwayServiceImpl` — StreamTrail | Receives live entries; from_beginning replays history | 2 tests |
| `HighwayServiceImpl` — StreamGates | Receives gate events | 1 test |
| `HighwayServiceImpl` — StreamEscalations | Receives escalation events | 1 test |
| `HighwayServiceImpl` — StreamWorkspaceChanges | Receives state changes | 1 test |
| PskAuthenticator | Register → authenticate → success; revoke → fail; wrong workspace → mismatch | 3 tests |
| AuthRateLimiter | Under limit → Ok; at limit → RateLimited; window expiry → Ok again | 3 tests |
| TLS | Load valid certs → ok; missing cert → error | 2 tests |
| Error mapping | All ErrorCategory variants → correct gRPC Code | 1 test (existing, verify) |
| Proto roundtrip | Serialize → deserialize for key message types (Envelope, Signal, Checkpoint) | 3 tests |

### 4.10 wacp-recovery (14 existing → target: 25)

| Module | Tests to add | Target |
|--------|-------------|--------|
| Corrupted snapshot JSON | Invalid JSON → fallback to full replay, same result | 2 tests |
| Partial trail corruption | Valid entries followed by garbage → recovery stops at last valid | 2 tests |
| Migration interrupted | migration_started without migration_completed → workspace Failed | 1 test |
| Multiple workspace states | 5 workspaces, interleaved events → each state correct | 2 tests |
| Large trail | 10,000 entries → recovery completes, state correct | 1 test |
| Clock recovery edge | Empty trail → clock at ZERO; single entry → clock at that timestamp + 1 | 2 tests |
| In-flight edge cases | Envelope created and delivered in same batch → not in-flight | 1 test |

### 4.11 wacp-runtime (53 existing → target: 85)

| Module | Tests to add | Target |
|--------|-------------|--------|
| RuntimeConfig — YAML loading | Valid YAML → all 47 fields populated; missing file → error | 3 tests |
| RuntimeConfig — env overrides | WACP_SERVER__AGENT_LISTEN overrides file value | 2 tests |
| RuntimeConfig — validation | TLS enabled but cert missing → error; port out of range → error | 3 tests |
| RuntimeConfig — deny unknown | Unknown field in YAML → error | 1 test |
| CLI — validate | Valid config → exit 0; invalid → exit 1 | 2 tests |
| CLI — defaults | Prints valid YAML | 1 test |
| Health | Start health server, GET /health → 200 with status | 3 tests (Starting, Ready, Draining) |
| Metrics | Start metrics server, GET /metrics → Prometheus text format | 2 tests |
| Logging | JSON format init, pretty format init | 2 tests |
| TLS | Load valid cert+key → ok; invalid cert → error; mTLS with client cert | 3 tests |
| Config merge | File defaults + env override + CLI flag — priority order | 2 tests |
| Shutdown | Graceful shutdown → all streams closed, health → Draining | 2 tests |
| Recovery integration | Crash scenario → restart → state matches pre-crash | 3 tests |
| Multi-worker | 3 workers dispatched → all bind → all produce checkpoints | 3 tests |

### 4.12 wacp-sdk (3 existing → target: 50)

| Module | Tests to add | Target |
|--------|-------------|--------|
| Agent properties | workspace_id, directive, context, role, visibility, authority — all return correct values after bind | 6 tests |
| Agent.signal() | All 11 signal types → RPC succeeds | 5 tests |
| Agent.signal_blocked() | Convenience method → emits BLOCKED signal | 1 test |
| Agent.signal_failed() | Convenience method → emits FAILED signal | 1 test |
| Agent.signal_escalation() | Convenience method → emits ESCALATION signal | 1 test |
| Agent.checkpoint() | Builder creates checkpoint, returns id + hash | 2 tests |
| CheckpointBuilder | All 7 builder methods chain correctly; create() sends RPC | 3 tests |
| Agent.send_envelope() | Builder sends envelope, returns id | 2 tests |
| EnvelopeBuilder | All 7 builder methods chain correctly; send() sends RPC | 3 tests |
| Agent.inbox() | Stream receives delivered envelopes | 2 tests |
| Agent.commands() | Stream receives coordinator commands | 2 tests |
| Agent.query_trail() | Returns trail entries matching filter | 2 tests |
| Agent.disconnect() | Clean disconnect, channels closed | 1 test |
| Error handling | Connect to wrong address → error; signal on closed agent → error; checkpoint with missing payload → error | 3 tests |
| Concurrent operations | Signal + checkpoint simultaneously → both succeed | 2 tests |
| Reconnection | Server restart → agent detects disconnect | 1 test |

All SDK tests use `InProcessTransport` with a test coordinator for deterministic behavior.

---

## 5. Unit Test Specification — TypeScript

### 5.1 Store (25 existing → target: 30)

| Module | Tests to add | Target |
|--------|-------------|--------|
| Notification slice | setEscalationBanner, dismissEscalationBanner | 2 tests |
| Edge cases | appendTrailEntry at exact cap boundary; resolveGate for non-existent gate | 3 tests |

### 5.2 Transport (18 existing → target: 40)

| Module | Tests to add | Target |
|--------|-------------|--------|
| `client.ts` | initTransport sets transport; getClient before init → throws; getTransport before init → throws | 3 tests |
| `session.ts` | connect → sets session connected; disconnect → sets disconnected; connect with invalid token → disconnected; reconnection increments retry count; max retries → disconnected | 5 tests (mock gRPC client) |
| `rpcs.ts` | respondToGate generates UUID; respondToEscalation abort; respondToEscalation delegate; injectEnvelope success; injectEnvelope failure | 5 tests (mock gRPC client) |
| `streams.ts` (existing) | Add: stream abort mid-iteration; workspace change updates existing view | 2 tests |
| Proto conversion edge cases | toTrailEntry with empty body; toWorkspaceView with all optional fields missing | 2 tests (verify existing + add) |
| Error classification completeness | Every ConnectError code mapped | 5 tests (extend existing) |

### 5.3 Components (58 existing → target: 85)

| Component | Tests to add | Target |
|-----------|-------------|--------|
| `MainLayout` | Renders sidebar + outlet; no crash on empty state | 2 tests |
| `LoginScreen` | Renders token input; calls onLogin with token; shows error; shows loading | 4 tests |
| `Sidebar` | Renders all 7 nav items; active item highlighted; badge counts for gates/escalations | 3 tests |
| `ConnectionBanner` | Shows reconnecting banner; shows disconnected banner; hides when connected; shows escalation banner; dismiss button works | 5 tests |
| `TaskGraphView` | Renders task nodes; shows status colors; renders dependencies; empty state | 4 tests |
| `CheckpointViewer` | Renders metadata fields; renders text payload; renders hex dump for binary; shows verified badge; shows loading; shows error | 5 tests |
| `App` (routing) | Default route → /trail; /workspaces/:id routes to detail panel; unknown route → redirect | 3 tests |
| Existing components | Extend: GatePanel modify editor opens; EscalationPanel send feedback navigates; TrailViewer clear filters resets | 1 test each (3 total) |

### 5.4 Notifications (4 existing → target: 8)

| Module | Tests to add | Target |
|--------|-------------|--------|
| Browser notification | notifyGate calls Notification when tab not focused; respects permission denied | 2 tests |
| Escalation double-tone | notifyEscalation calls createOscillator twice (for double tone) | 1 test |
| No AudioContext | Graceful fallback when AudioContext unavailable | 1 test |

---

## 6. Unit Test Specification — Python

### 6.1 types.py (13 existing → target: 20)

| Module | Tests to add | Target |
|--------|-------------|--------|
| Signal.to_proto() | All 11 signal types → correct proto enum value | 3 tests (extend existing) |
| Priority.to_proto() | All 3 priorities → correct proto enum value | 1 test (extend existing) |
| Round-trip | Proto enum → Signal/Priority → proto enum | 3 tests |

### 6.2 agent.py (1 existing → target: 35)

All tests use a mock gRPC channel (no real server).

| Method | Tests to add | Target |
|--------|-------------|--------|
| `Agent.connect()` | Success → sets workspace state; failure → raises | 2 tests |
| `Agent.workspace_id` | Returns bound workspace ID | 1 test |
| `Agent.directive` | Returns task directive from bind response | 1 test |
| `Agent.context` | Returns workspace context | 1 test |
| `Agent.role` | Returns assigned role | 1 test |
| `Agent.visibility` | Returns visibility list | 1 test |
| `Agent.authority` | Returns authority list | 1 test |
| `Agent.signal()` | Each of 11 types → RPC called with correct proto | 5 tests |
| `Agent.signal_blocked()` | Convenience → BLOCKED signal | 1 test |
| `Agent.signal_failed()` | Convenience → FAILED signal | 1 test |
| `Agent.signal_escalation()` | Convenience → ESCALATION signal | 1 test |
| `Agent.checkpoint()` | Builder → RPC → returns id | 2 tests |
| `Agent.send_envelope()` | Builder → RPC → returns id | 2 tests |
| `Agent.inbox` | Yields envelopes from stream | 2 tests |
| `Agent.commands` | Yields commands from stream | 2 tests |
| `Agent.query_trail()` | Returns filtered entries | 2 tests |
| `Agent.disconnect()` | Closes channels | 1 test |
| Error handling | Signal on disconnected agent; checkpoint with empty payload | 2 tests |
| Async iteration | inbox stops on disconnect; commands stops on disconnect | 2 tests |

### 6.3 proto/v1.py (0 existing → target: 5)

| Module | Tests to add | Target |
|--------|-------------|--------|
| Message construction | BindRequest, EmitSignalRequest, CreateCheckpointRequest | 3 tests |
| Enum accessibility | All proto enums importable and have expected values | 2 tests |

---

## 7. Integration Test Specification

Integration tests verify that two or more components work correctly together across crate/module boundaries. They use real implementations (not mocks) for the components under test, but may mock external dependencies (filesystem, network).

### 7.1 Rust Cross-Crate Integration Tests

Location: `tests/` directory at workspace root (new).

| Test Suite | Crates Involved | Scenarios | Tests |
|------------|----------------|-----------|-------|
| **trail_recovery** | wacp-trail + wacp-recovery | Append entries → recover → state matches; append + snapshot → recover from snapshot; corrupted trail → partial recovery | 5 |
| **taxonomy_permissions** | wacp-taxonomy + wacp-permissions | Load taxonomy → build PermissionEngine → evaluate actions; derived role permissions; envelope permission matrix | 4 |
| **fsm_workspace** | wacp-fsm + wacp-workspace | Workspace lifecycle through all FSM states; illegal transitions rejected by actor; terminal states reject commands | 4 |
| **coordinator_workspace** | wacp-coordinator + wacp-workspace | Dispatch → actor spawns; coordinator command → actor transitions; workspace event → coordinator state update; cascade failure propagation | 6 |
| **coordinator_trail** | wacp-coordinator + wacp-trail | Event logging through coordinator; trail entries match coordinator state changes; gate events recorded | 4 |
| **transport_coordinator** | wacp-transport + wacp-coordinator | Agent bind via InProcessTransport → coordinator creates workspace; send envelope via transport → delivered to target workspace | 5 |
| **runtime_assembly** | wacp-runtime + all | Full initialization sequence; config load → taxonomy → trail → recovery → coordinator → transport; shutdown sequence | 4 |
| **sdk_transport** | wacp-sdk + wacp-transport | Agent connects via InProcessTransport; bind → signal → checkpoint → complete lifecycle; concurrent agents | 5 |
| **migration_e2e** | wacp-coordinator + wacp-workspace + wacp-fsm | Migration start → snapshot capture → agent swap → migration complete; migration failure → workspace Failed | 3 |
| **gate_lifecycle** | wacp-coordinator + wacp-transport | Gate created → highway user approves → workspace proceeds; gate timeout → fallback executes | 3 |
| | | **Total** | **43** |

### 7.2 TypeScript Integration Tests

Location: `highway-ui/src/__integration__/` (new directory).

| Test Suite | Modules Involved | Scenarios | Tests |
|------------|-----------------|-----------|-------|
| **store_transport** | store + transport (mocked gRPC) | Trail stream → store entries accumulate; workspace stream → tree updates; gate stream → pending map populated | 4 |
| **session_lifecycle** | session + store + client | Connect → 4 streams open; disconnect → streams aborted; reconnect → streams re-opened with fromBeginning=false | 3 |
| **injection_flow** | InjectionForm + rpcs + store | Fill form → submit → RPC called → success message → form resets | 2 |
| **gate_response_flow** | GatePanel + rpcs + store | Approve gate → RPC called → gate resolved → card disappears | 2 |
| **escalation_feedback** | EscalationPanel + InjectionForm + routing | Send Feedback → navigates to /inject?workspace=X&type=feedback → form pre-populated | 2 |
| | | **Total** | **13** |

### 7.3 Python Integration Tests

Location: `sdk-python/tests/integration/` (new directory).

| Test Suite | Modules Involved | Scenarios | Tests |
|------------|-----------------|-----------|-------|
| **agent_lifecycle** | agent + proto + mock server | Connect → bind → signal → checkpoint → complete → disconnect | 3 |
| **stream_handling** | agent + proto + mock server | Inbox receives envelopes; commands receives visibility grants; streams close on disconnect | 3 |
| **error_propagation** | agent + proto + mock server | Server returns error → agent raises appropriate exception | 2 |
| | | **Total** | **8** |

---

## 8. End-to-End Test Specification

E2E tests exercise the complete system across language boundaries. They start a real Rust runtime, connect real Python agents and/or a simulated TypeScript highway client, and verify protocol-level invariants.

**Infrastructure:** A test harness binary (`wacp-e2e`) that starts the runtime in-process, connects agents via gRPC, and exercises the highway via gRPC-Web (or direct gRPC for simplicity).

Location: `tests/e2e/` at workspace root (new).

### 8.1 Agent Lifecycle

| # | Scenario | Steps | Verifies |
|---|----------|-------|----------|
| E1 | Single worker | Start runtime → agent binds → signals ready → receives directive → creates checkpoint → signals complete → workspace closes | Full agent lifecycle; trail records all events; workspace transitions Idle→Active→Closed |
| E2 | Multi-worker parallel | Start runtime → dispatch 3 tasks → 3 agents bind → all complete → all workspaces close | Concurrent workspace management; no cross-contamination |
| E3 | Agent disconnect | Agent binds → signals ready → TCP disconnect → liveness timeout → workspace fails | Liveness monitoring; failure detection |

### 8.2 Envelope Exchange

| # | Scenario | Steps | Verifies |
|---|----------|-------|----------|
| E4 | Worker-to-worker | Agent A and B bind → A sends envelope to B → B receives via inbox stream | Port rights; envelope delivery; inbox FIFO |
| E5 | Human injection | Highway client injects envelope into workspace → agent receives via inbox | Highway injection; human_injection trail event |
| E6 | Blocked send | Agent A sends envelope to workspace without port right → PERMISSION_DENIED | Permission enforcement at transport boundary |

### 8.3 Gate Flow

| # | Scenario | Steps | Verifies |
|---|----------|-------|----------|
| E7 | Gate approval | Task dispatched → task_approval gate → highway user approves → workspace created → agent binds | Gate lifecycle; highway interaction; workspace creation |
| E8 | Gate rejection | task_approval gate → highway user rejects → task cancelled | Gate rejection; task FSM transition |
| E9 | Gate timeout | task_approval gate (5s timeout) → no human response → fallback executes | Timeout enforcement; fallback action |

### 8.4 Escalation Flow

| # | Scenario | Steps | Verifies |
|---|----------|-------|----------|
| E10 | Escalation feedback | Agent signals ESCALATION → highway receives event → human injects feedback envelope → agent unblocks | Escalation lifecycle; feedback delivery |
| E11 | Escalation abort | Agent escalates → human aborts → workspace fails | Abort semantics; workspace transition to Failed |
| E12 | Escalation delegate | Agent escalates → human delegates → coordinator handles | Delegation path |

### 8.5 Failure and Recovery

| # | Scenario | Steps | Verifies |
|---|----------|-------|----------|
| E13 | Cascade failure | Parent workspace fails → child workspaces cascade fail → budget released | Ownership-bounded cascade; resource cleanup |
| E14 | Budget exhaustion | Agent exceeds token budget → workspace fails with budget_exceeded | Budget enforcement across dimensions |
| E15 | Crash recovery | Runtime processes 100 events → kill → restart → trail replay → state matches → agent reconnects | Recovery correctness; trail integrity; snapshot acceleration |

### 8.6 Migration

| # | Scenario | Steps | Verifies |
|---|----------|-------|----------|
| E16 | Agent migration | Agent A binds → checkpoints → disconnect → agent B binds with migration → B continues from A's state | Migration snapshot; state continuity; identity verification |
| E17 | Migration failure | Migration starts → timeout → workspace fails | Migration timeout enforcement |

### 8.7 Integration Pipeline

| # | Scenario | Steps | Verifies |
|---|----------|-------|----------|
| E18 | Direct merge | Worker completes → final checkpoint → integration succeeds → workspace closes | Integration lifecycle; merge strategy |
| E19 | Conflict detection | Two workers modify overlapping resources → conflict detected → escalated → resolved | Conflict detection; resolution strategies |

**Total E2E tests: 19**

---

## 9. CI Pipeline

### 9.1 Current State

```yaml
# .github/workflows/ci.yml (current)
- cargo build --workspace
- cargo clippy --workspace
- cargo test --workspace
- cargo fmt --check
- protoc compilation check
```

Missing: TypeScript tests, Python tests, integration tests, E2E tests.

### 9.2 Target Pipeline

```yaml
name: CI
on: [push, pull_request]

jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - cargo fmt --check
      - cargo clippy --workspace -- -D warnings
      - cargo test --workspace                    # Unit tests (687+)
      - cargo test --test '*' --workspace         # Integration tests (43)

  typescript:
    runs-on: ubuntu-latest
    steps:
      - pnpm install --frozen-lockfile
      - pnpm typecheck
      - pnpm test                                 # Unit tests (105+)
      - pnpm build                                # Verify production build

  python:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        python: ['3.11', '3.12', '3.13']
    steps:
      - pip install -e .[test]
      - pytest tests/                             # Unit tests (14+)

  proto:
    runs-on: ubuntu-latest
    steps:
      - protoc --proto_path=proto *.proto --descriptor_set_out=/dev/null
      - buf lint proto/                           # Lint proto definitions
      - # Verify generated code matches proto files:
      - cargo build -p wacp-transport             # Rust codegen
      - cd highway-ui && pnpm generate && git diff --exit-code src/gen/  # TS codegen

  e2e:
    runs-on: ubuntu-latest
    needs: [rust, typescript, python]             # Only after unit tests pass
    steps:
      - cargo build --release -p wacp-runtime
      - cargo test --test 'e2e_*'                 # E2E tests (19)
```

### 9.3 Test Matrix

| Trigger | Rust unit | Rust integration | TS unit | Python unit | Proto | E2E |
|---------|-----------|-----------------|---------|-------------|-------|-----|
| Push to branch | Yes | Yes | Yes | Yes | Yes | No |
| PR to main | Yes | Yes | Yes | Yes | Yes | Yes |
| Merge to main | Yes | Yes | Yes | Yes | Yes | Yes |

---

## 10. Phased Execution Plan

### Phase T1 — Critical Gaps (P0)

Fill the three critically undertested crates and fix CI.

| # | Task | Tests to add | Target total |
|---|------|-------------|-------------|
| T1.1 | wacp-sdk unit tests | +47 | 50 |
| T1.2 | wacp-transport unit tests (gRPC handlers) | +45 | 70 |
| T1.3 | Python SDK agent tests | +34 | 48 |
| T1.4 | CI: add TypeScript + Python test jobs | 0 (infra) | — |
| | **Subtotal** | **+126** | |

### Phase T2 — Runtime + Highway Transport (P1)

| # | Task | Tests to add | Target total |
|---|------|-------------|-------------|
| T2.1 | wacp-runtime config/health/metrics/TLS | +32 | 85 |
| T2.2 | highway-ui transport (session, rpcs, client) | +12 | 40 |
| T2.3 | highway-ui layout + remaining components | +21 | 85 |
| | **Subtotal** | **+65** | |

### Phase T3 — Integration Tests (P2)

| # | Task | Tests to add | Target total |
|---|------|-------------|-------------|
| T3.1 | Rust cross-crate integration tests | +43 | 43 |
| T3.2 | TypeScript integration tests | +13 | 13 |
| T3.3 | Python integration tests | +8 | 8 |
| | **Subtotal** | **+64** | |

### Phase T4 — E2E Tests (P2)

| # | Task | Tests to add | Target total |
|---|------|-------------|-------------|
| T4.1 | E2E test harness | 0 (infra) | — |
| T4.2 | E2E agent lifecycle + envelope exchange | +6 | 6 |
| T4.3 | E2E gate + escalation flows | +6 | 6 |
| T4.4 | E2E failure, recovery, migration | +5 | 5 |
| T4.5 | E2E integration pipeline | +2 | 2 |
| | **Subtotal** | **+19** | |

### Phase T5 — Hardening (P3)

| # | Task | Tests to add | Target total |
|---|------|-------------|-------------|
| T5.1 | Remaining Rust unit test gaps (types, clock, fsm, taxonomy, permissions, workspace, coordinator) | +71 | — |
| T5.2 | Remaining highway-ui unit test gaps (notifications, store edges) | +9 | — |
| T5.3 | Remaining Python unit test gaps (proto) | +7 | — |
| | **Subtotal** | **+87** | |

### Summary

| Phase | New tests | Cumulative |
|-------|-----------|-----------|
| Current | — | 806 |
| T1 — Critical Gaps | +126 | 932 |
| T2 — Runtime + Highway | +65 | 997 |
| T3 — Integration | +64 | 1,061 |
| T4 — E2E | +19 | 1,080 |
| T5 — Hardening | +87 | 1,167 |
| **Total** | **+361** | **1,167** |

---

*WACP test strategy — Akil Abderrahim and Claude Opus 4.6*
