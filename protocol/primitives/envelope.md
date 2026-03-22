# WACP: Envelope

## Metadata

```yaml
title: "WACP: Envelope"
id: wacp-spec-envelope
type: constituent-spec
tier: abstract
category: primitives
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4.2 (envelope primitive, lifecycle, delivery guarantees)
  - §5.5 (permission matrix)
  - §5.6 (port rights)
depends_on:
  - wacp-spec-workspace
  - wacp-spec-signal
  - wacp-spec-roles
  - wacp-spec-user
  - wacp-spec-identity
  - wacp-spec-clock
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, envelope, messaging, delivery, communication]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Envelope Types](#2-envelope-types)
3. [Schema](#3-schema)
4. [Message Flow Rules](#4-message-flow-rules)
5. [Lifecycle](#5-lifecycle)
6. [Delivery Guarantees](#6-delivery-guarantees)
7. [Redelivery](#7-redelivery)
8. [Message Patterns](#8-message-patterns)
9. [Error Handling](#9-error-handling)
10. [Trail Events](#10-trail-events)
11. [Conformance Requirements](#11-conformance-requirements)
12. [Implementation Notes](#12-implementation-notes)
13. [References](#13-references)

---

## 1. Purpose

Envelopes are the protocol's communication primitive. Where signals carry state — lightweight, fixed-type, upward-propagating — envelopes carry content. They are structured messages addressed to specific workspaces, validated against the permission matrix, and delivered with guaranteed ordering within a channel.

An envelope carries a directive from the coordinator to a worker, feedback from the coordinator in response to a query, or a query from a worker seeking clarification. Each has a typed payload, a priority, an explicit sender and receiver, and a place in the conversation thread through `in_reply_to`. Envelopes are the protocol's rich communication channel — everything that signals cannot express travels in an envelope.

Unlike signals, envelope types are extensible. The protocol defines three base types (`directive`, `feedback`, `query`). Applications register additional types through the taxonomy, each with its own send/receive permissions in the permission matrix. This is the inverse of the signal design: signals are a closed set because they drive the state machine; envelope types are open because the content of communication varies by domain.

## 2. Envelope Types

The protocol defines three base envelope types. These map to the three base roles' communication needs (PROTOCOL §5.2).

| Type | Direction | Purpose |
|------|-----------|---------|
| `directive` | Coordinator → Worker | Task assignment. Tells the worker what to do. |
| `feedback` | Coordinator → Worker | Response to a query, mid-task guidance, or instructions to unblock. |
| `query` | Worker → Coordinator | Request for clarification, additional context, or a decision. |

Three types, two directions. The coordinator sends downward (`directive`, `feedback`). Workers send upward (`query`). This asymmetry mirrors the signal system — upward communication is lightweight (signals and queries); downward communication carries instructions (directives and feedback).

**Directives and tasks.** When a directive originates from a task binding (PROTOCOL §4.6), the coordinator constructs the directive's payload from the task's `description` field and includes a `task_ref` in the payload's `attachments` linking the directive to the task it executes. This reference enables the runtime to map workspace events back to the task that caused them. Not all directives originate from tasks — the protocol permits directives without task references (e.g., during system bootstrap before task graphs exist).

**Delegation extends the base types at runtime.** When a workspace is granted the delegation capability (PROTOCOL §5.4), the runtime adds envelope permissions scoped to the delegate's subtree: the delegate can send `directive` and `feedback` to its children, and receive `query` from them. These are the same base types, not new types — delegation reuses the existing vocabulary within a narrower scope.

**Extensibility through the taxonomy.** Applications register additional envelope types through the taxonomy. Each new type specifies:
- A name (unique within the taxonomy)
- Which roles may send it, and to which roles
- Any required payload fields

For example, a `reviewer` derived role (PROTOCOL §5.3) introduces `report` — an envelope from the reviewer to the coordinator containing evaluation results. The `report` type is registered in the taxonomy alongside its permission row: `reviewer → report → coordinator`.

The base permission matrix (PROTOCOL §5.5) is the starting point. Each taxonomy-registered envelope type adds a row. The runtime loads the full matrix at initialization and evaluates it at every envelope send.

**The `type` field is a string, not a closed enum.** Unlike signals (which are a closed set of eleven), envelope types are open. The runtime validates envelope types against the taxonomy — an unregistered type is rejected.

## 3. Schema

Every envelope carries the same structure. The schema is uniform across all envelope types — base and taxonomy-registered alike. Type-specific behavior is governed by permissions and payload conventions, not by structural variation.

```yaml
envelope:
  id: string                      # runtime-assigned, globally unique
  from: workspace_id              # the sending workspace
  to: workspace_id                # the receiving workspace
  originator: user_id | "system"   # causal origin; inherited from workspace (PROTOCOL §4.8)
  type: string                    # base types: directive, feedback, query
                                  # extensible via taxonomy
  payload:
    format: string                # markdown | json | yaml | patch | binary
    content: string               # the message body
    attachments: list             # optional list of references (checkpoint IDs, URIs)
  in_reply_to: envelope_id        # references a previous envelope, null if initiating
  rights: list                    # optional port rights transferred with this envelope
                                  # each entry: {type: send|send_once, target: workspace_id}
  priority: enum
    - normal                      # processed in order
    - urgent                      # moved to front of inbox
    - blocking                    # receiver must act before continuing
  timestamp: datetime             # runtime-assigned at creation
  origin: enum
    - agent                       # normal agent-to-agent envelope (default)
    - human                       # injected via the human highway (PROTOCOL §8.1)
  status: enum
    - created
    - validated
    - delivered
    - acknowledged
    - rejected
