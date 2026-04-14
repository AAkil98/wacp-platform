---
id: wcon-highway
type: design
status: final
created: 2026-04-10T00:00:00
revised: 2026-04-14T00:00:00
authors: [AKIL Abderrahim, Claude Opus 4.6]
tags: [highway, oversight, gates, escalations, refusals, real-time]
depends_on: [wcon-architecture, wcon-sessions]
---

# WACP Console — Highway Integration

## Table of Contents

1. Overview
2. Highway Bridge Architecture
3. Trail Streaming
4. Gate Resolution
4A. Tool-Layer Refusals
5. Escalation Handling
6. Directive Injection
7. Event Enrichment
8. Oversight Dashboard UX
9. Notification Model
10. Invariants

---

## 1. Overview

The highway is WACP's human oversight mechanism. It provides four capabilities: visibility (monitoring agent work), gates (approval points that pause execution), injection (sending directives to running agents), and escalation handling (responding when agents request help). The Console is the primary human interface for all four.

This spec defines how the Console integrates with the HighwayService gRPC API to deliver these capabilities through the oversight dashboard. The highway bridge (`wcon-architecture` §4.1) is the backend component that connects the gRPC streams to the frontend. The session monitor (`wcon-sessions` §6) is the task that processes events. This spec sits on top of both: it defines the user-facing behavior, the action workflows, and the UX patterns for human-in-the-loop coordination.

The Console's highway integration is shaped by two principles:

1. **The Console relays, it does not decide.** The runtime enforces gate policies, timeout fallbacks, and escalation routing. The Console presents these events to the user, collects their decisions, and transmits them back. It never auto-approves a gate, auto-resolves an escalation, or silently drops a highway event.

2. **Highway actions respect session ownership.** The Console enforces per-user authorization on all highway interactions (`wcon-auth` §4.2). Operators can view and act on gates, escalations, and refusals only for sessions they own. Admins can act on any session. Viewers can observe but not resolve. Every gate resolution and escalation response carries the authenticated user's identity to the audit log.

## 2. Highway Bridge Architecture

### 2.1 Component Position

```
Frontend (Oversight Dashboard)
    │
    ├── WebSocket ──────────────────────────────────────────┐
    │   (enriched events: trail, gates, escalations,        │
    │    refusals*, workspace changes, session,              │
    │    notifications)                                      │
    │   * refusals are synthesized from StreamTrail by       │
    │     the session monitor (§4A) — no separate gRPC       │
    │     stream exists on the runtime side.                 │
    │                                                        │
    ├── REST ───────────────────────────────────────────────┐│
    │   (actions: gate resolve, escalation respond,         ││
    │    directive inject)                                   ││
    │                                                       ▼▼
    │                                              Highway Bridge
    │                                                   │
    │                                          ┌────────┼────────┐
    │                                          │ Enrichment      │
    │                                          │ Layer           │
    │                                          │ (§7)            │
    │                                          └────────┬────────┘
    │                                                   │
    │                                          ┌────────┼────────┐
    │                                          │ gRPC Streams    │
    │                                          │ (4 per session) │
    │                                          └────────┬────────┘
    │                                                   │
    │                                          HighwayService
    │                                          (WACP Runtime)
```

### 2.2 Stream-to-Frontend Mapping

The bridge receives events from four gRPC streams per active session (`wcon-sessions` §4.1) and routes them to the frontend through a single WebSocket connection per client. Three additional Console-synthesized channels carry events derived from the gRPC streams rather than from a dedicated upstream source.

| Source | Frontend channel | Dashboard surface |
|--------|-----------------|-------------------|
| `StreamTrail` | `trail` | Trail stream panel |
| `StreamGates` | `gates` | Gate queue |
| `StreamEscalations` | `escalations` | Escalation inbox |
| `StreamWorkspaceChanges` | `workspaces` | Workspace view |
| *Synthesized from `StreamTrail`* | `refusals` | Refusal panel (§4A) |
| *Synthesized from `StreamWorkspaceChanges` + task state aggregation* | `session` | Session header state (`wcon-sessions` §6.2 — emits `session_active`, `session_completed`, `session_failed`, `session_cancelled`) |
| *Synthesized cross-stream* | `notification` | Toast notifications and nav badge updates (§9, `wcon-ui` §2.1) |

The three synthesized channels exist because the frontend benefits from lifecycle-level and aggregate events that the runtime does not provide directly. The session monitor computes them by observing the four gRPC streams and applying the rules in `wcon-sessions` §6.2 (state derivation) and §6.3 (event processing).

Each WebSocket frame carries a typed event:

```json
{
  "channel": "gates",
  "session_id": "uuid",
  "event": { ... }
}
```

### 2.3 Action-to-gRPC Mapping

User actions flow in the reverse direction — from frontend REST calls to gRPC unary RPCs:

| User action | REST endpoint | gRPC RPC |
|------------|---------------|----------|
| Approve/reject gate | `POST /api/sessions/:sid/gates/:gid/resolve` | `HighwayService.RespondToGate` |
| Respond to escalation | `POST /api/sessions/:sid/escalations/:eid/respond` | `HighwayService.RespondToEscalation` |
| Inject directive | `POST /api/sessions/:sid/inject` | `HighwayService.InjectEnvelope` |

## 3. Trail Streaming

### 3.1 Trail Event Structure

Trail entries arrive from the runtime's `StreamTrail` RPC. Each entry is enriched (§7) and pushed to connected frontends.

**Enriched trail entry (WebSocket frame):**

```json
{
  "channel": "trail",
  "session_id": "uuid",
  "event": {
    "id": "trail-entry-id",
    "timestamp": "2026-04-10T14:30:00.123Z",
    "sequence_number": 42,
    "workspace_id": "ws-uuid",
    "workspace_label": "swe:implementer (Fast Implementer)",
    "actor": "agent",
    "event_type": "envelope_delivered",
    "summary": "Directive delivered to implementer workspace",
    "body": { ... }
  }
}
```

### 3.2 Trail Filtering

The oversight dashboard allows the user to filter the trail stream in real-time. Filters are applied client-side against the buffered trail entries and server-side for historical queries.

