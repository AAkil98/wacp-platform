---
id: wcon-sessions
type: design
status: final
created: 2026-04-10T00:00:00
revised: 2026-04-14T00:00:00
authors: [AAkil98]
tags: [sessions, lifecycle, orchestration, coordination, vertical-context, ownership]
depends_on: [wcon-auth, wcon-architecture, wcon-profiles, wcon-discovery]
---

# WACP Console — Session Lifecycle

## Table of Contents

1. Overview
2. Session Configuration
3. Session Validation
4. Session Launch
5. Runtime Mapping
6. Session Monitoring
7. Session Teardown
8. Reconnection and Recovery
9. Concurrent Sessions
10. Invariants

---

## 1. Overview

A session is a user-initiated coordination run. It is the highest-level action the Console performs — the moment the user stops configuring and starts coordinating. Everything else in the Console (discovery, profiles) exists to support this: the user discovers what agents can do, configures how they should behave, and then launches a session to make it happen.

The session lifecycle has five phases:

1. **Configure** — the user selects a vertical, a workflow, assigns profiles to role slots, and optionally sets budget overrides.
2. **Validate** — the backend checks that the configuration is complete and consistent.
3. **Launch** — the backend translates the configuration into WACP runtime operations (workspace creation, directive delivery, stream subscription).
4. **Monitor** — the backend tracks workspace states, task progress, and highway events, relaying them to the frontend in real-time.
5. **Teardown** — the session reaches a terminal state (completed, failed, or cancelled) and the backend cleans up runtime subscriptions.

The session schema and state machine are defined in `wcon-data-model` §4. The profile-to-WACP mapping is defined in `wcon-profiles` §4. This spec defines the behavioral rules for each phase: what happens, in what order, what can go wrong, and what the user sees.

Every session carries an `owner_user_id` — the authenticated user who created it (`wcon-auth` §5.1). Authorization governs who can create, view, act on, and cancel sessions: operators manage their own sessions; admins manage all sessions (`wcon-auth` §4.2).

## 2. Session Configuration

Configuration is the pre-launch phase where the user assembles all the pieces. The session is in the `configuring` state throughout.

### 2.1 Configuration Steps

The session launcher (`wcon-architecture` §4.2) guides the user through a sequential flow. Steps can be revisited in any order while the session is in `configuring` state.

**Step 1: Select vertical**

The user picks a vertical from the vertical registry. The selection populates the workflow dropdown (step 2), the vertical context form (step 4), and constrains the role slots (step 3).

