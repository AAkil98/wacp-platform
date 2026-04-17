---
id: wacp-wiring-strategy-b
type: impl
status: draft
created: 2026-04-17T00:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [strategy, runtime, agent-service, coordinator, workspace-fsm, follow-up]
depends_on: [wcon-wiring-strategy, wcon-llm-stub, wcon-w7-integration-tests]
---

# Wiring Strategy B — Runtime Agent-Service ↔ Coordinator

> **Scope.** The runtime-side counterpart to `impl/wiring-strategy.md`. The original strategy (W1–W7) wired the **Console** to the runtime: gRPC pool, session monitor, highway forwarding, cross-session queues. This follow-up wires the **runtime's own AgentService** to the coordinator and workspace actor so agent signals / checkpoints / envelopes actually drive the state machines the Console is already observing.
>
> **Positioning.** Lives alongside the original at `impl/wiring-strategy-b.md`. Follows the same structure (inventory → plan → execution order → risk map → acceptance) so a reader can diff the two documents to see what changed.
>
> **Why "B".** Audit §13.7.6 landed the deterministic `wacp-llm` stub provider and the I6 integration test that exercises it end-to-end. T7.2 / T7.3 (and the four inheritors T7.5 / T7.7 / T7.8 / T7.10) stayed `#[ignore]`-ed because the runtime's `AgentService` handlers in `wacp/crates/wacp-runtime/src/init.rs` are currently shells — they acknowledge requests but do not feed the coordinator or workspace FSM. Every ignored test asserts behavior that presupposes the wiring this document plans. Audit tracks this as §13.7.6b.

## Table of Contents

- 1. Situation Assessment
- 2. Inventory of Hollow Code (runtime side)
- 3. Wiring Plan (WA1–WA5)
- 4. Execution Order
- 5. Risk Map
- 6. Acceptance Criteria
- 7. References

---

## 1. Situation Assessment

The Console reached green on `cargo test -p console-integration` for six tests (T7.1 / T7.4 / T7.6 / T7.9 + the two new I6 scenarios) and has six tests `#[ignore]`-ed behind the same runtime-side gap. The §13.7.6 I6 suite proves the LLM stub serves deterministic responses to an agent binding via `wacp-sdk::Agent`. What it does **not** prove — because the runtime does not yet close the loop — is that those responses drive any coordinator state transition. The same gRPC calls that succeed in `llm_stub_e2e.rs::i6_stub_adapter_drives_agent_round_trip` will not, today, cause a workspace to transition `Active → Integrating → Closed`, will not emit a highway `GateEvent`, and will not trigger the session monitor's `StreamGates` driver to fire.

Concretely, with the stub provider installed and an agent bound via `Agent::connect()`:

- `EmitSignal { type: Complete }` returns `Ok` but the `WorkspaceActor`'s FSM never receives the trigger. `WorkspaceState` stays `Idle` forever.
- `CreateCheckpoint { status: Provisional, type: "task_approval" }` **does** persist the payload to SHA-256-addressed storage (`:839`) and index it by checkpoint id (`:848`) — so `GetCheckpoint` would find it — but **no `GateEvent` is emitted to the highway side** and the workspace actor gets no signal that a checkpoint was recorded. The Console's `StreamGates` never sees it.

**What is _not_ the blocker.** I initially assumed `SendEnvelope` and most coordinator handlers were also shells. A closer read of `init.rs` (lines `:765`, `:1531`, `:1594`, `:1654`, and the HighwayRequest block at `:1068–:1131`) shows otherwise — the three hollow handlers above are the _only_ structural gaps.

Wired and working today:

- `AgentRequest::SendEnvelope` — builds the envelope, notifies subscribers, calls `coordinator.route_envelope`, returns `Unavailable` when the target is down.
- `CoordinatorRequest::Decompose` / `SendDirective` / `SendFeedback` / `TriggerIntegration` / `CancelTask` / `AbortWorkspace` / `SuspendWorkspace` / `ResumeWorkspace` — all drive real state.
- `HighwayRequest::RespondToGate` — calls `gate_controller.resolve()`.
- `HighwayRequest::RespondToEscalation` — routes feedback envelopes or aborts via `coordinator.abort_workspace`.
- `CoordinatorRequest::TriggerIntegration` — sends `CoordinatorCommand::IntegrationSucceeded` to the workspace handle. **This is the pattern WA2 and WA3 should follow** — the plumbing to drive the workspace FSM from a gRPC handler already exists and is proven on the coordinator side.