| Filter | Values | Description |
|--------|--------|-------------|
| Workspace | workspace ID or label | Show entries for a specific workspace only |
| Event type | `signal`, `envelope`, `checkpoint`, `gate`, `escalation`, `state_change`, `tool_call_refused`, `quality_report` | Show entries of a specific type |
| Vertical checkpoint type | e.g., `compliance_check`, `phi_access_grant`, `reproducibility_checkpoint` | Show checkpoint events of a specific vertical-specific type (scoped to the session's vertical) |
| Refusal code | e.g., `COMPLIANCE_NOT_APPROVED`, `COMPUTE_BUDGET_EXCEEDED` | Show tool-layer refusal entries by status code (§4A.1) |
| Actor | `agent`, `human`, `system` | Show entries by actor type |
| Severity | `info`, `warning`, `error` | Show entries at or above a severity level |

Filters combine with AND logic. Active filters are shown as removable chips above the trail stream.

### 3.3 Trail Buffer Behavior

The frontend maintains a scrolling buffer of trail entries. The buffer size is governed by the `ui.trail_buffer_size` setting (default: 1000 entries, `wcon-data-model` §5.2).

| Behavior | Detail |
|----------|--------|
| New entry arrives | Appended to bottom of buffer; oldest entry evicted if buffer is full |
| Auto-scroll | Enabled by default — the stream scrolls to show the latest entry. Disabled when the user scrolls up to inspect older entries. Re-enabled when the user scrolls to the bottom. |
| Pause/resume | The user can pause the stream to freeze the display. Entries continue buffering; on resume, the display jumps to the latest entry. |
| Entry expansion | Each trail entry shows a summary line. Clicking expands to show the full body (envelope payload, signal details, checkpoint content). |

### 3.4 Trail History Query

For entries that have scrolled out of the buffer or for sessions where the frontend connected late:

**API:** `GET /api/sessions/:id/trail` (defined in `wcon-sessions` §6.5)

This queries the backend's in-memory trail buffer, not the runtime directly. For trail entries older than the backend's buffer, the user is directed to query the runtime's trail through its own interface — the Console's trail view is a real-time observation tool, not an archive.

## 4. Gate Resolution

Gates are the highway's synchronous control points. When the runtime activates a gate, it pauses a protocol transition and waits for human input. The Console is the interface through which the human responds.

### 4.1 Gate Event Structure

**Enriched gate event (WebSocket frame):**

```json
{
  "channel": "gates",
  "session_id": "uuid",
  "event": {
    "gate_id": "gate-uuid",
    "type": "task_approval",
    "workspace_id": "ws-uuid",
    "workspace_label": "finance:portfolio_manager (Senior PM)",
    "task_id": "task-uuid",
    "task_name": "Execute trade TXN-00847",
    "subject": {
      "description": "Agent requests approval to execute a trade that passed compliance pre-check",
      "context": { ... }
    },
    "vertical_context": {
      "vertical": "finance",
      "rationale": "Trade execution in production jurisdiction (SEC) — fiduciary review required",
      "context_snapshot": {
        "compliance_scope": "equities",
        "jurisdiction": "SEC"
      },
      "related_checkpoints": [
        {
          "type": "compliance_check",
          "id": "cp-uuid",
          "fields": {
            "trade_id": "TXN-00847",
            "status": "approved",
            "suitability_verified": true,
            "expires_at": "2026-04-11T10:19:08Z"
          }
        }
      ]
    },
    "timeout_ms": 300000,
    "timeout_at": "2026-04-11T10:24:08Z",
    "fallback_action": "reject",
    "created_at": "2026-04-11T10:19:08Z",
    "urgency": "normal"
  }
}
```

The `vertical_context` field is an enrichment added by the bridge (§4.7, §7), not a protocol field. It carries the vertical identifier, a human-readable rationale for the gate (distinct from the generic `subject.description`), a snapshot of the session-level context tags that influenced the gate, and any related vertical-specific checkpoints from the trail that give the gate additional meaning.

`vertical_context` is absent for gates from verticals with empty `context_schema` and no domain-specific rationale (e.g., many SWE gates). When present, it lets the frontend render gate detail with full domain awareness without a second API round trip.

### 4.2 Gate Types

The six gate types defined by WACP, and how each presents in the Console:

| Gate type | Trigger | User decision | Context shown |
|-----------|---------|---------------|---------------|
| `task_approval` | Coordinator submits a new task for execution | Approve task, reject task, modify task description | Task description, dependencies, assigned role, estimated resources |
| `workspace_create` | Runtime creates a new workspace | Approve creation, reject creation | Workspace role, parent workspace, purpose |
| `envelope_delivery` | Agent sends an envelope to another workspace | Approve delivery, reject delivery, modify payload | Envelope type, sender, receiver, payload preview |
| `integration` | Coordinator triggers integration of completed work | Approve integration, reject integration | Task results, checkpoints, quality assessment |
| `conflict_resolution` | Conflicting signals or results require arbitration | Choose resolution strategy | Conflict details, competing signals, workspace states |
| `workspace_abort` | Runtime or coordinator requests workspace termination | Approve abort, reject abort | Abort reason, workspace state, pending work |

### 4.3 Gate Queue

The oversight dashboard maintains an ordered queue of pending gates across all active sessions.

**Ordering:** gates are ordered by urgency, then by remaining time to timeout:

1. Gates with `urgency: critical` (escalation-triggered gates)
2. Gates with less than 20% of timeout remaining
3. Gates in chronological order (oldest first)

**Queue display:**

| Column | Content |
|--------|---------|
| Type | Gate type icon and label |
| Session | Session name (if viewing across sessions) |
| Workspace | Enriched workspace label (role + profile name) |
| Summary | One-line description of what the gate is asking (from `subject.description`) |
| Vertical rationale | When `event.vertical_context.rationale` is non-empty, rendered as a subtitle line below the summary (see §4.7). Omitted for gates without vertical context. |
| Timeout | Countdown timer showing remaining time |
| Actions | Approve / Reject buttons (type-specific action labels) |

### 4.4 Gate Resolution Workflow

**Step 1: Review.** The user clicks a gate in the queue to open the gate detail view. The detail view shows full context: the gate subject, the workspace state, recent trail entries for that workspace, and the available actions.

**Step 2: Decide.** The user selects an action:

| Decision | Description |
|----------|-------------|
| Approve | Allow the gated transition to proceed |
| Reject | Block the gated transition; the runtime applies the rejection behavior (abort workspace, cancel task, etc.) |
| Modify | Approve with modifications — available for `task_approval` and `envelope_delivery` gates. The user edits the task description or envelope payload before approving. |

**Step 3: Submit.** The user confirms their decision. The Console sends the resolution to the runtime.

**API:** `POST /api/sessions/:sid/gates/:gid/resolve`

```json
{
  "decision": "approve",
  "reason": "Looks good, proceed with implementation",
  "modifications": null
}
```

For `modify` decisions:

```json
{
  "decision": "modify",
  "reason": "Narrowing scope to auth module only",
  "modifications": {
    "task_description": "Implement authentication module — login and logout only, defer registration"
  }
}
```

**Step 4: Confirm.** The backend translates the decision into a `HighwayService.RespondToGate` gRPC call:

| Decision field | gRPC field |
|---------------|-----------|
| `decision` | `GateDecision` enum: `APPROVE`, `REJECT`, `MODIFY` |
| `reason` | `reason` string (recorded in trail) |
| `modifications` | `modified_subject` bytes (serialized modification payload) |

**Step 5: Feedback.** On success, the gate is removed from the pending queue. A trail entry confirms the resolution. The workspace resumes (or aborts, if rejected). The frontend receives both the gate resolution confirmation and the subsequent workspace state change through the WebSocket stream.

### 4.5 Gate Timeout

Each gate has a `timeout_ms` and a `fallback_action`. If the timeout expires before the user resolves the gate:

1. The runtime automatically applies the fallback action (approve, reject, or escalate).
2. The trail records the timeout and fallback.
3. The Console receives the resolution via the trail stream and removes the gate from the pending queue.
4. The oversight dashboard shows a notification: "Gate auto-resolved (timeout) — fallback: [action]".

The Console does not implement its own timeout logic — the runtime owns timeout enforcement. The Console displays the countdown for urgency awareness.

### 4.6 Batch Gate Resolution

When multiple gates of the same type are pending (e.g., multiple task approvals), the user can resolve them in batch:

**API:** `POST /api/sessions/:sid/gates/batch-resolve`

```json
{
  "resolutions": [
    { "gate_id": "gate-1", "decision": "approve", "reason": "Approved in batch" },
    { "gate_id": "gate-2", "decision": "approve", "reason": "Approved in batch" },
    { "gate_id": "gate-3", "decision": "reject", "reason": "Out of scope" }
  ]
}
```

Each resolution is sent as an individual `RespondToGate` gRPC call. The response reports success/failure per gate. Partial failure (some gates resolved, some failed) is possible — the response indicates which succeeded and which failed.

### 4.7 Vertical Gate Rationale Enrichment

Raw gate events from the runtime carry protocol-level information (gate type, subject, timeout) but not the domain-specific *why* that users need to evaluate the gate in a vertical context. The bridge enriches gate events with vertical-aware rationale before forwarding them to the frontend.

**Rationale sources.** The bridge assembles `vertical_context.rationale` (see §4.1) from three inputs:

1. **Session context.** The session's `config.context` (`wcon-sessions` §6.1) — e.g., if a DevOps session has `environment=production`, gates on deploy/rollback/secret_rotate tools include "Production environment: {tool} requires gate clearance" as the rationale prefix.
2. **Tool policy.** If the gate is triggered by a tool whose `ToolEntry.policy.kind == "requires_gate"` with `gate_condition` set, the policy's `description` and `gate_condition` are included in the rationale. The gate's existence is directly caused by the policy; the policy is the most specific available rationale.
3. **Trail context.** Recent trail entries for the same workspace (the last 5 before the gate) are scanned for vertical-specific checkpoint creations or refusals that gave rise to the gate. Matching checkpoints are added to `related_checkpoints`.

The rationale is a single free-text string intended for human reading, not parsing. The Console does not attempt to parse rationale on the frontend — it displays it verbatim.

**Rationale examples (illustrative, per-vertical):**

| Vertical | Gate type | Rationale example |
|----------|-----------|-------------------|
| DevOps | `task_approval` on `deploy_execute` | "Production environment: deploy_execute requires gate clearance. Target: api-gateway v4.2.1." |
| Finance | `task_approval` on `trade_execute` | "Trade execution in SEC jurisdiction (equities scope) — fiduciary review required. Related compliance_check: TXN-00847, approved, expires in 4m 12s." |
| Healthcare | `workspace_create` for clinical worker | "PHI access basis: consent. Patient consent ID: PAT-007-C-42. Worker scope: clinical_report_generate, lab_interpret." |
| MLOps | `task_approval` on `train_launch` | "Training job would consume 18 GPU-hours. Session compute budget: 50 GPU-h (28% remaining)." |
| DataSci | `task_approval` on `hypothesis_test` | "Hypothesis framework: NHST with Benjamini-Hochberg correction. Null hypothesis: no effect. Alpha: 0.05." |
| SWE | `task_approval` on `implement` | "" (empty — SWE gates generally do not need vertical rationale) |

**Enrichment is observational.** The rationale is derived from existing trail state and session context — the bridge does not invent rationale. If a gate's triggering policy is not present in the indexed manifest, or if the session context is empty, or if no related checkpoints are found in the recent trail, the rationale is empty and the frontend falls back to the generic `subject.description`. No synthesis, no guessing.

## 4A. Tool-Layer Refusals

Tool-layer refusals are a third class of workspace-blocking event, distinct from gates and escalations. A refusal occurs when an agent invokes a tool whose runtime-enforced policy (`wcon-discovery` §2.2.2 `ToolPolicy`) is not satisfied — e.g., `trade_execute` without a prior approved `compliance_check` checkpoint, or `train_launch` requesting more `max_hours` than the session's `compute_budget` allows.

Refusals are **not** gates: there is no approval mechanism and no timeout. Refusals are **not** escalations: there is no agent help request. A refusal is a hard "no" from the runtime, recorded as a trail entry, that blocks the refusing workspace until the prerequisite condition is met (checkpoint created, gate cleared, budget increased, classification overridden). The Console's role is to surface the refusal clearly and explain what needs to happen to unblock it.

This section specifies how the Console detects, enriches, relays, and displays refusal events.

### 4A.1 Refusal Detection

Refusals do not arrive on `StreamGates` or `StreamEscalations`. They arrive as **trail entries** on `StreamTrail` with specific characteristics:

| Signal | Meaning | Source |
|--------|---------|--------|
| `event_type` contains `"tool_call_refused"` (or the runtime's specific event type for tool refusal) | This is a refusal event | Trail entry |
| Status/error code matches one of the known refusal codes (below) | This identifies the policy kind | Trail entry |
| `workspace_id` references an `ACTIVE`/`BLOCKED` workspace in the session | This is the refusing agent | Trail entry |
| Tool name and rejected arg values | These identify *which* tool call was refused and with what arguments | Trail entry |
| Full `ToolPolicy` (kind + kind-specific fields) | This gives the rationale | **Console taxonomy index** (`ToolEntry.policy`, `wcon-data-model` §6.1) — the runtime does not repeat policy metadata in refusal trail entries. The Console resolves the policy by looking up `tool_name` in its own index at the moment of refusal detection. |

**Known refusal codes.** The Console treats the following status codes as refusal signals when they appear in trail entries:

| Code | Policy kind | Meaning |
|------|-------------|---------|
| `COMPLIANCE_NOT_APPROVED` | requires_checkpoint | Finance `trade_execute` without an approved `compliance_check` |
| `PHI_ACCESS_NOT_GRANTED` | requires_checkpoint | Healthcare clinical tools without a valid `phi_access_grant` |
| `HYPOTHESIS_NOT_DECLARED` | requires_checkpoint | DataSci `hypothesis_test` without a prior `declared_hypothesis` |
| `COMPUTE_BUDGET_EXCEEDED` | budget_limited | MLOps `train_launch` with `max_hours` > `compute_budget` |
| `ENVIRONMENT_GATE_REQUIRED` | requires_gate | DevOps production deploy/rollback/secret_rotate without environment gate |
| `SQL_DESTRUCTIVE_GATE_REQUIRED` | requires_gate | Analytics destructive SQL without explicit gate clearance |
| `CLASSIFICATION_BLOCKED` | classification_gated | Classified input blocked by default without override flag |

This list is illustrative of the current ecosystem. The set is open — new refusal codes added upstream are tolerated (they appear as generic refusals with `"policy_kind": "unknown"`) and surfaced to the user with the raw code.

The session monitor (`wcon-sessions` §6.3) is responsible for recognizing these trail entries and constructing `RefusalEvent` objects.

### 4A.2 Refusal Event Structure

**Enriched refusal event (WebSocket frame):**

```json
{
  "channel": "refusals",
  "session_id": "uuid",
  "event": {
    "refusal_id": "ref-uuid",
    "workspace_id": "ws-uuid",
    "workspace_label": "finance:portfolio_manager (Senior PM)",
    "tool_name": "trade_execute",
    "tool_args_preview": {
      "trade_id": "TXN-00847",
      "instrument": "AAPL",
      "side": "buy",
      "quantity": 1200
    },
    "policy_kind": "requires_checkpoint",
    "error_code": "COMPLIANCE_NOT_APPROVED",
    "reason": "No approved compliance_check checkpoint found for trade_id=TXN-00847 (or expired: most recent was 8 minutes ago, window is 5 minutes)",
    "policy_reference": {
      "vertical": "finance",
      "description": "Refuses without an approved compliance_check checkpoint whose trade_id matches and that was created within the last 5 minutes.",
      "checkpoint_type": "compliance_check",
      "matching_field": "trade_id",
      "expires_after_ms": 300000
    },
    "unblock_hint": "Create a compliance_check checkpoint with status=approved and trade_id=TXN-00847. Use the compliance_check tool or escalate to a compliance_officer role.",
    "trail_entry_id": "trail-uuid",
    "created_at": "2026-04-11T10:27:15Z"
  }
}
```

**Field notes:**
- `tool_args_preview` shows the arg values the agent attempted to pass, filtered to the fields relevant for understanding the refusal (e.g., `trade_id` matters for a compliance refusal; a large `notes` field does not). Sensitive values are scrubbed.
- `reason` is a human-readable explanation. For `requires_checkpoint` refusals, it includes *why* the check failed — missing entirely, expired, or `matching_field` mismatch.
- `policy_reference` is the resolved `ToolPolicy` entry from `ToolEntry.policy` (`wcon-data-model` §6.1), augmented with `vertical` which is derived from `ToolEntry.vertical` (the owning vertical of the refused tool). The wrapped `ToolPolicy` struct itself does not carry a vertical field — the session monitor adds it at event construction time so the frontend can render a "this policy belongs to vertical X" label without a second lookup. The rest of the `policy_reference` (kind, description, checkpoint_type, matching_field, expires_after_ms, gate_condition, budget_field, etc.) is a direct projection of the indexed `ToolPolicy`.
- `unblock_hint` is a suggestion for *what the user can do*. This is Console-generated, not runtime-provided. Hint templates per policy kind:
  - `requires_checkpoint`: "Create a {checkpoint_type} checkpoint with matching {matching_field}. Consider invoking {suggested_tool}."
  - `requires_gate`: "Resolve the prerequisite gate: {gate_condition}."
  - `budget_limited`: "Cancel the session and relaunch with a higher {budget_field_in_context}, or reduce the tool's requested {budget_field}."
  - `classification_gated`: "Invoke the tool with {override_flag}=true and obtain gate clearance."

### 4A.3 Refusal Queue

The oversight dashboard maintains a `pending_refusals` list per active session (`wcon-sessions` §6.1). Entries appear when the session monitor detects a refusal trail entry and disappear when:

- The monitor observes a subsequent trail entry creating the prerequisite checkpoint (for `requires_checkpoint`).
- The monitor observes a subsequent trail entry showing the tool call succeeded (indicating the prerequisite was met by some means the Console did not directly observe).
- The workspace transitions out of `BLOCKED` (e.g., the agent gave up and moved to a different task, or the session was cancelled).

**Refusal ordering.** Refusals are ordered by creation time (newest last), not by urgency — there is no timeout pressure driving ordering, and the user's action sequence typically needs to follow the agent's decision order.

**Stream channel.** Refusals are forwarded to frontends via a dedicated WebSocket channel, `refusals`, alongside `trail`, `gates`, `escalations`, and `workspaces`. This is a Console-layer channel — it has no corresponding gRPC stream in the runtime, because refusals are synthesized from trail entries by the session monitor, not delivered as first-class runtime events.

### 4A.4 Refusal Actions

The Console does **not** offer a "resolve refusal" button. Unblocking a refusal requires action upstream of the Console:

| Policy kind | Unblock path |
|-------------|-------------|
| `requires_checkpoint` | An agent (possibly in a different workspace) must create the prerequisite checkpoint. The user can inject a directive (§6) into the appropriate workspace asking it to create the checkpoint, or resume the session only after an out-of-band process has produced the checkpoint. |
| `requires_gate` | The corresponding gate must be resolved. The gate appears in the gate queue as usual; resolving it upstream unblocks the tool call. |
| `budget_limited` | The session's context budget must be changed. Because session context is immutable after launch (`wcon-data-model` §10.2), the user must cancel the current session and clone + relaunch with a larger budget. |
| `classification_gated` | The agent must retry the tool call with the appropriate override flag set to `true`. The user can inject a directive asking the agent to retry with the override. |

The refusal panel in the oversight dashboard (`wcon-ui` §7.2) surfaces the unblock path as text, with action buttons for the cases the Console can assist with: "Inject directive to this workspace" and "Cancel and clone session."

### 4A.5 Refusal vs Gate vs Escalation

When a workspace enters `BLOCKED`, the Console must classify the reason correctly. The classification flow:

1. Is there a fresh event on `StreamGates` matching this workspace? → **Gate.**
2. Is there a fresh event on `StreamEscalations` matching this workspace? → **Escalation.**
3. Is there a recent trail entry with a refusal code matching this workspace? → **Refusal.**
4. Otherwise → "blocked — reason unknown" (rare; indicates a stream-ordering race where the workspace state change arrived before the explanatory event). If the session transitions to a terminal state (`completed`, `failed`, `cancelled`) while a workspace remains classified as "blocked — reason unknown", the classification is finalized as `unknown` in the session record and logged as a warning. No recovery is attempted — the runtime's trail is the complete record for post-hoc investigation.

**Self-correcting classification.** The "blocked — reason unknown" state is provisional. The session monitor re-runs the classification every time a new event arrives on any stream for that workspace. When the explanatory event finally arrives (a gate, an escalation, or a refusal trail entry), the classification upgrades and the correct UI affordance surfaces. This is how the Console handles the race condition where `StreamWorkspaceChanges` delivers `BLOCKED` before `StreamTrail` delivers the refusal entry that explains why.

The session monitor (`wcon-sessions` §6.2, §6.3) implements this classification and its self-correction. Misclassification that persists after the stream gap closes (e.g., a refusal stuck in the gate queue) is a bug, not an invariant violation, but it is easy to detect in tests (`wcon-test` §5.3).

## 5. Escalation Handling

Escalations occur when an agent encounters a situation it cannot handle and requests human input. Unlike gates (which are policy-driven control points), escalations are agent-initiated requests for help.

### 5.1 Escalation Event Structure

**Enriched escalation event (WebSocket frame):**

```json
{
  "channel": "escalations",
  "session_id": "uuid",
  "event": {
    "escalation_id": "esc-uuid",
    "workspace_id": "ws-uuid",
    "workspace_label": "swe:implementer (Fast Implementer)",
    "reason": "Cannot resolve merge conflict in auth.rs — multiple approaches possible, need human decision",
    "context": {
      "file": "src/auth.rs",
      "conflict_type": "semantic",
      "options": ["Use version A (token-based)", "Use version B (session-based)", "Rewrite from scratch"]
    },
    "created_at": "2026-04-10T14:30:00Z"
  }
}
```

### 5.2 Escalation Inbox

The oversight dashboard shows pending escalations in an inbox-style view, separate from the gate queue. Escalations have no timeout — they block the workspace until resolved.

**Inbox display:**

| Column | Content |
|--------|---------|
| Workspace | Enriched workspace label |
| Reason | The agent's stated reason for escalating |
| Age | Time since escalation was created |
| Status | Pending / Responding |

### 5.3 Escalation Response Workflow

**Step 1: Review.** The user clicks an escalation to open the detail view. The view shows:
- The agent's reason for escalating
- The escalation context (structured data the agent provided)
- Recent trail entries for the workspace (what was the agent doing before escalating)
- The workspace's current state and checkpoints

**Step 2: Respond.** The user writes a response. The response is free-form text — it becomes a feedback envelope delivered to the agent's workspace.

**API:** `POST /api/sessions/:sid/escalations/:eid/respond`

```json
{
  "response": "Use version A (token-based auth). The session-based approach has been deprecated per the architecture decision in ADR-007.",
  "attachments": []
}
```

**Step 3: Deliver.** The backend translates the response into a `HighwayService.RespondToEscalation` gRPC call. The runtime delivers the response as a feedback envelope to the escalating workspace, which unblocks and resumes work.

**Step 4: Feedback.** The escalation is removed from the inbox. A trail entry records the response. The workspace transitions from `BLOCKED` to `ACTIVE`.

### 5.4 Escalation Without Response

If the user decides the escalation is invalid or the workspace should be aborted instead of helped:

- **Dismiss with abort:** the user can abort the workspace from the escalation detail view. This calls `CoordinatorService.AbortWorkspace` and removes the escalation.
- **No silent dismiss:** there is no "dismiss without action" option. The workspace remains blocked until it receives a response or is aborted. This prevents accidentally ignoring agent requests for help.

## 6. Directive Injection

Injection allows the user to send directives to any workspace in an active session. This is the proactive form of highway interaction — the user initiates communication rather than responding to an event.

### 6.1 Injection Use Cases

| Use case | Example |
|----------|---------|
| Course correction | "Stop working on the UI — focus on the API layer first" |
| Additional context | "The database schema has changed — here are the new column names" |
| Priority change | "This task is now P0 — complete it before moving to the next task" |
| Scope refinement | "Only implement the GET endpoint, skip PUT and DELETE for now" |

### 6.2 Injection Workflow

**Step 1: Select target.** The user selects a workspace from the workspace view in the oversight dashboard. Only workspaces in `ACTIVE` or `BLOCKED` state are valid targets.

**Step 2: Compose.** The user writes the directive content in a text area. The directive is a free-form message that the runtime delivers as a feedback envelope.

**API:** `POST /api/sessions/:sid/inject`

```json
{
  "workspace_id": "ws-uuid",
  "payload": "Focus on the authentication module only. Skip the registration flow — we will handle that in a separate session.",
  "envelope_type": "feedback"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `workspace_id` | yes | Target workspace |
| `payload` | yes | Directive content (text) |
| `envelope_type` | no | Envelope type for the injection (default: `"feedback"`) |

**Step 3: Confirm.** The UI shows a confirmation dialog: "Send directive to [workspace label]?" with the payload preview. Injection is a consequential action — it interrupts an agent's work and should be intentional.

**Step 4: Deliver.** The backend calls `HighwayService.InjectEnvelope` with the payload serialized as an envelope to the target workspace.

**Step 5: Feedback.** A trail entry records the injection. The trail entry shows the injection source as `"human"` and the payload content. The workspace processes the directive on its next envelope receive cycle.

### 6.3 Injection Constraints

| Constraint | Rationale |
|-----------|-----------|
| Target must be in `ACTIVE` or `BLOCKED` state | Delivering to a `CLOSED` or `FAILED` workspace is meaningless |
| Target must belong to the specified session | Prevents cross-session interference |
| Payload is non-empty | Empty directives serve no purpose |
| Payload max size: 64 KB | Practical limit for text directives; large payloads suggest the wrong tool |
| Rate limit: 10 injections per minute per session | Prevents accidental flooding from rapid UI interactions |

### 6.4 Injection to Coordinator

Injecting a directive to the coordinator workspace is a special case — it influences the coordination strategy rather than a single agent's work. The UI shows a distinct warning: "You are sending a directive to the coordinator. This may affect the overall coordination strategy." The confirmation dialog requires the user to acknowledge this.

## 7. Event Enrichment

Raw highway events carry runtime IDs (workspace UUIDs, task UUIDs) that are meaningless to the user. The highway bridge enriches every event with human-readable context before forwarding to the frontend.

### 7.1 Enrichment Table

| Raw field or event | Enriched field | Source | Applies to |
|--------------------|---------------|--------|------------|
| `workspace_id` | `workspace_label` | Session assignment mapping: `role_ref` + profile `name` (e.g., `"finance:portfolio_manager (Senior PM)"`) | All events |
| `task_id` | `task_name` | Task graph query: task description from workflow stage | Trail, gate events |
| `actor` (workspace ID) | `actor_label` | Same as workspace_label for agent actors; `"Human"` for human actors | Trail events |
| `event_type` (raw enum) | `event_type_label` | Human-readable event type name | Trail events |
| Gate event | `vertical_context` (rationale, context snapshot, related checkpoints) | Composed from session `config.context` + `ToolEntry.policy` + recent trail entries per §4.7 | Gate events |
| Trail entry recording a vertical-specific checkpoint | Field schema annotation (`CheckpointSchema.fields`) | `VerticalEntry.checkpoint_types` from the taxonomy index (`wcon-data-model` §6.1). The payload is unchanged; the annotation is attached to the WebSocket envelope for frontend rendering. | Trail events matching a vertical-specific checkpoint type |
| Trail entry with a refusal status code | Synthetic `RefusalEvent` (§4A.2) | Session monitor detects the refusal code, looks up `ToolEntry.policy` for the tool, composes the `RefusalEvent` with `policy_reference`, `unblock_hint`, etc. | Trail events; emitted on the `refusals` channel |
| Trail entry recording a quality report | Per-criterion verdict annotation | `VerticalEntry.quality_criteria` from the taxonomy index, joined with the trail entry's per-criterion verdicts | Trail events at session end |

### 7.2 Enrichment Cache

The enrichment layer caches three kinds of mappings, with different lifetimes:

1. **Workspace-to-label mapping** — built at launch time from the session's assignments (`wcon-sessions` §5.2). Assignments are immutable after launch, so this cache is populated once and not invalidated during the session's lifetime.

2. **Task-to-name mapping** — queried once from the runtime's task graph at session launch and cached. If the coordinator decomposes tasks further during the session (creating subtasks), the enrichment layer queries the task graph on cache miss and caches the result.

3. **Taxonomy-sourced enrichment data** (`ToolEntry.policy`, `VerticalEntry.checkpoint_types`, `VerticalEntry.quality_criteria`, `session.config.context`) — not a separate cache; the enrichment layer reads directly from the shared taxonomy index and session state. Both are snapshot-stable for the session's lifetime (taxonomy reloads affect new sessions only, and session context is immutable after launch per `wcon-data-model` §10.2), so no cache invalidation is needed.

The enrichment layer's overall memory cost is proportional to the number of active sessions × workspaces per session, plus a constant overhead per session for task-graph state. It does not duplicate taxonomy data.

### 7.3 Enrichment Failures

If enrichment fails (unknown workspace ID, task graph query failure), the event is forwarded to the frontend with the raw IDs. The enrichment layer never drops events. Missing labels are displayed as the raw UUID in the frontend — ugly but functional.

## 8. Oversight Dashboard UX

The oversight dashboard (`wcon-architecture` §4.2) is the unified interface for all highway interaction. It combines trail streaming, gate resolution, escalation handling, and directive injection into a single screen.

### 8.1 Dashboard Layout

```
┌─────────────────────────────────────────────────────────────────┐
│ Session: "Auth Feature Build"    State: ACTIVE    ⏱ 12m 34s     │
├───────────┬─────────────────────────────────────────────────────┤
│           │                                                     │
│ Workspace │  ┌──────────────────────────────────────────────┐   │
│ Tree      │  │ Trail Stream                                 │   │
│           │  │                                              │   │
│ ● coord   │  │ 14:30:01 [implementer] Checkpoint: auth.rs  │   │
│ ├─ planner│  │ 14:30:03 [tester] Signal: STARTED           │   │
│ ├─ impl ● │  │ 14:30:05 [implementer] Envelope: query      │   │
│ ├─ tester │  │ 14:30:08 [coordinator] Signal: CHECKPOINT   │   │
│ └─ review │  │ ▼ auto-scroll                               │   │
│           │  └──────────────────────────────────────────────┘   │
│ Resources │  ┌──────────────┐ ┌─────────────────────────────┐   │
│ impl: 62% │  │ Gates (2)    │ │ Escalations (1)             │   │
│ test: 15% │  │              │ │                             │   │
│ plan: 100%│  │ ⚠ task_appr  │ │ 🔴 implementer              │   │
│           │  │   timeout 4m │ │   "Cannot resolve merge..." │   │
│           │  │ ○ integration│ │                             │   │
│           │  │   timeout 9m │ │                             │   │
│           │  └──────────────┘ └─────────────────────────────┘   │
│           │  ┌──────────────────────────────────────────────┐   │
│           │  │ [Inject Directive...]                        │   │
│           │  └──────────────────────────────────────────────┘   │
└───────────┴─────────────────────────────────────────────────────┘
```

### 8.2 Panel Descriptions

**Session header:** session name, current state, elapsed time since launch. Shows a cancel button for active sessions.

**Workspace tree (left sidebar):** visual representation of the workspace hierarchy. Each workspace shows its role label, current state (color-coded), and resource usage as a progress bar. Clicking a workspace filters the trail stream and shows workspace-specific detail.

| Workspace state | Visual indicator |
|----------------|-----------------|
| `IDLE` | Gray circle |
| `ACTIVE` | Green filled circle |
| `BLOCKED` | Yellow filled circle |
| `SUSPENDED` | Blue outlined circle |
| `CLOSED` | Gray checkmark |
| `FAILED` | Red X |

**Trail stream (main panel, top):** scrolling feed of enriched trail entries. Supports filtering (§3.2), pause/resume, and entry expansion. This is the visibility capability.

Rendering adaptations for domain-specific content:

- **Vertical-specific checkpoint creation.** When a trail entry records creation of a checkpoint whose type matches the session vertical's `VerticalEntry.checkpoint_types` (e.g., Finance `compliance_check`, Healthcare `phi_access_grant`, MLOps `reproducibility_checkpoint`, DataSci `declared_hypothesis`, Analytics `data_snapshot`), the entry is rendered with a field-schema-driven table view rather than a generic JSON blob. The field rendering is described in `wcon-ui` §7.2. The field schema is sourced from the Console's indexed `CheckpointSchema.fields` (`wcon-data-model` §6.1), not from the trail payload itself — the payload carries field values, the schema gives them names, types, descriptions, and enum metadata.

- **Tool-layer refusal entries.** Entries with an `event_type` matching a tool refusal (see §4A.1) are rendered with a red left border and 🚫 icon. Summary line includes the tool name, error code, and short reason. Expansion shows the refusing arg values, the resolved `ToolPolicy` that triggered the refusal, and a "View in Refusal panel" link that jumps to the pending entry (§4A).

- **Quality report entries.** When a vertical's autonomous observer agent (e.g., Finance `auditor`, Healthcare `compliance`) emits a quality report at session end, the trail entry is rendered with a collapsible panel showing per-criterion verdicts (pass/warn/fail). The rendering is sourced from the entry's body plus the indexed `VerticalEntry.quality_criteria` for criterion names and weights.

The general rule: every trail entry starts with a one-line summary (timestamp, workspace label, short description); clicking expands it; the expanded view is either a vertical-schema-aware rendering (when the Console recognizes the entry type from the indexed manifest) or a generic formatted-JSON dump (when it does not). Unknown entry types are never hidden — they are displayed with their raw payload so nothing is lost.

**Gate queue (main panel, bottom-left):** ordered list of pending gates. Each gate shows type, workspace label, summary, vertical rationale (§4.7), and timeout countdown. Clicking opens the gate detail view for resolution. This is the gates capability.

**Escalation inbox (main panel, bottom-right):** list of pending escalations. Each shows workspace label, reason, and age. Clicking opens the escalation detail view for response. This is the escalation handling capability.

**Refusal panel (main panel, below the gate queue when non-empty):** list of pending tool-layer refusals (§4A). Each shows workspace label, tool name, error code, and unblock hint. Clicking expands to the full refusal event, resolved policy metadata, and available actions (inject directive, cancel+clone). This panel is only visible when `pending_refusals` is non-empty — it is not a permanent fixture like the gate queue. When empty, the space is given back to the gate queue and escalation inbox.

**Injection bar (main panel, bottom):** text input for directive injection. The target workspace is selected from the workspace tree. This is the injection capability.

### 8.3 Detail Overlays

Gate detail and escalation detail open as overlay panels that slide in from the right, covering the trail stream but leaving the workspace tree and gate/escalation lists visible. This allows the user to compare a gate's context with the workspace state without navigating away from the dashboard.

### 8.4 Multi-Session View

When multiple sessions are active, the dashboard header shows a session switcher (dropdown or tab bar). Each session has its own workspace tree, trail buffer, gate queue, escalation inbox, and refusal panel. Switching sessions swaps the entire dashboard content.

A consolidated gate queue across all sessions is available via:

**API:** `GET /api/gates/pending`

| Parameter | Type | Description |
|-----------|------|-------------|
| `session_id` | string | Filter to a specific session (omit for all) |
| `type` | string | Filter by gate type |
| `sort` | string | `urgency` (default), `timeout`, `created_at` |

**Authorization scoping:** cross-session endpoints are filtered by session ownership. Operators see only gates/escalations/refusals from sessions they own. Admins see all. Viewers see none (the endpoint returns empty). The filtering is server-side — the frontend never receives items the user is not authorized to act on. See `wcon-auth` §4.2 for the full permission matrix.

This, together with the analogous cross-session endpoints for escalations and refusals, powers the Oversight nav badge (`wcon-ui` §2.1) which aggregates pending gates + escalations + refusals across the user's visible active sessions. The badge breakdown ("2 gates · 1 escalation · 3 refusals") is computed client-side from the three endpoint responses or from the real-time channel state when the dashboard is connected.

## 9. Notification Model

Highway events that require user attention are surfaced through a notification system outside the oversight dashboard, so the user is alerted even when viewing other Console screens.

### 9.1 Notification Events

| Event | Notification | Priority |
|-------|-------------|----------|
| New gate (any type) | "Gate pending: [type] in [session] — [workspace]" | Normal |
| Gate timeout approaching (< 20% remaining) | "Gate expiring: [type] in [session] — [time] remaining" | High |
| Gate auto-resolved (timeout) | "Gate auto-resolved: [type] in [session] — fallback: [action]" | Normal |
| New escalation | "Escalation from [workspace] in [session]" | High |
| New tool-layer refusal | "Refusal: [tool] in [session] — [error_code] ([workspace])" | High |
| Refusal cleared | "Refusal resolved: [tool] in [session]" | Normal (suppressed if silent preference set) |
| Quality report available | "Session [name] completed with quality report" | Normal |
| Session completed | "Session [name] completed" | Normal |
| Session failed | "Session [name] failed: [reason]" | High |
| Runtime disconnected | "Lost connection to WACP runtime — reconnecting" | High |

### 9.2 Notification Delivery

Notifications are delivered through the WebSocket connection. The frontend renders them as:

1. **Toast notifications** — transient banners that auto-dismiss after 5 seconds (normal priority) or require manual dismissal (high priority).
2. **Badge counts** — the navigation bar shows badge counts for pending gates, escalations, and tool-layer refusals across all sessions. The Oversight nav item displays a single aggregated count with a hover breakdown (`wcon-ui` §2.1).
3. **Browser notifications** — if the user has granted notification permission and the Console tab is not focused, high-priority events trigger browser-level notifications.

### 9.3 Notification Preferences

Notification behavior is controlled by frontend-only preferences (stored in browser local storage, not in the Console settings table):

| Preference | Options | Default |
|-----------|---------|---------|
| Toast display | on / off | on |
| Browser notifications | on / off | off |
| Sound alert | on / off | off |
| Gate timeout warning threshold | percentage | 20% |

## 10. Invariants

### 10.1 No Event Suppression

Every highway event received from the runtime is forwarded to connected frontends. The bridge never silently drops, filters, or delays events. Client-side filtering (§3.2) is a display concern — the data is always delivered.

### 10.2 No Automatic Resolution

The Console never auto-resolves gates or auto-responds to escalations. These are human decisions. The runtime may auto-resolve gates on timeout (§4.5), but that is a runtime behavior — the Console does not trigger it.

### 10.3 Resolution Authenticity

Every gate resolution and escalation response sent to the runtime originates from an explicit user action. The Console does not generate synthetic responses, batch-resolve without user selection, or pre-fill decisions.

### 10.4 Enrichment Transparency

Enrichment (§7) adds labels but never modifies the event's semantic content. The raw fields (`workspace_id`, `task_id`, `gate_id`, etc.) are preserved alongside the enriched labels. The frontend can always access the underlying runtime identifiers.

### 10.5 Action Traceability

Every user action (gate resolution, escalation response, directive injection) produces exactly one trail entry in the runtime trail. The trail entry records the action, the actor (human), and the content. The Console's highway actions are fully auditable through the trail.

### 10.6 Session Scoping

All highway interactions are scoped to a session. A user cannot resolve a gate, respond to an escalation, or inject a directive without specifying which session the action belongs to. Cross-session actions are not possible through a single API call.

### 10.7 Refusal Provenance

Every `RefusalEvent` surfaced by the Console originates from a trail entry on `StreamTrail` whose `event_type` and status code match a tool-layer refusal (§4A.1). The Console never synthesizes refusals from workspace state changes alone, from user actions, or from inference about what the runtime "should have" done. If the runtime does not emit a refusal trail entry, the Console does not create a `RefusalEvent`. The `trail_entry_id` field in every `RefusalEvent` (§4A.2) is the load-bearing link to the originating evidence.

### 10.8 Refusal Non-Resolvability

The Console has no API endpoint or UI affordance that resolves a refusal directly. Refusal clearance always flows through one of three upstream paths: the agent creates the prerequisite checkpoint (for `requires_checkpoint`), a gate is resolved (for `requires_gate`), the session is cancelled and cloned with different context (for `budget_limited`), or the agent retries with the override flag (for `classification_gated`). The Console relays refusals; it does not decide them — same principle as §1 Overview, but scoped to refusals.

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-architecture | System Architecture | defines Highway Bridge component (§4.1), gate resolution data flow (§5.4), directive injection data flow (§5.5), concurrency model (§7) |
| wcon-sessions | Session Lifecycle | defines stream subscriptions (§4.1), event processing pipeline including refusal detection (§6.3), session monitor with `pending_refusals` (§6.1), teardown behavior (§7) |
| wcon-discovery | Agent & Role Discovery | defines `ToolPolicy` types (§2.2.2) and `ToolEntry.policy` index (§3.5) consumed by §4.7 rationale enrichment and §4A refusal resolution |
| wcon-data-model | Data Model | defines `VerticalEntry.checkpoint_types` (§6.1) used by trail rendering of vertical-specific checkpoints |
| wcon-ui | UI Design | defines the oversight dashboard panels including the refusals panel (§7.2) and vertical context badges |
| wcon-glossary | Glossary | defines gate, escalation, highway, trail, directive, oversight dashboard, tool-layer refusal, vertical-specific checkpoint |
| wcon-vision | Product Vision | establishes unified oversight (G4) and real-time trail streaming (SC7) as goals |
| wacp-protocol | WACP Protocol Specification | defines HighwayService gRPC API, gate types, escalation mechanism, trail structure |

*WACP Console -- authored by AKIL Abderrahim and Claude Opus 4.6*