```

**Field semantics:**

- **`id`** — globally unique, runtime-assigned (PROTOCOL §4.7, rule 1). Never reused (PROTOCOL §4.7, rule 6).
- **`from`** — the workspace that created the envelope. Always present — there are no anonymous envelopes.
- **`to`** — the target workspace. Always present — there are no broadcasts. If the coordinator needs to send the same directive to three workers, it creates three envelopes.
- **`originator`** — the causal origin of this envelope (PROTOCOL §4.8). On envelopes, the field carries the workspace's originator value: `user_id` for human-caused work, `"system"` for system-initiated work. When a human injects through the highway, the injected envelope carries `originator: <user_id>` of the injecting human. For coordinator-autonomous envelopes, `originator` is `"system"`. Immutable once set. Propagates: when an originator-bearing directive causes the creation of child workspaces, envelopes sent on behalf of those workspaces inherit the originator. This field is distinct from `origin` (which distinguishes agent vs. human as a binary) — `originator` names the specific causal origin.
- **`type`** — identifies the envelope type. Base types (`directive`, `feedback`, `query`) are defined by the protocol. Taxonomy-registered types are validated at runtime — an unregistered type is rejected.
- **`payload.format`** — declares how `content` should be interpreted. The format set is open — implementations may support additional formats beyond the five listed.
- **`payload.content`** — the message body. The protocol does not constrain content structure — it is opaque to the runtime. Meaning is between sender and receiver.
- **`payload.attachments`** — optional references to related objects. Typically checkpoint IDs or external URIs. The protocol does not fetch or validate attachments — they are metadata for the receiver.
- **`in_reply_to`** — links this envelope to a previous one, forming a conversation thread. Null for the first envelope in a thread. The runtime does not enforce thread integrity — it is a structural aid for agents and trail analysis.
- **`rights`** — optional port rights transferred with this envelope (PROTOCOL §5.6). A workspace holding a send right (or send-once right) to workspace C may include that right in an envelope to workspace B. Upon delivery, B gains the right and can communicate directly with C. Receive rights are never transferable. Transferred rights are recorded in the trail as `port_right_transferred` events. Most envelopes carry none.
- **`priority`** — governs inbox ordering. `normal` envelopes are processed in arrival order. `urgent` envelopes are moved to the front. `blocking` envelopes require the receiver to act before processing any other envelope. Priority is set by the sender and immutable after creation.
- **`timestamp`** — assigned by the runtime at creation, not by the agent. Monotonic within the sending workspace.
- **`origin`** — system-assigned and immutable. Defaults to `agent` for all envelopes created through normal protocol flow. Set to `human` for envelopes injected via the human highway (PROTOCOL §8.1). Trail analysis distinguishes human-injected messages from agent-generated ones by filtering on this field.
- **`status`** — tracks the envelope's position in its lifecycle (§5). Managed exclusively by the runtime — agents do not set status.

**Integrity.** Every envelope carries an integrity proof and origin binding, verified by the runtime before delivery. The integrity mechanism is implementation-defined; the protocol requires that it exists and is verified. See PROTOCOL §11.4 for the integrity model and §11.7 for non-repudiation guarantees.

## 4. Message Flow Rules

Five rules govern how envelopes move through the system. These are the normative constraints that every runtime must enforce. They are transport-agnostic (PROTOCOL §3.6) — whether envelopes travel as files on disk, over sockets, or through shared memory, the semantics are identical.

**Rule 1: All envelopes are addressed.** Every envelope names a source (`from`) and a destination (`to`). There are no broadcasts, no multicast, no "send to all workers." If the coordinator needs to send the same directive to three workers, it creates three envelopes — each with a distinct `to` field, each independently validated and delivered, each producing its own trail entries.

**Rule 2: All envelopes are validated before delivery.** The runtime checks three things before an envelope reaches the receiver's inbox:
- **Send right**: the sender holds a valid send right (or send-once right) to the receiver (PROTOCOL §5.6).
- **Permission**: the sender's role is authorized to send this envelope type to the receiver's role, per the permission matrix (PROTOCOL §5.5).
- **Structure**: the envelope carries all required fields (§3), the `type` is registered in the taxonomy, and the `to` workspace exists and is not in a sealed state (`integrating`, `closed`, or `failed`).

If any check fails, the envelope is rejected. A rejected envelope transitions to `rejected` status (§5), an `envelope_rejected` trail entry is recorded with the rejection reason, and the sender receives notification of the failure. The envelope never reaches the receiver's inbox.

**Rule 3: All envelopes are acknowledged.** When a validated envelope is placed in the receiver's inbox, the runtime automatically emits an `acknowledged` signal back to the sender. This is not optional and does not require agent action. The `acknowledged` signal's `ref` field carries the envelope ID, linking the acknowledgment to the specific envelope. Acknowledgment means "delivered to inbox," not "understood," "accepted," or "acting on." The receiver may never read the envelope — the acknowledgment is still valid.

**Rule 4: Envelopes are ordered within a channel.** A channel is a directed pair: `(from, to)`. Within a channel, envelopes are delivered in the order they were created (by `timestamp`). If the coordinator sends three envelopes to worker A, worker A receives them in send order. Across channels, no ordering is guaranteed — the coordinator may send to worker A then worker B, but worker B may receive its envelope first.

**Rule 5: Envelopes are durable.** Once an envelope passes validation (rule 2), it is written to the trail before delivery. The trail entry is the envelope's permanent record — if the runtime crashes between trail recording and inbox delivery, the envelope persists and delivery is retried on recovery. Envelopes are never silently dropped. Every envelope that passes validation eventually reaches the receiver's inbox or is recorded as undeliverable (receiver reached a terminal state after validation but before delivery).

**Interaction with workspace state.** Not all workspace states accept envelopes equally:
- **`idle`**: the inbox accepts envelopes. The first envelope typically triggers the transition to `active`.
- **`active`**: normal operation. The inbox accepts and the agent processes envelopes.
- **`blocked`**: the inbox still accepts envelopes — the workspace is suspended, not sealed. The coordinator's `feedback` envelope is the typical mechanism for unblocking.
- **`migrating`**: the inbox is frozen. Envelopes validated during migration are queued by the runtime and delivered when migration completes and the workspace resumes.
- **`suspended`**: the inbox continues to accept envelopes — they queue for later delivery. When the coordinator resumes the workspace, queued envelopes are delivered in order.
- **`integrating`**: the inbox is sealed. The worker has emitted `complete` — its work is done. The coordinator reads checkpoints during integration; no envelope exchange is needed.
- **`closed`, `failed`**: terminal states. The inbox is sealed. Envelopes addressed to a terminal workspace are rejected at validation (rule 2).

## 5. Lifecycle

An envelope moves through a linear sequence of states from creation to acknowledgment. There is one branch point — validation — where the envelope either proceeds to delivery or is rejected.

```
created → validated → delivered → acknowledged
               │
               └→ rejected