The blocker is narrower than the 2026-04-15 audit §5 might read today (it said both trees were "drift-free in production paths"; that remains broadly correct — the three hollow handlers are well-scoped exceptions). The wiring plan below is the smallest coherent change that closes the loop for T7.2 / T7.3. Phase WA5 adds a harness-side knob for T7.5.

---

## 2. Inventory of Hollow Code (runtime side)

All four handlers live in `wacp/crates/wacp-runtime/src/init.rs`, inside the `handle_agent_request` / `process` loop. Line numbers reflect the tree at the time of writing (`d8ca8ff` on `dev` + uncommitted §13.7.6 package).

### 2.1 `AgentRequest::Bind` — workspace lookup only

`init.rs:736–752` looks up the workspace in `self.coordinator.tree` and returns a `BindResponse` with `role: String::new()`, `directive: None`, `context: vec![]`, `visibility: vec![]`, `authority: vec![]`, `budget: None`. The handler ignores `auth_token` entirely. None of the fields that an agent needs to act (role, directive envelope, context payload, visibility set, authority set, budget) are populated.

**What it should do.** Read the `WorkspaceConfig` that was passed to `coordinator.dispatch()` at `:1459` / `:1459` and project its fields onto the response: `role` from `config.role`, `directive` from `config.directive` (the `Envelope`), `context` from `config.context`, `visibility` / `authority` from the corresponding `HashSet<String>`, `budget` from `config.budget`. The `WorkspaceActor` already carries this state (see `wacp-workspace::state::WorkspaceConfig`) — exposing it is a projection, not new logic.

### 2.2 `AgentRequest::EmitSignal` — no FSM drive

`init.rs:754–764` builds an `EmitSignalResponse` and returns. The `SignalType` is parsed from the proto but never fed to the workspace actor. `WorkspaceTrigger::{AgentReady, AgentStarted, AgentBlocked, AgentComplete, AgentFailed}` (defined in `wacp-fsm::workspace`) are the correct triggers; they already route through the workspace FSM to produce `WorkspaceEvent::StateChanged`, which the coordinator already consumes at `orchestrator.rs:82`. The missing link is the `CoordinatorCommand` dispatch — the `WorkspaceHandle::coordinator_tx` channel exists (see `wacp-workspace::state`) but is only wired to `DeliverEnvelope` and `Abort` branches today.

**What it should do.** Map `SignalType` → `WorkspaceTrigger`, look up the `WorkspaceHandle` for the bound workspace, send a new `CoordinatorCommand::ApplyTrigger(trigger)` (or a purpose-named `CoordinatorCommand::AgentSignal` variant), let the actor run the transition, and rely on the existing `WorkspaceEvent::StateChanged` / `Terminated` fan-out to update the coordinator tree. Escalation signals are a special case — they emit an `EscalationEvent` to the highway side, which the highway → console bridge already knows how to stream.

### 2.3 `AgentRequest::CreateCheckpoint` — persist works, no highway fan-out

`init.rs:827–868` does more than I originally claimed. It computes SHA-256 over the payload, persists bytes to `checkpoint_storage`, and indexes the resulting `checkpoint_id` into `checkpoint_index` so a later `HighwayRequest::GetCheckpoint` can find it (see `:1223`). What it **does not do** is (a) notify the workspace actor that a checkpoint was recorded, and (b) consult the `GateController` to fan provisional checkpoints into `GateEvent`s on the highway outbound stream.

Provisional checkpoints of the taxonomy-registered types that carry a `policy_kind` of `requires_checkpoint` are the ones that should trigger gates (per `wcon-highway` §4 and the `wcon-profiles` policy-aware tool validation). Without gate emission, `StreamGates` has nothing to stream, and the Console's gate queue / pending-gates endpoints (W3 / W6) stay empty — observable today only in the ignored tests because the happy-path tests do not seed gates from a live runtime path (T7.9 injects via `install_handle_with_gate` for coverage without a live runtime).

**What it should do.** After the existing persist + index step, (1) send a new `CoordinatorCommand::CheckpointRecorded { checkpoint_id, status, type }` (or equivalent) to the workspace handle so the actor's trail emits the appropriate event, and (2) when status is `Provisional` and the checkpoint type is gate-requiring per `GateController`, emit a `GateEvent` to the highway outbound channel that `StreamGates` subscribers consume.

