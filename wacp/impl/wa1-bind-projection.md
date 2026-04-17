---
id: wacp-wa1-bind-projection
type: coding
status: final
created: 2026-04-17T00:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [runtime, agent-service, bind, wa1]
depends_on: [wacp-wiring-strategy-b]
---

# Coding Spec — WA1: Bind Projects WorkspaceConfig

## 1. Scope

`AgentService::Bind` currently returns a stripped `BindResponse` with empty `role`, no `directive`, no `context`, no `visibility`, no `authority`, no `budget`. An agent that binds has nothing to act on. This phase populates every field of the response from the `WorkspaceConfig` that was passed to `coordinator.dispatch()` at workspace-creation time.

Out of scope: `auth_token` enforcement (Bind currently ignores it; that is deferred — see audit §13.7.6b follow-up list), FSM drive from signals (WA2), and checkpoint fan-out (WA3).

## 2. Data flow

`WorkspaceConfig` lives in `wacp_workspace::state`. It is consumed by `WorkspaceState::new(config)` inside `WorkspaceActor::spawn`, after which the fields live behind the actor's mpsc — not reachable from the init.rs request loop. Three options, chosen: cache a `Clone` of the config at creation time on `Runtime`, read on Bind, drop on Terminated. The cache is a `HashMap<String, WorkspaceConfig>` keyed by the workspace id string.

**Why cache, not a new `CoordinatorCommand::GetBindInfo`.** The config is effectively immutable post-spawn (the actor mutates `working_memory` / `inbox` / `checkpoint_register`, never the bind-relevant fields). A synchronous read from a HashMap is O(1); a new oneshot round-trip through the actor adds a context switch per Bind and complicates the actor's public API.

## 3. Changes

### 3.1 `wacp-runtime/src/init.rs`

- Add `use wacp_workspace::state::WorkspaceConfig;` at the top.
- Add `workspace_configs: HashMap<String, WorkspaceConfig>` field on `Runtime` (next to `workspace_timestamps`).
- Initialize to `HashMap::new()` in both `Runtime::init` (around `:268`) and `Runtime::init_in_memory` (around `:318`).
- In `CoordinatorRequest::SubmitGoal` at `:1349`: after `let ws_config = WorkspaceConfig { … };`, insert `self.workspace_configs.insert(ws_id.to_string(), ws_config.clone());` before the `self.coordinator.dispatch(DispatchRequest { task_id, config: ws_config })` call.
- Same insert in `CoordinatorRequest::Dispatch` at `:1434`.
- In `AgentRequest::Bind` at `:736`: after the tree lookup, look up the cached config. If present, populate `role`, `directive`, `context`, `visibility`, `authority`, `budget` from it. If absent (workspace was created via a path not yet cached), fall back to the current empty response — don't error.
- In the event-loop handler for `WorkspaceEvent::Terminated` at `:498`: `self.workspace_configs.remove(archived.id.as_ref());`

### 3.2 `BindResponse` projection

| proto field | source on `WorkspaceConfig` | conversion |
|---|---|---|
| `workspace_id` | `node.id` / config.id | `.to_string()` |
| `state` | tree `node.status` | `as i32` |
| `role` | `config.role` | `.clone()` |
| `directive` | `config.directive` (internal `Envelope`) | reuse the pattern at `notify_envelope_subs` (init.rs:659–673) |
| `context` | `config.context` | `.clone()` |
| `visibility` | `config.visibility` (HashSet<String>) | `.iter().cloned().collect()` |
| `authority` | `config.authority` | same |
| `budget` | `config.budget` (Option<ResourceBudget>) | `.as_ref().map(…)` into proto `ResourceBudget` |

Internal-to-proto envelope conversion is lifted into a free helper `fn envelope_to_proto(env: &Envelope) -> wacp_transport::wacp_v1::Envelope` to avoid duplicating the field list. Put it near `notify_envelope_subs` (private to the module).

Budget conversion: reuse the pattern at init.rs:1184–1198 (lifted into a helper `fn budget_to_proto(b: &ResourceBudget, warning_threshold: f32) -> wacp_transport::wacp_v1::ResourceBudget` if it simplifies, or inlined — WorkspaceConfig's budget has no warning_threshold so pass a zero default).

### 3.3 Tests — `wacp-runtime/src/tests.rs`

Three new cases:

1. `bind_returns_populated_fields_after_submit_goal` — in-memory runtime, call `SubmitGoal`, then Bind on the returned `root_workspace_id`, assert every field matches what SubmitGoal constructed.
2. `bind_returns_populated_fields_after_dispatch` — same but via `SubmitGoal` → `Dispatch`.
3. `bind_after_terminate_falls_back_empty` — dispatch, abort workspace (which produces Terminated), bind again → fields are empty (cache was cleaned up).

Optional fourth case: `bind_unknown_workspace_returns_not_found` — already covered today; ensure it still fails correctly.

## 4. Acceptance

- `cargo build -p wacp-runtime` and `cargo test -p wacp-runtime` green.
- `cargo test -p console-integration --test llm_stub_e2e` still green (no regression — the I6 test binds with a valid workspace but does not currently assert on the bind response shape).
- `cargo clippy -p wacp-runtime -- -D warnings` clean.

## 5. References

| ID | Title | Relationship |
|----|-------|--------------|
| wacp-wiring-strategy-b | Wiring Strategy B | parent (§3.1 WA1) |
| wacp-impl-runtime | WACP Runtime | constrains (all changes in `init.rs`) |

*WACP Platform — authored by Akil Abderrahim and Claude Opus 4.7 (1M context).*
