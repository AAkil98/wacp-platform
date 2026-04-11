# WACP Implementation: Coordinator Integration

```yaml
id: wacp-impl-integration
type: implementation-spec
status: complete
created: 2026-03-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §7 (integration and checkpoints)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-topology
  - wacp-impl-task-scheduling
  - wacp-impl-storage
  - wacp-spec-workspace
  - wacp-spec-checkpoint
  - wacp-spec-signal
  - wacp-mech-integration
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, integration, merge, conflict, resolution, salvage, checkpoint]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Integration Procedure](#2-integration-procedure)
3. [Coordinator Decision](#3-coordinator-decision)
4. [Merge Strategies](#4-merge-strategies)
5. [Conflict Detection](#5-conflict-detection)
6. [Conflict Resolution](#6-conflict-resolution)
7. [Salvage Integration](#7-salvage-integration)
8. [Integration Ordering](#8-integration-ordering)
9. [Trail Events](#9-trail-events)
10. [Invariant Enforcement](#10-invariant-enforcement)
11. [References](#11-references)

## 1. Purpose

This spec defines how the coordinator merges completed workspace output into the parent context. It answers "how does integration become code" — not "what integration means" (that's the integration mechanism spec) or "how tasks reach the completed state" (that's the task-scheduling spec) or "how checkpoints are stored" (that's the storage spec).

Integration is the assembly operation at the heart of the protocol's coordination model. An agent does work in a workspace, produces a final checkpoint, emits `complete`, and the workspace enters `integrating`. The coordinator then decides: accept (merge the checkpoint into the parent), revise (reject with feedback, requesting rework), or reject (abandon the work). If accepted, the coordinator selects a merge strategy, runs conflict detection, and resolves any conflicts. Only after all of this does the workspace reach `closed` and the task reach `integrated`.

**Scope.** The integration procedure from workspace `complete` signal to workspace `closed` or `failed`. The coordinator's accept/revise/reject decision. Three merge strategies — `direct`, `layered`, `evaluated` — their algorithms, selection criteria, and conflict coverage. Four conflict types — their detection mechanisms per strategy. Three resolution strategies — coordinator resolve, human escalation, agent rework. Salvage integration for failed workspaces (three guardrails). Integration ordering across siblings. Trail event production.

**Not in scope.** Task graph data structures (topology spec, §3). Task lifecycle state machine (task-scheduling spec, §2). Workspace lifecycle internals (runtime spec, §4). Checkpoint storage and content addressing (storage spec, §5). Signal propagation (runtime spec, §10). How agents produce checkpoints (sdk-agent spec).

**Design constraint.** Integration is sequential — one workspace at a time, each seeing the cumulative result of all prior integrations. There are no parallel merges. The coordinator is the single integration agent — it reads checkpoints, detects conflicts, and synthesizes results. Integration order affects conflict detection: integrating workspace A before B means B detects conflicts against A's merged output, not vice versa. The coordinator chooses the order deliberately.

---

## 2. Integration Procedure

Integration begins when a workspace emits `complete` and ends when the workspace reaches `closed` (success) or `failed` (revise, reject, or conflict failure). This section defines the end-to-end procedure from the coordinator's perspective.

### 2.1 Entry

The workspace actor emits a `complete` signal. The coordinator receives it and:

1. Validates that the workspace is in `active` state (the FSM transition `active → integrating` must be valid).
2. Transitions the workspace to `integrating`. The workspace becomes read-only — the inbox is sealed, no new checkpoints are accepted, the agent's streaming RPCs continue but receive no new messages.
3. Emits an `integrate` signal upward through the workspace tree (signal spec — notifies the parent that a merge is starting).
4. Transitions the task from `in_progress` to `completed` (task-scheduling spec, §9.1).

```rust
fn handle_complete_signal(&mut self, workspace_id: &WorkspaceId) {
    // 1–2. Workspace: active → integrating
    self.send_to_workspace(workspace_id, CoordinatorCommand::BeginIntegration);

    // 3. Propagate integrate signal
    self.propagate_signal(Signal {
        r#type: SignalType::Integrate,
        workspace_id: workspace_id.clone(),
        timestamp: self.clock.now(),
        reason: String::new(),
        context: bytes::Bytes::new(),
    });

    // 4. Task: in_progress → completed
    if let Some(task_id) = self.task_graph.workspace_task.get(workspace_id).cloned() {
        self.transition_task(&task_id, TaskTrigger::WorkspaceCompleted);
    }

    // 5. Queue for integration (§8 — ordering)
    self.integration_queue.push(workspace_id.clone());
    self.try_integrate_next();
}
```

### 2.2 The Integration Pipeline

When the coordinator picks a workspace from the integration queue (§8), it runs the integration pipeline:

```
Step  What                           Outcome
────  ─────────────────────────────  ──────────────────────
 1    Read final checkpoint          checkpoint or error
 2    Coordinator decision           accept / revise / reject
 3    Select merge strategy          direct / layered / evaluated
 4    Execute merge                  merged state or conflicts
 5    If conflicts: resolve          closed or failed
 6    Update parent context          parent workspace updated
 7    Close workspace                workspace → closed, task → integrated
