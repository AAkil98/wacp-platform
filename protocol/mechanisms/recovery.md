# WACP: Recovery

## Metadata

```yaml
title: "WACP: Recovery"
id: wacp-spec-recovery
type: constituent-spec
tier: abstract
category: mechanisms
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §10
depends_on:
  - wacp-spec-trail
  - wacp-spec-workspace
  - wacp-spec-signal
  - wacp-spec-envelope
  - wacp-spec-checkpoint
  - wacp-spec-clock
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, recovery, failure, degraded-mode, restart, trail-replay, resilience]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Failure Response](#2-failure-response)
3. [The Recovery Procedure](#3-the-recovery-procedure)
4. [Trail Events](#4-trail-events)
5. [Conformance Requirements](#5-conformance-requirements)
6. [Implementation Notes](#6-implementation-notes)

---

## 1. Purpose

Recovery is the protocol's restart contract. When the runtime crashes, the system's in-memory state — workspace lifecycle positions, timer countdowns, delivery queues, budget meters — is lost. What survives is the trail: the append-only, write-ahead record of everything that happened. Recovery reads the trail and reconstructs the system to the last known good state.

The protocol defines five invariants that govern recovery (PROTOCOL §10.4) and the recovery model (PROTOCOL §10.2): the trail is the single source of truth; if an event is recorded, it happened; if not, it did not. What the protocol does not define is the procedure — the specific steps by which the runtime reconstructs system state from the trail after a crash. This spec defines that procedure.

Three concerns:

- **Failure response** — how the runtime and coordinator respond to partial failures (trail write failures, transport failures) and correlated systemic failures (multiple agents failing from the same external cause). These responses prevent cascading damage while the system is still running.
- **The recovery procedure** — a protocol-required, five-step sequence that every conformant runtime must execute after a crash: trail integrity check, workspace state reconstruction, in-flight message recovery, timer reconstruction, clock recovery.
- **Recovery observability** — trail events that mark degraded periods and recovery completions, giving the coordinator and human operators visibility into infrastructure health.

## 2. Failure Response

The protocol classifies failures into three classes (PROTOCOL §10.1): agent-local, systemic, and runtime infrastructure. Agent-local failures are the agent's responsibility. Systemic failures require coordinator judgment. Runtime infrastructure failures require the recovery procedure (§3). This section defines the response mechanics for the first two classes and the partial failure modes that precede a full crash.

### 2.1 Agent-Local External Failures

An agent's backing model returns an error. A tool times out. An external API is unreachable. These are failures outside the protocol boundary but inside the agent's operational scope.

**Protocol contract.** The agent must translate external failures into protocol signals. The protocol does not see external dependencies — it sees workspace state. Three responses, ordered by reversibility:

1. **If recoverable:** emit `blocked` with a reason describing the failure (e.g., `reason: external_api_unavailable: {service, error}`). The coordinator receives the signal and decides: wait, send feedback, or abort. The `blocked` signal is reversible — the coordinator can unblock the workspace by sending feedback.

2. **If unrecoverable:** emit `failed` with a reason describing the failure. The workspace transitions to terminal state. The coordinator reads the trail and decides whether to retry in a new workspace.

3. **If uncertain:** emit `blocked` rather than `failed`. The coordinator can always abort a blocked workspace, but a failed workspace cannot be un-failed. When in doubt, the agent SHOULD prefer the reversible signal.

The protocol does not prescribe how agents interact with external services — retry policies, circuit breakers, and fallback strategies are agent-level concerns. The protocol requires only that the agent surfaces the failure through the signal mechanism rather than silently stalling. Liveness monitoring (Workspace spec, §6) serves as the backstop: an agent that stalls without signaling triggers a liveness warning, giving the coordinator an opportunity to intervene.

**Trail recording.** Agent-local external failures appear in the trail through existing mechanisms: `signal_emitted` entries with type `blocked` or `failed` and a descriptive reason. No new trail event types are needed — the `reason` field carries the external failure context.

### 2.2 Correlated Failure Detection

When multiple workspaces fail for the same external cause, the coordinator sees N independent failures. Without correlated failure awareness, the coordinator may respond to each failure individually — retrying all N workspaces that will also fail, burning budget and generating noise in the trail.

**Detection patterns.** The coordinator detects systemic failures by observing patterns in the trail:

1. **Temporal clustering.** Multiple `failed` or `blocked` signals arriving within a short time window from workspaces that share no directive dependency. The protocol does not define the window — the coordinator applies judgment based on the system's normal failure rate.

2. **Reason similarity.** Multiple failures with the same or similar `reason` values. An agent that fails with `reason: external_api_unavailable: openai` at the same time as three other agents with the same reason strongly suggests a systemic cause.

3. **Group correlation.** If all failures are within the same workspace group (Workspace spec, §9), batch operations may be the appropriate response.

The protocol does not automate systemic failure detection — it provides the trail data that makes detection possible. The coordinator is responsible for recognizing patterns and responding appropriately.

### 2.3 Coordinator Response to Systemic Failures

When the coordinator determines that failures are systemic rather than independent, it executes the following response pattern:

1. **Pause dispatch.** Stop creating new workspaces until the external dependency recovers. The coordinator can temporarily set effective concurrency to zero.

2. **Avoid redundant retries.** Do not retry failed workspaces if the cause is external and ongoing. Record a `system_degraded` trail entry (§4) documenting the coordinator's assessment.

3. **Batch abort if appropriate.** If an entire workspace group is affected, use batch abort (Workspace spec, §9) rather than waiting for each workspace to fail individually.

4. **Escalate to the human highway.** Systemic failures often exceed the coordinator's authority to resolve. An escalation or gate activation gives the human visibility into the situation and the power to decide: wait for recovery, abort the run, or adjust the workflow.

5. **Resume when recovered.** When the external dependency recovers (detected by successful workspace creation or agent signals), the coordinator resumes dispatch. Previously failed directives may be retried in new workspaces with context from the failed attempts.

**What the protocol does NOT do.** The protocol does not define health checks for external dependencies, does not prescribe circuit breaker patterns, and does not require the runtime to probe external services. These are implementation concerns. The protocol defines the coordinator's decision-making framework — pause, avoid waste, escalate, resume — using existing primitives.

### 2.4 Trail Write Failures

The trail is the protocol's most critical dependency. If trail writes fail, the no-gaps invariant (Trail spec, §4) is at risk.

**Requirement.** When a trail write fails, the runtime MUST NOT proceed with the operation the entry was meant to record. An envelope that cannot be logged to the trail MUST NOT be delivered. A state transition that cannot be recorded MUST NOT take effect. The trail write is a precondition, not a side effect.

**Retry.** The runtime SHOULD retry trail writes with bounded attempts (implementation-defined). If retries succeed, the operation proceeds normally.

**Degraded mode.** If trail writes persistently fail, the runtime enters degraded mode:

1. **No new operations are initiated.** No new envelopes delivered, no state transitions processed, no signals delivered. The write-ahead invariant (PROTOCOL §10.4, invariant 2) cannot be satisfied, so no operations may take effect.

2. **Active agents may continue.** Agents already processing work are unaware of the trail failure and may continue producing output. But that output cannot be recorded — checkpoints, signals, and envelopes emitted during degraded mode cannot take effect until trail writes recover.

3. **No data loss.** Blocking preserves correctness. The system stalls rather than corrupts. No operation proceeds without its trail entry.

4. **Recovery marker.** When trail writes recover, the runtime records a `system_degraded` trail entry (§4) marking the boundary between reliable and potentially unreliable trail data. This entry tells the coordinator and human operators exactly when the degraded period occurred.

The protocol does not require the runtime to halt immediately on trail write failure — a transient storage hiccup should not crash the system. But it requires that the runtime never silently drops trail entries. Operations without trail entries are operations that "never happened" — the runtime MUST NOT create a state where things happened but the trail doesn't know about them.

### 2.5 Transport Failure Cascade

Transport failures escalate through three levels:

1. **Transient failure.** The redelivery mechanism handles transient transport failures (Envelope spec, §7). Bounded retries with backoff. If the transport recovers within the retry window, delivery succeeds transparently.

2. **Persistent failure.** If all redelivery attempts exhaust, the runtime records `envelope_undeliverable` and notifies the sender (Envelope spec, §7.1). The coordinator decides how to respond — abort the target workspace, retry with a new workspace, or escalate.

3. **Total transport failure.** If the transport is completely unavailable (no messages can be delivered anywhere), the system is effectively frozen. Liveness warnings fire for all active workspaces as they stop producing trail entries. Timeouts eventually fail workspaces that receive no input. The coordinator, observing cascading liveness warnings and timeouts, should recognize the systemic failure pattern (§2.2) and escalate.

The protocol does not define transport-level recovery (reconnection, failover, message queue replay) — these are implementation concerns. The protocol defines the behavioral consequences: redelivery for transient failures, coordinator notification for persistent failures, and systemic failure recognition for total failures.

## 3. The Recovery Procedure

When the protocol runtime restarts after a crash, it executes the following recovery procedure. This procedure is protocol-defined — every conformant implementation MUST follow it. The procedure is idempotent (PROTOCOL §10.4, invariant 3): a crash during recovery does not corrupt state, and the next recovery attempt reads the same trail and reaches the same conclusions.

The procedure has five steps, executed in order. Each step depends on the previous step's completion.

### 3.1 Trail Integrity Check

The runtime verifies the trail's integrity before reconstructing state from it.

**Three checks:**

1. **Completeness.** The trail storage is accessible and readable. All tiers (hot, warm, cold) that contain entries for the current run are reachable. If any tier is unavailable, recovery cannot proceed for the workspaces whose entries are in that tier — the runtime reports the inaccessible tier and the affected workspaces.

2. **Monotonicity.** Timestamps within each workspace's local trail are strictly increasing (Clock spec, invariant 1). The runtime scans each workspace's entries and verifies that `entry[N].timestamp < entry[N+1].timestamp` for all consecutive entries. Any entry that violates monotonicity indicates a corrupted write during the crash. The runtime flags the violating entry and its workspace.

3. **Terminal consistency.** No trail entries exist after a workspace's terminal state transition. If entries appear after a `workspace_state_changed` to `closed` or `failed`, they indicate partial writes during the crash — operations that were initiated but not completed before the crash, after the terminal transition was already the last valid state. The runtime flags these entries.

**Corruption handling.** If the trail is corrupted beyond recovery — storage is destroyed, entries are unreadable, the hash chain is broken with no way to determine the valid prefix — the runtime cannot proceed. This is a catastrophic failure. The runtime MUST halt and report the corruption. The protocol does not define automatic repair of corrupted trails; this requires human intervention.

**Non-critical corruption.** If the corruption is limited — a few entries with violated monotonicity, or post-terminal entries — the runtime quarantines the flagged entries and proceeds with recovery using the valid portion of the trail. Quarantined entries are logged in the `recovery_completed` trail event (§4) for human review.

### 3.2 Workspace State Reconstruction

For each workspace referenced in the trail, the runtime reconstructs its current lifecycle state and internal model.

**Lifecycle state:**

1. Read all `workspace_state_changed` entries for the workspace, ordered by timestamp.
2. The most recent `to_state` value is the workspace's current state.
3. If no `workspace_state_changed` entry exists but a `workspace_created` entry does, the workspace is in `idle` state.

**In-flight transitions.** If a workspace was mid-transition when the crash occurred — for example, the agent emitted `complete` but the `workspace_state_changed` entry to `integrating` was not yet written — the workspace remains in its last recorded state. The agent's `complete` signal, if recorded in the trail as `signal_emitted`, is re-evaluated during in-flight message recovery (§3.3). The runtime processes it as if it just arrived, triggering the state transition that was interrupted. Similarly, a `failed` signal without a corresponding terminal transition, or a `blocked` signal without a corresponding `active` → `blocked` transition, triggers that transition during recovery.

**Workspace components.** Beyond lifecycle state, each workspace's internal model (Workspace spec, §2) is reconstructed from the trail:

| Component | Reconstruction source |
|-----------|----------------------|
| Lifecycle state | Most recent `workspace_state_changed` entry |
| Budget consumed | Cumulative `resource_usage` from `checkpoint_created` entries + `budget_warning`/`budget_exceeded` entries |
| Visibility set | `workspace_created` entry's `visibility_set` field + all `visibility_granted` entries for this workspace |
| Priority | Most recent `priority_changed` entry, or creation priority if none |
| Checkpoint chain | All `checkpoint_created` entries, ordered by timestamp |
| Inbox state | Derived from envelope trail entries (§3.3) |
| Local trail | The workspace's trail entries themselves |
| Group membership | `workspace_created` entry's `group_id` field, if present |
| Owner | `workspace_created` entry's `owner` field, or most recent `workspace_ownership_transferred` entry |
| Port rights | All `port_right_created`, `port_right_transferred`, `port_right_revoked`, and `port_right_consumed` entries (Envelope spec §10). The active rights set is: created rights minus revoked minus consumed, with holder adjusted by transfers. |

### 3.3 In-Flight Message Recovery

Messages that were in flight when the crash occurred — validated but not delivered, emitted but not propagated — must be recovered. The recovery procedure examines each message's trail lifecycle and takes the appropriate action.

**Envelope recovery.** For each envelope referenced in the trail, the runtime examines its lifecycle state:

| Trail state | Recovery action |
|-------------|-----------------|
| `envelope_created` only | Never delivered. Re-evaluate target workspace state: deliver to inbox if accepting, place in channel queue if deferring, record as `envelope_undeliverable` if rejecting (Envelope spec, §4, rule 2). |
| `envelope_created` + `envelope_delivered`, no `acknowledged` | Delivered but not acknowledged. Resume acknowledgment timeout tracking — normal redelivery (Envelope spec, §7.1) takes over if the ack does not arrive. |
| `envelope_created` + `envelope_delivered` + `acknowledged` | Fully processed. No action. |
| `envelope_created` + `envelope_undeliverable` | Permanently rejected or transport exhausted. No action. |

For envelopes in the first case (`created` only), the recovery procedure performs a fresh delivery attempt. If the envelope was partially delivered — it reached the inbox but the `envelope_delivered` trail entry was not written — the runtime checks the inbox before placement to prevent duplication. This inbox check is a recovery-specific operation; during normal operation, the write-ahead invariant ensures the trail entry precedes inbox placement, making this check unnecessary.

**Signal recovery.** For each signal with a `signal_emitted` entry but no corresponding `signal_delivered` entry, the signal is placed back in the parent workspace's signal queue (Signal spec, §3). Normal signal delivery resumes from there.

Signals that represent state-changing events (`complete`, `failed`, `blocked`) are additionally processed for their workspace state effects — if the signal was emitted and recorded but its corresponding workspace state transition was not, the recovery procedure triggers that transition (§3.2, in-flight transitions).

**Ordering during recovery.** Recovery redelivery respects the same ordering invariants as normal delivery:

- Multiple envelopes in the same channel are redelivered in creation-timestamp order.
- Redelivery for a channel completes before new envelopes in that channel are accepted — recovery deliveries and new deliveries are not interleaved within a channel (Envelope spec, §7.2).
- Signals are re-queued in emission order within each workspace's signal queue (Signal spec, §4).

**Idempotent recovery.** The recovery procedure produces the same result regardless of how many times it is executed. A crash during recovery does not corrupt state. This is possible because recovery operates on trail data (immutable, append-only) and uses deduplication to prevent double delivery. A redelivered envelope is caught by the per-workspace deduplication set (Envelope spec, §7.3). A reprocessed state-changing signal is caught by the workspace state machine (a transition that has already occurred cannot occur again). Every recovery operation is idempotent against the trail's immutable record.

### 3.4 Timer Reconstruction

Timeouts (Workspace spec, §6), liveness intervals (Workspace spec, §6), and budget enforcement (Workspace spec, §5) depend on timers. After a crash, all in-memory timers are lost. The runtime reconstructs them from the trail.

**Workspace timeout.** For each non-terminal workspace, calculate elapsed time from the `workspace_created` timestamp to the current time. If the timeout has already expired, transition the workspace to `failed` with `reason: timeout`. If not, schedule the timeout for the remaining duration.

**Liveness interval.** For each workspace with a liveness interval, find the most recent trail entry attributed to the workspace's agent (any `signal_emitted`, `checkpoint_created`, or `envelope_created` entry with the agent as actor). If the interval has already elapsed since that entry, record a `liveness_warning` trail entry. If not, schedule the next liveness check for the remaining duration.

**Budget enforcement.** Budget tracking is reconstructed from `checkpoint_created` entries (which carry `resource_usage`) and from `budget_warning`/`budget_exceeded` trail entries. The runtime resumes tracking from the last known consumption figures. The runtime's independent tracking may have been more current than the trail — this data is lost. The trail provides a lower bound on consumption; the runtime MUST re-establish authoritative tracking from this baseline.

**Compute timeout.** For workspaces where `compute_timeout` is configured, the runtime reconstructs accumulated active time from `workspace_state_changed` entries — summing the intervals during which the workspace was in `active` state. Time spent in `blocked`, `suspended`, or `migrating` states does not count toward compute timeout.

### 3.5 Clock Recovery

The protocol's clock must survive restarts (Clock spec, invariant 4). After recovery, the next timestamp assigned MUST be strictly greater than the last recorded timestamp in the trail.

**Procedure:**

1. Read the maximum timestamp from the global trail — the most recent entry across all workspaces and system-level entries.
2. Set the clock's initial value to at least `max_timestamp + 1` (or a value guaranteed to be strictly greater, depending on clock resolution).
3. Verify that the clock source is monotonic from this initial value forward.

This prevents trail entries from appearing to travel backward in time after recovery — a post-recovery entry with a timestamp earlier than a pre-crash entry would violate the trail's ordering guarantees and break the hash chain's integrity.

### 3.6 Recovery Completion

After all five steps complete, the runtime records a `recovery_completed` trail entry (§4). This entry serves as a marker in the trail: everything before it is pre-crash state; everything after it is post-recovery operation. Observability views (Trail spec, §6) should flag this boundary.

The runtime then transitions to normal operation:

- The scheduler resumes dispatching turns to active workspaces.
- Reconstructed timers begin counting.
- Signal and envelope delivery resumes.
- The coordinator is notified that recovery has completed, with the summary statistics recorded in the `recovery_completed` entry.

## 4. Trail Events

Two trail event types support failure and recovery observability.

**`system_degraded`** — recorded when the runtime detects a systemic or infrastructure failure.

```yaml
system_degraded:
  reason: string              # what caused the degradation
  scope: enum                 # systemic | trail_storage | transport | clock
  affected_workspaces: list   # workspace IDs affected (if known; empty for systemic scope)
  coordinator_action: string  # pause_dispatch | batch_abort | escalate | wait