```

**Five states, four transitions:**

**`created`** — The agent has constructed the envelope. The runtime assigns the `id`, `timestamp`, and `origin` fields (§3). At this point the envelope exists but has not been examined. No trail entry yet — the envelope is not part of the protocol record until it passes or fails validation.

**`validated`** — The runtime has verified send right, permission, and structure (§4, rule 2). On success, the envelope transitions to `validated` and an `envelope_created` trail entry is recorded — this is the envelope's entry into the permanent record. On failure, the envelope transitions to `rejected`.

**`delivered`** — The envelope has been placed in the receiver's inbox. The receiver's agent can now read and act on it. An `envelope_delivered` trail entry is recorded. The runtime emits an `acknowledged` signal to the sender's workspace with `ref` set to the envelope ID. Delivery and acknowledgment are a single atomic step from the protocol's perspective — there is no state between "in the inbox" and "acknowledged."

**`acknowledged`** — Terminal state. The envelope has been delivered and the sender has been notified. The envelope's lifecycle is complete. The agent may or may not act on the envelope — that is outside the protocol's scope. The protocol's obligation ends at delivery confirmation.

**`rejected`** — Terminal state. Validation failed. An `envelope_rejected` trail entry is recorded with the rejection reason (`permission_denied`, `no_send_right`, `invalid_type`, `invalid_structure`, `target_terminal`, `target_not_found`). The sender is notified of the rejection. The envelope never reaches the receiver.

**Every transition is recorded.** The trail captures the full lifecycle: creation (after validation), delivery, and rejection. The `status` field on the envelope (§3) reflects the current state. It is managed exclusively by the runtime — agents read it but never set it.

**No agent-driven transitions.** Unlike workspace state transitions — where agent signals trigger state changes — envelope state transitions are entirely runtime-managed. The agent creates the envelope and hands it to the runtime. From that point, the runtime validates, delivers, acknowledges, or rejects. The agent's only role is to construct the envelope and, on the receiving end, to read it from the inbox.

## 6. Delivery Guarantees

Envelopes carry stronger delivery properties than signals. Signals are fire-and-forget with at-least-once delivery. Envelopes are validated, durable, acknowledged, and ordered — the protocol provides explicit confirmation that a message reached its destination.

**At-most-once delivery.** A validated envelope is delivered to the receiver's inbox exactly once. The runtime does not duplicate envelopes. If an implementation's transport layer delivers a duplicate (e.g., due to retry after a network partition), the runtime deduplicates by envelope `id` before inbox placement. The receiver never sees the same envelope twice.

**Acknowledgment guarantee.** Every delivered envelope produces exactly one `acknowledged` signal to the sender. The acknowledgment is automatic and atomic with delivery (§5). If the sender's workspace is in a terminal state when the acknowledgment arrives, the signal is recorded in the global trail but has no recipient to act on it.

**Channel ordering.** Within a channel `(from, to)`, envelopes are delivered in creation order. This is a strict guarantee — not best-effort. The runtime must not reorder envelopes within a channel even under concurrent delivery pressure. Across channels, no ordering is guaranteed (§4, rule 4).

**Durability.** A validated envelope is written to the trail before delivery (§4, rule 5). The trail entry is the envelope's permanent record. If the runtime fails between trail recording and inbox placement, recovery replays the trail and completes delivery. The durability guarantee means: once an envelope passes validation, it will eventually be delivered or recorded as undeliverable.

**Undeliverable envelopes.** An envelope becomes undeliverable when the target workspace reaches a terminal state after validation but before delivery. This is a narrow race condition — the envelope was valid at validation time, but the target died before the envelope arrived. The runtime records an `envelope_undeliverable` trail entry with the envelope ID and the reason (target workspace's terminal state). The sender is notified. The envelope is not retried — the target no longer exists as a viable recipient.

**No delivery to terminal workspaces.** Envelopes addressed to workspaces in `closed` or `failed` states are rejected at validation (§4, rule 2), not at delivery. The undeliverable case above covers only the race condition where the workspace transitions to terminal between validation and delivery.

**Blocking priority semantics.** A `blocking` priority envelope, once delivered, requires the receiver to process it before any other envelope in the inbox. The runtime enforces this by placing `blocking` envelopes at the front of the inbox and pausing delivery of subsequent envelopes until the `blocking` envelope is consumed. If multiple `blocking` envelopes arrive, they are processed in channel order among themselves.

## 7. Redelivery

Redelivery is the mechanism that fulfills the durability guarantee (§6). It operates in two contexts: acknowledgment timeout during normal operation, and trail gap detection during recovery.

### 7.1 Acknowledgment-Timeout Redelivery

During normal operation, if no `acknowledged` signal arrives within a transport-defined timeout after delivery, the runtime redelivers the envelope.

**Algorithm:**

1. **Maximum attempts:** 3 redeliveries (4 total attempts including the initial delivery). This is a protocol constant (PROTOCOL §12.5), not configurable — it balances reliability against infinite retry storms.
2. **Backoff:** Each redelivery waits `attempt_number * base_timeout` before retrying (linear backoff). The base timeout is transport-specific.
3. **Exhaustion:** After all redelivery attempts fail, the runtime emits a `failed` signal to the sender with `reason: delivery_exhausted` and records an `envelope_undeliverable` trail entry. The coordinator decides how to respond.
4. **Deduplication interaction:** Redelivered envelopes carry the same `id` as the original. The receiver's deduplication logic (§7.3) ensures the agent processes the envelope exactly once, even if a delayed acknowledgment causes an unnecessary redelivery.

### 7.2 Recovery Redelivery

When the runtime restarts after a crash, it identifies envelopes with an `envelope_created` trail entry but no corresponding `envelope_delivered` or `envelope_undeliverable` entry. This gap means the envelope was accepted but never reached its destination.

**Recovery redelivery respects workspace state.** When the runtime attempts redelivery, the target workspace may have changed state since the envelope was originally validated:
- If the target is still in a deliverable state (`idle`, `active`, `blocked`): redelivery proceeds normally.
- If the target is in `migrating`: the envelope is queued and delivered when migration completes.
- If the target has reached a sealed state (`integrating`, `closed`, `failed`): the envelope is recorded as undeliverable. An `envelope_undeliverable` trail entry is created. The envelope is not retried further.

**Recovery redelivery preserves channel ordering.** If multiple envelopes in the same channel require redelivery, they are redelivered in their original creation order. The runtime does not interleave redelivered envelopes with new envelopes in the same channel — redelivery completes before new delivery resumes for that channel.

### 7.3 Deduplication

The receiver's transport layer maintains a set of delivered envelope IDs. If an envelope with a previously-seen ID arrives, the duplicate is:
1. Recorded in the trail as an `envelope_redelivered` event (for audit)
2. NOT delivered to the agent a second time
3. NOT acknowledged again (no duplicate `acknowledged` signal)

The deduplication window spans the workspace's lifetime. After a workspace reaches a terminal state, its deduplication set may be discarded — terminal workspaces reject all envelopes regardless.

### 7.4 Signal Deduplication

Signals are idempotent with respect to state transitions. If a signal arrives that would trigger a transition the workspace has already made (e.g., a duplicate `complete` signal arriving after the workspace is already in `integrating`), the duplicate is:
1. Recorded in the trail as `signal_delivered` (for audit — the trail is complete)
2. NOT processed for state transitions (the workspace is already past that state)

The state machine is the deduplication mechanism for signals: a transition is only valid if the current state matches the `from` state. Duplicates fail this check and are silently absorbed.

**No redelivery for rejected envelopes.** An envelope that failed validation (§5, `rejected` status) is never redelivered. Rejection is final — the envelope's structural or permission deficiency does not change on retry.

## 8. Message Patterns

The protocol supports interaction patterns built from envelopes and signals. These are abstract — applications compose them into concrete workflows. Each pattern shows the envelope and signal sequence between participants.

**Pattern 1: Command (one-way with acknowledgment)**

```
Coordinator                      Worker
     │                              │
     │──── envelope(directive) ────>│
     │<─── signal(acknowledged) ────│
     │<─── signal(started) ─────────│
     │         ...work...           │
     │<─── signal(complete) ────────│