```

**Step 1: Read final checkpoint.** The coordinator reads the workspace's checkpoint register (via the workspace actor) and locates the most recent checkpoint with `status: final`. If no final checkpoint exists, the coordinator cannot proceed with normal integration — it may attempt salvage (§7) or reject.

```rust
fn find_final_checkpoint(&self, workspace_id: &WorkspaceId) -> Option<CheckpointRef> {
    let checkpoints = self.get_workspace_checkpoints(workspace_id);
    checkpoints.iter()
        .rev()
        .find(|c| c.status == CheckpointStatus::Final)
        .map(|c| CheckpointRef {
            checkpoint_id: c.id.clone(),
            content_hash: c.content_hash.clone(),
            intent: c.intent.clone(),
            confidence: c.confidence,
        })
}
```

**Step 2: Coordinator decision.** The coordinator decides accept, revise, or reject (§3). This is exactly one decision per completion — no backward transitions (invariant 1).

**Step 3: Select merge strategy.** On accept, the coordinator selects `direct`, `layered`, or `evaluated` (§4).

**Step 4: Execute merge.** Run the selected strategy. If conflicts are detected, the workspace transitions from `integrating` to `conflicted`.

**Step 5: Resolve conflicts.** If the workspace is `conflicted`, the coordinator resolves each conflict (§6). All conflicts must be resolved before the workspace can exit `conflicted`.

**Step 6: Update parent context.** The merged artifacts are applied to the parent workspace's context — making the integrated output visible to sibling workspaces and the coordinator.

**Step 7: Close workspace.** The workspace transitions to `closed`. The task transitions to `integrated`. Port rights are expired (topology spec, §8.2). The task graph's readiness counters for dependents are decremented (topology spec, §3.3).

### 2.3 Revise and Reject Paths

If the coordinator decides to revise or reject at step 2, the pipeline short-circuits:

**Revise.** The workspace transitions to `failed` with `reason: revision_required`. The task transitions to `failed`. The coordinator creates a new workspace for the same task with a feedback directive explaining what needs to change. The task re-enters the dispatch pipeline via retry (task-scheduling spec, §7.1).

**Reject.** The workspace transitions to `failed` with `reason: rejected`. The task transitions to `failed`. The coordinator decides whether to retry (new workspace, fresh attempt) or cancel (task → `cancelled`).

In both cases, no merge is attempted. No conflict detection runs. The workspace's checkpoints remain in the checkpoint store for audit purposes but are not integrated.

---

## 3. Coordinator Decision

The coordinator makes exactly one decision per workspace completion: accept, revise, or reject. This decision is the integration gateway — nothing proceeds without it.

### 3.1 Decision Inputs

The coordinator evaluates:

1. **Final checkpoint content.** The payload, intent, confidence, and resource usage of the final checkpoint.
2. **Checkpoint chain.** The sequence of provisional and final checkpoints — the workspace's work history.
3. **Directive.** What was the workspace asked to do? The decision compares output against intent.
4. **Dependency context.** What inputs did the workspace receive? Were they consumed appropriately?
5. **Resource consumption.** How much of the budget was used? Over-consumption or under-consumption may signal issues.

### 3.2 Decision Logic

The decision is a coordinator policy — the protocol does not prescribe the algorithm. The initial implementation uses a rule-based approach:

```rust
pub enum IntegrationDecision {
    Accept { strategy: MergeStrategy },
    Revise { feedback: String },
    Reject { reason: String },
}

