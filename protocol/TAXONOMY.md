# WACP: Taxonomy

## Metadata

```yaml
title: "WACP: Taxonomy"
id: wacp-spec-taxonomy
type: constituent-spec
tier: abstract
category: foundations
status: complete
created: 2026-03-12
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §5.3 (role inheritance, derived roles)
  - §5.5 (permission matrix extensibility)
depends_on:
  - wacp-spec-roles
  - wacp-spec-envelope
  - wacp-spec-checkpoint
  - wacp-spec-identity
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, taxonomy, registry, derived-roles, envelope-types, checkpoint-types, extensibility]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [What the Taxonomy Contains](#2-what-the-taxonomy-contains)
3. [Derived Role Registry](#3-derived-role-registry)
4. [Envelope Type Registry](#4-envelope-type-registry)
5. [Checkpoint Type Registry](#5-checkpoint-type-registry)
6. [Registration Rules](#6-registration-rules)
7. [Validation](#7-validation)
8. [Canonical Derived Roles](#8-canonical-derived-roles)
9. [Taxonomy Lifecycle](#9-taxonomy-lifecycle)
10. [Conformance Requirements](#10-conformance-requirements)
11. [Implementation Notes](#11-implementation-notes)
12. [References](#12-references)

---

## 1. Purpose

The protocol defines base types — three roles (coordinator, worker, observer), three envelope types (directive, feedback, query), two checkpoint types (artifact, observation), and eleven signal types. These are the protocol's vocabulary. Applications need a larger vocabulary: reviewers, reports, decisions, analyses, and domain-specific checkpoint categories that the protocol cannot anticipate.

The taxonomy is the protocol's extension registry. It is where applications register derived roles, custom envelope types, and custom checkpoint types — extending the protocol's vocabulary without modifying the protocol itself. The taxonomy is loaded at initialization, validated for consistency, and consulted at runtime whenever the system encounters a type string it needs to verify.

Three design principles govern the taxonomy:

**Open where safe, closed where critical.** Envelope types and checkpoint types are open — applications register new ones through the taxonomy. Signal types are closed — they drive the state machine, and an unrecognized signal would have no defined effect. Roles are semi-open — derived roles extend base roles through single-level inheritance (Roles spec, §5), but the base roles and the coordinator singleton are fixed.

**Registration before use.** Every derived role, custom envelope type, and custom checkpoint type must be registered in the taxonomy before it can be used. An unregistered role name in a workspace creation request is rejected. An unregistered envelope type in a send operation is rejected. An unregistered checkpoint type in a creation operation is rejected. The runtime validates against the taxonomy — it is the source of truth for type validity.

**The taxonomy is a registry, not a schema.** It registers names, permissions, and constraints — not payload formats. An envelope type `report` is registered with its send/receive permissions, but its payload structure is application-defined. The taxonomy says "a reviewer can send a report to the coordinator." It does not say "a report must contain fields X, Y, Z." Payload validation is an application concern, not a protocol concern.

## 2. What the Taxonomy Contains

The taxonomy is a structured document — loaded by the runtime at initialization — containing three registries:

```yaml
taxonomy:
  id: string                      # unique taxonomy identifier
  version: string                 # taxonomy version (semantic versioning recommended)
  protocol_version: string        # the WACP protocol version this taxonomy targets
  roles: [role_definition]        # derived role registry (§3)
  envelope_types: [type_def]      # custom envelope type registry (§4)
  checkpoint_types: [type_def]    # custom checkpoint type registry (§5)