### 2.4 Non-blockers that looked like blockers

| Handler | Why I flagged it | Reality |
|---|---|---|
| `AgentRequest::SendEnvelope` | Assumed shell like EmitSignal | **Fully wired** at `:765–826` — builds envelope, notifies `envelope_subs`, calls `coordinator.route_envelope`, returns `Unavailable` when the actor is down. No changes needed. |
| `CoordinatorRequest::SendDirective` | Assumed untouched | Fully wired at `:1594–1621` via same `route_envelope` pattern. |
| `CoordinatorRequest::TriggerIntegration` | Assumed untouched | Fully wired at `:1654–1686`. Sends `CoordinatorCommand::IntegrationSucceeded` to the workspace handle — this is the template WA2 / WA3 should copy. |
| `HighwayRequest::RespondToGate` | Not investigated | Wired at `:1068–:1085` via `gate_controller.resolve()`. |
| `HighwayRequest::RespondToEscalation` | Not investigated | Wired at `:1086–:1132` with three action branches (feedback / abort / delegate). |

These handlers are worth re-reading before starting WA2 / WA3 because they already demonstrate the coordinator-command dispatch pattern that those phases need.

### 2.5 Summary table (revised)

| Handler | `init.rs` anchor | Current behavior | Target behavior | Lines to touch |
|---|---|---|---|---|
| `Bind` | `:736–:752` | Tree lookup only | Project `WorkspaceConfig` fields (role, directive, context, visibility, authority, budget) into response | ~30 |
| `EmitSignal` | `:754–:764` | Return OK | Map `SignalType → WorkspaceTrigger`, dispatch via `WorkspaceHandle.coordinator_tx` — use `TriggerIntegration`'s pattern at `:1654` as the template | ~40 |
| `CreateCheckpoint` | `:827–:868` | Persist payload + index | Add workspace-actor notify + conditional `GateEvent` emission after the existing persist block | ~60 |

Total: ~130 lines of new logic (down from my earlier ~150 estimate, with `SendEnvelope` removed). No new crate boundaries, no protocol changes, no gRPC contract updates.

---

## 3. Wiring Plan (WA1–WA5)

Naming convention `WAx` ("wiring, agent-side x") distinguishes from the `Wx` phases in the original strategy without redefining them.

### 3.1 Phase WA1 — Bind projects `WorkspaceConfig`

**What.** `AgentService::Bind` returns a populated `BindResponse`.

**Files.**
- Modified: `wacp/crates/wacp-runtime/src/init.rs` — `AgentRequest::Bind` branch.
- Possibly modified: `wacp/crates/wacp-workspace/src/state.rs` — if the `WorkspaceConfig` needs a helper that extracts the bind-response projection, add it there rather than growing the init loop.
- New: unit tests in `wacp-runtime/src/tests.rs` that dispatch a workspace via `CoordinatorService::Dispatch`, bind via `AgentService::Bind`, and assert every response field matches the dispatched config.

**Validation.** After `SubmitGoal` → `Dispatch`, an `Agent::connect()` call sees the real role, directive, context, and budget. I6 can be extended with an assertion on the bind-response shape.

### 3.2 Phase WA2 — Signals drive the workspace FSM

**What.** `EmitSignal` advances workspace state.

**Files.**
- Modified: `wacp-runtime/src/init.rs` — `AgentRequest::EmitSignal` branch.
- Modified: `wacp-workspace/src/state.rs` — add a `CoordinatorCommand::AgentSignal { trigger }` variant (or equivalent); the actor already runs FSM transitions via `WorkspaceFsm::transition`.
- Modified: `wacp-coordinator/src/orchestrator.rs` — no new method needed; the existing `WorkspaceEvent::StateChanged` consumer already closes the loop.
- New: unit tests that emit `Complete` → observe `Active → Integrating → Closed`, emit `Failed` → observe `Active → Failed`, etc.

**Validation.** T7.3 un-`#[ignore]`s; `cargo test -p console-integration --test lifecycle -- t7_3_session_completes_emits_final_frame` green.

### 3.3 Phase WA3 — CreateCheckpoint forwards to workspace actor (LANDED, narrowed)