fn decide_integration(
    &self,
    workspace_id: &WorkspaceId,
    checkpoint: &CheckpointRef,
) -> IntegrationDecision {
    // Rule 1: No final checkpoint → reject
    // (caller already checked; this is defense-in-depth)

    // Rule 2: Confidence too low → revise with feedback
    if checkpoint.confidence == Confidence::Low {
        return IntegrationDecision::Revise {
            feedback: "Checkpoint confidence is low; please verify and resubmit with higher confidence".into(),
        };
    }

    // Rule 3: Select strategy based on workspace role and directive scope
    let strategy = self.select_strategy(workspace_id);

    IntegrationDecision::Accept { strategy }
}
```

**Evaluator workflow (optional).** Before deciding, the coordinator may dispatch an evaluator workspace — an observer-role agent that reads the worker's checkpoint and produces an evaluation checkpoint. The coordinator then reads both checkpoints before deciding. This adds latency but increases decision quality for high-stakes tasks. The evaluator workflow is a coordinator-level configuration, not a protocol requirement.

### 3.3 Decision Trail Event

The decision is recorded as part of the `integration_started` trail entry (§9). The entry includes the decision (`accept`/`revise`/`reject`), the strategy (if accept), the checkpoint reference, and the mode (`normal` or `salvage`).

---

## 4. Merge Strategies

Three strategies define how workspace artifacts are incorporated into the parent context. Each has different cost, safety, and conflict coverage characteristics.

### 4.1 Direct Merge

**Algorithm.** Copy the workspace's final checkpoint artifacts into the parent context without comparison. No conflict detection.

```rust
fn merge_direct(
    &mut self,
    source: &WorkspaceId,
    target: &WorkspaceId,
    checkpoint: &CheckpointRef,
) -> MergeResult {
    let payload = self.read_checkpoint_payload(&checkpoint.content_hash);

    // Apply artifacts to parent context — blind copy
    self.apply_artifacts_to_parent(target, &payload);

    MergeResult::Success
}
```

**When to use.** The workspace had exclusive authority over its output scope — no other workspace could have modified the same resources. Overlap is structurally impossible. This is the fast path for isolated, non-overlapping tasks.

**Conflict coverage.** None. If there is overlap, direct merge silently overwrites. The coordinator must be confident that overlap is impossible before selecting this strategy.

### 4.2 Layered Merge

**Algorithm.** Apply workspace artifacts on top of the current parent state. Compare the workspace's modified resources against the parent's current state to detect overlap.

```rust
fn merge_layered(
    &mut self,
    source: &WorkspaceId,
    target: &WorkspaceId,
    checkpoint: &CheckpointRef,
) -> MergeResult {
    let payload = self.read_checkpoint_payload(&checkpoint.content_hash);
    let parent_state = self.read_parent_state(target);

    // Detect content overlap: resources modified by both source and
    // prior integrations into the parent
    let source_resources = extract_modified_resources(&payload);
    let parent_resources = extract_modified_resources(&parent_state);
    let overlapping: Vec<_> = source_resources.intersection(&parent_resources).collect();

    if !overlapping.is_empty() {
        let conflicts: Vec<Conflict> = overlapping.iter()
            .map(|resource| Conflict {
                conflict_type: ConflictType::ContentOverlap,
                resources: vec![resource.to_string()],
                description: format!(
                    "Resource '{}' modified by both source workspace and prior integration",
                    resource
                ),
            })
            .collect();
        return MergeResult::Conflicts(conflicts);
    }

    // No overlap — apply artifacts
    self.apply_artifacts_to_parent(target, &payload);

    MergeResult::Success
}
```

**When to use.** Workspaces had mostly non-overlapping directives, but minor overlap is possible. The coordinator wants mechanical overlap detection without full semantic analysis.

**Conflict coverage.** `content_overlap` only. Detects when two workspaces modified the same resource. Does not detect semantic contradiction, dependency violation, or constraint breach.

### 4.3 Evaluated Merge

**Algorithm.** The coordinator reads both the workspace's checkpoint and the parent's current state, analyzes them fully, and produces a synthesized result. This is the most expensive strategy but provides complete conflict coverage.

```rust
fn merge_evaluated(
    &mut self,
    source: &WorkspaceId,
    target: &WorkspaceId,
    checkpoint: &CheckpointRef,
) -> MergeResult {
    let payload = self.read_checkpoint_payload(&checkpoint.content_hash);
    let parent_state = self.read_parent_state(target);

    // Full conflict detection across all four types
    let mut conflicts = Vec::new();

    // 1. Content overlap (same as layered)
    let source_resources = extract_modified_resources(&payload);
    let parent_resources = extract_modified_resources(&parent_state);
    for resource in source_resources.intersection(&parent_resources) {
        conflicts.push(Conflict {
            conflict_type: ConflictType::ContentOverlap,
            resources: vec![resource.to_string()],
            description: format!("Resource '{}' modified by both", resource),
        });
    }

    // 2. Semantic contradiction (coordinator judgment)
    if let Some(contradiction) = self.detect_semantic_contradiction(&payload, &parent_state) {
        conflicts.push(contradiction);
    }

    // 3. Dependency violation (causal check)
    if let Some(violation) = self.detect_dependency_violation(source, &payload, &parent_state) {
        conflicts.push(violation);
    }

    // 4. Constraint breach (rule-based validation)
    let merged_candidate = self.synthesize_merge(&payload, &parent_state);
    if let Some(breach) = self.validate_constraints(&merged_candidate) {
        conflicts.push(breach);
    }

    if !conflicts.is_empty() {
        return MergeResult::Conflicts(conflicts);
    }

    // No conflicts — apply synthesized result
    self.apply_artifacts_to_parent(target, &merged_candidate);

    MergeResult::Success
}
```

**When to use.** High-stakes tasks where correctness is critical. Overlapping directives. Workflows where silent conflicts are unacceptable.

**Conflict coverage.** All four types: `content_overlap`, `semantic_contradiction`, `dependency_violation`, `constraint_breach`.

### 4.4 Strategy Selection

The coordinator selects a strategy based on the workspace's characteristics:

```rust
fn select_strategy(&self, workspace_id: &WorkspaceId) -> MergeStrategy {
    let node = match self.tree.nodes.get(workspace_id) {
        Some(n) => n,
        None => return MergeStrategy::Evaluated, // safe default
    };

    let siblings = self.tree.siblings(workspace_id);
    let has_integrated_siblings = siblings.iter().any(|s| {
        self.tree.nodes.get(s)
            .map_or(false, |n| n.status == WorkspaceStatus::Closed)
    });

    if !has_integrated_siblings {
        // First sibling to integrate — no prior state to conflict with
        MergeStrategy::Direct
    } else if self.is_non_overlapping_directive(workspace_id) {
        // Directive scope is disjoint from all prior integrations
        MergeStrategy::Direct
    } else {
        // Default: layered for overlap detection; evaluated for critical tasks
        if self.is_critical_task(workspace_id) {
            MergeStrategy::Evaluated
        } else {
            MergeStrategy::Layered
        }
    }
}
```

**Detection-strategy matrix:**

| Strategy | `content_overlap` | `semantic_contradiction` | `dependency_violation` | `constraint_breach` |
|----------|:-:|:-:|:-:|:-:|
| `direct` | — | — | — | — |
| `layered` | detected | — | — | — |
| `evaluated` | detected | detected | detected | detected |

### 4.5 Merge Result

```rust
pub enum MergeResult {
    Success,
    Conflicts(Vec<Conflict>),
}