```

**Field semantics:**

- **`id`** — a unique identifier for this taxonomy. Different applications may define different taxonomies for the same protocol version.
- **`version`** — the taxonomy's own version. Taxonomy versions are independent of protocol versions — an application may update its taxonomy without a protocol version change.
- **`protocol_version`** — the WACP protocol version this taxonomy is designed for. The runtime validates compatibility at load time.
- **`roles`** — zero or more derived role definitions (§3).
- **`envelope_types`** — zero or more custom envelope type definitions (§4).
- **`checkpoint_types`** — zero or more custom checkpoint type definitions (§5).

**The empty taxonomy is valid.** An application that uses only the base roles, base envelope types, and base checkpoint types needs no taxonomy entries. The runtime operates with the protocol's built-in vocabulary. The taxonomy extends; it does not replace.

## 3. Derived Role Registry

Each derived role entry defines a role that extends one of the two extensible base roles (worker or observer). The coordinator role is not extensible (Roles spec, §5).

```yaml
role_definition:
  name: string                    # unique role name (Identity spec, rule 2: opaque)
  extends: enum
    - worker
    - observer
  add: [capability]               # capabilities granted beyond the base role
  remove: [capability]            # capabilities revoked from the base role
  override:                       # properties that replace the base role's defaults
    checkpoint_types: [string]    # checkpoint types this role may create
```

**Capability format:**

```yaml
capability:
  action: enum
    - send                        # envelope send permission
    - receive                     # envelope receive permission (informational — receive rights are inbox-level)
    - read                        # visibility extension
    - create                      # checkpoint type creation permission
  target: string                  # what the capability applies to:
                                  # for send/receive: "envelope_type → target_role"
                                  # for read: "assigned_workspace" | "peer_workspace" | scope identifier
                                  # for create: checkpoint type name
```

**Resolution order.** When the runtime evaluates a permission for a derived role, it applies overrides to the base role's capabilities in a fixed order: start with the base role, apply `remove`, then apply `add` (Roles spec, §5). No ambiguity, no order-dependent surprises.

**Constraints (enforced at registration):**

1. **Single-level only.** A derived role cannot itself be extended. The `extends` field must name a base role, not another derived role (Roles spec, §5).
2. **No privilege escalation.** A derived role cannot add capabilities that exceed its base role's ceiling. A worker derivative cannot gain `create_workspace`, `perform_integration`, or `manage_budgets` — these are coordinator-level actions (Roles spec, §5).
3. **Name uniqueness.** No two derived roles may share a name within the same taxonomy. Role names must also not collide with the three base role names (`coordinator`, `worker`, `observer`).
4. **Checkpoint type consistency.** If a derived role overrides `checkpoint_types`, every listed type must be registered in the checkpoint type registry (§5) or be a base type (`artifact`, `observation`).

## 4. Envelope Type Registry

Each custom envelope type entry defines a new envelope type with its send/receive permissions.

```yaml
envelope_type_definition:
  name: string                    # unique type name (must not collide with base types)
  description: string             # what this envelope type carries
  permissions:                    # who can send and receive
    - sender_role: string         # role name (base or derived) that may send this type
      receiver_role: string       # role name (base or derived) that may receive this type
```

**Base types are always available.** The three base envelope types (`directive`, `feedback`, `query`) are defined by the protocol and do not need taxonomy registration. They are always valid. Custom types are additive — they extend the permission matrix (Roles spec, §6) with new rows.

**Permission matrix extension.** Each custom envelope type adds one or more rows to the permission matrix. At initialization, the runtime builds the full matrix: base rows (from Roles spec, §6) plus taxonomy rows (from this registry). At every envelope send, the runtime checks the full matrix.

**Constraints (enforced at registration):**

1. **Name uniqueness.** No custom type may share a name with a base type (`directive`, `feedback`, `query`) or with another custom type in the same taxonomy.
2. **Role existence.** Every `sender_role` and `receiver_role` must be a base role or a derived role registered in the same taxonomy (§3).
3. **No empty permissions.** Every custom type must have at least one permission row — a type that no one can send or receive is useless.

## 5. Checkpoint Type Registry

Each custom checkpoint type entry defines a new checkpoint type with its role permissions.

```yaml
checkpoint_type_definition:
  name: string                    # unique type name (must not collide with base types)
  description: string             # what this checkpoint type represents
  permitted_roles: [string]       # roles that may create checkpoints of this type
  required_fields: [string]       # optional: additional payload fields required for this type
