# WACP — Workspace Agent Coordination Protocol

[![License: CC BY-SA 4.0](https://img.shields.io/badge/License-CC%20BY--SA%204.0-lightgrey.svg)](https://creativecommons.org/licenses/by-sa/4.0/)

**A formal protocol specification for coordinating autonomous agents in distributed, fault-tolerant systems.**

WACP defines the rules by which autonomous agents — particularly AI agents — coordinate work. It specifies what agents say, how they communicate, and the structures they operate within. It does not specify how the underlying system schedules, allocates, or manages resources; that is the domain of the operating system layer.

---

## Table of Contents

- [Overview](#overview)
- [The Five Questions](#the-five-questions)
- [Design Principles](#design-principles)
- [Core Primitives](#core-primitives)
- [Roles and Permissions](#roles-and-permissions)
- [Mechanisms](#mechanisms)
- [Topology](#topology)
- [Taxonomy (Extension Registry)](#taxonomy-extension-registry)
- [Repository Structure](#repository-structure)
- [Reading Guide](#reading-guide)
- [Status](#status)
- [Authors](#authors)
- [License](#license)

---

## Overview

Modern AI systems increasingly require multiple agents working together — decomposing problems, producing artifacts in parallel, reviewing each other's output, and integrating results. WACP provides the coordination surface for this: the contracts between agents, between agents and the coordinator, and between all parties and the audit record.

WACP is a **protocol specification**, not an implementation. It is deliberately transport-agnostic — implementable over files, HTTP, pipes, message queues, or anything else. The coordination model is independent of how messages physically move.

Key properties:

- **Isolation through structure.** Workspaces are hard boundaries; communication is explicit.
- **Immutability by default.** Checkpoints, trails, and closed workspaces cannot be modified.
- **Capability-based security.** The runtime enforces permissions; agents cannot grant themselves rights.
- **Human oversight is architectural.** Humans can observe, approve, inject, and escalate — without stopping the protocol.
- **Full auditability.** Every event produces exactly one trail entry. No gaps. No inference required.

---

## The Five Questions

WACP is organized around five fundamental coordination questions:

| Question | Answer | Primitive |
|---|---|---|
| **Where** does an agent work? | Workspaces — isolated, bounded execution contexts | `workspace` |
| **How** do agents communicate? | Envelopes and signals — structured messages and typed notifications | `envelope`, `signal` |
| **How** is progress recorded? | Checkpoints — immutable snapshots of work | `checkpoint` |
| **How** is work organized? | Tasks — units of work forming dependency graphs | `task` |
| **What** happened? | The trail — append-only audit log | `trail` |

---

## Design Principles

Every design decision in WACP traces back to at least one of these principles:

1. **Messages over mutations.** Agents never modify shared state directly. All coordination happens through explicit, typed messages.
2. **Roles are structural, not suggested.** Permissions are walls, not guidelines. Enforced by the runtime.
3. **Explicit lifecycle, no inference.** Every state transition is declared. If the runtime does not know an agent's state, the agent has not declared it.
4. **Context is scoped, not shared.** Each workspace has a defined boundary of visibility. Least privilege, applied to attention.
5. **History is first-class.** The trail is not a log to be parsed — it is a structured record with typed entries.
6. **Protocol over tooling.** WACP defines a protocol, not an application. Transport-agnostic by design.
7. **Human access is architectural.** Autonomy is a spectrum configured per workflow, not a binary switch.
8. **Ordering requires a clock.** Trail integrity, signal ordering, timeouts, and replay all depend on a well-defined logical clock.

---

## Core Primitives

### Workspace

The unit of isolation. A bounded context assigned to exactly one agent, containing everything that agent can see and act on. Workspaces form a tree rooted at the coordinator. Each workspace has a linear lifecycle with 9 states (`idle` through `closed`), resource budgets, a visibility set (what it can read), and an authority set (what it can do). Once closed, a workspace is immutable — revision requires creating a new one.

### Envelope

The unit of communication. A structured message addressed to a specific workspace. Three base types: `directive` (coordinator to worker), `feedback` (coordinator to worker), and `query` (worker to coordinator). Envelopes carry priority levels, payload flexibility, and reply-to threading. Extensible through the taxonomy.

### Signal

The unit of notification. A lightweight, typed event that an agent emits to declare a state change. There are exactly 11 signal types (`ready`, `started`, `blocked`, `checkpoint`, `complete`, `failed`, `escalation`, `migrated`, `suspended`, `resumed`, `cancelled`). The signal set is **closed** — it drives the state machine and cannot be extended.

### Checkpoint

The unit of progress. An immutable snapshot of work product. Two base types: `artifact` (produced output) and `observation` (noticed information). Checkpoints carry intent, confidence level, and status. They form a linear chain within each workspace — revisions create new checkpoints referencing the previous one.

### Task

The unit of work. A structured assignment with explicit dependencies, forming a directed acyclic graph (DAG). Tasks support decomposition of goals into executable units with priority and resource estimates. One-to-many relationship with workspaces (a task can be retried in multiple workspaces).

### Trail

The unit of history. An append-only, immutable, timestamped record of every protocol event. Hash-chained for tamper evidence. Two scopes: local (per workspace) and global (system-wide). The trail is the single source of truth — recovery, observability, and security all derive from it.

### Identity

The unit of uniqueness. Defines how identifiers are generated, scoped, and validated across the protocol. Identifiers are opaque, globally unique, and never reused.

### User

The unit of human identity. Defines how humans are represented in the protocol — their identity, hierarchy, and relationship to workspaces. Every workspace has an owner (the human on whose behalf it exists) and an originator (the human or system that caused its creation).

---

## Roles and Permissions

WACP defines three base roles:

| Role | Purpose | Key capabilities |
|---|---|---|
| **Coordinator** | Orchestrates work | Creates workspaces, dispatches directives, evaluates results, integrates output |
| **Worker** | Produces output | Receives directives, creates checkpoints, emits signals, sends queries |
| **Observer** | Monitors activity | Reads trails and checkpoints, cannot send envelopes or create checkpoints |

Roles are extensible through **single-level inheritance**. A derived role (e.g., `reviewer`) extends exactly one base role, inheriting its permissions and applying overrides. Derived roles are registered in the taxonomy before use.

Permissions are enforced across five dimensions: **send**, **receive**, **emit**, **create**, and **access**. The permission matrix is enforced at runtime — it is capability-based, not advisory.

---

## Mechanisms

### Integration

The coordinator's deliberate operation to merge completed workspace output into the parent. Three strategies: `direct` (copy as-is), `layered` (overlap detection), and `evaluated` (full coordination with conflict resolution). Supports salvage integration for recovering partial work from failed workspaces.

### Recovery

Fault tolerance through replay. The trail's immutability and completeness enable deterministic recovery — replay trail entries to restore any workspace to its last known good state. Handles workspace failures, message loss, coordinator failures, and cascade failures.

### Human Highway

The explicit protocol path for human oversight. Humans can inject directives, resolve conflicts, approve task transitions, and make decisions at gates. Every human action is recorded in the trail. Gates — checkpoints requiring human decision — are first-class protocol elements.

### Security

Cryptographic guarantees for protocol integrity. Hash-chained trails provide tamper evidence. Capability-based access control prevents privilege escalation. The security model assumes workers are potentially untrusted and enforces boundaries that survive compromised agents.

---

## Topology

The topology layer defines the structural relationships between protocol objects:

| Structure | Spec | Description |
|---|---|---|
| **Workspace tree** | `tree.md` | Parent-child hierarchy of workspaces |
| **Task graph** | `graph.md` | DAG of task dependencies |
| **Causal ordering** | `causation.md` | Happens-before relationships between events |
| **Channels** | `channels.md` | Message-passing pathways between workspaces |
| **Ownership** | `ownership.md` | Which humans own which workspaces |
| **Visibility** | `visibility.md` | What each workspace can read |

---

## Taxonomy (Extension Registry)

The taxonomy (`TAXONOMY.md`) is the protocol's extension mechanism. It registers:

- **Derived roles** — application-specific roles that inherit from base roles (e.g., `reviewer` extends `worker`)
- **Custom envelope types** — domain-specific message types beyond the three base types (e.g., `report`, `review`)
- **Custom checkpoint types** — domain-specific output types beyond `artifact` and `observation` (e.g., `decision`, `analysis`)

The taxonomy follows three rules: **open where safe, closed where critical** (envelopes and checkpoints are extensible; signals are not); **registration before use** (unregistered types are rejected at runtime); and **registry, not schema** (it registers names and permissions, not payload formats).

---

## Repository Structure

```
wacp/
├── PROTOCOL.md                  # Authoritative protocol specification
├── TAXONOMY.md                  # Extension registry for derived types
├── README.md                    # This file
├── LICENSE                      # CC BY-SA 4.0
│
├── primitives/                  # Core data structures
│   ├── workspace.md             #   Execution containers and isolation
│   ├── envelope.md              #   Structured messages
│   ├── signal.md                #   Lightweight state notifications
│   ├── checkpoint.md            #   Immutable work products
│   ├── task.md                  #   Work units and DAG structure
│   ├── trail.md                 #   Audit log and recovery log
│   ├── identity.md              #   Identifier uniqueness and opaqueness
│   └── user.md                  #   Human identity and user hierarchy
│
├── foundations/                  # Baseline concepts
│   ├── clock.md                 #   Monotonic logical time
│   └── roles.md                 #   Authorization and permission matrix
│
├── mechanisms/                  # Operations
│   ├── integration.md           #   Assembly and merge operations
│   ├── recovery.md              #   Fault tolerance and repair
│   ├── human-highway.md         #   Human oversight integration
│   └── security.md              #   Cryptographic guarantees
│
└── topology/                    # Structural relationships
    ├── tree.md                  #   Workspace hierarchy
    ├── graph.md                 #   Task graph structure
    ├── causation.md             #   Causal ordering
    ├── channels.md              #   Message passing
    ├── ownership.md             #   Workspace ownership
    └── visibility.md            #   Data visibility model
```

---

## Reading Guide

**If you want the full picture**, start with `PROTOCOL.md`. It is the authoritative specification — approximately 70KB covering all primitives, roles, lifecycle states, and integration procedures in a single document.

**If you want to understand a specific concept**, go directly to the relevant constituent spec in `specs/`. Each spec is self-contained with its own rules, examples, and conformance requirements.

**Recommended reading order for newcomers:**

1. `PROTOCOL.md` §1–3 — Scope, vocabulary, and design principles
2. `specs/primitives/workspace.md` — The foundational abstraction
3. `specs/primitives/envelope.md` — How agents communicate
4. `specs/primitives/signal.md` — How state changes propagate
5. `specs/primitives/checkpoint.md` — How progress is recorded
6. `specs/primitives/task.md` — How work is organized
7. `specs/primitives/trail.md` — How history is preserved
8. `specs/foundations/roles.md` — Who can do what
9. `specs/mechanisms/integration.md` — How results are assembled
10. `specs/mechanisms/human-highway.md` — How humans participate
11. `TAXONOMY.md` — How the protocol is extended

**If you want to implement WACP**, the conformance requirements in each spec define the minimum a runtime must enforce. Start with the workspace lifecycle state machine and the permission matrix.

---

## Status

WACP v0.1 is **complete**. The protocol specification and all 20 constituent specs are published.

| Component | Status | Document |
|---|---|---|
| Protocol specification | Complete | `PROTOCOL.md` |
| Taxonomy | Complete | `TAXONOMY.md` |
| Primitives (8 specs) | Complete | `specs/primitives/` |
| Foundations (2 specs) | Complete | `specs/foundations/` |
| Mechanisms (4 specs) | Complete | `specs/mechanisms/` |
| Topology (6 specs) | Complete | `specs/topology/` |

---

## Authors

- **Akil Abderrahim** — Lead
- **Claude Opus 4.6** — Co-author

---

## License

This work is licensed under [Creative Commons Attribution-ShareAlike 4.0 International (CC BY-SA 4.0)](https://creativecommons.org/licenses/by-sa/4.0/).

You are free to share and adapt this material for any purpose, including commercially, provided you give appropriate credit and distribute contributions under the same license.