pub struct Conflict {
    pub conflict_type: ConflictType,
    pub resources: Vec<String>,
    pub description: String,
}

pub enum ConflictType {
    ContentOverlap,
    SemanticContradiction,
    DependencyViolation,
    ConstraintBreach,
}

pub enum MergeStrategy {
    Direct,
    Layered,
    Evaluated,
}
```

---

## 5. Conflict Detection

Conflict detection runs as part of the merge strategy (§4). Each conflict type has a distinct detection mechanism.

### 5.1 Content Overlap

**Mechanism.** Structural comparison of modified resource sets. Two workspaces modified the same resource (file, field, object).

**Detection.** Compute `source_modified ∩ parent_modified`. Non-empty intersection means overlap. The comparison is by resource identifier (file path, key, field name) — not by content. Two workspaces modifying the same file is an overlap even if they modified different lines.

**Available in.** `layered` and `evaluated` strategies.

### 5.2 Semantic Contradiction

**Mechanism.** Judgmental — the coordinator (or an evaluator agent) recognizes that two outputs are logically incompatible even though they don't touch the same resources.

**Detection.** The coordinator analyzes the intent and content of the workspace's output against the parent's current state. This is not a mechanical comparison — it requires understanding the domain. In the initial implementation, semantic contradiction detection is delegated to an evaluator workspace (§3.2, evaluator workflow). Without an evaluator, semantic contradictions are not detected.

**Available in.** `evaluated` strategy only.

### 5.3 Dependency Violation

**Mechanism.** Causal — a workspace's output depends on an assumption that a prior integration invalidated. The workspace started before the dependency was integrated, and the integration changed something the workspace relied on.

**Detection.** Compare the workspace's dependency context (the checkpoint references it received at dispatch time — §6 of task-scheduling spec) against the current parent state. If the parent state has diverged from what the workspace assumed, the workspace's output may be invalid.

```rust
fn detect_dependency_violation(
    &self,
    source: &WorkspaceId,
    payload: &[u8],
    parent_state: &[u8],
) -> Option<Conflict> {
    let task_id = self.task_graph.workspace_task.get(source)?;
    let task = self.task_graph.tasks.get(task_id)?;

    // Check if any dependency was integrated after this workspace started
    let deps = self.task_graph.reverse.get(task_id)?;
    for dep_id in deps {
        let dep_task = self.task_graph.tasks.get(dep_id)?;
        if dep_task.status == TaskStatus::Integrated {
            // The dependency was integrated — check if the parent state
            // now differs from what was in this workspace's context
            if self.dependency_context_diverged(source, dep_id) {
                return Some(Conflict {
                    conflict_type: ConflictType::DependencyViolation,
                    resources: vec![dep_id.to_string()],
                    description: format!(
                        "Dependency '{}' was integrated after workspace started; \
                         parent state may have diverged from workspace assumptions",
                        dep_task.name
                    ),
                });
            }
        }
    }

    None
}
```

**Available in.** `evaluated` strategy only.

### 5.4 Constraint Breach

**Mechanism.** Rule-based — the merged output violates a workflow-level constraint (size limit, format requirement, validation rule, test suite).

**Detection.** The coordinator merges the workspace's output with the parent state tentatively (without committing), then validates the merged result against configured constraints. A constraint failure produces a `constraint_breach` conflict.

```rust
fn validate_constraints(&self, merged_candidate: &[u8]) -> Option<Conflict> {
    for constraint in &self.workflow_constraints {
        if let Err(violation) = constraint.validate(merged_candidate) {
            return Some(Conflict {
                conflict_type: ConflictType::ConstraintBreach,
                resources: violation.affected_resources,
                description: violation.message,
            });
        }
    }
    None
}
```

**Available in.** `evaluated` strategy only.

---

## 6. Conflict Resolution

When conflicts are detected, the workspace transitions from `integrating` to `conflicted`. The coordinator resolves each conflict independently using one of three strategies.

### 6.1 Resolution Strategies

```rust
pub enum ResolutionStrategy {
    CoordinatorResolve,
    HumanEscalate,
    AgentRework,
}