**What.** Agent checkpoints now flow to the bound workspace actor. The actor pushes onto `state.checkpoint_register`, updates `resource_meter`, emits `WorkspaceEvent::CheckpointCreated`, and auto-signals per protocol §7.2.

**Status.** Landed in `<WA3 sha>`. Coding spec at `wacp/impl/wa3-checkpoint-forward.md`.

**Scope narrowed.** The original WA3 plan folded gate fan-out into this phase. Implementation revealed the machinery does not exist: `GateType` (wacp-types/src/enums.rs:143) has no `CheckpointApproval` variant, `GateController::open_gate` is task-based, and there is no gate-resolution→actor-resume callback. Adding those is ~150–200 LOC of new cross-cutting surface (proto enum addition, new controller method, new `WorkspaceEvent` variant for resume, interceptor wiring for the resolution path). Carved out as WA3.5.

### 3.3.5 Phase WA3.5 — Checkpoint-approval gates (LANDED)

**What.** Provisional checkpoints of gate-requiring taxonomy types create `GateEvent`s on the highway outbound stream; `RespondToGate` resolutions feed back into the workspace actor to resume the paused workspace.

**Scope.**
- Add `GateType::CheckpointApproval` to `wacp-types/src/enums.rs` (+ proto companion in `primitives.proto`).
- New method `GateController::open_checkpoint_gate(workspace_id, checkpoint_id, checkpoint_type, timeout_ms, fallback)`.
- Extend `AgentRequest::CreateCheckpoint` handler: after the actor forward, if the checkpoint is `Provisional` and the type is gate-requiring per the taxonomy, call `open_checkpoint_gate` and fan the resulting `GateEvent` into `self.gate_subs`.
- Extend `HighwayRequest::RespondToGate`: on approve/modify, send a new `CoordinatorCommand::CheckpointApproved { checkpoint_id }` (or equivalent) to the workspace actor; on reject, send `CheckpointRejected`. Actor transitions Blocked→Active on approve.

**Effort.** 4–5 hours actual (matched estimate). Coding spec at `wacp/impl/wa3-5-checkpoint-gates.md`.

**Validation.** Runtime-side proven via WA3.5 unit tests (5 coord + 5 workspace + 5 runtime, all green). Console-level un-ignore of T7.2 / T7.10 / T7.8 deferred to a follow-up — see §4.

### 3.3.6 Phase WA3.6 — Auto-integration on Complete (LANDED)

**What.** Discovered while writing the T7.3 un-ignore: when an agent emits `Complete`, the FSM transitions `Active→Integrating`, but nothing advances `Integrating→Closed`. Today only `CoordinatorRequest::TriggerIntegration` (init.rs:1654) produces the `IntegrationSucceeded` command that closes the workspace, and no caller triggers it automatically.

**Scope.**
- In `coordinator::handle_event` (`wacp-coordinator/src/orchestrator.rs:80`), on `WorkspaceEvent::StateChanged { to: Integrating }`, synchronously invoke `IntegrationEngine::integrate` (already present at `wacp-coordinator/src/integration.rs:49`) against the workspace's last checkpoint; on `Success`, send `CoordinatorCommand::IntegrationSucceeded` to the workspace handle; on `Conflict`/`Failed`, send the corresponding command.
- This is coordinator-side only, no runtime changes.

**Effort.** 2–3 hours actual (matched estimate). Coding spec at `wacp/impl/wa3-6-auto-integration.md`. Implementation note: `Coordinator::handle_event` became `async` to allow the coordinator-tx send to complete inside the event-handler call; all call sites (`init.rs` × 2, coordinator/runtime test files × 4) updated to `.await`. Backwards-compat preserved via the unchanged `&mut self` receiver and new field defaults.

**Validation.** Runtime-side proven via WA3.6 unit tests (4 coord + 1 runtime, all green). Console-level un-ignore of T7.3 / T7.7 deferred to a follow-up — see §4.

### 3.4 Phase WA4 — **removed**

Original WA4 proposed wiring `AgentRequest::SendEnvelope`. A closer reading of `init.rs:765–:826` shows it is already wired — see §2.4. T7.7 / T7.8 therefore do not need a new dispatch path; they un-`#[ignore]` on WA2 (signals drive FSM so 10 sessions can reach `Complete`) and WA3 (gates make traffic high enough to exercise slow-consumer pacing).

