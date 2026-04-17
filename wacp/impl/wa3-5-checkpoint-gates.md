---
id: wacp-wa3-5-checkpoint-gates
type: coding
status: final
created: 2026-04-17T00:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [runtime, coordinator, gates, checkpoints, wa3-5]
depends_on: [wacp-wiring-strategy-b, wacp-wa3-checkpoint-forward]
---

# Coding Spec — WA3.5: Checkpoint-Approval Gates

## 1. Scope

`AgentRequest::CreateCheckpoint` now forwards to the workspace actor (WA3). `HighwayRequest::RespondToGate` resolves task gates via `GateController::resolve` but has no hook into the workspace actor. There is no `GateType::CheckpointApproval` variant in `wacp-types`/`proto`, no `open_checkpoint_gate` method, and no `CoordinatorCommand` path back into a blocked workspace actor on approval.

WA3.5 closes that loop so that provisional checkpoints emit highway `GateEvent`s, block the workspace until resolved, and resume (approve) or fail (reject) via the existing FSM triggers.

### 1.1 In scope

1. `GateType::CheckpointApproval` variant in `wacp-types/src/enums.rs` + `GATE_TYPE_CHECKPOINT_APPROVAL = 7` in `proto/primitives.proto`.
2. `GateController::open_checkpoint_gate` + `resolve_checkpoint` — parallel `pending_checkpoints` map keeping existing `resolve()` / `PendingGate` / `GateResolution` shapes untouched for backcompat with all existing call sites + tests.
3. Workspace actor:
   - `handle_create_checkpoint` transitions `AgentBlocked` (Active → Blocked) after emitting CheckpointCreated + auto-Signal when `status == Provisional`. Keeps checkpoint-creation ordering before the block on the trail.
   - Two new `CoordinatorCommand` variants: `CheckpointApproved { checkpoint_id }` → `AgentStarted` trigger (Blocked → Active); `CheckpointRejected { checkpoint_id }` → `CoordinatorAbort` trigger (Blocked → Failed). `checkpoint_id` is correlation-only; the actor does not look it up.
4. `wacp-runtime/src/init.rs`:
   - `CreateCheckpoint` — after the WA3 actor forward, if `status == Provisional` and the workspace handle exists, open a checkpoint gate and fan the proto `GateEvent` to `self.gate_subs` (dropping dead senders).
   - `RespondToGate` — try `resolve_checkpoint` first; on match, send `CheckpointApproved` (Approve/Modify) or `CheckpointRejected` (Reject) to the workspace actor, then return the existing `GateResponseAck` with `applied: true`. On miss, fall through to `resolve` for task gates.

### 1.2 Out of scope

- Taxonomy-aware gate-requiring predicate. For v1, **all** provisional checkpoints (regardless of type) open a gate. Narrowing via `wcon-profiles` policy-aware lookup is deferred — `stub_responses.yaml` already only emits provisional checkpoints for `task_approval`-typed checkpoints, which is what T7.2 expects.
- Timeout fallback for checkpoint gates. `GateController::timeout` stays task-only; no `timeout_checkpoint` added. The default 30 s + AutoApprove fallback only applies to task gates; checkpoint gates block indefinitely until `RespondToGate` resolves them. If T7.2's 2 s window needs a timeout safety net later, a follow-up can mirror `timeout()`.
- Changes to `resolve`'s existing signature or `GateResolution` variants. Preserving them keeps 8 pre-existing tests green.

### 1.3 Why provisional = block

Per protocol §7.2 rule 5 the actor auto-signals `Checkpoint` on every checkpoint. The block-on-provisional rule is a WA3.5 extension specific to gate-requiring semantics: a human must acknowledge a provisional result before the workspace continues. Making the actor the source of truth (rather than the runtime injecting a separate `CheckpointBlock` command) guarantees `CheckpointCreated` / `Signal(Checkpoint)` are emitted before the block's `StateChanged`, which is the correct observable order on the trail.

## 2. Type + proto changes

### 2.1 `wacp/crates/wacp-types/src/enums.rs:143`

Append `CheckpointApproval` to `GateType`. Serde derivation yields the string `"CheckpointApproval"`; existing callers do not pattern-match exhaustively on `GateType` in production paths, so no downstream matches need `_ =>` arms (verify via `cargo check` — `GateController::open_gate`, `open_gate` tests, and the highway-ui pb are the only consumers).

### 2.2 `wacp/proto/primitives.proto:93`

Append `GATE_TYPE_CHECKPOINT_APPROVAL = 7;`. tonic_build auto-regenerates the Rust enum on next build; no manual transport-layer conversion helper is needed because `gate_type as i32` already serializes correctly onto the proto `GateEvent.type` field.

## 3. `wacp-coordinator/src/gate.rs`

Additive-only:

