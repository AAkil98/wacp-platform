---
id: wcon-w2-proto-shapes
type: impl
status: final
created: 2026-04-15T05:45:00
revised: 2026-04-15T05:45:00
authors: [AAkil98, Claude Opus 4.6]
tags: [w2, proto, review, coordinator, agent]
depends_on: [wcon-w2-launch-flow]
---

# W2 — Proto-Shape Review (gate for session_launcher.rs)

> Review of the `CoordinatorService` and `AgentService` proto surfaces consumed by the launch flow. Cites proto file + line number for every request / response field the launcher depends on. This note is the W2.1 deliverable — the spec at `wcon-w2-launch-flow.md` gates impl on this doc landing first.

## Table of Contents

- 1. Scope & Outcome
- 2. SubmitGoal
- 3. Decompose
- 4. Dispatch
- 5. SendDirective
- 6. AbortWorkspace
- 7. Runtime-Side Behavioral Quirks (important)
- 8. Corrected Launch Sequence
- 9. Error Codes We Expect

---

## 1. Scope & Outcome

**Scope:** determine the exact RPC sequence, request/response shapes, and error semantics used by `SessionLauncher::launch()`. The original `wcon-w2-launch-flow` spec was sketched before reading proto — this note corrects two misreadings:

- There is no `CreateSession` RPC. `SubmitGoal` itself creates the goal and dispatches a root workspace atomically on the runtime side.
- `SendEnvelope` on **AgentService** is agent-to-agent messaging — not the launch directive. The launch directive is carried *inside* `DispatchRequest.directive_payload` and by `CoordinatorService::SendDirective` for subsequent directives.

**Outcome:** the corrected launch sequence (§8) uses 4 RPCs total — `SubmitGoal`, `Decompose`, `Dispatch × N`, (no `SendDirective` needed at launch; the directive ships with `Dispatch`) — plus a finalize transaction. On failure, `AbortWorkspace` rolls back.

## 2. SubmitGoal

**Proto:** `wacp/proto/coordinator.proto:11`, `60-69`.

```proto
rpc SubmitGoal(SubmitGoalRequest) returns (SubmitGoalResponse);

message SubmitGoalRequest {
    string description = 1;
    bytes context = 2;
    string client_request_id = 3;
}

message SubmitGoalResponse {
    string goal_id = 1;
    string root_workspace_id = 2;
}
```

**Runtime handler:** `wacp/crates/wacp-runtime/src/init.rs:1314-1392`. Allocates a goal id, creates a root task, dispatches a "worker"-role workspace bound to that task, binds task → workspace, and advances task state `Draft → Pending → Assigned`. The `context` bytes become the initial directive envelope payload.

**Fields the launcher sets:**
- `description` — session display name or goal description. Source: `sessions.name` (fall back to `sessions.workflow` if name is null).
- `context` — serialized session context bytes. Source: `sessions.context` (a JSON string column; serialize to bytes as-is).
- `client_request_id` — UUIDv4, for idempotency tracing.

**Fields the launcher reads:**
- `root_workspace_id` — persist to `sessions.coordinator_workspace_id`.
- `goal_id` — currently unused by the console (the runtime-side task graph owns the goal).

