# WACP: Communication Topology

## Metadata

```yaml
title: "WACP: Communication Topology"
id: wacp-spec-channels
type: constituent-spec
tier: abstract
category: topology
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4.2 (envelope lifecycle, delivery, channels)
  - §5.5 (permission matrix)
  - §5.6 (port rights)
depends_on:
  - wacp-spec-tree
  - wacp-spec-envelope
  - wacp-spec-signal
  - wacp-spec-workspace
  - wacp-spec-roles
  - wacp-spec-identity
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, topology, channels, port-rights, communication-graph, envelope-threads, conversation]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [The Port Rights Graph](#2-the-port-rights-graph)
3. [Channels](#3-channels)
4. [Envelope Threads](#4-envelope-threads)
5. [Communication Topology and the Structural Tree](#5-communication-topology-and-the-structural-tree)
6. [Invariants](#6-invariants)
7. [Trail Events](#7-trail-events)
8. [Conformance Requirements](#8-conformance-requirements)
9. [Implementation Notes](#9-implementation-notes)
10. [References](#10-references)

---

## 1. Purpose

The workspace tree defines containment. The task graph defines dependency. The causal forest defines attribution. The communication topology defines who can talk to whom, through what, and in what order.

The protocol has two communication mechanisms: envelopes (content, addressed, validated) and signals (state, typed, upward-propagating). They use different topologies:

- **Signals** propagate upward through the workspace tree (Tree spec §4.1). The signal topology IS the structural tree traversed in reverse. No separate structure is needed.
- **Envelopes** travel point-to-point between workspaces, governed by port rights. The envelope topology is a directed graph determined by the port rights table — independent of the workspace tree's structure.

This spec defines the envelope communication topology: the port rights graph, the channel abstraction, and the envelope thread structure. Signal topology is fully defined by the Tree spec and is not repeated here.

## 2. The Port Rights Graph

### 2.1 Formal Definition

The port rights graph is a directed graph where nodes are workspaces and edges are active send rights.

**Formally:** Given a set of workspaces W and a set of active port rights R, the port rights graph G = (W, E) where:
- E = {(A, B) | workspace A holds a valid send right to workspace B}
- Each edge may have multiplicity (A may hold multiple send rights to B)

### 2.2 Formal Properties

| Property | Value |
|----------|-------|
| Type | Directed graph (not necessarily acyclic, not necessarily connected) |
| Nodes | Workspaces |
| Edge semantics | `A → B` means "A holds a send right to B and can deliver envelopes" |
| Edge types | Two: `send` (persistent), `send_once` (consumed on use). The `receive` right is a per-node capability (a workspace receives on its own inbox), not a graph edge. |
| Cycles | Allowed — A may send to B and B may send to A (the coordinator-worker interaction is inherently bidirectional) |
| Connected | Not required — a workspace may have no send rights to any other workspace (and no workspace may have rights to it) |
| Mutability | **Highly mutable.** Edges are created, transferred, consumed, and revoked during execution. |
| Relationship to structural tree | Independent — a child may have no send right to its parent (though this is unusual), and a workspace may have a send right to a structurally distant workspace. |

### 2.3 Edge Lifecycle

Port rights have their own lifecycle. Each right (edge in the graph) is:

**Created** — by the coordinator at workspace creation, based on the permission matrix (PROTOCOL §5.5, Roles spec). The matrix determines the initial graph: coordinators get send rights to workers (for directives), workers get send rights to the coordinator (for queries).

**Active** — the right is held by a workspace and can be used to send envelopes. A send right remains active until revoked or until the holding workspace reaches a terminal state.

**Transferred** — a workspace includes a send right in an envelope's `rights` field (Envelope spec §3). On delivery, the right is removed from the sender and granted to the receiver. This is a graph mutation: the edge `(A, B)` disappears and `(C, B)` appears (where C is the receiver of the envelope).

**Consumed** — a `send_once` right is destroyed after one use. The envelope is delivered, and the edge is removed from the graph. This is a graph deletion with no replacement.

**Revoked** — the coordinator revokes a send right (PROTOCOL §5.6). The edge is removed immediately. Envelopes in flight on the revoked right are delivered (they were accepted before revocation), but no new envelopes may be sent. This is how the coordinator enforces communication boundaries dynamically.

**Expired** — when a workspace reaches a terminal state (`closed` or `failed`), all its send rights become invalid. Outbound edges are removed. Inbound edges (rights held by other workspaces to send TO this workspace) are effectively dead — envelopes to a terminal workspace are rejected with `reason: target_terminal`.

### 2.4 Initial Graph

The permission matrix (PROTOCOL §5.5, Roles spec) determines the initial port rights graph at workspace creation:

| Sender role → Envelope type | Receiver role | Initial right |
|---|---|---|
| Coordinator → `directive` | Worker | `send` |
| Coordinator → `feedback` | Worker | `send` |
| Worker → `query` | Coordinator | `send` |

For a simple tree (coordinator + N workers), the initial graph is:

```
Coordinator → Worker-1  (send, for directives/feedback)
Coordinator → Worker-2  (send)
...
Coordinator → Worker-N  (send)
Worker-1 → Coordinator  (send, for queries)
Worker-2 → Coordinator  (send)
...
Worker-N → Coordinator  (send)
```

This is a star topology — the coordinator at the center, bidirectional edges to each worker, no worker-to-worker edges. Delegation and right transfer can reshape this initial topology.

### 2.5 Delegation and the Graph

When a workspace has `delegate: true` (PROTOCOL §5.4), it can create child workspaces and gains coordinator-like send rights to them. Delegation extends the graph beyond the initial star:

```
Coordinator → Delegate-A   (send)
Delegate-A → Coordinator   (send)
Delegate-A → Worker-A1     (send, delegate-granted)
Delegate-A → Worker-A2     (send, delegate-granted)
Worker-A1 → Delegate-A     (send, delegate-granted)
Worker-A2 → Delegate-A     (send, delegate-granted)
```

The delegate acts as a local hub — its sub-graph is a smaller star within the larger star. Workers A1 and A2 can communicate with Delegate-A but not directly with the coordinator (unless granted additional rights).

### 2.6 Right Transfer and Graph Evolution

Right transfer is the mechanism by which the port rights graph evolves beyond the initial matrix-derived topology.

**Example:** Coordinator gives Worker-A a `send_once` right to Worker-B by including it in a directive envelope. On delivery, Worker-A holds a one-shot communication path to Worker-B. Worker-A sends a single envelope to Worker-B. The right is consumed. The edge disappears.

This enables patterns not possible with the initial matrix: worker-to-worker communication, callback patterns, and ad-hoc coordination channels.

## 3. Channels

### 3.1 Definition

A channel is a directed pair `(from, to)` — a specific sender-receiver combination. Within a channel, envelope delivery is strictly ordered by creation timestamp (Envelope spec §4, rule 4).

**Formally:** A channel C = (A, B) exists whenever workspace A holds a send right to workspace B. The channel carries envelopes from A to B in creation order.

### 3.2 Channel Properties

| Property | Value |
|----------|-------|
| Ordering | Strict within a channel — envelopes delivered in creation order |
| Cross-channel ordering | None — no ordering guarantee between channels |
| Multiplicity | One channel per directed pair. Multiple send rights from A to B do not create multiple channels — they all share one ordered channel. |
| Lifetime | A channel exists as long as the sender holds at least one valid send right to the receiver. When all rights expire or are revoked, the channel closes. |
| Bidirectionality | Two separate channels: `(A, B)` and `(B, A)`. They are independent — ordering in one does not affect the other. |

### 3.3 Channel Count

The number of active channels in the system is bounded by the number of active send rights (ignoring multiplicity). In the initial star topology with N workers:

- N channels: coordinator → worker-i (one per worker)
- N channels: worker-i → coordinator (one per worker)
- Total: 2N channels

Delegation, right transfer, and dynamic right creation can increase this. The upper bound is O(W²) where W is the number of active workspaces — each pair could have two channels. In practice, the graph is sparse — most workspaces communicate only with the coordinator or their delegate.

## 4. Envelope Threads

### 4.1 Definition

An envelope thread is a tree of envelopes linked by the `in_reply_to` field. The root of the thread is an envelope with `in_reply_to: null` (the initiating message). Subsequent envelopes reference the envelope they respond to.

**Formally:** An envelope thread is a rooted tree T = (E, R) where:
- E is a set of envelopes
- R = {(e₁, e₂) | e₂.in_reply_to == e₁.id} — edges from parent to reply
- The root envelope has `in_reply_to: null`

### 4.2 Thread Properties

| Property | Value |
|----------|-------|
| Type | Rooted tree (rooted at the initiating envelope with `in_reply_to: null`) |
| Nodes | Envelopes |
| Edge semantics | `e₁ → e₂` means "e₂ is a reply to e₁" |
| Branching | Allowed — multiple envelopes may reply to the same envelope |
| Cross-workspace | Yes — a thread may span workspaces (coordinator sends directive, worker queries, coordinator responds, worker queries again) |
| Cross-channel | Yes — the directive travels on channel (coordinator → worker), the query on channel (worker → coordinator), the feedback on channel (coordinator → worker). A single thread uses multiple channels. |
| Enforcement | None — the runtime does not enforce thread integrity (Envelope spec §3). `in_reply_to` is a structural aid, not a constraint. An envelope may reference any previous envelope, regardless of thread structure. |

### 4.3 Thread Shapes

**Linear thread.** `directive → query → feedback → query → feedback`. A single back-and-forth conversation between coordinator and worker. The most common shape.

**Branching thread.** A directive spawns multiple queries from different workspaces (if the directive was sent to multiple workers, each worker's query references the same directive). The thread branches at the directive.

**Detached reply.** An envelope with `in_reply_to` referencing an envelope from a different thread. The runtime does not prevent this — the `in_reply_to` field is advisory. Thread analysis must handle cross-linked envelopes.

### 4.4 Threads and Channels

Threads and channels are orthogonal structures:

- A **channel** is a transport property — it governs ordering between a specific sender and receiver.
- A **thread** is a conversation property — it governs the logical grouping of envelopes regardless of who sent them.

A single thread typically uses two channels (coordinator → worker and worker → coordinator). A single channel may carry envelopes from multiple threads (the coordinator sends directives for two different conversations to the same worker).

## 5. Communication Topology and the Structural Tree

The port rights graph is structurally independent of the workspace tree — a send right can exist between any two workspaces, regardless of their parent-child relationship. But in practice, the two are correlated:

### 5.1 Initial Correlation

At creation, the permission matrix grants send rights based on roles, which are assigned at workspace creation. The coordinator creates worker workspaces as its children in the tree. The initial port rights graph mirrors the tree's parent-child relationships: coordinator → worker and worker → coordinator.

### 5.2 Divergence

The correlation breaks as the system evolves:

- **Right transfer** creates edges between structurally unrelated workspaces.
- **Delegation** creates local hubs that mirror subtree structure.
- **Right revocation** removes edges that initially mirrored the tree.
- **Terminal workspaces** remove edges from the graph while the tree retains them (dead nodes).

The port rights graph is a live, mutable communication topology. The workspace tree is a stable, monotonically growing containment structure. They start aligned and progressively diverge.

### 5.3 Signal vs. Envelope Paths

| Dimension | Signals | Envelopes |
|-----------|---------|-----------|
| Topology | The workspace tree (upward only) | The port rights graph (any direction between right-holding workspaces) |
| Structure | Tree (fixed) | Directed graph (mutable) |
| Ordering | Emission order within a workspace | Creation order within a channel |
| Addressing | Implicit (propagate to parent) | Explicit (addressed to a specific workspace) |
| Content | State only (11 types) | Arbitrary payload |
| Gating | None — signals always propagate | Port rights — no right, no delivery |

The two communication paths serve different purposes. Signals report state changes through the hierarchy. Envelopes carry content between specific workspaces. The structural tree governs signals; the port rights graph governs envelopes.

## 6. Invariants

| # | Invariant | Enforced by |
|---|-----------|-------------|
| CH-1 | **No right, no delivery.** An envelope cannot be delivered unless the sender holds a valid send right (or send-once right) to the receiver at time of submission. Exception: human-injected envelopes bypass role-based send restrictions (PROTOCOL §8.1, Human-Highway spec §2.3); all other validation (structure, target existence, type registration) still applies. | Runtime, at validation |
| CH-2 | **Channel ordering.** Within a channel `(A, B)`, envelopes are delivered in creation order. | Runtime, at delivery |
| CH-3 | **Send-once consumed.** A send-once right is destroyed after one successful use. | Runtime, at delivery |
| CH-4 | **Revocation is immediate.** When the coordinator revokes a right, no new envelopes may be sent on that right. In-flight envelopes are delivered. | Runtime, at revocation |
| CH-5 | **Receive rights are non-transferable.** A workspace's receive right to its own inbox cannot be granted to another workspace. | Runtime, at transfer |
| CH-6 | **Right creation, transfer, revocation, and consumption are recorded in the trail.** | Runtime |

## 7. Trail Events

Communication topology changes produce events defined in other specs:

| Operation | Trail event | Defined in |
|-----------|------------|------------|
| Right creation (at workspace creation) | `port_right_created` | Envelope spec §10 |
| Right transfer (via envelope) | `port_right_transferred` | Envelope spec §10 |
| Right revocation | `port_right_revoked` | Envelope spec §10 |
| Right consumption (send-once) | `port_right_consumed` | Envelope spec §10 |
| Envelope creation | `envelope_created` | Envelope spec §10 |
| Envelope delivery | `envelope_delivered` | Envelope spec §10 |
| Envelope rejection | `envelope_rejected` | Envelope spec §10 |

The port rights graph's full history is reconstructable from these trail events.

## 8. Conformance Requirements

| # | Requirement | Level |
|---|-------------|-------|
| CH-C1 | Send right validation MUST precede envelope delivery. Envelopes without valid rights MUST be rejected. | Core |
| CH-C2 | Channel ordering MUST be enforced: envelopes within `(from, to)` delivered in creation order. | Core |
| CH-C3 | Send-once rights MUST be consumed on use. No second envelope may be sent on a consumed right. | Core |
| CH-C4 | Right revocation MUST take immediate effect on new envelope submissions. In-flight envelopes MUST still be delivered. | Standard |
| CH-C5 | Right transfer via envelope MUST be processed atomically on delivery: the sender loses the right, the receiver gains it. | Standard |
| CH-C6 | All port right lifecycle events (creation, transfer, revocation, consumption) MUST be recorded in the trail. | Standard |
| CH-C7 | The runtime MUST support communication topology queries — enumerating active send rights for a workspace (outbound and inbound). | Standard |

**7 requirements.** Core ensures the fundamental port rights invariant (no right, no delivery) and channel ordering. Standard ensures dynamic graph operations (transfer, revocation) and trail recording.

## 9. Implementation Notes

*This section is non-normative.*

**Port rights table.** The port rights graph is naturally a table: `(holder_workspace, target_workspace, right_type, right_id, status)`. Outbound queries filter on `holder_workspace`. Inbound queries filter on `target_workspace`. The table is small — most workspaces have 1-2 outbound rights and 1 inbound right.

**Channel as delivery queue.** Each channel `(A, B)` is a FIFO queue. Envelopes from A to B are enqueued in creation order. The delivery engine dequeues and delivers. No sorting needed — insertion order is delivery order. Concurrent senders to the same target use separate queues (one per channel).

**Thread reconstruction.** To reconstruct a conversation thread, start from any envelope and follow `in_reply_to` upward to the root. To find all replies to an envelope, query `in_reply_to == envelope_id`. A reverse index `Map[envelope_id → Set[reply_envelope_id]]` enables efficient thread traversal in both directions.

**Graph snapshot.** To visualize the current communication topology, query all active send rights and render as a directed graph. Color by right type (send = solid, send_once = dashed). Group by workspace tree structure to show how the communication graph relates to the containment hierarchy.

**Dead edge cleanup.** When a workspace reaches a terminal state, its outbound rights become useless and inbound rights become dead (deliveries rejected). Implementations may lazily clean these edges — mark as inactive rather than deleting. The trail preserves the full history regardless.

## 10. References

| Spec | Sections referenced | Relationship |
|------|-------------------|--------------|
| [Envelope](../primitives/envelope.md) | §3 (schema, port rights transfer, in_reply_to), §4 (flow rules, channel ordering, send right validation), §5 (lifecycle), §10 (trail events) | Envelopes are the content that flows through the communication topology |
| [Signal](../primitives/signal.md) | §3 (propagation rules) | Signals use a different topology (the structural tree); this spec covers envelope topology |
| [Workspace Tree](tree.md) | §2 (structural tree), §4.1 (traversal) | The structural tree determines signal topology; the port rights graph is independent but initially correlated |
| [Workspace](../primitives/workspace.md) | §4 (creation, initial rights), §6 (workspace components: inbox) | Workspaces are the nodes of the communication graph |
| [Roles](../foundations/roles.md) | §3 (permission matrix) | The permission matrix determines the initial port rights graph |
| [PROTOCOL.md](../PROTOCOL.md) | §4.2 (envelope, delivery), §5.5 (permission matrix), §5.6 (port rights) | Canonical communication model definition |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