```rust
#[derive(Debug, Clone)]
pub struct PendingCheckpointGate {
    pub gate_id: GateId,
    pub workspace_id: WorkspaceId,
    pub checkpoint_id: CheckpointId,
    pub checkpoint_type: String,
    pub timeout_ms: u64,
    pub fallback: GateFallback,
    pub created_at: u64,
}

impl GateController {
    pub fn open_checkpoint_gate(
        &mut self,
        workspace_id: WorkspaceId,
        checkpoint_id: CheckpointId,
        checkpoint_type: String,
        timeout_ms: Option<u64>,
        fallback: Option<GateFallback>,
    ) -> GateEvent { ... }

    pub fn resolve_checkpoint(
        &mut self,
        gate_id: &GateId,
    ) -> Option<PendingCheckpointGate> { ... }

    pub fn is_checkpoint_pending(&self, gate_id: &GateId) -> bool { ... }
}
```

`resolve_checkpoint` takes no `GateDecision` — the caller in init.rs already has the decision and routes it; `resolve_checkpoint` just detaches the pending gate from the map and returns its context. This keeps `GateResolution`'s two-variant shape intact.

`open_checkpoint_gate` builds a `GateEvent` with `gate_type: GateType::CheckpointApproval`, `subject: checkpoint_id.as_ref().as_bytes().to_vec()` (makes `subject` a round-trippable reference for debugging), `workspace_id`, `task_id: None`, and the same `timeout_ms` / `fallback_action` plumbing as `open_gate`.

## 4. `wacp-workspace/src/actor.rs`

- Extend `CoordinatorCommand` with `CheckpointApproved { checkpoint_id: String }` and `CheckpointRejected { checkpoint_id: String }`. Both variants carry the id for correlation and future trail attribution; the actor does not look them up.
- Extend `handle_coordinator_cmd`:
  - `CheckpointApproved { .. }` → `self.transition(WorkspaceTrigger::AgentStarted).await` (Blocked → Active). If the state isn't Blocked the FSM emits `TransitionError::IllegalTransition`, which the actor already wraps in `WorkspaceEvent::Error`. Callers reading that event can correlate by `workspace_id`.
  - `CheckpointRejected { .. }` → `self.transition(WorkspaceTrigger::CoordinatorAbort).await` (Blocked → Failed). Same error-path semantics.
- Extend `handle_create_checkpoint` — after emitting `CheckpointCreated` + auto-`Signal(Checkpoint)`, if `status == CheckpointStatus::Provisional`, call `self.transition(WorkspaceTrigger::AgentBlocked).await`. The FSM's Active → Blocked is the valid path; from any other state this emits an Error event which is acceptable (it proves the workspace was not in Active when the provisional arrived — a protocol-level oddity to surface).

## 5. `wacp-runtime/src/init.rs`

### 5.1 `CreateCheckpoint` — post-WA3 extension (after `:973`)

```rust
// WA3.5: provisional checkpoints open a highway gate so an operator
// can approve or reject. The actor-side block already happened as a
// side effect of the WA3 forward (provisional triggers AgentBlocked
// inside handle_create_checkpoint).
if request.status == wacp_transport::wacp_v1::CheckpointStatus::Provisional as i32 {
    if self.coordinator.tree.get(&ws_id).is_some() {
        let gate_event = self.gate_controller.open_checkpoint_gate(
            ws_id.clone(),
            CheckpointId::from(checkpoint_id.as_str()),
            request.r#type.clone(),
            None, // default timeout
            None, // default fallback
        );
        fan_checkpoint_gate_to_subs(&mut self.gate_subs, &gate_event).await;
    }
}
```

`fan_checkpoint_gate_to_subs` is a new free helper that (a) builds a `wacp_transport::wacp_v1::GateEvent` from the internal `GateEvent` and (b) `try_send`-fans to every subscriber, dropping any whose channel closed — same pattern `fan_trail_to_subs` uses in init.rs:578+ (verify exact anchor during impl). If no pattern exists today, inline the three-line fan-out directly.

Proto conversion is direct: `GateType as i32` serializes, `gate_id.to_string()`, `subject.clone()`, `workspace_id.to_string()`, `task_id: String::new()` (checkpoint gates have no task), `timeout_ms` / `fallback_action` verbatim, `created_at: None` (timestamp-free for now — the Console fills in receive-time).

### 5.2 `RespondToGate` — at `:1182`

```rust
HighwayRequest::RespondToGate { request, reply } => {
    let gate_id = GateId::from(request.gate_id.as_str());
    let decision = match request.decision { ... /* unchanged */ };

    // WA3.5: checkpoint gates first — remove from the checkpoint map and
    // route the decision to the workspace actor. If the gate_id belongs
    // to a task gate (no checkpoint entry), fall through to the pre-WA3.5
    // resolve() path.
    if let Some(cp_gate) = self.gate_controller.resolve_checkpoint(&gate_id) {
        let cmd = match decision {
            GateDecision::Approve | GateDecision::Modify =>
                CoordinatorCommand::CheckpointApproved {
                    checkpoint_id: cp_gate.checkpoint_id.to_string(),
                },
            GateDecision::Reject =>
                CoordinatorCommand::CheckpointRejected {
                    checkpoint_id: cp_gate.checkpoint_id.to_string(),
                },
        };
        let applied = if let Some(handle) = self.coordinator.handle(&cp_gate.workspace_id) {
            handle.coordinator_tx.send(cmd).await.is_ok()
        } else {
            false
        };
        let response = wacp_transport::wacp_v1::GateResponseAck {
            gate_id: request.gate_id,
            applied,
            client_request_id: request.client_request_id,
        };
        let _ = reply.send(Ok(response));
        return;
    }

    let applied = self.gate_controller.resolve(&gate_id, decision).is_some();
    /* unchanged from here */
}
```