### 3.5 Phase WA5 — Dispatch-failure injection for T7.5 (LANDED)

**What.** A test-only knob that makes the second `CoordinatorService::Dispatch` in a multi-task launch return an error, so the Console's W2 rollback assertion can fire.

**Files.**
- Modified: `wacp-console/integration/src/runtime_harness.rs` — expose a `spawn_with_failure_points(FailureConfig)` constructor that wraps an interceptor `tower::Service` around the gRPC channel. The runtime itself stays untouched; the interceptor rejects the Nth request on the coordinator surface.
- Modified: `wacp-console/integration/tests/chaos.rs::t7_5_partial_launch_failure_rolls_back` — body fills in; un-`#[ignore]`s.

This phase is harness-side only (no runtime changes) and can land in parallel with WA1–WA4. It is listed here because its `#[ignore]` reason ("needs dispatch-failure injection on top of the stub provider") points here as the close-out.

**Validation.** T7.5 un-`#[ignore]`s.

**Implementation note (2026-04-17).** Landed via `dec6385`. Took the tonic mock-server path (12 unary RPCs + 1 streaming RPC forward to the real runtime, `dispatch` short-circuits with Unavailable on the Nth call). The mock lives inline in `tests/chaos.rs::failure_proxy` rather than in `runtime_harness.rs` per the original strategy — keeps `tonic` + `wacp-transport` as dev-deps (one less Cargo.toml churn). Each forwarded RPC is a one-line `self.upstream.clone().method(req).await`; streaming is pumped via a spawned task. ~3 h actual.

---

## 4. Execution Order

```
[§13.7.6 DONE]  Stub LlmAdapter + I6 integration test         0.28 s walltime
  │             (2026-04-17; wcon-llm-stub)
  │
WA1: Bind projects WorkspaceConfig                            ~2 hours
  │     (unit-testable in isolation; no dependency on agent side)
  │
WA2: EmitSignal drives FSM                                    ~3 hours
  │     (depends on WA1 for a non-empty bind — the agent needs a
  │      role + directive to decide which signal to emit in I6-
  │      derived lifecycle tests. Use TriggerIntegration at
  │      init.rs:1654 as the implementation template.)
  │
WA3: CreateCheckpoint fans into gates                         ~3–4 hours
  │     (the most interaction surface — extends the existing
  │      persist block at :839 with workspace-actor notify +
  │      GateController fan-out. Unit tests bound most of the
  │      risk; the live path is the integration test.)
  │
WA5: Dispatch-failure injection (harness-side)                ~2 hours
  │     (can run in parallel; lands T7.5 once WA2 lands T7.3)
  │
[Un-ignore sweep]  T7.2 / T7.3 / T7.5 / T7.7 / T7.8 / T7.10   ~1 hour
                   (remove `#[ignore]`, fill bodies, run the
                    suite. Bodies are already sketched in the
                    `// Future:` comments in each test file.)