```

The coordinator sends a directive; the worker acts. The `acknowledged` signal confirms delivery. The `started` and `complete` signals report progress through the signal system, not through envelopes. This is the most common pattern — every workspace begins with a command.

**Pattern 2: Query-Response (round trip)**

```
Worker                         Coordinator
     │                              │
     │──── envelope(query) ────────>│
     │<─── signal(acknowledged) ────│
     │         ...thinking...       │
     │<─── envelope(feedback) ──────│
     │──── signal(acknowledged) ───>│
```

A worker requests clarification or a decision. The coordinator responds with feedback. Both envelopes produce acknowledgment signals. The `in_reply_to` field links the feedback to the original query, forming a thread. Multiple query-response cycles may occur within a single workspace — the worker is not limited to one question.

**Pattern 3: Blocked-then-unblocked**

```
Worker                         Coordinator
     │                              │
     │──── signal(blocked) ────────>│
     │         ...suspended...      │
     │<─── envelope(feedback) ──────│
     │──── signal(acknowledged) ───>│
     │──── signal(started) ────────>│
     │         ...resumed...        │
```

The worker cannot continue and emits a `blocked` signal with a reason. The coordinator reads the reason and sends a `feedback` envelope with instructions to unblock. The worker resumes and emits `started`. This pattern combines signals (upward notification) with envelopes (downward instruction) — each primitive in its natural direction.

**Pattern 4: Delegation chain**

```
Coordinator          Delegate              Worker
     │                  │                     │
     │── directive ───>│                     │
     │<── acknowledged ─│                     │
     │<── started ──────│                     │
     │                  │── directive ───────>│
     │                  │<── acknowledged ────│
     │                  │<── started ─────────│
     │                  │       ...work...    │
     │                  │<── complete ────────│
     │                  │       ...integrate..│
     │<── complete ─────│                     │