**Errors observed:** the handler only returns `Ok(_)` on the happy path and `Internal` on task-graph insertion failure (very rare — happens on duplicate goal id, which is monotonic so shouldn't). No auth, no validation. Map `Internal` → `LaunchError::Step { step: SubmitGoal, recoverable: false }`.

## 3. Decompose

**Proto:** `wacp/proto/coordinator.proto:14`, `71-87`.

```proto
rpc Decompose(DecomposeRequest) returns (DecomposeResponse);

message TaskDefinition {
    string name = 1;
    string description = 2;
    repeated string depends_on = 3;   // task IDs
    string role = 4;
    bytes directive_payload = 5;
    repeated string tools = 6;        // tool names to mount
}

message DecomposeRequest {
    repeated TaskDefinition tasks = 1;
    string client_request_id = 2;
}

message DecomposeResponse {
    repeated string task_ids = 1;
}
```

**Runtime handler:** `wacp/crates/wacp-runtime/src/init.rs:1531-1569`. Iterates `request.tasks`, inserts each into the task graph, approves each (so status becomes `Pending` / dispatchable). Returns the allocated task ids in the **same order** as the request vector (confirmed by inspection — `task_ids.push(tid.to_string())` after each insert).

**Important behaviors:**
- If `add_task` fails for a given definition, the handler logs a `warn!` and **skips** that task; the response vector will be *shorter* than the request. The launcher must check `response.task_ids.len() == request.tasks.len()` and treat a mismatch as a partial-decompose failure.
- `depends_on` refers to task ids from the *same* request batch (or a prior Decompose). For W2, the console submits one flat batch of independent tasks (one per assignment), so `depends_on` stays empty for all entries.

**Fields the launcher sets per assignment:**
- `name` — `"{session.workflow}:{assignment.role_ref}"`.
- `description` — free-form; use the role's human label from taxonomy (or `assignment.role_ref` if no label).
- `depends_on` — empty (flat fan-out).
- `role` — `assignment.role_ref`.
- `directive_payload` — the serialized directive blob (§4 below).
- `tools` — `profile.effective_tools()` (allowlist-minus-denylist intersected with the vertical's tool catalog). Falls back to `profile.tool_allowlist` parsed JSON; empty vector if the profile has neither.

**Ordering note:** the launcher builds `TaskDefinition`s in `slot_position` order (from `session_assignments.slot_position` ASC) so the returned `task_ids` map positionally to assignments.

## 4. Dispatch

**Proto:** `wacp/proto/coordinator.proto:25`, `116-128`.

```proto
rpc Dispatch(DispatchRequest) returns (DispatchResponse);

message DispatchRequest {
    string task_id = 1;
    string role = 2;
    bytes directive_payload = 3;
    repeated string tools = 4;
    ResourceBudget budget = 5;
    string client_request_id = 6;
}

message DispatchResponse {
    string workspace_id = 1;
    string task_id = 2;
}
```

**Runtime handler:** `wacp/crates/wacp-runtime/src/init.rs:1418-1477`. Validates `task_id` exists in the graph (else returns `NotFound`), dispatches a new workspace with `role` and `directive_payload` in the initial directive envelope, binds `task → workspace`, advances task state to `Assigned`.

**Important behavior — tools + budget are IGNORED:**

The handler at `init.rs:1418-1477` does NOT copy `request.tools` or `request.budget` into the `WorkspaceConfig`. Both fields are silently dropped. The workspace is always spawned with empty visibility / authority / budget. Consequences:

- Per-assignment tool restrictions from the profile do not take effect yet.
- Per-assignment resource budgets do not take effect yet.

**This is a runtime-side gap, not a console bug.** The console should still send these fields (contract compliance — the RPC accepts them and forwards-compat matters). The gap is tracked for a future W-phase on the wacp side; for W2 the acceptance bar cannot assert tools/budget reach the worker.

**Fields the launcher sets per assignment:**
- `task_id` — from `Decompose` response, positionally.
- `role` — `assignment.role_ref`.
- `directive_payload` — same JSON blob as the Decompose `TaskDefinition.directive_payload`. Serialized as `serde_json::to_vec(&DirectivePayload { llm_provider, llm_model, llm_temperature, llm_max_tokens, autonomy, tool_refs, per_assignment_context })`.
- `tools` — profile tool refs (same list as Decompose).
- `budget` — `ResourceBudget { max_tokens, max_wall_time_ms, max_cost_micros, warning_threshold, … }` derived from the assignment's budget overrides (if set) else the session's budget, defaulting to zero-valued fields where the profile has no value. `max_storage_bytes` and `max_network_bytes` are not in the profile — default 0.
- `client_request_id` — per-dispatch UUIDv4.

**Response:** `workspace_id` persisted to `session_assignments.workspace_id`. `task_id` echoed back is useful for log correlation only.

**Errors observed:**
- `NotFound` — `task_id` not in graph. Shouldn't happen post-Decompose unless a race with an external cancel; map to `LaunchError::Step { step: Dispatch, recoverable: false }`.
- Anything else → wrap as `LaunchError::Step { step: Dispatch, reason, source: Some(status), recoverable: status.code().is_transient() }`.

## 5. SendDirective

**Proto:** `wacp/proto/coordinator.proto:39`, `154-162`.

```proto
rpc SendDirective(SendDirectiveRequest) returns (SendDirectiveResponse);

message SendDirectiveRequest {
    string workspace_id = 1;
    bytes payload = 2;
    string client_request_id = 3;
}

message SendDirectiveResponse {
    string envelope_id = 1;
}
```

**Not used at launch.** `Dispatch` already ships the initial directive via `directive_payload`. `SendDirective` is for *subsequent* directives from the coordinator to a live workspace (e.g., mid-session course correction). W4 (directive injection) is the likely consumer.

## 6. AbortWorkspace

**Proto:** `wacp/proto/coordinator.proto:28`, `130-136`.

```proto
rpc AbortWorkspace(AbortWorkspaceRequest) returns (AbortWorkspaceResponse);

message AbortWorkspaceRequest {
    string workspace_id = 1;
    string reason = 2;
    string client_request_id = 3;
}

message AbortWorkspaceResponse {}
```

**Runtime handler:** `wacp/crates/wacp-runtime/src/init.rs:1478-1484`. Delegates to `self.coordinator.abort_workspace(&ws_id).await` which is infallible from the RPC's perspective — always replies `Ok(())`.

**Rollback semantics.** Used by the launcher's rollback path to tear down workspaces created by an incomplete launch. Tolerated individually: a single abort failure logs but does not halt the rollback chain (spec §4.2).

## 7. Runtime-Side Behavioral Quirks (important)

These are specific to the current `wacp-runtime` implementation; the launcher codes against the proto contract, but tests / assertions must account for them.

| Quirk | Where | Consequence for W2 |
|-------|-------|--------------------|
| `SubmitGoal` auto-dispatches a "worker"-role root workspace | `init.rs:1349-1377` | The root workspace does real work, not just coordinate. Console models this as `sessions.coordinator_workspace_id`, but it's actually a worker. Adjust naming if confusion surfaces (W3/W5). |
| `Dispatch` ignores `request.tools` and `request.budget` | `init.rs:1418-1477` | Per-assignment tool restrictions + budgets do not flow to workers yet. Launcher tests cannot assert these reached the worker. |
| `Decompose` silently skips tasks that fail `add_task` | `init.rs:1553-1565` | Response vector can be shorter than request. Launcher MUST verify lengths match. |
| `SubmitGoal` uses `request.context` as the initial directive envelope payload | `init.rs:1361` | Session's `context` JSON becomes the root workspace's first directive. Document this so downstream W3/W4 understand the semantics. |
| Workspace IDs are monotonic `"ws-{n}"`; task IDs are `"task-{goal-N}"` or `"task-decompose-{n}"` | `init.rs:1347, 1320, 1536` | Stable within a single runtime process, not UUIDs. For DB foreign-key correlation this is fine because we just store opaque strings. |

## 8. Corrected Launch Sequence

Revised 5-step view for the launcher:

```
Step 1: SubmitGoal
        req:  description = session.name | session.workflow
              context     = session.context (bytes)
              client_request_id = uuid()
        resp: goal_id, root_workspace_id
        persist: sessions.coordinator_workspace_id = root_workspace_id

Step 2: Decompose
        req:  tasks = [TaskDefinition for each assignment, slot_position order]
                 each: name, description, role, directive_payload, tools
              client_request_id = uuid()
        resp: task_ids (length MUST equal assignments length; else partial-decompose failure)

Step 3: For i in 0..assignments.len():
        Dispatch
        req:  task_id = task_ids[i]
              role = assignments[i].role_ref
              directive_payload = same as step 2
              tools = profile.effective_tools()
              budget = resolved ResourceBudget
              client_request_id = uuid()
        resp: workspace_id
        defer persist until Step 5 (transactional finalize)

Step 4: (no-op — SendDirective is not used at launch; Dispatch carries the directive)

Step 5: Finalize — single SQLite transaction:
        UPDATE sessions
          SET state = 'active',
              coordinator_workspace_id = <from step 1>,
              launched_at = now()
          WHERE id = :session_id AND state = 'launching';
        UPDATE session_assignments
          SET workspace_id = <from step 3>
          WHERE id = :assignment_id;   -- per assignment

        If UPDATE sessions affects 0 rows → someone cancelled mid-launch. Rollback all workspaces.
```

On any step failure at or after Step 3 (workspaces exist), rollback:
- Collect every workspace id created so far (root + dispatched[0..i-1]).
- `AbortWorkspace(workspace_id, reason="launch_rollback: <original-reason>")` for each. Tolerate per-call failure.
- Session transitions `LAUNCHING → FAILED` with `reason = "launch_step_{step}: {original_reason}"`.

If Step 2 fails, rollback aborts only the root workspace (created in Step 1).
If Step 1 fails, no rollback needed — just transition to FAILED.

## 9. Error Codes We Expect

Mapping from `tonic::Code` → `LaunchError::Step { recoverable }` for the launcher's error mapper:

| Code | Recoverable? | HTTP |
|------|--------------|------|
| `Unavailable`, `DeadlineExceeded`, `ResourceExhausted` | yes | 503 |
| `NotFound` (task/workspace missing) | no | 502 Bad Gateway (runtime-side state bug) |
| `FailedPrecondition` (illegal transition) | no | 409 Conflict |
| `InvalidArgument` (bad payload) | no | 500 (launcher built a bad request — reportable bug) |
| `Internal`, everything else | no | 502 |

The handler translates `LaunchError::Step { recoverable: true }` → `ApiError::ServiceUnavailable`; `recoverable: false` → `ApiError::BadGateway` or more specific if available.

---

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-w2-launch-flow | W2 — Launch Flow | parent (this note is the §4.0 review artifact) |
| wcon-sessions | Session System | constrains (§5.3 launch data flow) |
| wcon-wiring-phases | Wiring Phases | parent (§3 W2 row) |

*WACP Workspace — authored by AKIL Abderrahim and Claude Opus 4.6*