```

**Base types are always available.** The two base checkpoint types (`artifact`, `observation`) are defined by the protocol. Custom types are additive.

**Role-type binding.** The `permitted_roles` list determines which roles may create checkpoints of this type. At checkpoint creation, the runtime checks whether the workspace's role is in the permitted list for the checkpoint's type (Checkpoint spec, §4, rule 3).

**Constraints (enforced at registration):**

1. **Name uniqueness.** No custom type may share a name with a base type (`artifact`, `observation`) or with another custom type in the same taxonomy.
2. **Role existence.** Every role in `permitted_roles` must be a base role or a derived role registered in the same taxonomy (§3).
3. **At least one role.** Every custom type must permit at least one role to create it.

## 6. Registration Rules

Five rules govern taxonomy registration. These ensure the taxonomy is internally consistent and compatible with the protocol's base definitions.

**Rule 1: Taxonomy is loaded at initialization.** The runtime loads the taxonomy before any workspaces are created. The taxonomy is not modified during a run — it is a static configuration. If a different taxonomy is needed, a new run is started.

**Rule 2: Cross-registry consistency.** Derived roles that reference custom checkpoint types or custom envelope types must reference types that are registered in the same taxonomy. A derived role that adds `create: review` requires a checkpoint type `review` in the checkpoint type registry. A derived role that adds `send: report → coordinator` requires an envelope type `report` in the envelope type registry.

**Rule 3: Base definitions are not overridable.** The taxonomy cannot redefine base roles, base envelope types, base checkpoint types, or signal types. The three base roles are protocol-defined. The three base envelope types are protocol-defined. The two base checkpoint types are protocol-defined. The eleven signal types are protocol-defined and not extensible through the taxonomy.

**Rule 4: Names are opaque strings.** Consistent with the Identity spec (rule 2), taxonomy names are opaque — the protocol does not parse or derive meaning from them. The name `reviewer` has no protocol-level meaning beyond "a registered derived role." Semantic meaning is application-defined.

**Rule 5: Taxonomy versioning is independent.** The taxonomy has its own version number, independent of the protocol version. An application may evolve its taxonomy (adding new derived roles or envelope types) without changing the protocol version, as long as the new taxonomy is compatible with the declared `protocol_version`.

## 7. Validation

The runtime validates the taxonomy at load time. Validation failures prevent the run from starting — a malformed taxonomy is a configuration error, not a runtime error.

**Validation checks:**

| Check | What | Failure |
|-------|------|---------|
| Protocol compatibility | `taxonomy.protocol_version` matches the runtime's protocol version | Reject taxonomy |
| Name uniqueness (roles) | No duplicate role names, no collision with base roles | Reject taxonomy |
| Name uniqueness (envelope types) | No duplicate type names, no collision with base types | Reject taxonomy |
| Name uniqueness (checkpoint types) | No duplicate type names, no collision with base types | Reject taxonomy |
| Inheritance validity | Every `extends` field names a base role, not a derived role | Reject role |
| No privilege escalation | No derived role adds coordinator-level capabilities | Reject role |
| Cross-registry references | Checkpoint types in role overrides exist in the checkpoint type registry | Reject role |
| Envelope type role references | Sender/receiver roles exist (base or derived) | Reject envelope type |
| Checkpoint type role references | Permitted roles exist (base or derived) | Reject checkpoint type |
| Non-empty permissions | Every envelope type has at least one permission row | Reject envelope type |
| Non-empty role list | Every checkpoint type permits at least one role | Reject checkpoint type |

**Validation is all-or-nothing.** If any entry fails validation, the entire taxonomy is rejected. The runtime does not load a partial taxonomy — consistency requires that all cross-references resolve.

**Runtime type checking.** After the taxonomy is loaded, the runtime builds type lookup tables:

- **Role lookup:** `Map[role_name → effective_permission_set]` — base role plus resolved add/remove/override.
- **Envelope type lookup:** `Set[valid_type_name]` — base types plus registered custom types. Plus the permission matrix rows for each type.
- **Checkpoint type lookup:** `Map[type_name → Set[permitted_role_name]]` — base types plus registered custom types with their role bindings.

These tables are consulted at every workspace creation (role validation), envelope send (type validation + permission check), and checkpoint creation (type validation + role check). Lookup is O(1) per operation.

## 8. Canonical Derived Roles

The protocol does not require any derived roles. But one role appears so frequently in the spec documentation — as the canonical example of role extension — that it warrants a standard definition: the reviewer.

### 8.1 Reviewer

A reviewer is a worker that evaluates another workspace's output rather than producing original artifacts.

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

**What a reviewer does:**

- Receives a directive from the coordinator pointing to a workspace whose checkpoint should be evaluated.
- Reads the target workspace's checkpoint (via visibility grant — the `read: assigned_workspace` capability in `add`).
- Produces a `review` checkpoint containing the evaluation.
- Sends a `report` envelope to the coordinator with the evaluation summary.

**What a reviewer cannot do (removed from worker):**

- Send `query` envelopes to the coordinator — reviewers report, they don't ask questions.
- Create `artifact` checkpoints — reviewers evaluate, they don't produce.

**Dependent registrations.** The `reviewer` role requires two companion entries in the taxonomy:

1. **Envelope type `report`:**
```yaml
envelope_types:
  - name: report
    description: Evaluation results from a reviewer to the coordinator
    permissions:
      - sender_role: reviewer
        receiver_role: coordinator