- Source: `GET /api/verticals` (backed by the taxonomy index, which in turn reads from the runtime's `GET /v1/verticals` per `wcon-discovery` §2.2)
- Constraint: at least one vertical must be available. If the vertical registry is empty, the session launcher is disabled (`wcon-discovery` §8.1).
- Each vertical card shows its `name`, `defining_constraint`, and summary counts (tasks/workflows/tools). See `wcon-ui` §6.2 step 1 for the rendering.
- Changing the vertical resets steps 2–5.

**Step 2: Select workflow**

The user picks a workflow from the selected vertical's workflow list. The selection defines the role slots that need profile assignments.

- Source: `GET /api/verticals/:id/workflows` (per-vertical endpoint) — returns `WorkflowSummary[]` from the manifest (`wcon-discovery` §2.2.2).
- The workflow summary carries `stage_count` and `gated_stage_count` but **not** per-stage detail (stages, their roles, their dependencies). See §2.4 for how the Console derives role slots in the absence of per-stage metadata.
- Changing the workflow resets steps 3–5.

**Step 3: Assign profiles to role slots**

For each role slot the Console has derived (see §2.4), the user assigns a profile from the profile library.

- The profile selector filters to profiles whose `role_ref` matches the slot's role.
- If no matching profiles exist for a slot, the user can create one inline (opens the profile editor with the role pre-selected; the new profile is saved to the library and immediately assigned).
- Multiple slots for the same role can use the same profile or different profiles.
- Each assignment pins the profile's current version (`wcon-profiles` §5.4).

**Step 4: Vertical context**

The user supplies values for the vertical's context fields. This step is dynamic: its form is populated from `VerticalEntry.context_schema` (`wcon-data-model` §6.1) for the selected vertical.

- **Skip condition:** if the selected vertical's `context_schema` is empty (e.g., SWE), this step is automatically skipped. The wizard progress bar still shows the step but with a "not required" label.
- **Per-field rendering:** each entry in `context_schema` becomes one form field. The widget is determined by `ContextField.type`:

  | `type` | Widget |
  |--------|--------|
  | `"string"` | Text input |
  | `"number"` | Number input (integer or decimal per description) |
  | `"boolean"` | Toggle |
  | `"enum"` | Dropdown with the entries in `enum_values` |

  The `description` is shown as helper text. The `default` (if present) pre-populates the field.
- **Required vs optional:** fields with `required: true` block progression to step 5 until filled. Fields with `required: false` can be left blank.
- **Examples per vertical:**
  - **SWE:** empty — step is skipped.
  - **DevOps:** `environment` (enum: `dev` / `staging` / `production`, required). UI shows a warning banner when `production` is selected (`wcon-ui` §6.2).
  - **MLOps:** `compute_budget` (number, required) — units are GPU-hours per the field's `description`. Enforced at the tool layer by `tool_policies.train_launch.kind == "budget_limited"` with `budget_field: max_hours`.
  - **Finance:** `compliance_scope` (string, required) and `jurisdiction` (enum: `SEC` / `FINRA` / `MiFID II` / `FCA` / `other`, required).
  - **Healthcare:** `phi_access_basis` (enum: `consent` / `de_identified`, required). Follow-up fields depend on the selected value and are collected at the runtime's tool-layer refusal time, not in this step — the Console captures only the basis.
  - **Analytics:** `data_snapshot_id` (string, required).
  - **DataSci:** `hypothesis_framework` (typically a template identifier — string, required).

  Exact field names, types, and required flags are sourced from the live manifest. The Console does not hardcode them — the list above is illustrative of the current ecosystem.

- **Validation:** client-side validation checks `required`, type coercion, and (for enums) membership in `enum_values`. Server-side validation runs again at §3.1 launch time.

**Step 5: Set overrides (optional)**

The user can override budget limits at the session level or per-assignment level.

- Session-level overrides apply to all assignments unless overridden per-assignment.
- Per-assignment overrides apply to a single role slot.
- Budget precedence: assignment override → session override → profile default → no limit (`wcon-profiles` §4.3).
- Override fields: `budget_max_cost_micros`, `budget_max_tokens`, `budget_max_wall_time_ms`.
- **MLOps note:** MLOps GPU-hour budget is **not** part of `ResourceBudget`. It is captured in step 4 as `compute_budget` and delivered to the runtime as session context (§4.1 step 3). Editing it here would be a mistake — the override fields in step 5 are Console-enforced resource limits, not vertical-specific compute metrics.

### 2.2 Configuration API

**Create session:**

`POST /api/sessions`

```json
{
  "vertical": "finance",
  "workflow": "finance:trade-execution",
  "context": {
    "compliance_scope": "equities",
    "jurisdiction": "SEC"
  }
}
```

**Authorization:** requires `operator` or `admin` console role. The session's `owner_user_id` is set to the authenticated user.

Returns `201 Created` with a session in `configuring` state. The `vertical`, `workflow`, and `context` are recorded but not yet validated against the taxonomy. The `context` field is optional at create time — the user may supply it now or update it later via `PATCH`. It is required to be complete (per §3.1 `MISSING_CONTEXT` / `INVALID_CONTEXT` checks) before launch.

A session for a vertical with empty `context_schema` (e.g., SWE) may omit `context` entirely:

```json
{
  "vertical": "swe",
  "workflow": "swe:implement-feature"
}
```

**Update assignments:**

`PUT /api/sessions/:id/assignments`

**Authorization:** the authenticated user must be the session's owner or an admin.

**Mode B request (no per-stage data, current default):**

```json
{
  "assignments": [
    {
      "role_ref": "finance:analyst",
      "profile_id": "uuid-1"
    },
    {
      "role_ref": "finance:portfolio_manager",
      "profile_id": "uuid-2",
      "budget_max_cost_micros": 200000
    }
  ]
}
```

**Mode A request (when per-stage workflow data is available):**

```json
{
  "assignments": [
    {
      "role_ref": "finance:analyst",
      "stage_id": "analyze",
      "profile_id": "uuid-1"
    },
    {
      "role_ref": "finance:compliance_officer",
      "stage_id": "compliance",
      "profile_id": "uuid-3"
    },
    {
      "role_ref": "finance:analyst",
      "stage_id": "review",
      "profile_id": "uuid-4"
    }
  ]
}
```

Replaces all assignments for the session. Each assignment pins the profile's current version at the time of the call. The `slot_position` is assigned by the backend from the array order — the client does not send it; the backend's zero-based index over the request's `assignments` array becomes the stored `slot_position`.

**Mode selection rule.** If any assignment in the request body carries a `stage_id` field, the request is treated as Mode A and **every** assignment must carry a `stage_id` that matches a stage in the selected workflow (`ROLE_MISMATCH` fires at validation otherwise). If no assignment carries `stage_id`, the request is treated as Mode B and the Console records NULL `stage_id` for every row. Mixing modes within a single request is rejected with `422 Unprocessable Entity` at the API layer (before full validation).

**Update session overrides and context:**

`PATCH /api/sessions/:id`

```json
{
  "budget_max_cost_micros": 1000000,
  "budget_max_wall_time_ms": 600000,
  "context": {
    "compliance_scope": "fixed-income",
    "jurisdiction": "FINRA"
  }
}
```

Sets session-level budget overrides and/or replaces the entire context map. Partial context updates are not supported — the `context` field in a PATCH body replaces the stored value wholesale. To clear a single field, the client sends the full context map with the field omitted or set to `null`.

Only valid while the session is in `configuring` state. After launch, `PATCH` rejects any change to `context` (or any other field covered by §10.2 immutability) with `409 Conflict`.

**Get session:**

`GET /api/sessions/:id`

Returns the full session record including assignments, resolved profile summaries, computed budget chain, and the vertical context map. The response annotates each context field with its `context_schema` metadata (type, required, enum values, description) so the frontend can render the review step (`wcon-ui` §6.2 step 6) without re-fetching the vertical manifest.

### 2.3 Configuration Constraints

| Constraint | Enforced at |
|-----------|-------------|
| Vertical must exist in taxonomy index | Validation (§3) |
| Workflow must exist in the selected vertical | Validation (§3) |
| Every role slot (per §2.4) must have an assigned profile | Validation (§3) — `MISSING_ASSIGNMENT` |
| Every required field in the vertical's `context_schema` must be set | Validation (§3) — `MISSING_CONTEXT` |
| Context field values must match their declared type/enum/range constraints | Validation (§3) — `INVALID_CONTEXT` |
| Assigned profiles must be valid (role_ref resolves, tools valid) | Validation (§3) |
| Profile `role_ref` must match the slot's role | Assignment time (§2.2) — the API rejects mismatched assignments immediately |

### 2.4 Role Slot Derivation

A "role slot" is a placeholder in the session's configuration that needs a profile assigned before launch. The Console derives role slots from the selected workflow using one of two modes, depending on what metadata is available:

**Mode A: Stage-aware (preferred when per-stage detail is available).**

If the Console has access to per-stage workflow metadata — via a supplementary endpoint the runtime may provide in the future, or a projection from the upstream TypeScript source during development — each stage becomes one role slot. The slot's role is the stage's `role_id`. Slots preserve stage order and carry the stage name for UI display (`wcon-ui` §6.2 step 3).

Stage-aware slot lists:
- Preserve workflow intent (user assigns a distinct profile per stage).
- Allow different profiles for the same role across different stages (e.g., a junior analyst for "analyze" and a senior analyst for "review").
- Enable stage-aware validation (ROLE_MISMATCH checks each slot against its stage's declared role).

**Mode B: Role-aware fallback (when per-stage detail is not available).**

When the Console does not have per-stage metadata — the current state of the REST contract, since `WorkflowSummary` carries only counts — role slots are derived from the vertical's role set. One slot per distinct role in `VerticalEntry.roles`. The slot's role is simply the role ID; no stage name is displayed.

Role-aware slot lists:
- Cover every role the vertical declares, regardless of which workflow is selected (a broader-than-strictly-needed view).
- Use the same profile for every stage that shares a role, which is the common case.
- Simplify the UI (`wcon-ui` §6.2 step 3) to a flat list without stage headers.

**Mode selection.** The Console picks Mode A if per-stage metadata is available for the selected workflow; otherwise Mode B. The choice is not user-visible except as UI differences in step 3. Both modes produce a valid `session_assignments` list that satisfies `MISSING_ASSIGNMENT` and `ROLE_MISMATCH` validation (§3.1).

**Transition path.** When the upstream manifest is extended to include per-stage detail (a future runtime enhancement), the Console will switch to Mode A uniformly. Existing Mode B sessions will not be affected — their assignment records are self-sufficient.

## 3. Session Validation

When the user triggers launch, the session transitions from `configuring` to `validating`. Validation is a synchronous gate — either all checks pass and the session proceeds to `launching`, or validation fails and the session returns to `configuring` with error details.

### 3.1 Validation Checks

Validation runs the following checks in order. All checks execute before returning — the response contains all violations, not just the first.

| # | Check | Error code | Detail |
|---|-------|-----------|--------|
| 1 | Vertical exists | `UNKNOWN_VERTICAL` | `vertical` must resolve in the taxonomy index |
| 2 | Workflow exists | `UNKNOWN_WORKFLOW` | `workflow` must exist in the vertical's workflow list |
| 3 | All role slots filled | `MISSING_ASSIGNMENT` | Every role slot (per §2.4: stage-aware in Mode A, role-aware in Mode B) must have a corresponding assignment |
| 4 | Profiles exist | `UNKNOWN_PROFILE` | Each assigned `profile_id` must exist in the profile library |
| 4a | Profiles not deleted | `DELETED_PROFILE_IN_ASSIGNMENT` | Each assigned profile must be live (`deleted_at IS NULL`). A profile deleted after assignment but before launch is caught here — the user must reassign the slot (see `wcon-profiles` §2.3 step 4 for the deletion-time warning) |
| 5 | Profile versions exist | `UNKNOWN_VERSION` | Each pinned `(profile_id, profile_version)` must exist |
| 6 | Role match | `ROLE_MISMATCH` | Every assignment's `profile.role_ref` must match the slot's declared role. In Mode A the slot's role comes from the workflow stage's `role_id`; in Mode B it comes from the vertical role the slot was derived from. In both modes the role_ref must also be a role declared by the selected vertical (or a base protocol role). |
| 7 | Profile validity | `INVALID_PROFILE` | Each profile must pass full profile validation (`wcon-profiles` §3) against the current taxonomy |
| 8 | Context required fields | `MISSING_CONTEXT` | Every field in the vertical's `context_schema` with `required: true` must be present and non-null in `session.context` |
| 9 | Context field values | `INVALID_CONTEXT` | Every present field in `session.context` must satisfy its `ContextField` constraints: strict type match (no coercion), `enum` values must appear in `enum_values`, numbers must be finite and not NaN, strings must be non-empty when required |
| 10 | Budget validity | `INVALID_BUDGET` | Budget overrides (session and assignment level) must be non-negative integers |
| 11 | Runtime reachable | `RUNTIME_UNREACHABLE` | The WACP runtime must be reachable via gRPC health check |

**Context validation detail.**

- `MISSING_CONTEXT` is emitted once per missing required field with details:

  ```json
  {
    "check": "MISSING_CONTEXT",
    "field": "jurisdiction",
    "required_by_vertical": "finance",
    "message": "Field 'jurisdiction' is required by vertical 'finance' but is not set"
  }
  ```

- `INVALID_CONTEXT` is emitted once per failing field with details including the expected type/constraint:

  ```json
  {
    "check": "INVALID_CONTEXT",
    "field": "jurisdiction",
    "value": "EU",
    "expected_type": "enum",
    "enum_values": ["SEC", "FINRA", "MiFID II", "FCA", "other"],
    "message": "Field 'jurisdiction' value 'EU' is not one of the allowed enum values"
  }
  ```

- Extra fields in `session.context` that are not declared in `context_schema` are **ignored**, not rejected. This preserves forward compatibility: the Console can accept context from a client that knows about a newer manifest version than the server, and the runtime will see all the fields regardless of whether the Console understands them. A soft warning (not a violation) is logged.

**Type matching is strict.** A `ContextField` with `type: "number"` rejects a JSON string even if it could be parsed as a number — the client must send a JSON number. A `ContextField` with `type: "boolean"` rejects `0`/`1`/`"true"`/`"false"` — it must be JSON `true` or `false`. A `ContextField` with `type: "enum"` rejects any value not case-sensitively present in `enum_values`. The Console does not normalize, coerce, or infer types. Clients (including the wizard UI) are responsible for sending correctly-typed values; widgets in `wcon-ui` §6.2 step 4 emit the right JSON types by construction.

The strictness choice is deliberate: coercion would mask schema drift (a field that changed from string to enum would silently still "work" for non-matching values), and the Console has no authoritative way to decide how to coerce between types without risking data loss.

### 3.2 Validation API

**Launch (triggers validation):**

`POST /api/sessions/:id/launch`

**Success path:** session transitions `configuring → validating → launching`. Returns `202 Accepted` with the session in `launching` state.

**Failure path:** session transitions `configuring → validating → configuring`. Returns `422 Unprocessable Entity` with violations:

```json
{
  "error": "validation_failed",
  "violations": [
    {
      "check": "MISSING_ASSIGNMENT",
      "message": "No profile assigned to role slot 'swe:tester' (stage 'test')",
      "role_ref": "swe:tester",
      "stage_id": "test"
    },
    {
      "check": "RUNTIME_UNREACHABLE",
      "message": "Cannot connect to WACP runtime CoordinatorService at [::1]:9092",
      "address": "[::1]:9092"
    }
  ]
}
```

In Mode B (§2.4) the `stage_id` field is omitted from `MISSING_ASSIGNMENT` and `ROLE_MISMATCH` violations — only `role_ref` is present, because there is no per-stage context. Clients must tolerate both shapes.

### 3.3 Taxonomy Snapshot

Validation checks the taxonomy index as it exists at the moment of validation. If a taxonomy reload occurs between configuration and launch, previously valid roles or tools may become invalid. This is correct — the user is launching against the current state of the system, not the state that existed when they started configuring.

## 4. Session Launch

Launch translates the validated session configuration into WACP runtime operations. The session is in the `launching` state throughout. This is the heaviest operation the Console performs — it involves multiple sequential gRPC calls that must all succeed for the session to become active.

### 4.1 Launch Sequence

```
 ┌─ 1. Create coordinator workspace
 │     └── CoordinatorService.CreateSession
 │           └── returns session_id, coordinator_workspace_id
 │
 ├─ 2. Submit goal
 │     └── CoordinatorService.SubmitGoal
 │           └── provides the workflow description as the top-level goal
 │
 ├─ 3. Create worker workspaces (one per role slot per §2.4)
 │     └── for each assignment:
 │           ├── CoordinatorService.Dispatch
 │           │     └── creates workspace with role binding + resource budget
 │           └── AgentService.SendEnvelope (directive)
 │                 └── delivers the directive payload with:
 │                       - LLM config, tools, task (`wcon-profiles` §4.2)
 │                       - context: session.context (pass-through, agent-visible)
 │
 ├─ 4. Subscribe to streams
 │     ├── HighwayService.StreamTrail
 │     ├── HighwayService.StreamGates
 │     ├── HighwayService.StreamEscalations
 │     └── HighwayService.StreamWorkspaceChanges
 │
 └─ 5. Transition to active
       └── session state → active, launched_at = now()
```

**Context delivery mechanics.** Session context (§2.1 step 4) flows to the runtime via the directive envelope payload — the mechanism that is guaranteed to exist today:

- The directive envelope payload (`wcon-profiles` §4.2) carries the context as a sibling of `llm`/`tools`/`system_prompt`, making the values visible to the agent itself. This lets an agent know, for example, that it is operating in a `production` DevOps environment before deciding whether to proceed.

If and when the runtime exposes a workspace-metadata slot (e.g., a `metadata` map on `CoordinatorService.CreateSession` or `CoordinatorService.Dispatch`), the Console should additionally attach session context to that metadata so that tool-layer policies enforced at workspace scope (e.g., `trade_execute` reading `compliance_scope`, `train_launch` reading `compute_budget`) can access the values without depending on the agent to pass them through to tool calls. This is an open coordination with upstream — until it lands, the Console's only delivery channel is the directive payload, and tool-layer policies that read session-level context rely on the agent to forward the relevant fields to tool arguments.

**The runtime is authoritative for policy enforcement.** The Console delivers context; the runtime checks it. If the runtime refuses a tool call because the context is missing a required field or its value is stale (e.g., a `compliance_check` checkpoint expired beyond its `expires_after_ms` window), that refusal arrives as a trail entry (§6.3) — the Console does not pre-validate refusal conditions beyond the `MISSING_CONTEXT` / `INVALID_CONTEXT` checks at launch time.

### 4.2 Step Details

**Step 1: Create coordinator workspace**

The Console calls `CoordinatorService.CreateSession` to establish the coordination context in the runtime.

| Parameter | Source |
|-----------|--------|
| Session metadata | Session ID, vertical, workflow |

The returned `session_id` and `coordinator_workspace_id` are stored in the session record.

**Step 2: Submit goal**

The Console calls `CoordinatorService.SubmitGoal` with the workflow's top-level description as the goal text. This establishes the task graph root in the runtime.

**Step 3: Create worker workspaces**

For each assignment in the session, the Console:

1. Calls `CoordinatorService.Dispatch` to create a worker workspace:
   - Role: the assignment's `role_ref`
   - Budget: the resolved budget (assignment override → session override → profile default), mapped to WACP `ResourceBudget` (`wcon-profiles` §4.3)
   - Task: in Mode A (§2.4) derived from the workflow stage the assignment corresponds to; in Mode B the task description is generic ("perform {role} work within workflow {workflow_id}") and the coordinator refines it via envelopes at runtime.

2. Calls `AgentService.SendEnvelope` to deliver the directive:
   - Directive payload constructed per `wcon-profiles` §4.2 (LLM config, effective tool set, system prompt from vertical, session context pass-through)
   - The `workspace_id` returned from step 1 is recorded in the `session_assignments` row

**Iteration order.** In Mode A, worker workspaces are created in workflow stage order (stages without dependencies could be parallelized, but sequential creation simplifies error handling). In Mode B, iteration follows the order of the `session_assignments` list, which is the order the user entered them in step 3 of configuration. In both modes the order is deterministic and recorded in the session record for debugging.

**Step 4: Subscribe to streams**

The backend opens four gRPC streaming RPCs against the HighwayService, scoped to the session's coordinator workspace:

| Stream | Purpose | Backend consumer |
|--------|---------|-----------------|
| `StreamTrail` | Trail entries for all workspaces in the session | Session monitor task → trail relay |
| `StreamGates` | Gate events requiring human resolution | Session monitor task → gate relay |
| `StreamEscalations` | Escalation events from agents | Session monitor task → escalation relay |
| `StreamWorkspaceChanges` | Workspace state transitions | Session monitor task → workspace state tracking |

Each stream is consumed by a dedicated Tokio task. All four feed into the session monitor task via channels (`wcon-architecture` §7).

**Step 5: Transition to active**

Once all workspaces are created, directives delivered, and streams subscribed, the session transitions to `active` and `launched_at` is recorded. The frontend receives a WebSocket notification and transitions to the oversight dashboard.

### 4.3 Launch Failure

If any step fails during launch, the session transitions to `failed`:

| Failure point | Behavior |
|---------------|----------|
| Coordinator workspace creation fails | Session → `failed`. No cleanup needed — nothing was created. |
| Goal submission fails | Session → `failed`. The coordinator workspace exists but has no work. The runtime will garbage-collect idle workspaces. |
| Worker workspace N creation fails | Session → `failed`. Previously created workspaces are recorded in the session record for debugging. The Console does not attempt to abort them — the runtime handles orphaned workspaces. |
| Directive delivery fails | Session → `failed`. Same as above. |
| Stream subscription fails | Session → `failed`. Workspaces may be running without Console observation. The runtime continues independently; the Console records the failure. |

The Console does not implement partial launch recovery. If launch fails at step 3 out of 5 workspaces, the entire session fails. The user can inspect the failure, fix the issue (e.g., runtime connectivity), and create a new session. This simplicity is justified by the launch sequence being fast (seconds, not minutes) and the configuration being trivially reproducible.

### 4.4 Launch Idempotency

The launch endpoint (`POST /api/sessions/:id/launch`) is not idempotent. Calling it twice on the same session returns `409 Conflict` if the session is no longer in `configuring` state. A failed session cannot be re-launched — the user creates a new session (optionally cloning the configuration from the failed one).

## 5. Runtime Mapping

### 5.1 Entity Mapping

The Console's session model maps to WACP runtime constructs:

| Console concept | WACP runtime construct | Relationship |
|----------------|----------------------|-------------|
| Session | Coordinator session | 1:1 — one Console session creates exactly one runtime session |
| Session (coordinator) | Coordinator workspace | 1:1 — the root workspace that owns the task graph |
| Assignment (role slot) | Worker workspace | 1:1 — each role slot creates one worker workspace at launch (Mode A: one per stage; Mode B: one per vertical role — see §2.4) |
| Profile | Directive payload + budget | The profile's operational fields are delivered as directive content; budget fields become workspace resource limits |
| Workflow stage | Task + workspace | Mode A only — each stage becomes a task in the task graph, executed by the corresponding workspace. In Mode B the workflow's task graph is assembled by the coordinator at runtime; the Console only provides the role→profile map. |
| Workflow stage dependencies | Task dependencies | Mode A only — stage `depends_on` maps to task DAG edges. In Mode B the coordinator is responsible for stage ordering. |
| Session context | Directive payload `context` field | Session context flows as part of every worker's directive envelope (`wcon-profiles` §4.2). The runtime may additionally attach context to workspace metadata if a metadata slot is available; see §4.1. |
| Session state | Aggregated workspace states | The Console derives session state from the collective state of all workspaces (see §6.2) |

### 5.2 ID Mapping Table

The session record maintains a mapping between Console IDs and runtime IDs:

| Console ID | Runtime ID | Stored in |
|-----------|-----------|----------|
| `session.id` (UUID) | `coordinator_workspace_id` | `sessions.coordinator_workspace_id` |
| `assignment.id` (UUID) | Worker `workspace_id` | `session_assignments.workspace_id` |

This mapping is established at launch time and is immutable thereafter. It enables the Console to:
- Correlate incoming trail entries (which carry workspace IDs) with session assignments (which carry profile names and role labels)
- Annotate real-time events with human-readable context for the oversight dashboard
- Route gate and escalation events to the correct session's frontend connections

### 5.3 One-Way Relationship

The runtime has no knowledge of the Console's session model. It operates on workspaces, tasks, envelopes, and signals. The Console is one of potentially many clients of the runtime's gRPC API. The runtime does not call back to the Console — all information flows from runtime to Console via streaming RPCs initiated by the Console.

## 6. Session Monitoring

Once active, the session is monitored in real-time by the backend's session monitor task. The monitor aggregates events from the runtime and maintains an in-memory representation of the session's current state.

**Stream authorization.** Real-time streams (WebSocket) and monitoring endpoints are authorized per-session: operators can observe only their own sessions; admins can observe any session. Unauthorized access returns `403` on the REST monitoring endpoints and rejects the WebSocket upgrade (`wcon-auth` §4.2). Highway actions (gate approval, escalation response, directive injection) follow the same ownership rule.

### 6.1 In-Memory Session State

For each active session, the backend maintains:

```
ActiveSession
├── session_id: String
├── config: SessionConfig              -- immutable snapshot from launch
│   ├── vertical: String
│   ├── workflow: String
│   ├── context: HashMap<String, JsonValue>   -- vertical-specific context tags
│   ├── assignments: Vec<Assignment>
│   └── budgets: BudgetChain
├── workspace_states: HashMap<String, WorkspaceState>
│   └── key: workspace_id
│       value: current WorkspaceState (IDLE, ACTIVE, BLOCKED, etc.)
├── task_states: HashMap<String, TaskStatus>
│   └── key: task_id
│       value: current TaskStatus (PENDING, IN_PROGRESS, COMPLETED, etc.)
├── pending_gates: Vec<GateEvent>          -- gates awaiting human resolution
├── pending_escalations: Vec<EscalationEvent>
├── pending_refusals: Vec<RefusalEvent>    -- tool-layer refusals blocking workspaces
│   └── RefusalEvent                        -- canonical form; wire format in `wcon-highway` §4A.2
│       ├── refusal_id: String               -- Console-assigned UUID
│       ├── workspace_id: String
│       ├── tool_name: String
│       ├── tool_args_preview: JsonValue     -- filtered arg values relevant to the refusal
│       │                                       (sensitive values scrubbed)
│       ├── policy_kind: ToolPolicyKind      -- requires_checkpoint | requires_gate
│       │                                       | budget_limited | classification_gated | unknown
│       ├── error_code: String               -- COMPLIANCE_NOT_APPROVED, PHI_ACCESS_NOT_GRANTED,
│       │                                       HYPOTHESIS_NOT_DECLARED, COMPUTE_BUDGET_EXCEEDED,
│       │                                       ENVIRONMENT_GATE_REQUIRED, ...
│       │                                       (unknown codes preserved verbatim)
│       ├── reason: String                   -- human-readable explanation including *why*
│       │                                       the check failed (missing / expired / mismatch)
│       ├── policy_reference: ToolPolicy     -- full policy resolved from taxonomy index
│       │                                       (kind, description, all kind-specific fields)
│       ├── unblock_hint: String             -- Console-generated suggestion of user action
│       │                                       per policy kind (see `wcon-highway` §4A.2)
│       ├── trail_entry_id: String           -- link to the originating trail entry
│       └── created_at: Timestamp
├── resource_usage: HashMap<String, ResourceUsage>
│   └── key: workspace_id
│       value: current resource consumption (cost, tokens, wall time — the three ResourceBudget dimensions)
├── vertical_metrics: HashMap<String, f64>  -- session-level accumulators for vertical-specific
│   └── key: field_name (e.g., "max_hours" for MLOps `budget_limited` policy on `train_launch`)
│       value: cumulative consumption across all workspaces in the session
│   (empty for verticals without `budget_limited` tool policies; see §6.4)
├── trail_buffer: VecDeque<TrailEntry> -- recent trail entries (bounded)
└── subscribers: Vec<WebSocketSender>  -- connected frontends
```

This state is derived entirely from the runtime's event streams. It is not persisted — if the backend restarts, it is rebuilt from the runtime (see §8). The `config.context` field carries the same values stored in `sessions.context` (`wcon-data-model` §4.1) — it is copied into memory at launch time and not mutated thereafter.

### 6.2 State Derivation

The Console derives session-level state from workspace-level events:

| Workspace event | Session state effect |
|----------------|---------------------|
| Any workspace → `ACTIVE` | Session stays `active` |
| Any workspace → `BLOCKED` (gate) | Session stays `active`; gate added to `pending_gates` |
| Any workspace → `BLOCKED` (escalation) | Session stays `active`; escalation added to `pending_escalations` |
| Any workspace → `BLOCKED` (tool-layer refusal) | Session stays `active`; refusal event added to `pending_refusals` with the error code (`COMPLIANCE_NOT_APPROVED`, `PHI_ACCESS_NOT_GRANTED`, `HYPOTHESIS_NOT_DECLARED`, `COMPUTE_BUDGET_EXCEEDED`, `ENVIRONMENT_GATE_REQUIRED`, etc.) and a reference to the tool-layer policy that triggered it |
| Any workspace → `FAILED` | Session stays `active` unless the coordinator fails (see below) |
| Coordinator workspace → `FAILED` | Session → `failed`; all monitoring stops |
| All workspaces → `CLOSED` | Session → `completed`; all tasks reached terminal state |
| All tasks → `COMPLETED` or `INTEGRATED` | Session → `completed` |

**Classifying the `BLOCKED` reason.** A workspace can enter `BLOCKED` for three reasons: a gate, an escalation, or a tool-layer refusal. The session monitor must correctly classify each case so that the oversight dashboard surfaces the right affordance:

- **Gate:** the block originates from a `gate_opened` event on `StreamGates`. The monitor adds the gate to `pending_gates` and leaves `pending_refusals` empty for that workspace.
- **Escalation:** the block originates from an escalation signal on `StreamEscalations`. The monitor adds the escalation to `pending_escalations`.
- **Tool-layer refusal:** the block originates from a trail entry with `event_type` such as `tool_call_refused` and a refusal status code identifying the policy violation. This is neither a gate (no approval mechanism) nor an escalation (no agent help request) — it is a workspace BLOCKED state that resolves only when the user creates the prerequisite checkpoint or grants the prerequisite gate. The monitor adds the refusal to `pending_refusals` and the oversight dashboard renders a dedicated affordance (see `wcon-highway` §4A / `wcon-ui` §7).

The session transitions to `completed` when the runtime indicates all work is done. The Console does not independently decide when work is complete — it follows the runtime's task lifecycle.

### 6.3 Event Processing

The session monitor task processes events from the four gRPC streams:

**Trail events** (`StreamTrail`):
1. Parse the trail entry.
2. Annotate with session context (map workspace ID to role name, profile name).
3. Classify by event type. Four subclasses of trail entry receive special handling:
   - **Vertical-specific checkpoint creation.** If the entry records creation of a checkpoint whose `checkpoint_type` appears in the session's `VerticalEntry.checkpoint_types` (e.g., Finance `compliance_check`, Healthcare `phi_access_grant`, MLOps `reproducibility_checkpoint`, DataSci `declared_hypothesis`, Analytics `data_snapshot`), the monitor annotates the entry with the checkpoint's field schema (from `wcon-data-model` §6.1 `CheckpointSchema.fields`) so the frontend can render each field with its declared type, description, and enum metadata rather than a generic JSON blob. The annotation is added to the trail entry's envelope, not to the raw payload — the runtime's payload is preserved verbatim (`wcon-highway` §7).
   - **Tool-layer refusal.** If the entry records a tool call refused by the runtime with a status code matching a known tool-layer refusal (e.g., `COMPLIANCE_NOT_APPROVED`, `PHI_ACCESS_NOT_GRANTED`, `HYPOTHESIS_NOT_DECLARED`, `COMPUTE_BUDGET_EXCEEDED`, `ENVIRONMENT_GATE_REQUIRED`), the monitor builds a `RefusalEvent` (see §6.1 struct), resolves the triggering `ToolPolicy` from the taxonomy index, and adds it to `pending_refusals`. This is how tool-layer refusals become first-class Console objects — they do not come through `StreamGates` or `StreamEscalations`; they surface as trail entries with specific status codes.
   - **Checkpoint creation that resolves a pending refusal.** If the monitor is tracking a `pending_refusal` whose `policy_kind == "requires_checkpoint"` and the trail entry records creation of a checkpoint of the required type with a matching `matching_field` value (compared against the refusal's `tool_args_preview` for the same field name), the refusal is removed from `pending_refusals`. Subsequent tool calls will succeed; the session monitor does not retry on behalf of the agent.
   - **Successful tool retry that clears a refusal.** If a trail entry records a successful invocation of a tool for which a `pending_refusal` exists on the same workspace (same `tool_name`), the refusal is cleared regardless of `policy_kind` — the runtime has accepted the call, the prerequisite is evidently met. This is the catch-all for refusals that resolved through a mechanism the Console did not directly observe (e.g., a `classification_gated` refusal cleared because the agent retried with the override flag).
4. Append the (possibly annotated) entry to `trail_buffer` (evicting oldest entry if buffer is full, per `ui.trail_buffer_size` setting).
5. Broadcast to all subscribers via WebSocket.

**Gate events** (`StreamGates`):
1. Parse the gate event.
2. Enrich with session context (which workspace, which role, which profile, and — for domain-scoped gates — the relevant slice of `session.context` so the dashboard can render the gate with full vertical context per `wcon-highway` §4.7).
3. Add to `pending_gates`.
4. Broadcast to all subscribers.

**Escalation events** (`StreamEscalations`):
1. Parse the escalation event.
2. Enrich with session context.
3. Add to `pending_escalations`.
4. Broadcast to all subscribers.

**Workspace change events** (`StreamWorkspaceChanges`):
1. Update `workspace_states` map.
2. If the new state is `BLOCKED`, classify the reason per §6.2 (gate / escalation / tool-layer refusal) and cross-reference `pending_gates`, `pending_escalations`, and `pending_refusals` to ensure the correct affordance is surfaced. If a workspace becomes `BLOCKED` without a corresponding entry in any of the three pending collections, log a warning and fall back to displaying "blocked — reason unknown" pending a trail entry that clarifies the cause. This "unknown reason" state is self-correcting: when the explanatory trail entry arrives on `StreamTrail`, the classification is re-run and the display updates.
3. If the workspace transitions out of `BLOCKED` (to `ACTIVE`, `SUSPENDED`, or any non-blocking state), any `pending_refusals` entries for that workspace are cleared. The rationale: if the workspace is no longer blocked, whatever prerequisite the refusal depended on is either met or irrelevant. This is the fallback for refusal clearance when the monitor did not directly observe the resolving event.
4. Check for session-level state transitions (§6.2).
5. If session state changes, update the SQLite session record.
6. Broadcast the workspace state change to all subscribers.

### 6.4 Resource Tracking

The session monitor tracks resource consumption per workspace from trail entries containing `ResourceUsage` data. The oversight dashboard displays:

- Per-workspace: current usage vs. budget (tokens consumed, cost incurred, wall time elapsed)
- Per-session: aggregate usage across all workspaces
- Budget warnings: when any workspace exceeds its `warning_threshold` (default 80%), the dashboard shows a warning indicator

Resource tracking is observational — the Console displays it but does not enforce it. Budget enforcement is the runtime's responsibility.

**Vertical-specific resource metrics.** In addition to the three `ResourceBudget` dimensions (cost, tokens, wall time), some verticals track a domain-specific resource against a session context value. The canonical example is MLOps: `compute_budget` (GPU-hours) is compared against accumulated `train_launch` tool invocations. The session monitor tracks these by:

1. Scanning trail entries for tool invocations whose `tool_name` matches a policy entry of kind `budget_limited` in the session's vertical manifest.
2. Extracting the `budget_field` argument value from each invocation (e.g., `max_hours` for `train_launch`).
3. Accumulating the sum into the session-level `vertical_metrics` map, keyed by the `budget_field` name.

The accumulated value is stored in `ActiveSession.vertical_metrics` (see §6.1 struct) and rendered in the dashboard header as a usage counter (`wcon-ui` §7.2 — "used=31.2 GPU-h" for MLOps). It is informational — the runtime enforces the budget by refusing tool calls that would exceed it (`COMPUTE_BUDGET_EXCEEDED` refusal surfaces via §6.3 refusal detection). The Console's counter tracks accepted invocations; refused invocations do not consume budget.

For verticals without `budget_limited` policies, the `vertical_metrics` map is empty. The dashboard header shows only the base resource counters from `resource_usage`.

### 6.5 Monitoring API

**Session state:**

`GET /api/sessions/:id/state`

Returns the current in-memory state: workspace states, task states, pending gates count, pending escalations count, pending refusals count, aggregate resource usage, and a snapshot of `config.context` so the frontend can render the session header with vertical context badges (`wcon-ui` §7.2).

**Pending refusals:**

`GET /api/sessions/:id/refusals`

Returns the list of pending tool-layer refusals for the session. Response body is `{ "items": [RefusalEvent, ...] }` where each entry matches the wire format in `wcon-highway` §4A.2:

```json
{
  "items": [
    {
      "refusal_id": "ref-uuid",
      "workspace_id": "ws-uuid",
      "workspace_label": "finance:portfolio_manager (Senior PM)",
      "tool_name": "trade_execute",
      "tool_args_preview": {
        "trade_id": "TXN-2026-Q1-00847",
        "instrument": "AAPL",
        "side": "buy",
        "quantity": 1200
      },
      "policy_kind": "requires_checkpoint",
      "error_code": "COMPLIANCE_NOT_APPROVED",
      "reason": "No approved compliance_check checkpoint found for trade_id=TXN-2026-Q1-00847 (most recent was 8 minutes ago; window is 5 minutes)",
      "policy_reference": {
        "vertical": "finance",
        "kind": "requires_checkpoint",
        "description": "Refuses without an approved compliance_check checkpoint whose trade_id matches and that was created within the last 5 minutes.",
        "checkpoint_type": "compliance_check",
        "matching_field": "trade_id",
        "expires_after_ms": 300000
      },
      "unblock_hint": "Create a compliance_check checkpoint with status=approved and trade_id=TXN-2026-Q1-00847. Use the compliance_check tool or escalate to a compliance_officer role.",
      "trail_entry_id": "trail-uuid",
      "created_at": "2026-04-11T10:27:15Z"
    }
  ]
}
```

`workspace_label` is a derived field (not stored in the `RefusalEvent` struct in §6.1) — the monitor computes it on the fly from the assignment-to-profile mapping (§5.2). Every other field is a direct projection of the in-memory struct.

Clients use this endpoint to render the refusal panel in the oversight dashboard (`wcon-ui` §7.2). Resolutions happen via agent action (creating the prerequisite checkpoint, or retrying after context changes) — there is no user-facing `POST /refusals/:id/resolve` endpoint because the user's role is to create the missing prerequisite, not to override the refusal.

**Trail entries:**

`GET /api/sessions/:id/trail`

Returns trail entries from the buffer with filtering:

| Parameter | Type | Description |
|-----------|------|-------------|
| `workspace_id` | string | Filter to a specific workspace |
| `event_type` | string | Filter by event type |
| `since` | string | ISO 8601 timestamp — entries after this time |
| `limit` | integer | Max entries (default 100, cap 500) |

**Real-time stream:**

`WebSocket /api/sessions/:id/stream`

Opens a WebSocket connection that receives all real-time events for the session: trail entries, gate events, escalation events, workspace state changes, and resource usage updates. Events are JSON-framed.

## 7. Session Teardown

### 7.1 Completion

A session completes when all tasks reach terminal status. The session monitor detects this from workspace change events and task status updates.

**Process:**
1. Session state → `completed`.
2. Record `closed_at = now()` in the SQLite session record.
3. Close all gRPC stream subscriptions for this session.
4. Send a `session_completed` event to all WebSocket subscribers.
5. Remove the active session state from memory.
6. WebSocket connections remain open — the frontend transitions to a summary view showing final state.

### 7.2 Failure

A session fails when an unrecoverable error occurs: the coordinator workspace fails, the runtime disconnects, or a critical workspace failure propagates.

**Process:**
1. Session state → `failed`.
2. Record `closed_at = now()` and the failure reason in the session record.
3. Close all gRPC stream subscriptions.
4. Send a `session_failed` event to subscribers with the failure reason.
5. Remove the active session state from memory.

The Console does not attempt to abort runtime workspaces on failure. The runtime manages its own workspace lifecycle — failed Console sessions may leave workspaces running that the runtime will eventually clean up.

### 7.3 Cancellation

The user cancels a session through the oversight dashboard or the API.

**API:** `POST /api/sessions/:id/cancel`

**Authorization:** the authenticated user must be the session's owner or an admin (`wcon-auth` §4.2).

**Process:**
1. Check authorization — return 403 if the user is not the owner and not an admin.
2. Verify the session is in a non-terminal state (`configuring`, `validating`, `launching`, or `active`). If the session is already `completed`, `failed`, or `cancelled`, return `409 Conflict`.
3. Session state → `cancelled`. Record `closed_at = now()`.
4. Cleanup depends on the prior state:
   - **From `configuring` or `validating`:** no runtime resources exist — skip steps 5–6.
   - **From `launching`:** best-effort cleanup of partially-created workspaces. Call `CoordinatorService.AbortWorkspace` on the coordinator workspace if it was created; tolerate failure (some workspaces may not exist yet).
   - **From `active`:** call `CoordinatorService.AbortWorkspace` on the coordinator workspace. This propagates to all child workspaces.
5. Close all gRPC stream subscriptions (if any were opened).
6. Send a `session_cancelled` event to subscribers.
7. Remove the active session state from memory.

Cancellation is best-effort: if the abort call to the runtime fails (e.g., runtime disconnected), the Console still transitions the session to `cancelled` and cleans up locally. The runtime may continue processing, but the Console is done observing.

### 7.4 Post-Teardown

After teardown (any terminal state), the session record in SQLite is permanent. The user can view the session's configuration, assignments, and final state through the session history view. Trail entries are not persisted by the Console — the runtime's trail is the authoritative record. The Console's trail buffer is discarded on teardown.

## 8. Reconnection and Recovery

### 8.1 gRPC Stream Reconnection

gRPC streams to the runtime may drop due to network issues, runtime restarts, or transient errors. The session monitor handles reconnection per stream:

1. Detect stream disconnection (gRPC status: UNAVAILABLE, CANCELLED, or stream EOF).
2. Enter reconnection loop with exponential backoff: 100ms, 200ms, 400ms, 800ms, 1600ms, cap at 5s.
3. On successful reconnection, re-subscribe to the stream from the last known sequence number (trail) or from the current time (gates, escalations, workspace changes).
4. After 30 consecutive failed reconnection attempts (approximately 2.5 minutes at cap), mark the session as `failed` with reason `"runtime_disconnected"`.

During reconnection, the session stays in `active` state. The frontend shows a "reconnecting" indicator. Events may be missed during the gap — the Console does not guarantee lossless delivery. The runtime's trail is the complete record.

### 8.2 Backend Restart Recovery

If the Console backend process restarts while sessions are active:

1. On startup, query the `sessions` table for all sessions in `active` state.
2. For each active session, attempt to rebuild the in-memory state:
   a. Call `CoordinatorService.GetWorkspace` for each assignment's `workspace_id` to get current workspace state.
   b. Call `CoordinatorService.GetTaskGraph` to get current task states.
   c. Re-subscribe to all four gRPC streams (trail, gates, escalations, workspace changes via HighwayService).
3. If recovery succeeds, the session continues monitoring from the current point. Events that occurred during the downtime are not replayed — the Console missed them.
4. If recovery fails (runtime unreachable, workspaces no longer exist), transition the session to `failed` with reason `"recovery_failed"`.

Recovery is best-effort. The Console's real-time monitoring is an observation layer — missing events during a restart does not affect the runtime's operation or the trail's integrity.

### 8.3 Frontend Reconnection

If the frontend's WebSocket connection drops:

1. The frontend reconnects automatically with exponential backoff.
2. On reconnection, the frontend calls `GET /api/sessions/:id/state` to get the current session state snapshot.
3. The frontend resumes receiving real-time events from the new WebSocket connection.
4. The frontend may request recent trail entries via `GET /api/sessions/:id/trail?since=<last_seen>` to fill the gap.

## 9. Concurrent Sessions

### 9.1 Multiple Active Sessions

The Console supports multiple active sessions simultaneously. Each session is independent: separate gRPC streams, separate in-memory state, separate WebSocket event channels.

### 9.2 Resource Implications

Each active session consumes:

| Resource | Per-session cost |
|----------|-----------------|
| Tokio tasks | 1 monitor + 4 gRPC stream readers + N WebSocket writers |
| gRPC streams | 4 streaming RPCs to the runtime |
| Memory | In-memory state: workspace states, task states, trail buffer, pending gates/escalations/refusals, `config.context` snapshot, vertical metrics map |
| SQLite writes | Infrequent — state transitions and resource usage updates |

### 9.3 Session Limit

The Console does not enforce a hard limit on concurrent sessions. Practical limits arise from runtime capacity (how many concurrent coordination sessions the runtime supports) and backend resources (memory for trail buffers, Tokio tasks for streams). A configurable soft limit (`settings` key: `sessions.max_active`, default: 10) logs a warning when exceeded but does not block new sessions.

### 9.4 Session Listing

**API:** `GET /api/sessions`

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | string | Filter by state: `active`, `completed`, `failed`, `cancelled`, or `configuring` |
| `vertical` | string | Filter by vertical |
| `sort` | string | Sort field: `created_at` (default), `launched_at`, `state` |
| `order` | string | Sort order: `desc` (default), `asc` |
| `limit` | integer | Max items per page (default 20, cap 100) |
| `cursor` | string | Pagination cursor |

Active sessions appear with real-time state summaries (workspace states, pending gate count). Historical sessions show their final state and timestamps.

### 9.5 Session Cloning

To re-run a previous session's configuration, the user can clone a session:

**API:** `POST /api/sessions/:id/clone`

**Process:**
1. Load the source session's configuration (vertical, workflow, assignments, context).
2. Create a new session with the same vertical and workflow.
3. Copy assignments, but pin each profile to its current version (not the version pinned in the original session). If a profile has been deleted since the original session, the assignment is omitted and the user must fill the slot manually.
4. **Context carry-forward.** Copy the source session's `context` map wholesale into the clone, then apply vertical-specific freshness reset rules:

    | Vertical | Fields reset to blank on clone | Fields carried forward | Rationale |
    |----------|-------------------------------|------------------------|-----------|
    | SWE | — | — | No context schema |
    | DevOps | — | `environment` | Environment is a deliberate choice per session; the user reviews it in step 4 but we carry the last value |
    | MLOps | `compute_budget` | — | Budgets are per-run decisions; carrying forward encourages accidental reuse |
    | Finance | — | `compliance_scope`, `jurisdiction` | Jurisdiction rarely changes between runs for the same desk |
    | Healthcare | `phi_access_basis` | — | Each run requires fresh consent scope; no implicit reuse |
    | Analytics | `data_snapshot_id` | — | Snapshots drift; the user must reconfirm |
    | DataSci | — | `hypothesis_framework` | Hypothesis framework is a reusable template |

    These rules are implemented as a per-vertical "clone policy" mapping that the session cloner consults. The defaults above are a starting point; they may be refined per-vertical as usage patterns emerge. The default for an unknown vertical (one the Console has not seen before) is **reset all required fields** — the safer choice is to force the user through step 4 rather than silently inherit stale values.

5. Do not copy budget overrides — the user sets these fresh.
6. Return the new session in `configuring` state with any reset required fields flagged for step 4.

Session cloning is a convenience for re-running configurations, not a replay mechanism. The new session is fully independent. Pinned checkpoint state from the original session (e.g., a Finance `compliance_check` that was valid in the original run) is **never** carried forward — checkpoints are per-workspace, per-run, and subject to runtime freshness windows (`expires_after_ms`).

## 10. Invariants

### 10.1 State Machine Integrity

Session state transitions follow the state machine defined in `wcon-data-model` §4.3. No transition outside the defined edges is permitted. The session manager enforces this by checking the current state before every transition and rejecting invalid transitions with `409 Conflict`.

### 10.2 Configuration Immutability

Once a session leaves the `configuring` state, its `vertical`, `workflow`, `context`, and `session_assignments` are immutable. No API call can modify them. The session record in SQLite faithfully represents the configuration that was used at launch time. Mirrors `wcon-data-model` §10.2 inv. 2 and 6.

Specifically, `PATCH /api/sessions/:id` with a `context` body field is rejected with `409 Conflict` if the session is not in `configuring` state. Context is frozen at launch — the runtime has received it, directive envelopes have been delivered, and any change would diverge the Console's view from what the agents are operating against.

### 10.3 Launch Atomicity

A session either launches completely (all workspaces created, all directives delivered, all streams subscribed) or fails entirely. There is no partially-launched state. If step 3 fails on workspace 4 of 5, the session is `failed`, not `active` with 4 workspaces.

### 10.4 Monitor Consistency

The in-memory session state is derived exclusively from runtime events. The session monitor never fabricates workspace states, task states, or trail entries. If the runtime says a workspace is `ACTIVE`, the monitor records `ACTIVE`. The Console's observation is a faithful projection of the runtime's state.

### 10.5 Terminal State Finality

Sessions in `completed`, `failed`, or `cancelled` states cannot transition to any other state. All runtime resources (gRPC streams, Tokio tasks) associated with the session are released on entering a terminal state. The session record is retained permanently in SQLite.

### 10.6 Session-Profile Isolation

Profile operations (update, delete, rollback) never affect active sessions. Sessions pin `(profile_id, profile_version)` at assignment time. The session manager reads the pinned version, never the profile's current version.

### 10.7 Runtime Independence

Console session failure does not cause runtime failure, and runtime continuation does not depend on Console observation. If the Console crashes, the runtime continues its coordination run. If the Console reconnects, it observes the current state without affecting it.

### 10.8 Ownership Immutability

A session's `owner_user_id` is set at creation and never changes. Authorization checks (view, act, cancel) use the immutable `owner_user_id` plus the requesting user's console role (`wcon-auth` §4.2, §5.1).

### 10.9 Authorization at Every Boundary

Every state-changing session operation (create, launch, cancel, gate resolve, escalation respond, inject) checks the authenticated user's console role and session ownership before proceeding. Monitoring endpoints and WebSocket streams check read authorization. The frontend hides unauthorized affordances as a convenience; the backend enforces access independently (`wcon-auth` §4.3).

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-auth | Authentication & Authorization | defines ownership, stream authorization, and per-action access rules for sessions (§4.2, §5) |
| wcon-architecture | System Architecture | defines Session Manager component (§4.1), session launch data flow (§5.3), concurrency model (§7) |
| wcon-data-model | Data Model | defines session schema (§4), state machine (§4.3), assignment schema (§4.2), budget precedence, `sessions.context` column (§4.1), `VerticalEntry.context_schema` / `tool_policies` / `checkpoint_types` (§6.1) |
| wcon-profiles | Profile System | defines profile-to-WACP mapping (§4), session pinning (§5.4), validation rules (§3), directive context pass-through (§4.2) |
| wcon-discovery | Agent & Role Discovery | provides taxonomy index for vertical, workflow, and context schema selection (§4.1, §2.2) |
| wcon-highway | Highway Integration | defines refusal event surfacing (§4A), vertical-specific checkpoint rendering (§8), gate enrichment with vertical context (§4.5) |
| wcon-ui | UI Design | defines context wizard step (§6.2 step 4), refusal panel, vertical context badges (§7) |
| wcon-glossary | Glossary | defines session, vertical, workflow, coordinator, worker, workspace, workspace context tag, tool-layer policy, vertical-specific checkpoint |
| wcon-vision | Product Vision | establishes session launch without code (G3) and oversight (G4) as goals, vertical-agnosticism (BC4) |
| wacp-protocol | WACP Protocol Specification | defines workspace lifecycle, task lifecycle, trail, CoordinatorService, AgentService, HighwayService |
| wacp-taxonomy | WACP Taxonomy crate | defines `VerticalManifest.context_schema` and `ContextField` types consumed by step 4 |

*WACP Console -- authored by AAkil98*
