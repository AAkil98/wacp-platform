# WACP: Clock

## Metadata

```yaml
title: "WACP: Clock"
id: wacp-spec-clock
type: constituent-spec
tier: abstract
category: foundations
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §3.8 (ordering requires a clock)
depends_on: []
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, clock, time, timestamps, durations, ordering]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Two Functions of Time](#2-two-functions-of-time)
3. [Clock Invariants](#3-clock-invariants)
4. [Causal Ordering](#4-causal-ordering)
5. [Clock Consumers](#5-clock-consumers)
6. [What the Protocol Does Not Prescribe](#6-what-the-protocol-does-not-prescribe)
7. [Clock Recovery](#7-clock-recovery)
8. [Conformance Requirements](#8-conformance-requirements)
9. [Implementation Notes](#9-implementation-notes)
10. [References](#10-references)

---

## 1. Purpose

The clock is the protocol's time substrate. Every primitive — workspace, envelope, signal, checkpoint, trail entry — carries a timestamp assigned by the clock. Every duration — timeouts, liveness intervals, compute budgets, redelivery backoff — is measured against it. Every ordering guarantee the protocol makes — total order within a workspace, partial order across workspaces, causal ordering through message delivery — depends on the clock being correct.

Without a well-defined clock, the trail has no order, timeouts have no meaning, recovery has no anchor, and replay is non-deterministic.

This spec defines what the clock must provide, what it must guarantee, and what it leaves to the implementation.

## 2. Two Functions of Time

The protocol uses time in two fundamentally different ways, and the clock must serve both.

**Timestamps** answer the question *when did this happen?* Every trail entry, envelope, signal, checkpoint, and gate event carries a timestamp. Timestamps are the protocol's ordering mechanism — they establish which event came first within a workspace, and they anchor the causal relationships between events across workspaces.

Timestamps are:
- Assigned by the runtime, never by agents. An agent cannot self-timestamp.
- Immutable once assigned. A timestamp is part of the event's identity.
- Comparable. Given two timestamps from the same workspace, the protocol can always determine which is earlier.

**Durations** answer the question *how long?* Timeouts (PROTOCOL §6.6), liveness intervals (PROTOCOL §6.6), compute budgets (PROTOCOL §6.6), and redelivery backoff (PROTOCOL §4.2) are all durations. Durations are the protocol's enforcement mechanism — they define how long a workspace may live, how long silence is tolerated, how much compute time is permitted.

Durations are:
- Declared at configuration time (workspace creation, workflow definition).
- Measured by the runtime against the same clock source that produces timestamps. A timeout set in wall-clock seconds must be measured by a wall clock, not a logical clock.
- Expressed in a consistent unit within a deployment. The unit is implementation-defined, but mixing units within a single deployment is non-conformant.

The distinction matters because timestamps and durations have different failure modes. A timestamp that is wrong corrupts ordering. A duration that is wrong corrupts enforcement. Both are critical, but they break different things.

## 3. Clock Invariants

Four invariants govern the clock. A conformant implementation must satisfy all four. Violating any one invalidates the protocol's ordering, enforcement, and recovery guarantees.

**Invariant 1: Monotonic within a workspace.** Timestamps assigned to events within a single workspace must strictly increase. No two events in the same workspace may share a timestamp. This guarantees a total order within every workspace — given any two events in the same workspace, the protocol can always determine which happened first.

**Invariant 2: Partial order across workspaces.** The protocol does not require synchronized clocks across workspaces. Two events in different workspaces with the same timestamp value are concurrent — neither happened before the other. Cross-workspace ordering is established only through causal links (§4), not through timestamp comparison. The trail records both events; analysis must tolerate concurrency.

**Invariant 3: Sufficient resolution.** The clock must distinguish events that occur in rapid succession within a workspace. If two events are produced faster than the clock's resolution, the implementation must artificially separate their timestamps to preserve invariant 1. The protocol does not prescribe a specific resolution — nanoseconds, microseconds, or milliseconds are all valid, provided no collisions occur within a workspace.

**Invariant 4: Durable across restarts.** The clock's state must survive runtime restarts within a workspace's lifetime. If the runtime restarts while a workspace is active, the next timestamp must be greater than the last recorded timestamp. This prevents trail entries from appearing to travel backward in time after recovery. The recovery spec depends on this invariant to reconstruct timers and re-anchor the clock.

## 4. Causal Ordering

Two events in different workspaces have no guaranteed ordering unless a causal link exists between them. The clock does not provide cross-workspace ordering — causality does.

A causal link is established when one protocol operation directly produces another:

| Cause (workspace A) | Effect (workspace B) | Ordering guarantee |
|---------------------|---------------------|-------------------|
| Envelope created | Envelope delivered | Creation happened before delivery |
| Signal emitted | Signal delivered to parent | Emission happened before delivery |
| Checkpoint created | Integration event in parent | Checkpoint happened before integration |
| Gate triggered | Gate resolved by human | Trigger happened before resolution |

These links form a directed acyclic graph — the protocol's happened-before relation. Given events *a* and *b*:

- If *a* causally precedes *b*, then *a* happened before *b*. This holds regardless of timestamp values.
- If *b* causally precedes *a*, then *b* happened before *a*.
- If neither causally precedes the other, they are **concurrent**. The protocol makes no claim about which happened first, even if their timestamps suggest an order.

The global trail records all events. Cross-workspace queries MUST NOT assume total ordering. Tools that display events across workspaces SHOULD make concurrency visible rather than imposing an arbitrary order.

Implementations using wall clocks may observe a total order in practice. The protocol does not depend on it. Implementations using logical clocks produce the partial order directly. Both are conformant.

## 5. Clock Consumers

Every protocol mechanism that uses the clock falls into one of two categories: it reads a timestamp or it measures a duration. This section enumerates all consumers so that an implementer knows exactly what the clock must serve.

**Timestamp consumers:**

| Consumer | What is timestamped | Section |
|----------|-------------------|---------|
| Trail entries | Every event in the system | PROTOCOL §9.1 |
| Envelopes | Creation time | PROTOCOL §4.2 |
| Signals | Emission time, delivery time | PROTOCOL §4.3 |
| Checkpoints | Creation time | PROTOCOL §7.1 |
| Gate events | Trigger time, resolution time, timeout time | PROTOCOL §8.4 |
| Escalation events | Receipt time, resolution time, timeout time | PROTOCOL §8.1 |

**Duration consumers:**

| Consumer | What is measured | Section |
|----------|-----------------|---------|
| Workspace timeout | Time in active/blocked/conflicted before forced failure | PROTOCOL §6.6 |
| Compute budget | Wall time consumed by agent execution | PROTOCOL §6.6 |
| Liveness interval | Silence duration since last trail activity | PROTOCOL §6.6 |
| Redelivery backoff | Delay between retry attempts | PROTOCOL §4.2 |
| Gate timeout | Time between gate trigger and fallback execution | PROTOCOL §8.4 |
| Escalation timeout | Time between escalation receipt and fallback execution | PROTOCOL §8.1 |

An implementation that satisfies the four invariants (§3) and serves all consumers listed above is conformant for the Clock spec.

## 6. What the Protocol Does Not Prescribe

The clock spec is deliberately silent on the following. Each is an implementation choice, not a protocol requirement.

**Clock source.** Wall clock, logical clock (Lamport), or hybrid logical clock are all valid. The protocol's invariants are compatible with all three. A wall clock provides human-readable timestamps and natural duration measurement but requires monotonicity enforcement after NTP adjustments. A logical clock provides causal ordering natively but cannot measure wall-time durations — implementations using logical clocks must maintain a separate wall-clock source for duration consumers. A hybrid logical clock combines both properties.

**Timestamp format.** ISO 8601, Unix epoch (integer or float), ULID-embedded timestamps, or any representation that supports strict comparison is valid. The protocol requires comparability and monotonicity, not a specific encoding.

**Resolution.** Nanoseconds, microseconds, milliseconds — any resolution that prevents collisions within a workspace (invariant 3) is valid. Implementations SHOULD document their resolution so that operators can reason about high-throughput scenarios.

**Synchronization.** The protocol does not require clock synchronization across workspaces. The partial-order model (invariant 2) explicitly avoids this dependency, keeping the protocol transport-agnostic. Implementations that do synchronize clocks (e.g., via NTP) gain tighter cross-workspace timestamp comparison but MUST NOT depend on it for correctness — causal ordering (§4) is the only cross-workspace ordering guarantee.

## 7. Clock Recovery

When the runtime crashes and restarts, the clock must be re-established before any protocol operations resume. The recovery spec defines the full procedure; this section specifies the clock's role within it.

**Step 1: Read the last recorded timestamp.** The trail is the clock's recovery anchor. The runtime reads the most recent trail entry's timestamp. This is the floor — no new timestamp may be less than or equal to this value.

**Step 2: Re-anchor the clock.** The runtime initializes its clock source with a value strictly greater than the recovered floor. For wall clocks, this is naturally satisfied if the restart takes any measurable time. For logical clocks, the counter must be set to at least `floor + 1`. For hybrid clocks, both components must satisfy their respective constraints.

**Step 3: Reconstruct active timers.** Workspace timeouts, liveness intervals, and budget compute timers are reconstructed from the trail. For each non-terminal workspace:
- **Timeout:** remaining = original timeout − (recovered timestamp − workspace creation timestamp). If remaining ≤ 0, the workspace has expired and transitions to `failed`.
- **Liveness:** remaining = liveness interval − (recovered timestamp − last trail entry timestamp for this workspace). If remaining ≤ 0, a liveness warning is emitted immediately.
- **Compute budget:** consumed wall time is reconstructed from checkpoint `resource_usage` entries. The timer resumes from the accumulated total.

**Invariant:** after recovery, the clock satisfies all four invariants as if no crash had occurred. The trail's ordering is continuous across the crash boundary — no gap, no backward jump, no ambiguity.

## 8. Conformance Requirements

| Requirement | Level | Description |
|-------------|-------|-------------|
| Four clock invariants | Core | All four invariants (§3) MUST be satisfied. Non-negotiable at any level. |
| Timestamp assignment | Core | Runtime MUST assign all timestamps. Agents MUST NOT self-timestamp. |
| Duration measurement | Core | Runtime MUST measure durations against the same clock source that produces timestamps. |
| Duration unit consistency | Core | All durations within a deployment MUST use the same unit. |
| All timestamp consumers served | Standard | All six timestamp consumers (§5) MUST receive runtime-assigned timestamps. |
| All duration consumers served | Standard | All six duration consumers (§5) MUST have their durations enforced by the runtime. |
| Clock recovery | Standard | Runtime MUST implement the three-step recovery procedure (§7). |
| Timer reconstruction | Standard | All active timers MUST be reconstructed after recovery with no gap in enforcement. |
| Resolution documentation | Full | Implementations SHOULD document their clock resolution. |

## 9. Implementation Notes

These notes are non-normative. They capture practical guidance for implementers.

**Wall clock with monotonicity guard.** The simplest conformant implementation is a wall clock wrapped in a monotonicity guard: `next = max(now(), last + 1)`. This satisfies invariant 1 (monotonic), handles NTP adjustments and leap seconds gracefully, provides natural duration measurement, and produces human-readable timestamps. Most single-runtime deployments will use this approach.

**Logical clock limitations.** A pure Lamport clock satisfies invariants 1, 2, and 4 naturally, and produces the causal partial order directly. However, it cannot measure wall-time durations — `counter 47 − counter 12` is not "35 seconds." Implementations using logical clocks MUST maintain a separate wall-clock source for duration consumers (timeouts, liveness, compute budgets, redelivery backoff). This adds complexity; the benefit is that causal ordering is built into the timestamp rather than inferred from protocol operations.

**Hybrid logical clock.** An HLC combines a wall-clock component with a logical counter. It provides human-readable timestamps, causal ordering, and monotonicity in a single value. This is the recommended approach for deployments that span multiple runtimes or require both human-readable timestamps and strong causal ordering. The tradeoff is implementation complexity.

**High-throughput scenarios.** If an implementation produces events faster than its clock resolution (e.g., multiple signals within the same millisecond), the monotonicity guard in the wall-clock approach handles this automatically — it increments past the collision. Implementations should monitor the gap between real time and assigned timestamps; a persistent divergence indicates the clock resolution is insufficient for the workload.

## 10. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §3.8 | §1, §3 | Design principle: ordering requires a clock |
| §4.2 | §2, §5 | Envelope creation timestamp, redelivery backoff duration |
| §4.3 | §5 | Signal emission/delivery timestamps |
| §6.6 | §2, §5 | Workspace timeout, compute budget, liveness interval |
| §7.1 | §5 | Checkpoint creation timestamp |
| §8.1 | §5 | Escalation receipt/resolution/timeout timestamps |
| §8.4 | §5 | Gate trigger/resolution/timeout timestamps |
| §9.1 | §5 | Trail entry timestamps |

### Constituent Specs

| Spec | Referenced in | Topic |
|------|--------------|-------|
| Recovery spec | §3, §7 | Recovery procedure — clock re-anchoring, timer reconstruction |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*

