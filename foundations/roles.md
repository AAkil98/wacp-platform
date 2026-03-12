# WACP: Roles & Permissions

## Metadata

```yaml
title: "WACP: Roles & Permissions"
id: wacp-spec-roles
type: constituent-spec
tier: abstract
category: foundations
status: complete
created: 2026-02-24
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §5.1–§5.6 (role model, base roles, inheritance, permission matrix, delegation, port rights)
depends_on: []
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, roles, permissions, authorization, capability]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Role Model](#2-role-model)
3. [Base Roles](#3-base-roles)
4. [Delegation Capability](#4-delegation-capability)
5. [Role Inheritance](#5-role-inheritance)
6. [Permission Matrix](#6-permission-matrix)
7. [Trail Events](#7-trail-events)
8. [Conformance Requirements](#8-conformance-requirements)
9. [Implementation Notes](#9-implementation-notes)
10. [References](#10-references)

---

## 1. Purpose

Roles define what each agent is permitted to do. Permissions enforce those boundaries. Together they are the protocol's authorization model — the mechanism that ensures an agent can only send, receive, create, and read what the protocol explicitly allows.

This is not advisory. If a role lacks permission to perform an action, the runtime rejects the action. Enforcement is structural, not cooperative — it does not depend on the agent choosing to comply. In OS terms, roles are capability-based access control: an agent holds exactly the capabilities its role grants, no more.

This spec defines the base roles, the permission matrix, the inheritance model for derived roles, and the rules governing how permissions interact with the rest of the protocol.

## 2. Role Model

A role is a named set of capabilities bound to a workspace at creation. It governs five dimensions of an agent's authority:

1. **Send** — which envelope types the agent can send, and to which roles.
2. **Receive** — which envelope types the agent can receive, and from which roles.
3. **Emit** — which signal types the agent can emit.
4. **Create** — which checkpoint types the agent can produce.
5. **Access** — what the agent can read (own workspace, other workspaces, local trail, global trail) and what it can modify (own workspace files only, or nothing).

These five dimensions are exhaustive. Every protocol action an agent can take falls into one of them. An action not covered by the agent's role is prohibited.

**Assignment rules:**

- A role is assigned to a workspace at creation by the coordinator. The agent does not choose its role.
- A role cannot change during a workspace's lifetime. If a different role is needed, a new workspace is created.
- One workspace, one role. A workspace does not hold multiple roles simultaneously.
- The coordinator role is unique — exactly one workspace holds it, and it is assigned by the runtime at initialization (PROTOCOL §5.1), not by another coordinator.

**Enforcement point:**

The runtime evaluates permissions at the moment of action, not at planning time. When an agent attempts to send an envelope, the runtime checks the permission matrix before the envelope enters the delivery pipeline. When an agent attempts to create a checkpoint, the runtime verifies the checkpoint type is permitted for that role. Rejection is immediate and recorded in the trail.

## 3. Base Roles

The protocol defines three base roles. These represent the three fundamental capability levels in the system: orchestrate, produce, and observe. Applications define specialized roles through inheritance (§5).

**Coordinator**

The root agent. Exactly one per run. It decomposes tasks, delegates work, evaluates results, and integrates outcomes. It sees across all workspaces but modifies none directly — it orchestrates through envelopes and protocol operations, never by reaching into a workspace.

| Dimension | Capabilities |
|-----------|-------------|
| Send | `directive` → Worker; `feedback` → Worker |
| Receive | `query` ← Worker; signals from direct child workspaces (via signal propagation); all descendant signals visible via global trail |
| Emit | `ready`, `started`, `failed` (lifecycle); `integrate`, `acknowledged` (role-specific) |
| Create | — (no checkpoints; the coordinator produces no artifacts) |
| Access | Read: all workspaces, global trail. Modify: none directly. |

**Protocol-level actions** (coordinator privileges enforced by the runtime):
- Create and abort workspaces
- Assign roles to workspaces
- Grant the `delegate` capability (§4)
- Perform integration
- Manage workspace budgets (PROTOCOL §6.6)
- Grant dynamic visibility (PROTOCOL §6.8)
- Configure the human highway (PROTOCOL §8)

**Worker**

The producing agent. Receives a directive, works within its workspace, declares progress through checkpoints. Deliberately isolated — it knows its task and nothing else, unless granted additional capabilities.

| Dimension | Capabilities |
|-----------|-------------|
| Send | `query` → Coordinator |
| Receive | `directive` ← Coordinator; `feedback` ← Coordinator |
| Emit | `ready`, `started`, `blocked`, `checkpoint`, `complete`, `failed`, `escalation` |
| Create | `artifact` checkpoints |
| Access | Read: own workspace, own directive, own local trail. Modify: own workspace files. |

A worker may be granted additional capabilities at creation — notably `delegate` (§4) — but its base capability set is minimal by design.

**Observer**

The monitoring agent. Read-only access to designated workspaces and the trail. Produces no artifacts but may record observations. The foundation for dashboards, metrics, external system bridges, and human-facing interfaces.

| Dimension | Capabilities |
|-----------|-------------|
| Send | — (no envelopes) |
| Receive | — (no envelopes) |
| Emit | `ready`, `started`, `complete`, `failed`, `escalation` |
| Create | `observation` checkpoints |
| Access | Read: designated workspaces (read-only), trail (local or global as configured). Modify: none. |

**Why three and not four.** The original protocol defined Reviewer as a fourth base role. On closer examination, a reviewer is a worker with a different checkpoint type (`review` instead of `artifact`), read access to another workspace, and a different envelope type (`report` instead of `query`). These are permission overrides, not a different capability level. Reviewer becomes the first canonical derived role — defined in the taxonomy, not in the protocol.

## 4. Delegation Capability

Delegation is the ability to create child workspaces within your subtree. It is a capability, not a role — any workspace can hold it if the coordinator grants it at creation.

**Granting delegation:**

The coordinator includes `delegate: true` in the workspace creation parameters. This is a one-time decision, frozen at creation like authority (PROTOCOL §6.4). A workspace cannot acquire delegation after leaving `idle` state, and delegation cannot be revoked.

**What a delegate can do:**

A workspace with `delegate` can:
- Create child workspaces within its own subtree. The delegate becomes the parent in the workspace tree.
- Assign roles to those children (from the set of roles available in the current run's taxonomy).
- Send `directive` and `feedback` envelopes to its children. These envelope permissions are added automatically when delegation is granted — they are not part of the base Worker role.
- Receive `query` envelopes and signals from its children.
- Perform integration of its children's checkpoints into its own workspace.

A delegate is a local coordinator for its subtree. The root coordinator remains the global coordinator.

**What a delegate cannot do:**

- Create workspaces outside its subtree. A delegate's children are its children, not siblings.
- Exceed its own budget. Children draw from the delegate's remaining budget. If the delegate has 10,000 tokens remaining, the sum of all children's budgets cannot exceed 10,000.
- Grant visibility it does not have. A child's visibility set must be a subset of the delegate's visibility set.
- Grant authority it does not have. Same containment rule.
- Grant delegation without the coordinator's approval. A delegate cannot make its children delegates — only the root coordinator can grant delegation. This prevents unbounded delegation depth unless the coordinator explicitly permits it.
- Escape the coordinator's view. All events in delegated subtrees appear in the global trail. The coordinator sees everything.

**Budget inheritance:**

When a delegate creates a child workspace with a budget, the child's budget is carved from the delegate's remaining budget. The runtime enforces this:
- At child creation: `child.budget ≤ delegate.remaining_budget`
- At runtime: the delegate's consumption includes all children's consumption. A budget warning or exceeded event on the delegate accounts for its entire subtree.
- If the delegate is aborted, its same-owner children are failed and its cross-owner children are reparented to the coordinator (Tree spec §4.3, Ownership spec §5.1 — a child cannot outlive its parent, but failure cascade is ownership-bounded).

**Trail events:**

Delegation produces standard `workspace_created` trail events. The `parent` field identifies the delegate (not the root coordinator) as the creating workspace. The global trail captures the full subtree structure.

## 5. Role Inheritance

Applications often need roles that are close to a base role but not identical. WACP supports **single-level inheritance**: a derived role extends exactly one base role, then applies overrides.

A derived role definition has:
- **name** — unique identifier for this role (Identity spec §2, rule 2: opaque).
- **extends** — the base role this derives from. Must be one of: `worker`, `observer`. The coordinator role is not extensible — it is a singleton assigned by the runtime at initialization, and its capability set is already the ceiling. A derived coordinator would either be a reduced coordinator (undermining orchestration) or a renamed coordinator (no functional change). Neither is meaningful.
- **add** — capabilities granted beyond the base role.
- **remove** — capabilities revoked from the base role.
- **override** — properties that replace the base role's defaults.

Example — a `reviewer` derived from worker:
```yaml
roles:
  reviewer:
    extends: worker
    add:
      - send: report → coordinator
      - read: assigned_workspace
    remove:
      - send: query → coordinator
      - create: artifact
    override:
      checkpoint_types: [review]