```

**Total effort (final, 2026-04-17 close):**
- Landed: WA1 (2 h), WA2 (2 h), WA3 narrowed (1.5 h), WA3.5 (4–5 h), WA3.6 (2–3 h), WA5 (3 h), T7.3 + WorkspaceState fix (1.5 h), un-ignore sweep T7.2/T7.7/T7.8/T7.10 (1.5 h thanks to the WA3.5/WA3.6/WA5 ground prep + the discovered direct-broadcast pattern for T7.8). Total ≈ 17.5–19 h.
- Original 9–12 h estimate was undersized by ~50–75 %. Two new bugs surfaced (Rust↔proto enum offset on GateType + WorkspaceState) that wouldn't have appeared in unit tests alone — the integration sweep was the forcing function.

**Critical path:** WA3 (checkpoints → actor) → WA3.5 (gate fan + resume) → un-ignore sweep. WA3.6 was parallelizable but had its own async-cascade ripples. WA5 was fully independent (harness-side only) and shipped late in the day.

---

## 5. Risk Map

| Risk | Impact | Mitigation |
|---|---|---|
| Bind-response projection leaks fields that should be scoped to the workspace owner | Agent sees metadata it has no business seeing (e.g., another workspace's budget or authority set) | Project **only** the fields that the proto already declares. The `BindResponse` message is the contract — if it says `visibility: repeated string`, that's what the agent gets. No implicit expansion. |
| Signal dispatch races with coordinator tree updates | An agent emits `Complete` and reads trail state before the `Terminated` event lands on the tree | `WorkspaceActor` already serializes via its mpsc; the coordinator's event consumer at `orchestrator.rs:82` is single-threaded per coordinator instance. The race window is small and already handled. Document the ordering expectation in WA2's coding spec. |
| Gate fan-out double-fires (provisional checkpoint emitted twice → two gates) | Operator sees duplicate gates in the queue | Key gates by `(workspace_id, content_hash)` — same invariant the trail already uses for checkpoint deduplication. Test for it in WA3. |
| `GateController` policy evaluation is more complex than a simple type-match | Risk of false negatives (real gate type not triggering) or false positives | Read `wcon-discovery` §3.5 and the existing `GateController` tests before writing WA3. The policy is taxonomy-registered; the implementation already exists for the task-approval pattern. |
| SendEnvelope payload size can be large | Blocking the agent handler behind the coordinator's mpsc while a big envelope serializes | The mpsc is bounded and `route_envelope` returns `bool`, not a stream; the worst case is backpressure on the agent handler, not a deadlock. Monitor in WA4's integration test. |
| The six ignored tests assume behaviors that overlap but are not identical | Wiring WA1–WA4 un-`#[ignore]`s tests in non-atomic order → intermediate commits have red suites | Land WA1 → WA2 → WA3 → WA4 in a single branch; the un-ignore sweep is the final commit. Each intermediate commit keeps the tests ignored. |
| Runtime binary size grows — integration tests slow | `RuntimeHarness::spawn_default` walltime regresses | Baseline from §13.7.6: I6 full scenario 0.28 s. Fail the CI job if that jumps past 2 s. |

---

## 6. Acceptance Criteria

A branch that closes WA1–WA5 lands when:

1. `cargo test -p wacp-runtime` green, including WA1–WA3 unit tests for the three real handlers.
2. `cargo test -p console-integration` green with **zero `#[ignore]`-ed tests** in `lifecycle.rs` / `chaos.rs` / `cross_session.rs`. Specifically:
   - `t7_2_open_ws_drive_gate_observe_resume` — passes.
   - `t7_3_session_completes_emits_final_frame` — passes.
   - `t7_5_partial_launch_failure_rolls_back` — passes.
   - `t7_7_ten_concurrent_sessions_complete` — passes, no monitor task > 50 MB RSS.
   - `t7_8_slow_ws_consumer_does_not_starve_others` — passes.
   - `t7_10_w4_resolve_clears_w6_pending_within_500ms` — passes.
3. `cargo test -p console-integration --test llm_stub_e2e` still green — the new wiring does not regress the stub-provider round trip.
4. `cargo clippy --workspace -- -D warnings` and `cargo fmt --check --all` both clean.
5. `AUDIT-2026-04-15.md` §13.5 "§11 #6 — LLM stub (runtime-side follow-up)" entry removed; §13.1 item 6 status updated to **done**; §13.7.6b row in §13.8 struck through.
6. `wacp-console/specs/coding/wcon-w7-integration-tests.md` §5.1 deviation note removed.
7. Per-phase coding specs at `wacp/impl/wa{1..5}-*.md` (or equivalent per-author preference) authored before implementation — same pattern the W1–W7 phases followed with `wacp-console/specs/coding/wcon-w{1..7}-*.md`.

---

## 7. References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-wiring-strategy | Wiring Strategy — Console ↔ Runtime Integration | predecessor (W1–W7) |
| wcon-llm-stub | Coding Spec — LLM Stub Provider | informs (the un-ignoring acceptance criterion was split here) |
| wcon-w7-integration-tests | W7 — Integration Tests | validates (every `#[ignore]` in its §5.1–§5.4 closes here) |
| wacp-impl-runtime | WACP Implementation: Runtime | constrains (all handler changes land in `wacp/crates/wacp-runtime/src/init.rs`) |
| wacp-impl-llm-adapters | WACP Implementation: LLM Adapter Framework | stub is the first production consumer |
| wcon-sessions | WACP Console — Session Lifecycle | validates (launch + monitor + cancel invariants asserted by the un-ignored tests) |
| wcon-highway | WACP Console — Highway Integration | validates (gate fan-out is the WA3 deliverable) |

*WACP Platform — authored by AKil Abderrahim and Claude Opus 4.7 (1M context).*