```

A delegate receives a directive from the coordinator, creates child workspaces, and manages them. The delegate sends directives and feedback to its children, and receives queries from them — using the same base envelope types within a narrower scope (§2). The coordinator sees the delegate's signals; the children's signals are visible in the global trail.

## 9. Error Handling

Envelope errors fall into three categories: rejection at validation, failure during delivery, and unresponsive receivers. Each has a defined protocol response.

**Rejection.** An envelope that fails validation (§4, rule 2; §5, `rejected` status) produces:
1. An `envelope_rejected` trail entry with the rejection reason.
2. Notification to the sender.

Rejection reasons are a closed set:

| Reason | Meaning |
|--------|---------|
| `permission_denied` | Sender's role cannot send this envelope type to the receiver's role |
| `no_send_right` | Sender does not hold a valid send right to the receiver |
| `invalid_type` | Envelope type is not registered in the taxonomy |
| `invalid_structure` | Required fields missing or malformed |
| `target_not_found` | The `to` workspace does not exist |
| `target_terminal` | The `to` workspace is in `closed`, `failed`, or `integrating` state |

The sender may correct the issue and retry with a new envelope (new `id`). A rejected envelope's `id` is consumed — it cannot be reused even after correction.

**Delivery failure.** An envelope that passes validation but cannot be placed in the receiver's inbox (§6, undeliverable envelopes) produces:
1. An `envelope_undeliverable` trail entry with the envelope ID and reason.
2. Notification to the sender.

This occurs only when the target workspace transitions to a terminal state between validation and delivery — a narrow race condition. The envelope is not retried (§7). The coordinator reads the trail to understand the failure and decides whether to create a new workspace or abort.

**Redelivery exhaustion.** If all redelivery attempts fail (§7.1), the runtime:
1. Records an `envelope_undeliverable` trail entry.
2. Emits a `failed` signal to the sender with `reason: delivery_exhausted`.

The coordinator decides the response: abort the target workspace, attempt the directive in a new workspace, or escalate to the human highway.

**Sender in terminal state.** If the sender's workspace reaches a terminal state after sending an envelope but before receiving the `acknowledged` signal, the acknowledgment is recorded in the global trail but has no recipient. This is not an error — the envelope was delivered successfully. The sender simply cannot observe the acknowledgment.

**Malformed retry.** An agent that receives a rejection may construct a corrected envelope and send it. The corrected envelope is a new envelope with a new `id` — it is not a "retry" in the protocol's sense. It follows the full lifecycle from `created` through validation. If the original rejection was `permission_denied`, a corrected envelope with the same sender and receiver will be rejected again — permissions are structural, not transient.

## 10. Trail Events

Every envelope lifecycle transition produces a trail entry. The envelope system generates five envelope event types and four port right event types.

| Event | When | Body |
|-------|------|------|
| `envelope_created` | Envelope passes validation | `envelope_id`, `from`, `to`, `type`, `priority`, `in_reply_to`, `originator`, `timestamp` |
| `envelope_delivered` | Envelope placed in receiver's inbox | `envelope_id`, `from`, `to`, `delivered_at` |
| `envelope_rejected` | Envelope fails validation | `envelope_id`, `from`, `to`, `type`, `reason`, `timestamp` |
| `envelope_undeliverable` | Target reached terminal state after validation, or redelivery exhausted | `envelope_id`, `from`, `to`, `reason`, `timestamp` |
| `envelope_redelivered` | Duplicate envelope detected and suppressed | `envelope_id`, `from`, `to`, `timestamp` |

Port right operations also produce trail entries. These are recorded alongside envelope events because port rights are transferred via envelopes and govern envelope delivery.

| Event | When | Body |
|-------|------|------|
| `port_right_created` | Send right granted at workspace creation (per permission matrix) | `right_id`, `right_type`, `holder`, `target`, `created_by` |
| `port_right_transferred` | Send right transferred via envelope delivery (§3, `rights` field) | `right_id`, `right_type`, `from_holder`, `to_holder`, `target`, `via_envelope` |
| `port_right_revoked` | Coordinator revokes a send right | `right_id`, `right_type`, `holder`, `target`, `revoked_by`, `reason` |
| `port_right_consumed` | Send-once right consumed after use | `right_id`, `holder`, `target`, `via_envelope` |

**Two entries per successful envelope.** A successfully delivered envelope produces `envelope_created` (at validation) and `envelope_delivered` (at inbox placement). These are separate events because they happen at different moments — validation may precede delivery by a measurable interval, especially under load or during migration queuing. The `envelope_created` entry is written to the sender's local trail. The `envelope_delivered` entry is written to the receiver's local trail. Both appear in the global trail.

**One entry for failures.** A rejected envelope produces only `envelope_rejected`. An undeliverable envelope produces `envelope_created` (it passed validation) and then `envelope_undeliverable`. A deduplicated redelivery produces only `envelope_redelivered` — the original `envelope_created` and `envelope_delivered` entries already exist.

**The trail is the envelope's permanent record.** Once an envelope is recorded in the trail, it exists regardless of what happens next. If the receiver fails before reading the envelope, the `envelope_created` and `envelope_delivered` entries survive. Recovery reconstructs the inbox from trail entries to identify unprocessed envelopes.

**No payload in trail entries.** Trail entries reference envelopes by `id` but do not duplicate the payload. The trail records *that* an envelope was sent, delivered, or rejected — not *what* the envelope contained. Payload content is retrievable through the envelope itself (by `id`). This keeps the trail compact and avoids duplicating potentially large payloads across local and global trails.

**Relationship to signal trail events.** Every delivered envelope also produces a `signal_emitted` trail entry for the `acknowledged` signal. The envelope trail and signal trail are complementary — the envelope trail tracks the message lifecycle, the signal trail tracks the acknowledgment. Cross-referencing uses the `acknowledged` signal's `ref` field, which carries the envelope ID.

## 11. Conformance Requirements

| Requirement | Level | Description |
|-------------|-------|-------------|
| Three base envelope types | Core | Runtime MUST implement `directive`, `feedback`, and `query` as base types. |
| Envelope schema | Core | Every envelope MUST carry all fields defined in §3. Runtime-assigned fields (`id`, `timestamp`, `origin`, `status`) MUST be set by the runtime, never by agents. |
| Addressed delivery | Core | Every envelope MUST name a `from` and `to` workspace. No broadcast or multicast. |
| Send right validation | Core | Runtime MUST verify the sender holds a valid send right before delivery. Envelopes without valid rights MUST be rejected. |
| Permission validation | Core | Runtime MUST check the permission matrix before delivery. Unauthorized envelopes MUST be rejected. |
| Structure validation | Core | Runtime MUST verify required fields, registered type, and non-terminal target before delivery. |
| Automatic acknowledgment | Core | Runtime MUST emit an `acknowledged` signal to the sender upon delivery. The signal's `ref` field MUST carry the envelope ID. |
| Channel ordering | Core | Envelopes within a channel `(from, to)` MUST be delivered in creation order. |
| Durability | Core | Validated envelopes MUST be written to the trail before delivery. |
| Lifecycle states | Core | Every envelope MUST progress through the lifecycle defined in §5. Status transitions MUST be managed exclusively by the runtime. |
| Rejection reasons | Core | Rejected envelopes MUST carry a reason from the closed set defined in §9. |
| Trail recording | Core | Every lifecycle transition MUST produce the corresponding trail entry defined in §10. |
| At-most-once delivery | Standard | Runtime MUST deduplicate envelopes by `id` before inbox placement. No duplicate deliveries. |
| Redelivery on timeout | Standard | If no acknowledgment arrives within the transport timeout, the runtime MUST redeliver per §7.1 (3 attempts, linear backoff). |
| Redelivery on recovery | Standard | After runtime failure, validated-but-undelivered envelopes MUST be redelivered per §7.2. |
| Redelivery ordering | Standard | Redelivered envelopes in the same channel MUST be delivered in original creation order. |
| Bounded redelivery | Standard | Redelivery attempts MUST NOT exceed 3 (protocol constant). Exhaustion MUST be recorded in the trail. |
| Workspace state interaction | Standard | Runtime MUST enforce the inbox acceptance rules per workspace state (§4): queued during `migrating`; sealed during `integrating` and terminal states. |
| Blocking priority | Standard | `blocking` envelopes MUST be placed at the front of the inbox. Subsequent deliveries MUST pause until the `blocking` envelope is consumed. |
| Taxonomy validation | Standard | Runtime MUST reject envelope types not registered in the taxonomy. |
| Originator field | Standard | Envelopes resulting from human highway injections MUST carry `originator: user_id` (PROTOCOL §4.8). `originator` MUST be immutable once set. |
| Port right transfer | Standard | The `rights` field MUST be processed on delivery: transferred rights are granted to the receiver, send-once rights are consumed. Transfers MUST be recorded in the trail. |
| No payload in trail | Full | Trail entries MUST reference envelopes by `id` without duplicating payload content. |

## 12. Implementation Notes

These notes are non-normative. They capture practical guidance for implementers.

**Envelope as a rich struct.** Unlike signals (fixed-size, eleven types), envelopes carry variable-length payloads and open-ended types. Implementations should represent the envelope metadata (id, from, to, type, priority, status, timestamps) as a fixed struct and the payload as a separate, lazily-loaded component. The metadata is needed for validation, routing, and trail recording; the payload is needed only by the receiving agent.

**Inbox as a priority queue.** The workspace inbox is naturally a priority queue ordered by: `blocking` first, then `urgent`, then `normal`, with creation-order tiebreaking within each priority level. A simple three-bucket design — one queue per priority — satisfies the ordering and blocking requirements. The `blocking` bucket pauses drain from the other two until empty.

**Validation as a two-phase check.** Send right validation, permission validation, and structure validation are independent and can run in any order. In practice, checking structure first (cheap — field presence, type registration) before permissions (requires matrix lookup) before send rights (requires rights table lookup) reduces unnecessary work when envelopes are malformed. All must pass; the first failure determines the rejection reason.

**Channel ordering with concurrent senders.** Channel ordering requires that envelopes from workspace A to workspace B arrive in timestamp order. If A sends two envelopes in quick succession, the runtime must not reorder them during delivery. The simplest implementation serializes delivery per channel — a per-channel lock or queue. More sophisticated implementations may use timestamp-ordered delivery buffers. The key invariant: within a channel, delivery order matches creation order.

**Deduplication window.** The deduplication set (envelope IDs already delivered) must span the workspace's lifetime. For long-running workspaces, this set grows unboundedly. Implementations may use a bloom filter for approximate deduplication with a fallback to trail lookup for confirmation. The false-positive cost of a bloom filter is a trail query — not a missed duplicate.

**Trail-before-delivery pattern.** The durability guarantee (§6) requires writing the trail entry before inbox placement. This is the envelope system's write-ahead log. Implementations should treat the trail write and inbox placement as a two-phase operation: trail write commits the intent, inbox placement fulfills it. Recovery scans for committed-but-unfulfilled entries and completes delivery.

**Acknowledged signal as a side effect.** The `acknowledged` signal is emitted by the runtime as a side effect of inbox placement, not by the receiving agent. Implementations should wire acknowledgment into the delivery path — after the envelope is placed in the inbox and the `envelope_delivered` trail entry is recorded, the runtime emits the signal. This is a single atomic sequence: trail write → inbox placement → acknowledgment signal emission.

**Taxonomy type lookup.** Envelope type validation requires checking whether the type string is registered in the taxonomy. This lookup happens on every envelope send. Implementations should precompute the set of valid types at initialization (when the taxonomy is loaded) and store it as a hash set. The lookup is then O(1) per envelope — no taxonomy traversal at send time.

**Redelivery timer management.** Each in-flight envelope (validated but not yet acknowledged) requires a timer. For systems with thousands of concurrent envelopes, a timer-wheel implementation is more efficient than per-envelope timers. The base timeout should be calibrated to the transport's expected round-trip time — too short causes unnecessary redeliveries, too long delays failure detection.

## 13. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §3.6 | §4 | Protocol Over Tooling — transport-agnostic semantics |
| §4.2 | §1–§12 | Envelope primitive — field list, lifecycle, delivery guarantees |
| §4.6 | §2 | Task primitive — directive-task binding, `task_ref` in attachments |
| §4.7 | §3 | Identity rules — runtime-assigned IDs, non-recyclable |
| §4.8 | §3 | User identity — originator propagation, ownership |
| §5.2 | §2 | Base roles — coordinator, worker, observer communication needs |
| §5.3 | §2 | Role inheritance — derived roles and taxonomy-registered types |
| §5.4 | §2 | Delegation — envelope permissions scoped to subtree |
| §5.5 | §4, §11 | Permission matrix — envelope send/receive authorization |
| §5.6 | §3, §4 | Port rights — send right validation, right transfer via envelope |
| §8.1 | §3 | Human highway — `origin: human` for injected envelopes |
| §10.3 | §7.2 | Recovery procedure — redelivery of validated-but-undelivered envelopes |
| §11.4 | §3 | Message integrity — envelope integrity proof and origin binding |
| §11.7 | §3 | Non-repudiation — guarantees for envelope origin |
| §12.5 | §7.1 | Protocol constants — redelivery attempts (3) |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
