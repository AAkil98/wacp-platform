---
id: wacp-wa2-emit-signal-fsm
type: coding
status: final
created: 2026-04-17T00:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [runtime, agent-service, fsm, wa2]
depends_on: [wacp-wiring-strategy-b, wacp-wa1-bind-projection]
---

# Coding Spec — WA2: EmitSignal drives the Workspace FSM

## 1. Scope

`AgentService::EmitSignal` at `init.rs:754` used to return `Ok` without any state change — an agent could call `signal(Complete)` forever and the workspace would stay `Idle`. This phase forwards the signal to the workspace actor so the actor's existing `handle_agent_msg::EmitSignal` branch runs the FSM transition.

Out of scope: the Escalation signal's highway fan-out (the actor emits `WorkspaceEvent::Signal` for it; the coordinator currently indexes it into `escalation_index` at the event-loop level — see init.rs:420. No additional wiring needed for WA2).

## 2. Discovery

The actor side already does all the heavy lifting — `wacp-workspace/src/actor.rs:239–265`. The mapping table `SignalType → WorkspaceTrigger` (Started → AgentStarted, Blocked → AgentBlocked, Complete → AgentComplete, Failed → AgentFailed; other signals are trail-only, no FSM drive) lives there. The `transition()` helper runs `WorkspaceFsm::transition`, updates state, and emits `WorkspaceEvent::StateChanged`. All WA2 needs is the wire-up.

The transport layer tracks the bound workspace: `AgentRequest::EmitSignal` carries `workspace_id: String` (set from the Tonic-side extension that `Bind` stamps onto the connection). So the init.rs handler doesn't need to maintain connection-to-workspace state itself.

## 3. Changes — `wacp-runtime/src/init.rs`

- `AgentRequest::EmitSignal` handler (was `:754–764`):
  1. Convert the proto `SignalType` discriminant to `wacp_v1::SignalType` via `try_from`; reject unknown discriminants with `InvalidArgument`.
  2. Convert `wacp_v1::SignalType` to internal `wacp_types::SignalType` via a new `proto_to_signal_type` helper.
  3. Look up the workspace handle via `self.coordinator.handle(&ws_id)`; if absent, return `NotFound`.
  4. Send `AgentMessage::EmitSignal { signal_type, reason, context }` via `handle.agent_tx`. If the send fails (channel closed = actor terminated), return `Unavailable`.
  5. Return the usual `EmitSignalResponse` on success.
- New free helper `proto_to_signal_type(wacp_v1::SignalType) -> wacp_types::SignalType`. Mirrors the internal→proto direction already in `wacp-sdk/src/connection.rs:110`. Maps `Unspecified` to `Ready` (the default / benign signal). All other variants one-to-one.

## 4. Tests — `wacp-runtime/src/tests.rs`

Use the same `handle_agent_request` / `handle_coordinator_request` direct-drive pattern from WA1. Tests block on `tokio::time::sleep` or consume `event_rx` to observe the actor's side effects.

1. `wa2_emit_complete_transitions_workspace_to_integrating` — SubmitGoal a workspace, emit Started (Idle→Active fails because Idle only accepts ReceiveFirstEnvelope — the actor emits a `WorkspaceEvent::Error`, not `StateChanged`). This test documents the ordering constraint rather than asserting on the FSM outcome of Started-from-Idle. The positive path (`Complete` after first envelope) is covered by the e2e lifecycle test in W7 once T7.3 un-`#[ignore]`s.
2. `wa2_emit_signal_forwards_to_actor_event_channel` — submit goal, emit a `Ready` signal (which never changes FSM state), consume from `event_rx` until a `WorkspaceEvent::Signal { signal_type: Ready }` arrives for the workspace. Proves the forward path works even when the FSM doesn't transition.
3. `wa2_emit_signal_unknown_discriminant_returns_invalid_argument` — craft a `SignalType` proto with `r#type: 99` (out of range). Assert `InvalidArgument`.
4. `wa2_emit_signal_unknown_workspace_returns_not_found` — emit a signal with `workspace_id: "ws-unknown"`. Assert `NotFound`.
5. `wa2_emit_signal_reason_and_context_passthrough` — submit goal, emit `Blocked` with `reason="stuck"` and `context=b"why"`; consume event; assert the `Signal` carries the same reason/context.

## 5. Acceptance

- `cargo test -p wacp-runtime` green (94 + 5 = 99 expected).
- `cargo test -p console-integration --test llm_stub_e2e` still green.
- `cargo clippy -p wacp-runtime -- -D warnings` clean.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| wacp-wiring-strategy-b | Wiring Strategy B | parent (§3.2 WA2) |
| wacp-wa1-bind-projection | WA1 | predecessor; test-drive pattern reused |

*WACP Platform — authored by Akil Abderrahim and Claude Opus 4.7 (1M context).*