```

Example — a `senior_worker` that can read peer workspaces:
```yaml
roles:
  senior_worker:
    extends: worker
    add:
      - read: peer_workspace
    override:
      checkpoint_types: [artifact, decision]
```

**Constraints:**

1. **Single-level only.** A derived role cannot itself be extended. `senior_worker` extends `worker`, but nothing can extend `senior_worker`. This prevents deep hierarchies and keeps permission resolution trivial — at most one lookup: check the derived role, fall back to its base.

2. **No privilege escalation.** A derived role cannot add capabilities that exceed its base role's ceiling. A worker derivative cannot gain `create_workspace`, `perform_integration`, or `manage_budgets` — these are coordinator-level actions. A derived role can narrow or redirect its base's capabilities, not transcend them. The one exception: delegation, which is granted by the coordinator at workspace creation, not through role inheritance.

3. **Registered before use.** All derived roles must be registered in the taxonomy (TAXONOMY.md) before they can be assigned to workspaces. An unregistered role name in a workspace creation request is rejected by the runtime.

4. **Resolution is deterministic.** When the runtime evaluates a permission for a derived role, it applies the overrides to the base role's capabilities in a fixed order: start with base, apply `remove`, then apply `add`. No ambiguity, no order-dependent surprises.

## 6. Permission Matrix

The permission matrix is the runtime's lookup table. Given a role and an action, it returns allow or deny. The matrix governs envelope routing, checkpoint creation, signal emission, and access control.

**Base envelope permissions:**

| Sender Role → Envelope Type | Eligible Receivers |
|-----------------------------|--------------------|
| Coordinator → `directive` | Worker |
| Coordinator → `feedback` | Worker |
| Worker → `query` | Coordinator |

This is the minimum matrix. Three roles, two envelope directions, three envelope types. Derived roles extend it — a `reviewer` adds `report → Coordinator`. Applications register additional envelope types through the taxonomy, each with its own send/receive row.

**Delegation extends the matrix at runtime.** When a workspace is granted `delegate`, the runtime adds:
- Delegate → `directive` → delegate's children
- Delegate → `feedback` → delegate's children
- Delegate's children → `query` → delegate

These entries are scoped to the delegate's subtree — they do not affect the global matrix.

**Signal permissions:**

| Role | Can emit |
|------|----------|
| Coordinator | `ready`, `started`, `failed` (lifecycle); `integrate`, `acknowledged` (role-specific) |
| Worker | `ready`, `started`, `blocked`, `checkpoint`, `complete`, `failed`, `escalation` |
| Observer | `ready`, `started`, `complete`, `failed`, `escalation` |

Lifecycle signals (`ready`, `started`, `failed`) are universal — every workspace emits them as part of the state machine. The coordinator does not emit `blocked` (never blocked), `checkpoint` (produces no checkpoints), `complete` (closure is governed by system lifecycle, not the `complete` signal), or `escalation` (activates the human highway directly through gates). Observers cannot emit `blocked` or `checkpoint` — they do not produce work that can be blocked or checkpointed.

**Checkpoint permissions:**

| Role | Can create |
|------|-----------|
| Coordinator | — |
| Worker | `artifact` |
| Observer | `observation` |

Derived roles override this — `reviewer` creates `review` instead of `artifact`. The taxonomy registers additional checkpoint types with their permitted roles.

**Access permissions:**

| Role | Read | Modify |
|------|------|--------|
| Coordinator | All workspaces, global trail | None directly |
| Worker | Own workspace, own directive, own local trail | Own workspace files |
| Observer | Designated workspaces (read-only), trail (configurable scope) | None |

Visibility grants (PROTOCOL §6.8) extend read access at runtime. Authority is frozen at creation. The access column in this table represents the base — the minimum before any grants.

**Evaluation rules:**

1. The runtime evaluates permissions at the moment of action.
2. For derived roles: start with base role's row, apply `remove`, then apply `add`.
3. If the action is not explicitly permitted, it is denied. There is no default-allow.
4. Every denial is recorded in the trail as a permission rejection event.

## 7. Trail Events

The permission system produces the following trail events:

**`envelope_rejected`** — emitted when the runtime denies an envelope send due to permission failure. The existing `envelope_rejected` trail event (PROTOCOL §9.5) covers this with `reason: permission_denied`.

**`checkpoint_rejected`** — emitted when the runtime denies a checkpoint creation because the role is not permitted to create that checkpoint type. Uses the existing `checkpoint_rejected` event (PROTOCOL §9.5) with `reason: permission_denied`.

**`workspace_created`** — the existing event (PROTOCOL §9.5) records the assigned role. When a delegate creates a child workspace, the `parent` field identifies the delegate. No new event type is needed — the standard `workspace_created` body captures delegation through the existing `parent` field.

**No new event types are required.** The permission system operates through existing trail events by adding `reason: permission_denied` to rejection events. This is deliberate — permissions are an enforcement layer over existing operations, not a separate event stream.

## 8. Conformance Requirements

| Requirement | Level | Description |
|-------------|-------|-------------|
| Three base roles | Core | Runtime MUST implement Coordinator, Worker, and Observer with the capability sets defined in §3. |
| Permission matrix enforcement | Core | Runtime MUST evaluate the permission matrix at the moment of action. Default-deny for any action not explicitly permitted. |
| Role assignment at creation | Core | Roles MUST be assigned at workspace creation and MUST NOT change during the workspace's lifetime. |
| Single coordinator | Core | Exactly one workspace per run MUST hold the Coordinator role. |
| Denial recording | Core | Every permission denial MUST produce a trail event. |
| Single-level inheritance | Standard | Runtime MUST support derived roles with add/remove/override against exactly one base role. |
| No privilege escalation | Standard | Runtime MUST reject derived role definitions that exceed the base role's capability ceiling. |
| Taxonomy registration | Standard | Runtime MUST reject workspace creation with an unregistered derived role. |
| Delegation capability | Standard | Runtime MUST support the `delegate` capability with budget inheritance, visibility containment, and authority containment as defined in §4. |
| Delegation depth control | Standard | Only the root coordinator MAY grant delegation. Delegates MUST NOT grant delegation to their children unless the coordinator explicitly permits it. |
| Deterministic resolution | Full | Permission resolution for derived roles MUST follow the fixed order: base → remove → add. |

## 9. Implementation Notes

These notes are non-normative. They capture practical guidance for implementers.

**Permission matrix as a lookup table.** The most straightforward implementation is a hash map keyed by `(sender_role, action_type, target_role)` returning `allow` or `deny`. For derived roles, compute the effective permission set at workspace creation (base → remove → add) and cache it — no need to resolve inheritance at every action.

**Delegation as scoped coordinator.** A delegate acts as a coordinator for its subtree but remains a worker in the global context. Implementers should model this as two permission sets: the workspace's role permissions (worker) for interactions with the root coordinator, and a scoped coordinator permission set for interactions with its children. The runtime selects the correct set based on the direction of the operation.

**Budget tracking for delegates.** When a delegate creates children, the runtime must maintain a running total of allocated child budgets. The simplest approach: `delegate.remaining = delegate.budget - delegate.consumed - sum(child.budget for each child)`. Reject child creation if the child's requested budget would make `remaining` negative. Track the delegate's subtree consumption as a single aggregate for budget warning and exceeded events.

**Reviewer as canonical derived role.** The first derived role most applications will register is `reviewer`. A reference definition:

```yaml
roles:
  reviewer:
    extends: worker
    add:
      - send: report → coordinator
      - read: assigned_workspace
    remove:
      - send: query → coordinator
      - create: artifact
    override:
      checkpoint_types: [review]
```

Implementations MAY pre-register `reviewer` as a built-in derived role for convenience, provided it follows the standard inheritance mechanism and is overridable through the taxonomy.

## 10. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §5.1–§5.6 | §1, §2, §3, §5, §6 | Role model, base roles, inheritance, permission matrix, delegation, port rights (defines this spec) |
| §5.1 | §2 | Assignment rules — coordinator role assigned by runtime at initialization |
| §6.4 | §4 | Authority frozen after workspace leaves `idle` — delegation grant is one-time |
| §6.5 | §4 | Workspace tree — child cannot outlive parent (delegate abort cascades) |
| §6.6 | §3, §4 | Resource budgets — coordinator manages budgets; delegate budget inheritance |
| §6.8 | §3, §6 | Dynamic visibility grants — extends read access at runtime |
| §8 | §3 | Human highway — coordinator configures |
| §9.5 | §7 | Event registry — `envelope_rejected`, `checkpoint_rejected`, `workspace_created` |

### Constituent Specs

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Identity spec | §2, rule 2 | §5 | Opaque identifiers — derived role names follow identity rules |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](../PROTOCOL.md) | Taxonomy: [TAXONOMY.md](../TAXONOMY.md)*