```

The `scope` field classifies the degradation. `systemic` indicates a correlated external failure affecting multiple workspaces (§2.2). `trail_storage` indicates trail write failures (§2.4). `transport` indicates total transport failure (§2.5). `clock` indicates the clock source is unreliable. The `coordinator_action` field records what the coordinator decided to do in response — it is informational, recorded after the decision, not a trigger.

**`recovery_completed`** — recorded as the final step of the recovery procedure (§3.6).

```yaml
recovery_completed:
  downtime: duration                # elapsed time between crash and recovery completion
  workspaces_recovered: integer     # workspaces successfully reconstructed
  workspaces_failed: integer        # workspaces that could not be recovered (transitioned to failed)
  envelopes_redelivered: integer    # in-flight envelopes requeued for delivery
  signals_requeued: integer         # in-flight signals placed back in signal queues
  timers_reconstructed: integer     # timeouts, liveness intervals, and budget timers restored
  trail_entries_examined: integer   # total trail entries read during recovery
  quarantined_entries: integer      # entries flagged during trail integrity check (§3.1)
```

The `recovery_completed` entry is the boundary marker. Everything before it in the trail is pre-crash state; everything after it is post-recovery operation. If `quarantined_entries > 0`, the human operator should review the flagged entries to determine whether any data was lost during the crash.

## 5. Conformance Requirements

| Requirement | Level | Description |
|-------------|-------|-------------|
| Write-ahead enforcement | Core | The runtime MUST NOT proceed with any protocol operation until its trail entry is durably written. Trail write failure MUST block the operation. |
| Trail integrity check | Core | The recovery procedure MUST verify trail completeness, timestamp monotonicity, and terminal consistency before reconstructing state (§3.1). |
| Workspace state reconstruction | Core | The recovery procedure MUST reconstruct every workspace's lifecycle state from `workspace_state_changed` trail entries (§3.2). |
| Envelope recovery | Core | The recovery procedure MUST identify in-flight envelopes (created but not delivered or failed) and re-evaluate them for delivery (§3.3). |
| Signal recovery | Core | The recovery procedure MUST identify undelivered signals and requeue them for delivery (§3.3). |
| Timer reconstruction | Core | The recovery procedure MUST reconstruct workspace timeouts, liveness intervals, and budget tracking from trail entries (§3.4). |
| Clock recovery | Core | After recovery, the clock MUST produce timestamps strictly greater than the maximum timestamp in the trail (§3.5). |
| Recovery completion marker | Core | The recovery procedure MUST record a `recovery_completed` trail entry as its final step (§3.6). |
| Idempotent recovery | Core | The recovery procedure MUST produce the same result regardless of how many times it is executed (§3.3). |
| No silent drops | Core | The runtime MUST NOT silently drop trail entries, envelopes, or signals. If an operation cannot be recorded or delivered, the runtime MUST retry, escalate, or halt (PROTOCOL §10.4, invariant 4). |
| Degraded mode | Standard | When trail writes persistently fail, the runtime MUST enter degraded mode: block new operations, allow active agents to continue, record a `system_degraded` entry when writes recover (§2.4). |
| Trail write retry | Standard | The runtime SHOULD retry failed trail writes with bounded attempts before entering degraded mode (§2.4). |
| Agent failure signaling | Standard | The protocol MUST support agent-local failure translation via `blocked` and `failed` signals with descriptive reasons (§2.1). Liveness monitoring MUST serve as backstop. |
| Recovery event recording | Standard | The runtime MUST record `system_degraded` and `recovery_completed` trail entries with the schemas defined in §4. |
| Budget reconstruction baseline | Standard | Budget tracking after recovery MUST use trail data as the authoritative baseline. The runtime MUST NOT assume pre-crash in-memory budget data survived (§3.4). |
| Corruption handling | Full | The runtime SHOULD quarantine non-critically corrupted trail entries and proceed with recovery using the valid portion (§3.1). Quarantined entries SHOULD be reported in the `recovery_completed` event. |
| In-flight transition recovery | Full | The recovery procedure SHOULD detect state-changing signals (`complete`, `failed`, `blocked`) that were emitted but whose corresponding state transitions were not recorded, and trigger those transitions (§3.2). |
| Workspace component reconstruction | Full | The runtime SHOULD reconstruct full workspace internal models (visibility set, priority, group membership, owner) from the trail, not only lifecycle state (§3.2). |
| Compute timeout reconstruction | Full | The runtime SHOULD reconstruct accumulated compute time from workspace state transition entries (§3.4). |

## 6. Implementation Notes

These notes are non-normative. They capture practical guidance for implementers.

**Recovery as a single-pass scan.** The recovery procedure can be implemented as a single forward scan of the global trail. For each entry, the runtime updates an in-memory model: workspace states, envelope lifecycle maps, signal delivery maps, timer baselines. At the end of the scan, the model contains the reconstructed system state. This is more efficient than per-workspace scans because it reads the trail once rather than once per workspace.

**Delivery ledger for fast envelope recovery.** The envelope recovery step (§3.3) requires matching `envelope_created` entries with their downstream lifecycle entries. A full trail scan to find these pairs can be expensive. Implementations can maintain a persistent "delivery ledger" — a separate index of in-flight envelopes (created but not yet delivered or failed). The ledger is updated alongside trail writes and provides O(1) lookup during recovery. The trail remains the authoritative source; the ledger is a performance cache.

**Timer reconstruction precision.** Timer reconstruction (§3.4) uses trail timestamps to calculate elapsed time. The precision of the reconstruction depends on the gap between the last trail entry before the crash and the current time. During this gap, no trail entries were written — the runtime does not know what happened. For workspace timeouts (wall-clock), this gap is counted as elapsed time (the workspace was timing out whether the runtime was running or not). For compute timeouts (active time only), the gap should NOT be counted — the workspace was not actively computing during the crash.

**Progressive recovery.** For large trails, the recovery scan may take significant time. Implementations may support progressive recovery — reconstructing workspaces in priority order (critical first, background last) and beginning to serve reconstructed workspaces before the full scan completes. This reduces recovery latency for high-priority work at the cost of implementation complexity. The protocol requires only that all workspaces are eventually reconstructed; it does not require that recovery is atomic.

**Testing recovery.** A conformant implementation should be testable against these scenarios:

1. **Clean crash recovery.** Crash the runtime mid-operation. Restart. Verify all workspace states match their trail-reconstructed states.
2. **Envelope in-flight recovery.** Create an envelope, crash before delivery. Verify the envelope is delivered after recovery.
3. **Signal in-flight recovery.** Emit a signal, crash before parent delivery. Verify the signal reaches the parent after recovery.
4. **Timer reconstruction.** Create a workspace with a 10-second timeout. Crash at 5 seconds. Recover after 8 seconds. Verify the workspace has 2 seconds remaining, not 10.
5. **Idempotent recovery.** Run recovery. Crash during recovery. Run recovery again. Verify the final state is identical.
6. **Degraded mode entry and exit.** Simulate persistent trail write failure. Verify operations block. Restore writes. Verify `system_degraded` entry is recorded and operations resume.
7. **Budget reconstruction.** Consume half a workspace's token budget. Crash. Verify budget tracking resumes from the trail baseline, not from zero.

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*