Decision mapping: `Modify` is treated as `Approve` in the checkpoint case — matches the task-gate convention in `GateController::resolve` at `:99–102` and keeps one knob for operators who want to approve with altered state.

## 6. Tests

### 6.1 `wacp-coordinator/src/tests.rs`

Add after the existing task-gate block (~line 1695):

1. `open_checkpoint_gate_creates_pending` — opens, asserts `is_checkpoint_pending` + `pending_checkpoint_count == 1`.
2. `open_checkpoint_gate_returns_event` — asserts returned `GateEvent.gate_type == CheckpointApproval`, `task_id.is_none()`, `subject == checkpoint_id bytes`.
3. `resolve_checkpoint_removes_pending` — opens, resolves, asserts pending count 0, returns `Some(PendingCheckpointGate)` with matching `workspace_id` + `checkpoint_id`.
4. `resolve_checkpoint_already_resolved_returns_none` — second `resolve_checkpoint` call returns `None`.
5. `resolve_task_gate_unaffected_by_checkpoint_path` — open one of each, `resolve_checkpoint(task_gate_id)` returns `None`, `resolve(task_gate_id, Approve)` still works. Proves parallel-map isolation.

### 6.2 `wacp-workspace/src/tests.rs`

1. `provisional_checkpoint_transitions_active_to_blocked` — spawn actor, drive to Active via first envelope, send `CreateCheckpoint { status: Provisional }`, drain events; expect `CheckpointCreated`, `Signal(Checkpoint)`, `StateChanged(Active→Blocked)`.
2. `final_checkpoint_does_not_block` — same flow with `status: Final`; drain events; expect `CheckpointCreated` + `Signal(Checkpoint)` only, no `StateChanged`.
3. `checkpoint_approved_resumes_blocked_workspace` — block via provisional checkpoint, send `CoordinatorCommand::CheckpointApproved { checkpoint_id: "cp-1" }`, expect `StateChanged(Blocked→Active)`.
4. `checkpoint_rejected_fails_blocked_workspace` — block via provisional, send `CheckpointRejected`, expect `StateChanged(Blocked→Failed)` + eventual `Terminated`.
5. `checkpoint_approved_on_active_workspace_emits_error` — create a provisional *final* checkpoint (doesn't block), then send `CheckpointApproved` while Active; expect `WorkspaceEvent::Error` (FSM illegal transition Active→Active via AgentStarted).

### 6.3 `wacp-runtime/src/tests.rs`

1. `wa3_5_provisional_checkpoint_emits_gate` — SubmitGoal → first envelope → CreateCheckpoint(Provisional); subscribe a `gate_subs` receiver first; drain one GateEvent; assert `gate_type == GATE_TYPE_CHECKPOINT_APPROVAL`, `workspace_id` matches, `subject` bytes decode to the checkpoint_id string.
2. `wa3_5_final_checkpoint_does_not_emit_gate` — same flow with Final; assert no GateEvent within 200 ms.
3. `wa3_5_respond_to_checkpoint_gate_approves_routes_to_actor` — provisional checkpoint + gate subscription, call `RespondToGate(Approve)`, drain events; expect `StateChanged(Blocked→Active)` for the workspace.
4. `wa3_5_respond_to_checkpoint_gate_rejects_fails_workspace` — same, `Reject`; expect `StateChanged(Blocked→Failed)`.
5. `wa3_5_respond_to_gate_falls_through_for_task_gates` — open a task gate via `GateController::open_gate`, respond; assert `applied: true` (backcompat with existing behaviour).

## 7. Acceptance

- `cargo test -p wacp-runtime` green (~108 tests = 103 + 5 WA3.5).
- `cargo test -p wacp-coordinator` green (existing + 5 WA3.5).
- `cargo test -p wacp-workspace` green (existing + 5 WA3.5).
- `cargo test -p console-integration --test llm_stub_e2e` still green (no regression).
- `cargo clippy --workspace -- -D warnings` clean.
- `cargo fmt --check --all` clean.
- T7.2 un-`#[ignore]`s separately (part of the sweep in `impl/wiring-strategy-b.md` §4) once WA3.6 + WA5 also land.

## 8. References

| ID | Title | Relationship |
|----|-------|--------------|
| wacp-wiring-strategy-b | Wiring Strategy B | parent (§3.3.5) |
| wacp-wa3-checkpoint-forward | WA3 | predecessor; provides the actor-forward path this spec extends |

*WACP Platform — authored by Akil Abderrahim and Claude Opus 4.7 (1M context).*