```

2. **Checkpoint type `review`:**
```yaml
checkpoint_types:
  - name: review
    description: Evaluation of another workspace's checkpoint
    permitted_roles: [reviewer]
```

**The reviewer is a recommendation, not a requirement.** Implementations may pre-register the reviewer as a convenience, but applications may define their own reviewer variant or omit it entirely. The protocol prescribes the extension mechanism; the reviewer demonstrates it.

### 8.2 Other Common Derived Roles

These are patterns observed across the protocol's examples. They are suggestions for application designers, not protocol requirements.

**Senior Worker.** A worker with cross-peer visibility and additional checkpoint types.

```yaml
roles:
  senior_worker:
    extends: worker
    add:
      - read: peer_workspace
    override:
      checkpoint_types: [artifact, decision]
```

Requires checkpoint type `decision` in the checkpoint type registry.

**Auditor.** An observer that monitors specific workspace groups.

```yaml
roles:
  auditor:
    extends: observer
    add:
      - read: designated_group
    override:
      checkpoint_types: [observation, audit_finding]
```

Requires checkpoint type `audit_finding` in the checkpoint type registry.

## 9. Taxonomy Lifecycle

The taxonomy is static within a run but evolves across runs.

**Within a run:**

1. The taxonomy is loaded at initialization.
2. Validation runs (§7). If validation fails, the run does not start.
3. The runtime builds type lookup tables from the validated taxonomy.
4. The taxonomy is immutable for the duration of the run. No types can be added, modified, or removed.
5. Every type validation — role assignment, envelope send, checkpoint creation — checks against the loaded taxonomy.

**Across runs:**

1. Between runs, the application may modify the taxonomy: add derived roles, add envelope types, add checkpoint types, or modify existing entries.
2. The new taxonomy is loaded at the next run's initialization.
3. If the protocol version has changed, the taxonomy's `protocol_version` field must be updated. The runtime validates compatibility.

**Taxonomy and trail.** The taxonomy's contents are not recorded entry-by-entry in the trail. The taxonomy is a configuration artifact — its version identifier is recorded in the first trail entry of each run, enabling post-hoc analysis to determine which taxonomy was active. Trail queries that encounter an unrecognized type name consult the taxonomy version recorded at run start.

**Backward compatibility.** Removing a derived role or type from the taxonomy means that trails from prior runs may reference types that no longer exist. This is not an error — the trail is a historical record. Implementations should handle unrecognized type names in trail queries gracefully (display the raw name rather than rejecting the query).

## 10. Conformance Requirements

| Requirement | Level | Description |
|-------------|-------|-------------|
| Taxonomy loading | Core | Runtime MUST load the taxonomy at initialization, before workspace creation. |
| Taxonomy validation | Core | Runtime MUST validate the taxonomy per the checks in §7. Invalid taxonomies MUST prevent the run from starting. |
| Registration before use | Core | Runtime MUST reject workspace creation with an unregistered role, envelope sends with an unregistered type, and checkpoint creation with an unregistered type. |
| Base types always valid | Core | The three base roles, three base envelope types, and two base checkpoint types MUST be valid without taxonomy registration. |
| Signal types not extensible | Core | The taxonomy MUST NOT register new signal types. The runtime MUST reject taxonomies that attempt to extend the signal set. |
| Derived role resolution | Core | Derived role permissions MUST be resolved in the fixed order: base → remove → add (Roles spec, §5). |
| Single-level inheritance | Core | Derived roles MUST extend a base role, not another derived role. The runtime MUST reject taxonomies with multi-level inheritance. |
| No privilege escalation | Core | Derived roles MUST NOT add coordinator-level capabilities. The runtime MUST reject such definitions. |
| Name uniqueness | Core | All names within each registry MUST be unique. No custom name may collide with a base name. |
| Cross-registry consistency | Standard | Derived role references to custom types MUST resolve within the same taxonomy. |
| Immutable during run | Standard | The taxonomy MUST NOT change during a run. |
| Taxonomy version in trail | Standard | The taxonomy identifier and version MUST be recorded in the first trail entry of each run. |
| Envelope permission matrix extension | Standard | Custom envelope types MUST extend the permission matrix with their registered permission rows. |
| Checkpoint role validation | Standard | Checkpoint creation MUST be validated against the `permitted_roles` list for the checkpoint's type. |
| Empty taxonomy valid | Full | A run with no taxonomy (or an empty taxonomy) MUST operate using only base types. |
| Backward-compatible trail queries | Full | Unrecognized type names in trail queries from prior runs SHOULD be handled gracefully. |

## 11. Implementation Notes

These notes are non-normative. They capture practical guidance for implementers.

**Taxonomy as a YAML/JSON file.** The simplest implementation is a YAML or JSON file loaded at startup. The file contains the three registries. Validation is a one-time pass over the file at load time. The validated entries are converted to in-memory lookup tables.

**Type lookup as hash sets.** For envelope type validation, a hash set of valid type names (base types + registered custom types) provides O(1) lookup per envelope send. For checkpoint type validation, a hash map from type name to permitted role set provides O(1) lookup per checkpoint creation. For role validation, a hash map from role name to effective permission set provides O(1) lookup per workspace creation.

**Permission matrix as a flat table.** The base permission matrix (Roles spec, §6) plus taxonomy-registered rows form a single table: `(sender_role, type, receiver_role) → allow`. Build this table at initialization from the base matrix plus the taxonomy's envelope type permissions. Lookup at send time is a single hash map query.

**Taxonomy versioning with checksums.** To detect accidental taxonomy changes between runs, implementations may compute a checksum of the taxonomy file and record it alongside the version identifier in the trail. A checksum mismatch between the declared version and the computed hash indicates an undeclared change — a configuration management issue, not a protocol error.

**Pre-registering the reviewer.** Implementations that pre-register the `reviewer` role (§8.1) should do so as part of the default taxonomy, not as a protocol built-in. The reviewer follows the standard registration mechanism — it is overridable through the taxonomy. An application that registers its own `reviewer` with different capabilities replaces the default.

**Taxonomy migration.** When an application evolves its taxonomy between runs, existing trails may reference types from the old taxonomy. Implementations should maintain a taxonomy archive — a mapping from taxonomy version to taxonomy content — so that historical trail queries can resolve type names from any prior run. This is an operational concern, not a protocol requirement.

## 12. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §5.3 | §1, §3 | Role inheritance — derived roles and taxonomy registration |
| §5.5 | §1, §4 | Permission matrix — extensibility through taxonomy-registered types |

### Constituent Specs

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Roles spec | §3, §5, §6 | §1, §3, §4, §7 | Base roles, single-level inheritance, permission matrix, no privilege escalation |
| Envelope spec | §2, §4 | §1, §4, §7 | Envelope type validation, open type system |
| Checkpoint spec | §2, §4 | §1, §5, §7 | Checkpoint type validation, role-type compatibility |
| Signal spec | §2 | §1, §6 | Closed signal set — not extensible through taxonomy |
| Identity spec | §2 | §3, §6 | Opaque identifiers — role and type names follow identity rules |
| Trail spec | §9 | §9 | Event registry — taxonomy version recorded in trail |

---

*WACP constituent specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](PROTOCOL.md) | Taxonomy: this document*
