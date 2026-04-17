---
id: wacp-wa3-6-auto-integration
type: coding
status: final
created: 2026-04-17T00:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [coordinator, integration, wa3-6]
depends_on: [wacp-wiring-strategy-b, wacp-wa2-emit-signal-fsm]
---

# Coding Spec — WA3.6: Auto-Integration on Complete

## 1. Scope

WA2 wired `EmitSignal` so an agent's `Complete` signal advances the workspace FSM `Active → Integrating`. Nothing then drives `Integrating → Closed`. The runtime-side `CoordinatorService::TriggerIntegration` (`init.rs:1849`) is the only producer of `CoordinatorCommand::IntegrationSucceeded`, and no caller invokes it automatically when an agent finishes.

WA3.6 closes that gap entirely on the coordinator side: when `WorkspaceEvent::StateChanged { to: Integrating }` fires, the coordinator runs `IntegrationEngine::integrate` against the workspace's last checkpoint and dispatches the corresponding command back to the actor.

### 1.1 In scope

1. New `last_checkpoint: HashMap<String, Checkpoint>` field on `Coordinator`, populated from `WorkspaceEvent::CheckpointCreated`, removed on `WorkspaceEvent::Terminated`.
2. `Coordinator::handle_event` becomes `async`. The Integrating branch calls a new private `auto_integrate(workspace_id)` which:
   - If a cached checkpoint exists: builds an `IntegrationRequest { workspace_id, strategy: Direct, mode: Normal, checkpoint }`, calls `IntegrationEngine::integrate`, and maps the result to the matching `CoordinatorCommand` (`IntegrationSucceeded` / `ConflictDetected` / `IntegrationFailed`).
   - If no cached checkpoint exists: falls back to `CoordinatorCommand::IntegrationSucceeded`. Mirrors the blind behaviour of `TriggerIntegration` so an agent that emits `Complete` without ever checkpointing still terminates cleanly.
3. All `coordinator.handle_event(&event)` call sites updated to `.await` (`init.rs` × 2, `coordinator/src/tests.rs`, `runtime/src/tests.rs` × 3).

### 1.2 Out of scope

- Real conflict detection. `IntegrationEngine::integrate` (`integration.rs:53`) currently returns `Success` for any `Normal` mode; the call is structural so future engine work can thread real outcomes through here without further changes.
- Strategy selection from confidence (the engine has `IntegrationPipeline::select_strategy` for this; not used in v1 because the engine ignores the strategy in `Normal` mode).
- Conflict resolution wiring (`ConflictResolver`, `ConflictResolution`). When the engine starts producing `Conflict(_)` results, follow-up code can route into the conflict resolver from the same `auto_integrate` path.

## 2. Changes — `wacp-coordinator/src/orchestrator.rs`

- Add `use crate::integration::{IntegrationEngine, IntegrationRequest, IntegrationResult};`.
- Add `last_checkpoint: HashMap<String, Checkpoint>` field; default `new()` initializes empty.
- Convert `handle_event` to `async fn`, add the `Integrating` and `CheckpointCreated` arms, extend the existing `Terminated` arm to clear the checkpoint cache for that workspace.
- Add private `async fn auto_integrate(&mut self, workspace_id: &WorkspaceId)`.

## 3. Caller updates

`async` propagation, mechanical:

| File | Line(s) | Change |
|---|---|---|
| `wacp-runtime/src/init.rs` | 354, 725 | `handle_event(&event).await` |
| `wacp-runtime/src/tests.rs` | 151, 239, 443 | `handle_event(&event).await` |
| `wacp-coordinator/src/tests.rs` | 4003 (drain helper) | `handle_event(&event).await` |

No public-API breakage outside the `&mut self` receiver becoming async — there is no external caller of this method outside the workspace.

## 4. Tests

### 4.1 `wacp-coordinator/src/tests.rs`

1. `wa3_6_caches_checkpoint_from_event` — synthesize a `WorkspaceEvent::CheckpointCreated`; call `handle_event(&event).await`; access cache via a new test-only helper (`coordinator.last_checkpoint_for_test(ws_id)`) that returns `Option<&Checkpoint>`.
2. `wa3_6_terminate_clears_checkpoint_cache` — push a checkpoint then send `Terminated`; cache entry gone.
3. `wa3_6_integrating_with_no_checkpoint_sends_success` — synthesize `StateChanged { to: Integrating }` for a workspace registered via `dispatch`; assert the workspace handle's `coordinator_tx` receives `IntegrationSucceeded`.
4. `wa3_6_integrating_with_checkpoint_sends_success` — push a `CheckpointCreated`, then `StateChanged { to: Integrating }`; assert `IntegrationSucceeded` (engine's default for Normal/Direct).

### 4.2 `wacp-runtime/src/tests.rs`

1. `wa3_6_complete_signal_drives_workspace_to_closed` — submit goal, deliver activation envelope, emit `Complete` signal via the agent handler, drain events, assert: `StateChanged(Active→Integrating)` followed by `StateChanged(Integrating→Closed)` then `Terminated(Closed)`.

## 5. Acceptance

- `cargo test -p wacp-coordinator` green (existing 383 + 4 = 387).
- `cargo test -p wacp-runtime` green (existing 108 + 1 = 109).
- `cargo test -p console-integration --test llm_stub_e2e` still green.
- `cargo clippy --workspace -- -D warnings` clean (production code).
- T7.3 un-`#[ignore]`s in the sweep once WA3.6 lands.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| wacp-wiring-strategy-b | Wiring Strategy B | parent (§3.3.6) |
| wacp-wa2-emit-signal-fsm | WA2 | predecessor; provides the Active→Integrating drive |

*WACP Platform — authored by Akil Abderrahim and Claude Opus 4.7 (1M context).*