pub struct Resolution {
    pub conflict: Conflict,
    pub strategy: ResolutionStrategy,
    pub outcome: ResolutionOutcome,
}

pub enum ResolutionOutcome {
    Resolved { merged_content: bytes::Bytes },
    Escalated { gate_id: GateId },
    Rework { feedback: String },
}
```

### 6.2 Coordinator Resolve

The coordinator examines the conflicting artifacts and resolves the conflict by producing a synthesized result.

```rust
fn resolve_coordinator(
    &mut self,
    workspace_id: &WorkspaceId,
    conflict: &Conflict,
    payload: &[u8],
    parent_state: &[u8],
) -> Resolution {
    let merged = self.synthesize_resolution(conflict, payload, parent_state);

    Resolution {
        conflict: conflict.clone(),
        strategy: ResolutionStrategy::CoordinatorResolve,
        outcome: ResolutionOutcome::Resolved { merged_content: merged },
    }
}
```

**When to use.** The coordinator has sufficient context to resolve the conflict — typically `content_overlap` where the coordinator can determine which version to keep, or `dependency_violation` where the coordinator can identify whether the divergence matters.

### 6.3 Human Escalation

The conflict is routed to the human highway for resolution. A gate event is created with the full conflict context.

```rust
fn resolve_escalate(
    &mut self,
    workspace_id: &WorkspaceId,
    conflict: &Conflict,
) -> Resolution {
    let gate_id = GateId::generate();

    // Route to workspace owner via highway
    let gate_event = GateEvent {
        gate_id: gate_id.clone(),
        r#type: GateType::ConflictResolution,
        subject: serialize_conflict_context(conflict),
        workspace_id: workspace_id.clone(),
        task_id: self.task_graph.workspace_task
            .get(workspace_id).cloned().unwrap_or_default(),
        timeout_ms: self.config.conflict_escalation_timeout_ms,
        fallback_action: "fail".into(),
        created_at: self.clock.now(),
    };

    self.route_gate_to_highway(gate_event);

    Resolution {
        conflict: conflict.clone(),
        strategy: ResolutionStrategy::HumanEscalate,
        outcome: ResolutionOutcome::Escalated { gate_id },
    }
}
```

The human may approve the merge (workspace → `closed`), reject it (workspace → `failed`), or modify the merged output. If the human does not respond within the timeout, the workspace transitions to `failed` with `reason: conflict_timeout`.

**When to use.** `semantic_contradiction` (requires human judgment), high-stakes `constraint_breach`, or any conflict exceeding the coordinator's resolution authority.

### 6.4 Agent Rework

The current workspace is failed. A new workspace is created with a feedback directive that includes the conflict context — what conflicted, why the merge could not proceed, and what the new agent should do differently.

```rust
fn resolve_rework(
    &mut self,
    workspace_id: &WorkspaceId,
    conflict: &Conflict,
) -> Resolution {
    let feedback = format!(
        "Integration conflict: {}. Resources: {:?}. \
         Please rework your output to avoid this conflict.",
        conflict.description,
        conflict.resources,
    );

    Resolution {
        conflict: conflict.clone(),
        strategy: ResolutionStrategy::AgentRework,
        outcome: ResolutionOutcome::Rework { feedback },
    }
}
```

**When to use.** The conflict is best resolved by the agent that produced the output — typically when the agent has domain knowledge the coordinator lacks.

**Effect on workspace.** Agent rework fails the entire workspace — there is no partial resolution. If any single conflict in a multi-conflict set triggers agent rework, the workspace transitions to `failed` and all conflicts are included in the feedback directive.

### 6.5 Resolution Procedure

The coordinator resolves conflicts in order, applying one strategy per conflict:

```rust
fn resolve_conflicts(
    &mut self,
    workspace_id: &WorkspaceId,
    conflicts: Vec<Conflict>,
    payload: &[u8],
    parent_state: &[u8],
) -> ConflictResolutionResult {
    let mut resolutions = Vec::new();
    let mut has_rework = false;

    for conflict in &conflicts {
        let strategy = self.select_resolution_strategy(conflict);

        match strategy {
            ResolutionStrategy::CoordinatorResolve => {
                let r = self.resolve_coordinator(workspace_id, conflict, payload, parent_state);
                resolutions.push(r);
            }
            ResolutionStrategy::HumanEscalate => {
                let r = self.resolve_escalate(workspace_id, conflict);
                resolutions.push(r);
                // Wait for human response — resolution is async
                return ConflictResolutionResult::Pending(resolutions);
            }
            ResolutionStrategy::AgentRework => {
                let r = self.resolve_rework(workspace_id, conflict);
                resolutions.push(r);
                has_rework = true;
                break; // agent rework fails the workspace — no further resolution
            }
        }
    }

    if has_rework {
        ConflictResolutionResult::Rework(resolutions)
    } else {
        ConflictResolutionResult::Resolved(resolutions)
    }
}

