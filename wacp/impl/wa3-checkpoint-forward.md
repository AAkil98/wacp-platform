---
id: wacp-wa3-checkpoint-forward
type: coding
status: final
created: 2026-04-17T00:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [runtime, agent-service, checkpoint, wa3]
depends_on: [wacp-wiring-strategy-b, wacp-wa2-emit-signal-fsm]
---

# Coding Spec — WA3: CreateCheckpoint Forwards to Workspace Actor

## 1. Scope

### 1.1 Narrowed from wiring-strategy-b §3.3

The original WA3 plan had two pieces: (a) forward the checkpoint to the workspace actor, (b) fan provisional checkpoints into `GateEvent`s on the highway outbound stream so the Console's `StreamGates` driver sees them. Implementation revealed that (b) requires machinery that doesn't exist today:

- `GateType` (in `wacp-types/src/enums.rs:143`) has no `CheckpointApproval` variant. The enum is closed: `TaskApproval | WorkspaceCreate | EnvelopeDelivery | Integration | ConflictResolution`.
- `GateController::open_gate()` (in `wacp-coordinator/src/gate.rs:52`) takes a `TaskId` and builds a task-based `GateEvent`. It does not model checkpoints.
- There is no callback path from "gate resolved" back into the workspace actor's FSM. `RespondToGate` (`init.rs:1068`) calls `gate_controller.resolve(…)` and returns; the workspace does not resume based on the outcome.

Building checkpoint-approval gates end-to-end adds a new `GateType` variant, a new `open_checkpoint_gate()` method, a resolution-to-FSM callback wiring, and at least one new `WorkspaceEvent` for the actor's response. ~150–200 lines of cross-cutting machinery, well beyond WA3's 3–4 h estimate and not required to make the checkpoint path *useful* — just to make it observable via the existing gate stream.

This spec narrows WA3 to deliverable (a) only. Part (b) is carved out as **WA3.5** below and will be addressed in a follow-up. T7.2's un-ignore criterion therefore shifts: closing WA3 does not close T7.2 by itself. The audit tracking table is updated accordingly.

### 1.2 In scope

- Forward every `AgentRequest::CreateCheckpoint` to the bound workspace's actor via `AgentMessage::CreateCheckpoint`. The actor's existing `handle_create_checkpoint` (`wacp-workspace/src/actor.rs:290`) records the checkpoint on `state.checkpoint_register`, updates `resource_meter.usage.storage_bytes`, emits `WorkspaceEvent::CheckpointCreated(cp)`, and auto-emits `WorkspaceEvent::Signal(Checkpoint)` per protocol §7.2 rule 5.
- Preserve the existing runtime-side persist + index logic (SHA-256 store, `checkpoint_index` for `GetCheckpoint` lookups) — it is load-bearing for `HighwayRequest::GetCheckpoint`.

### 1.3 Out of scope — WA3.5 (follow-up)

- Gate fan-out for provisional checkpoints.
- New `GateType::CheckpointApproval` variant and its proto companion.
- `GateController::open_checkpoint_gate()`.
- `RespondToGate` → workspace-actor resumption callback.

Tracked in `impl/archive/wiring-strategy-b.md` as **WA3.5** (added in the same commit that lands this spec).

## 2. Changes — `wacp-runtime/src/init.rs`

- `AgentRequest::CreateCheckpoint` handler (existing at `:827–:868`):
  - Keep the persist + index block unchanged.
  - After indexing, look up the workspace handle via `self.coordinator.handle(&ws_id)`.
  - If the handle is present, send `AgentMessage::CreateCheckpoint { checkpoint_type, payload, content_hash, intent, status, confidence, resource_usage }` via `handle.agent_tx`. Convert proto status/confidence to internal via the same pattern `CheckpointBuilder::create` uses in reverse (`wacp-sdk/src/builder.rs:72–80`).
  - If the handle is absent (workspace actor terminated between the index insert and the forward), log at `warn` and still return the already-assigned `checkpoint_id` — the payload is persisted so downstream queries still work, and rejecting the response after having indexed would leave orphan state. The workspace actor cannot act on it, which matches the semantics of "the checkpoint is archived but not observed."
- Introduce the proto→internal helpers `proto_to_checkpoint_status(i32) -> CheckpointStatus` and `proto_to_confidence(i32) -> Confidence`. Mirrors the internal→proto direction in `wacp-sdk/src/builder.rs:72–80`.

## 3. Tests — `wacp-runtime/src/tests.rs`

Same direct-drive pattern as WA1/WA2.

1. `wa3_create_checkpoint_updates_actor_state` — submit a goal, create a checkpoint via the handler, drain events until `WorkspaceEvent::CheckpointCreated` arrives; assert the checkpoint's `content_hash` matches SHA-256 of the payload.
2. `wa3_create_checkpoint_emits_auto_signal` — continue from the test above; the actor also emits a `WorkspaceEvent::Signal { signal_type: Checkpoint }` per the auto-signal rule. Assert that event appears on the stream.
3. `wa3_create_checkpoint_unknown_workspace_still_persists` — the runtime indexes the checkpoint even when the actor is unreachable. Create a checkpoint with a `workspace_id` that is not in the tree; assert the response contains a non-empty `checkpoint_id` and a `content_hash`, and assert `GetCheckpoint` via the highway path returns the stored payload. (No actor event fires.)
4. `wa3_proto_status_confidence_roundtrip` — construct requests with each `CheckpointStatus` / `Confidence` proto variant; assert the actor emits matching internal-type checkpoints via the `CheckpointCreated` event.

## 4. Acceptance

- `cargo test -p wacp-runtime` green (expect ~103 tests: 99 + 4 WA3 cases).
- `cargo test -p console-integration --test llm_stub_e2e` still green.
- `cargo clippy -p wacp-runtime -- -D warnings` clean.

## 5. References

| ID | Title | Relationship |
|----|-------|--------------|
| wacp-wiring-strategy-b | Wiring Strategy B | parent (§3.3 WA3 + new §3.3.5 WA3.5) |
| wacp-wa2-emit-signal-fsm | WA2 | predecessor; forward pattern reused |

*WACP Platform — authored by Akil Abderrahim and Claude Opus 4.7 (1M context).*