pub enum ConflictResolutionResult {
    Resolved(Vec<Resolution>),
    Pending(Vec<Resolution>),   // waiting for human escalation
    Rework(Vec<Resolution>),    // workspace will be failed
}
```

**All-or-nothing on rework.** If any conflict triggers agent rework, the workspace fails. The remaining conflicts are included in the feedback but not individually resolved — the new agent addresses all of them in the rework attempt.

**Escalation pauses resolution.** If a conflict is escalated to a human, the coordinator pauses resolution for that workspace. The workspace remains in `conflicted` until the human responds or the timeout expires. Other workspaces in the integration queue (§8) continue processing — only the escalated workspace is blocked.

---

## 7. Salvage Integration

When a workspace fails but has produced checkpoints, the coordinator may attempt to salvage partial work. Salvage integration extracts usable artifacts from a failed workspace's checkpoint chain.

### 7.1 When Salvage Applies

Salvage is attempted when:
1. The workspace is in `failed` state.
2. The workspace has at least one checkpoint (provisional or final).
3. The coordinator decides the partial work is worth evaluating.

Common triggers: `reason: budget_exceeded` (agent ran out of budget mid-work), `reason: timeout` (agent ran out of time), `reason: agent_failure` (agent encountered an error but had already made progress).

Salvage is a coordinator decision, not automatic. The coordinator may choose not to salvage — in which case the workspace's checkpoints remain in the store for audit purposes but are not integrated.

### 7.2 Checkpoint Selection

The coordinator selects a checkpoint from the failed workspace's chain:

```rust
fn select_salvage_checkpoint(
    &self,
    workspace_id: &WorkspaceId,
) -> Option<CheckpointRef> {
    let checkpoints = self.get_workspace_checkpoints(workspace_id);

    // Prefer final checkpoint if available
    if let Some(final_cp) = checkpoints.iter().rev()
        .find(|c| c.status == CheckpointStatus::Final)
    {
        return Some(checkpoint_to_ref(final_cp));
    }

    // Otherwise, use most recent provisional
    checkpoints.last().map(|c| checkpoint_to_ref(c))
}
```

### 7.3 Three Guardrails

Salvage integration is subject to three mandatory guardrails (integration mechanism spec):

**Guardrail 1: Evaluated strategy only.** Salvage integration must use the `evaluated` merge strategy. `direct` and `layered` are forbidden. The coordinator must actively evaluate the salvaged checkpoint for usability — blind copying from a failed workspace is unsafe.

**Guardrail 2: Confidence treated as low.** Regardless of the checkpoint's self-reported `confidence` field, the coordinator treats it as `Confidence::Low` for decision-making. The agent's confidence was reported before the failure — it may have been interrupted before acting on its own assessment.

**Guardrail 3: Trail transparency.** Every trail entry produced during salvage integration carries `mode: salvage`. Downstream consumers — evaluators, analysis tools, humans reviewing the trail — can distinguish salvaged integrations from normal ones. Salvage is never silent.

```rust
fn salvage_integrate(
    &mut self,
    source: &WorkspaceId,
    target: &WorkspaceId,
) -> IntegrationResult {
    let checkpoint = match self.select_salvage_checkpoint(source) {
        Some(c) => c,
        None => return IntegrationResult::NoCheckpoint,
    };

    // Guardrail 2: override confidence
    let mut salvage_ref = checkpoint.clone();
    salvage_ref.confidence = Confidence::Low;

    // Trail: integration_started with mode: salvage
    self.write_integration_started(source, target, MergeStrategy::Evaluated, Mode::Salvage);

    // Guardrail 1: evaluated strategy only
    let result = self.merge_evaluated(source, target, &salvage_ref);

    match result {
        MergeResult::Success => {
            self.write_integration_completed(source, target, Mode::Salvage);
            IntegrationResult::Salvaged
        }
        MergeResult::Conflicts(conflicts) => {
            // Salvaged work with conflicts — coordinator decides
            self.handle_salvage_conflicts(source, target, conflicts)
        }
    }
}
```

### 7.4 Salvage Does Not Resurrect

The source workspace remains `failed` after salvage. Salvage integration merges artifacts into the parent — it does not change the failed workspace's state. The workspace is dead; its work product may live on.

---

## 8. Integration Ordering

When multiple sibling workspaces complete around the same time, the coordinator must decide the integration order. Integration is sequential — one workspace at a time (§1, design constraint). The order affects conflict detection: integrating A before B means B's merge runs against the state that includes A's output.

### 8.1 Integration Queue

The coordinator maintains a queue of workspaces awaiting integration:

```rust
pub struct IntegrationQueue {
    pending: VecDeque<WorkspaceId>,
    in_progress: Option<WorkspaceId>,
}

impl IntegrationQueue {
    pub fn push(&mut self, workspace_id: WorkspaceId) {
        self.pending.push_back(workspace_id);
        self.reorder();
    }

    pub fn next(&mut self) -> Option<WorkspaceId> {
        if self.in_progress.is_some() {
            return None; // one at a time
        }
        let id = self.pending.pop_front()?;
        self.in_progress = Some(id.clone());
        Some(id)
    }

    pub fn complete(&mut self) {
        self.in_progress = None;
    }
}
```

### 8.2 Ordering Heuristics

The queue is reordered when a new workspace is added. The coordinator applies heuristics:

1. **Dependency order.** If task A depends on task B, workspace B's integration should complete before A's. This reduces `dependency_violation` conflicts — A's output was built on B's, so integrating B first establishes the base that A assumed.

2. **Priority.** `urgent` tasks integrate before `normal` tasks, independent of dependency relationships.

3. **Confidence.** Higher-confidence checkpoints integrate first. A high-confidence integration establishes a stronger base; subsequent integrations are compared against well-established state, reducing false conflicts.

4. **FIFO tiebreaker.** Equal-priority, equal-confidence, unrelated tasks are integrated in completion order.

```rust
fn reorder(&mut self) {
    let graph = &self.task_graph;
    self.pending.make_contiguous().sort_by(|a, b| {
        let task_a = graph.workspace_task.get(a).and_then(|t| graph.tasks.get(t));
        let task_b = graph.workspace_task.get(b).and_then(|t| graph.tasks.get(t));

        // Dependency order: if B depends on A, A comes first
        let a_before_b = self.is_dependency_of(a, b);
        let b_before_a = self.is_dependency_of(b, a);
        if a_before_b && !b_before_a { return std::cmp::Ordering::Less; }
        if b_before_a && !a_before_b { return std::cmp::Ordering::Greater; }

        // Priority (higher first)
        let pri_a = task_a.map_or(0, |t| t.priority);
        let pri_b = task_b.map_or(0, |t| t.priority);
        pri_b.cmp(&pri_a)
    });
}
```

### 8.3 Ordering Affects Detection

The protocol explicitly acknowledges that integration order determines which conflicts surface. If workspaces A and B both modify the same resource:
- Integrating A first: B detects `content_overlap` against A's merged output.
- Integrating B first: A detects `content_overlap` against B's merged output.

The coordinator should integrate the more authoritative workspace first (higher confidence, earlier dependency) to minimize false conflicts — the "base" integration is the one other workspaces are compared against.

---

## 9. Trail Events

Five integration-specific trail event types. All carry a `mode` field (`normal` or `salvage`).

### 9.1 Event Definitions

| Event | When | Key fields |
|-------|------|------------|
| `integration_started` | Coordinator begins merge | `source`, `target`, `strategy`, `mode`, `checkpoint_ref`, `decision` |
| `integration_completed` | Merge succeeds | `source`, `target`, `strategy`, `mode`, `result` |
| `integration_aborted` | Merge abandoned (revise/reject) | `source`, `target`, `mode`, `reason` |
| `conflict_detected` | Conflict found during merge | `workspace_id`, `conflict_type`, `resources`, `description` |
| `conflict_resolved` | Conflict resolved | `workspace_id`, `conflict_type`, `resolution_strategy`, `outcome` |

### 9.2 Event Patterns

**Conflict-free integration.** Two entries:
```
integration_started → integration_completed
```

**Integration with conflicts.** Four or more entries:
```
integration_started → conflict_detected (×N) → conflict_resolved (×N) → integration_completed
```

**Revise or reject.** Two entries:
```
integration_started → integration_aborted
```

### 9.3 Conflict Pairing

Every `conflict_detected` entry must be paired with a `conflict_resolved` entry for the same conflict type and workspace. The only exception is timeout: if the workspace transitions to `failed` with `reason: conflict_timeout`, the timeout is the implicit resolution — no explicit `conflict_resolved` entry is written for the timed-out conflict, but a `workspace_state_changed` entry records the failure.

### 9.4 Recovery

Integration state is transient — it exists only while a workspace is in `integrating` or `conflicted` state. On recovery:

- A workspace in `integrating` with `integration_started` but no `integration_completed` or `integration_aborted`: the coordinator re-runs the integration from step 2 (decision). The `integration_started` trail entry tells recovery which workspace was mid-integration.
- A workspace in `conflicted` with unresolved conflicts: the coordinator re-evaluates conflicts. If escalation was in progress, the gate event is re-emitted to the highway.
- A workspace in `integrating` with no `integration_started`: the coordinator re-queues it for integration.

---

## 10. Invariant Enforcement

Twenty-two invariants from the integration mechanism spec, grouped by category.

### 10.1 Process Invariants

| Invariant | How enforced |
|-----------|-------------|
| One decision per completion | `decide_integration` called once; result determines path |
| No workspace resurrection | Revise/reject transition to `failed`; new workspace for rework |
| Final checkpoint prerequisite | `find_final_checkpoint` returns `None` → reject or salvage |
| Sequential merge | `IntegrationQueue.in_progress` gate — one at a time |
| Detection before resolution | `merge_*` returns conflicts; `resolve_conflicts` called only on `MergeResult::Conflicts` |

### 10.2 Strategy Invariants

| Invariant | How enforced |
|-----------|-------------|
| Strategy determines detection coverage | `merge_direct` has no detection; `merge_layered` checks overlap only; `merge_evaluated` checks all four |
| Salvage: evaluated only | `salvage_integrate` hardcodes `merge_evaluated` |
| Coordinator synthesizes in evaluated merge | `synthesize_merge` produces new output in `merge_evaluated` |

### 10.3 Conflict Invariants

| Invariant | How enforced |
|-----------|-------------|
| Conflicts → conflicted state | Workspace transitions `integrating → conflicted` when `MergeResult::Conflicts` returned |
| All conflicts must resolve | `resolve_conflicts` iterates all; rework short-circuits by failing workspace |
| Per-conflict independence | Each conflict gets its own `select_resolution_strategy` call |
| Conflict timeout | Timer in coordinator's `FuturesUnordered`; expiry → `failed` with `reason: conflict_timeout` |

### 10.4 Salvage Invariants

| Invariant | How enforced |
|-----------|-------------|
| Failed source required | `salvage_integrate` only called for `Failed` workspaces |
| Source remains failed | `salvage_integrate` does not change source workspace state |
| Evaluation required | Guardrail 1: `merge_evaluated` mandatory |
| Confidence lowered | Guardrail 2: `salvage_ref.confidence = Confidence::Low` |
| Trail transparency | Guardrail 3: all trail entries carry `mode: salvage` |

### 10.5 Trail Invariants

| Invariant | How enforced |
|-----------|-------------|
| Event completeness | Every integration path writes started + completed/aborted |
| Conflict pairing | `conflict_detected` and `conflict_resolved` written in pairs |
| Integrate signal emitted | `propagate_signal(Integrate)` at entry (§2.1) |

### 10.6 Task-Integration Coupling

| Invariant | How enforced |
|-----------|-------------|
| Workspace closed → task integrated | `terminate_workspace` with `Closed` triggers `IntegrationSucceeded` |
| Checkpoint-task linkage | `task.checkpoint_ref` set before `WorkspaceCompleted` transition (task-scheduling spec, §9.1) |

---

## 11. References

### Protocol Specs

| Spec | Referenced in | Topic |
|------|--------------|-------|
| `mechanisms/integration.md` | §1–§10 | Integration procedure, merge strategies, conflicts, resolution, salvage, 22 invariants |
| `primitives/checkpoint.md` | §2.2, §7.2 | Checkpoint lifecycle, provisional/final, content hash, confidence |
| `primitives/signal.md` | §2.1 | `complete` and `integrate` signals |
| `primitives/workspace.md` | §2.1, §6 | `integrating` and `conflicted` states |
| PROTOCOL.md §7 | §1 | High-level integration rules |

### Implementation Specs

| Spec | Referenced in | Topic |
|------|--------------|-------|
| `impl/runtime.md` §4 | §2.3 | Generic state machine engine, workspace FSM |
| `impl/runtime.md` §10 | §2.1 | Signal propagation (integrate signal) |
| `impl/topology.md` §3 | §1, §8.2 | Task graph, readiness counters, sibling enumeration |
| `impl/topology.md` §8.2 | §2.2 | Workspace termination, port rights expiration |
| `impl/task-scheduling.md` §2 | §1, §2.1 | Task lifecycle, `WorkspaceCompleted` trigger |
| `impl/task-scheduling.md` §7 | §2.3 | Retry on revise/reject |
| `impl/task-scheduling.md` §9.1 | §2.1 | Signal-to-task mapping |
| `impl/storage.md` §5 | §2.2 | Checkpoint store, content-addressable payloads |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/PROTOCOL.md) | Taxonomy: [TAXONOMY.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/TAXONOMY.md)*
