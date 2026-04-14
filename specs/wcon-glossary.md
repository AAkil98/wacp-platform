---
id: wcon-glossary
type: design
status: final
created: 2026-04-09T00:00:00
revised: 2026-04-14T00:00:00
authors: [AAkil98]
tags: [glossary, terminology, foundation]
---

# WACP Console Glossary

## Table of Contents

1. Protocol Primitives (Inherited)
2. Protocol Roles (Inherited)
3. Protocol Mechanisms (Inherited)
4. Ecosystem Concepts (Inherited)
5. Console-Native Concepts
6. Console UI Concepts
7. Console Internal Concepts
8. Flagged Ambiguities

---

## 1. Protocol Primitives (Inherited)

These terms are inherited from WACP (`wacp-protocol`) with identical definitions. The Console does not redefine them — it consumes them through the gRPC API.

### workspace

An isolated execution container for a single agent performing a unit of work. Holds the agent's context (directive, checkpoints, local trail), resource budget, and state. Each workspace has exactly one role and executes exactly one task.

- **Category:** inherited
- **Aliases to avoid:** container, sandbox, environment
- **Relationships:** workspace belongs to workspace tree; workspace executes one task; workspace has one role
- **Source:** `wacp-protocol` — primitives/workspace

### envelope

The protocol's communication primitive carrying structured content between workspaces. Typed payload (directive, feedback, query, or taxonomy-registered types), explicit sender and receiver, validated against the permission matrix before delivery.

- **Category:** inherited
- **Aliases to avoid:** message, packet, request
- **Relationships:** envelope travels between workspaces; envelope carries payload; envelope requires port rights
- **Source:** `wacp-protocol` — primitives/envelope

### signal

The protocol's notification primitive carrying state, not content. Fixed-type (eleven base types), propagates upward through the workspace tree, drives workspace state transitions and task lifecycle changes.

- **Category:** inherited
- **Aliases to avoid:** event, notification, status update
- **Relationships:** signal originates from workspace; signal drives task state; signal propagates to parent
- **Source:** `wacp-protocol` — primitives/signal

### checkpoint

The protocol's progress primitive — an immutable record of what an agent has produced (artifact or observation), captured at a specific moment with explicit intent and confidence level. Forms a linear, append-only chain within each workspace.

- **Category:** inherited
- **Aliases to avoid:** output, result, snapshot
- **Relationships:** checkpoint belongs to workspace; checkpoint forms chain with parent checkpoint
- **Source:** `wacp-protocol` — primitives/checkpoint

### trail

The complete, ordered, immutable record of everything that happened. Append-only, queryable by workspace/actor/event type/time. Single source of truth for recovery, observability, and security audits. Hash-chained for tamper evidence.

- **Category:** inherited
- **Aliases to avoid:** log, audit log, history, event stream
- **Relationships:** trail contains trail entries; every protocol event produces exactly one trail entry
- **Source:** `wacp-protocol` — primitives/trail

### task

A structured unit of work with explicit dependencies, resource estimates, and lifecycle state. A node in a directed acyclic graph (DAG) representing the work plan. A task has a one-to-many relationship to workspaces across time (retries).

- **Category:** inherited
- **Aliases to avoid:** job, work item, ticket
- **Relationships:** task forms DAG with other tasks; task is executed by workspace; task belongs to task graph
- **Source:** `wacp-protocol` — primitives/task

### port

A directed send permission between workspaces (sender to receiver). Types: `send` (persistent) and `send_once` (consumed after one use). No port right means no delivery — this is a core protocol invariant.

- **Category:** inherited
- **Aliases to avoid:** channel, connection, link
- **Relationships:** port connects two workspaces directionally; port enables envelope delivery
- **Source:** `wacp-protocol` — topology/channels

## 2. Protocol Roles (Inherited)

### coordinator

The root agent. Exactly one per run. Decomposes goals into task graphs, delegates work to workers, evaluates results, performs integration. Sees across all workspaces and the global trail. Modifies workspaces only through envelopes and protocol operations.

- **Category:** inherited
- **Aliases to avoid:** orchestrator, manager, master
- **Relationships:** coordinator is root of workspace tree; coordinator creates worker workspaces; coordinator owns task graph
- **Source:** `wacp-protocol` — foundations/roles

### worker

The producing agent. Receives a directive, works within its isolated workspace, declares progress through checkpoints and signals. Knows its task and nothing else unless granted additional capabilities.

- **Category:** inherited
- **Aliases to avoid:** agent (too generic), executor, node
- **Relationships:** worker operates within one workspace; worker receives directives from coordinator
- **Source:** `wacp-protocol` — foundations/roles

### observer

The monitoring agent. Read-only access to designated workspaces and the trail. Produces no artifacts but may record observations. Foundation for dashboards, metrics, and human-facing interfaces.

- **Category:** inherited
- **Aliases to avoid:** watcher, monitor, viewer
- **Relationships:** observer reads designated workspaces; observer reads trail
- **Source:** `wacp-protocol` — foundations/roles

### derived role

A role that extends exactly one base role (worker or observer, never coordinator) with single-level inheritance. Defined in the taxonomy with add/remove/override capability modifiers. Cannot exceed the base role's permission ceiling.

- **Category:** inherited
- **Aliases to avoid:** custom role, extended role, sub-role
- **Relationships:** derived role inherits from one base role; derived role is registered in taxonomy
- **Source:** `wacp-protocol` — TAXONOMY

## 3. Protocol Mechanisms (Inherited)

### directive

A base envelope type sent by the coordinator to a worker containing a task assignment. The binding between a task's description and a workspace's execution.

- **Category:** inherited
- **Aliases to avoid:** instruction, command, assignment
- **Relationships:** directive is an envelope type; directive references a task; directive targets a workspace
- **Source:** `wacp-protocol` — primitives/envelope

### gate

A synchronous control point where the human highway pauses a transition, waits for human input, and resumes or cancels based on the response. Six types: task approval, workspace creation, envelope delivery, integration, conflict resolution, workspace abort. Independently configurable per workflow with timeout fallback behavior.

- **Category:** inherited
- **Aliases to avoid:** approval, blocker, review point
- **Relationships:** gate is managed by highway; gate pauses a protocol transition; gate requires human resolution
- **Source:** `wacp-protocol` — mechanisms/human-highway

### escalation

A signal emitted when an agent needs human input to proceed. Dual delivery: propagates to parent workspace AND activates the human highway. Requires a reason field.

- **Category:** inherited
- **Aliases to avoid:** alert, interrupt, help request
- **Relationships:** escalation is a signal type; escalation activates highway; escalation blocks workspace
- **Source:** `wacp-protocol` — primitives/signal

### highway

The protocol's human oversight mechanism providing four capabilities: visibility (monitoring), gates (approval points), injection (sending directives to workspaces), and escalation handling (responding to agent escalations). Independently configurable per workflow.

- **Category:** inherited
- **Aliases to avoid:** oversight layer, human loop, approval system
- **Relationships:** highway manages gates; highway receives escalations; highway enables injection
- **Source:** `wacp-protocol` — mechanisms/human-highway

### taxonomy

The protocol's extension registry. Applications register derived roles, custom envelope types, and custom checkpoint types — extending the protocol's vocabulary without modifying the protocol itself. Loaded at runtime initialization, immutable within a run.

- **Category:** inherited
- **Aliases to avoid:** registry, schema, type system
- **Relationships:** taxonomy contains derived roles, custom envelope types, custom checkpoint types; taxonomy is loaded by runtime
- **Source:** `wacp-protocol` — TAXONOMY

## 4. Ecosystem Concepts (Inherited)

### vertical

A domain parameterization of WACP defining roles, task types, tools, default agent profiles, workflows, quality criteria, gate policies, a context schema, tool-layer policies, and domain-specific checkpoint types. A vertical defines domain behavior (what roles, tasks, and enforcement rules exist in that domain), not protocol behavior. Each vertical has a **defining constraint** — the distinctive enforcement rule that characterizes its domain.

The ecosystem currently ships seven verticals:

| Vertical | Defining constraint |
|----------|---------------------|
| `swe` | DAG validation — plan→implement→test→review with stage dependencies |
| `devops` | Environment-scoped blast radius (dev / staging / production gating) |
| `mlops` | Compute-budget gating + reproducibility checkpoints |
| `finance` | Regulatory pre-check + fiduciary duty (compliance_check precedes trade_execute) |
| `healthcare` | HIPAA PHI access grant (consent or de-identification basis) |
| `analytics` | SQL safety + query reproducibility (data snapshot pinning) |
| `datasci` | Pre-declared hypotheses + statistical rigor |

- **Category:** inherited
- **Aliases to avoid:** domain, module, plugin, extension
- **Relationships:** vertical contains roles, task types, tools, default profiles, workflows, context schema, tool policies, checkpoint types, quality criteria; vertical parameterizes the runtime for a domain
- **Source:** `wacp-ecosystem` (packages per vertical + generated `vertical.yaml` manifests served via `GET /v1/verticals[/{id}]`)

### defining constraint

The one-sentence characterization of a vertical's distinctive enforcement rule. Stored in `VerticalManifest.defining_constraint` (upstream) / `VerticalEntry.defining_constraint` (Console index). Surfaced in: vertical cards in the discovery browser and session launcher step 1, the vertical detail header, gate rationale enrichment, and refusal panel context.

The defining constraint is not a machine-parseable predicate — it is human-readable prose intended to answer the question "why is this vertical different from the others?" See `wcon-discovery` §2.2.2, `wcon-ui` §4.5.

- **Category:** native
- **Aliases to avoid:** domain constraint (too generic), vertical description, vertical summary
- **Relationships:** defining constraint is one field of a vertical manifest; it characterizes the vertical's enforcement surface
- **Source:** this project (term coined here; field sourced from upstream)

### autonomy

A spectrum level governing whether an agent runs without human intervention or pauses at gates for human approval. In WACP, autonomy applies at the profile level and determines gate activation. The Console uses three presets: `autonomous` (no gates), `assisted` (selective gates), `supervised` (all gates). Configured per profile, enforced by the highway.

In the upstream ecosystem manifest, per-role default profiles declare autonomy as `gated` (equivalent to assisted/supervised) or `autonomous`. The Console's three-level enum is a superset — a Console profile specifies a preset, which maps to the runtime's gate policy.

Observer-based autonomous roles (`finance:auditor`, `health:compliance`, `analytics:validator`, `datasci:reviewer`, `devops:monitor`, `mlops:evaluator`, `swe:reviewer`) are a common pattern across verticals: read-only background review with no gating. These are not a red flag — they are the designed mechanism for quality observation, and `wcon-profiles` §3.3 does not warn on them.

- **Category:** inherited
- **Aliases to avoid:** independence, freedom, control level
- **Relationships:** autonomy is set per profile; autonomy determines gate activation; autonomy is enforced by highway
- **Source:** `wacp-ecosystem` (per-vertical `profiles[].autonomy`); `wacp-local` — session

### task type

A declarative descriptor of one kind of work a vertical supports. Contains `id` (namespaced, e.g., `finance:trade`), `name`, `description`, `workflow_id` (the default workflow for this task type), and `keywords` (representative terms for search and CLI task-type detection — not full regex). The task type is the mapping between "what the user wants to do" and "which workflow runs it."

Each vertical ships its own set of task types. SWE has seven (implement, refactor, debug, etc.); Finance has nine (trade, rebalance, onboard, compliance check, etc.); Healthcare has eight; and so on. Surfaced in the vertical detail view (`wcon-ui` §4.5) and in global search (`wcon-discovery` §5).

- **Category:** inherited (from `wacp-ecosystem`, surfaced in the Console)
- **Aliases to avoid:** task template, task kind, workflow
- **Relationships:** task type references one workflow; task type belongs to one vertical; task type has detection keywords
- **Source:** `wacp-ecosystem` (per-vertical `task_types[]`)

### quality criterion

One weighted dimension in a vertical's quality rubric. Contains `id`, `name`, `description`, and `weight` (1.0 = equal weight). Each vertical ships its own rubric — typically six criteria at weight 1. Example for Finance: `regulatory_compliance`, `audit_trail_integrity`, `fiduciary_duty`, `risk_disclosure`, `data_provenance`, `documentation`.

Used by autonomous observer agents (e.g., Finance `auditor`, Healthcare `compliance`) to produce a *quality report* at session end — a trail entry assigning a pass/warn/fail verdict per criterion. The Console renders the report in the oversight dashboard when the session reaches a terminal state (`wcon-ui` §7.2).

- **Category:** inherited
- **Aliases to avoid:** quality dimension, rubric item, evaluation criterion
- **Relationships:** quality criterion belongs to a vertical; quality criterion is evaluated by an observer agent; quality criteria together form a quality report
- **Source:** `wacp-ecosystem` (per-vertical `quality_criteria[]`)

### quality report

A trail entry emitted at session end that assigns a pass/warn/fail verdict to each of a vertical's quality criteria. Produced by the vertical's autonomous observer agent, not the Console. Rendered in the oversight dashboard as an end-of-session panel (`wcon-ui` §7.2).

- **Category:** inherited
- **Aliases to avoid:** quality summary, final report, evaluation
- **Relationships:** quality report contains per-criterion verdicts; quality report is the output of a quality criterion evaluation; quality report is rendered by the oversight dashboard
- **Source:** `wacp-ecosystem` (quality evaluator functions in each vertical's `quality.ts`)

### workspace context tag

A domain-specific value attached to a workspace at dispatch time, declared by a vertical's `context_schema`. Examples: `environment` (DevOps, enum: dev/staging/production), `compute_budget` (MLOps, number), `data_snapshot_id` (Analytics, string), `phi_access_basis` (Healthcare, enum: consent/de_identified), `compliance_scope` + `jurisdiction` (Finance), `hypothesis_framework` (DataSci).

Context tags are not budgets, not profiles, not workflow stages — they are first-class configuration the user supplies at session launch (`wcon-sessions` §2.1 step 4). They are stored in `sessions.context` (JSON), delivered to the runtime as workspace metadata at dispatch time and as a `context` field in the directive payload (`wcon-profiles` §4.2), and read by tool-layer policies for enforcement.

- **Category:** native (term coined to name a concept the upstream manifest formalizes as `context_schema`)
- **Aliases to avoid:** workspace parameter, session variable, configuration
- **Relationships:** workspace context tag is declared by a vertical's context_schema; context tag is supplied at session launch; context tag is read by tool-layer policies and agents
- **Source:** this project (term); upstream `VerticalManifest.context_schema` / `ContextField` (data)

### tool-layer policy

A runtime-enforced rule attached to a specific tool in a vertical's manifest, declaring a prerequisite condition that must be satisfied for the tool to execute. Four kinds: `requires_checkpoint` (a prior checkpoint of a named type must exist, optionally with a matching field value and freshness window), `requires_gate` (a protocol gate must be cleared), `budget_limited` (a named tool-arg field is capped by a session context value), `classification_gated` (a classified input is blocked unless an override flag is set).

Tool-layer policies are **not** gates and **not** escalations in the protocol sense. They are tool-call-level enforcement that produces trail entries with specific status codes (`COMPLIANCE_NOT_APPROVED`, `PHI_ACCESS_NOT_GRANTED`, `COMPUTE_BUDGET_EXCEEDED`, etc.). The Console surfaces them in the profile editor (as non-blocking warnings on policy-gated tools, `wcon-profiles` §3.2) and in the oversight dashboard (as refusal events in the refusal panel, `wcon-highway` §4A).

- **Category:** native (term coined to name the concept the upstream manifest formalizes as `tool_policies`)
- **Aliases to avoid:** tool gate (conflates with protocol gate), tool permission, tool restriction
- **Relationships:** tool-layer policy belongs to one tool in one vertical; policy enforces a prerequisite condition; policy violation produces a tool-layer refusal
- **Source:** this project (term); upstream `VerticalManifest.tool_policies` / `ToolPolicy` (data)

### tool-layer refusal

A runtime response to a tool invocation that violates the tool's tool-layer policy. Arrives as a trail entry (not as a gate or escalation). The Console's session monitor detects refusal trail entries by status code, constructs a `RefusalEvent`, and adds it to `pending_refusals` for the session (`wcon-sessions` §6.3). Surfaced in the oversight dashboard's refusal panel (`wcon-highway` §4A). Communicated to the frontend via the `refusals` WebSocket channel — a Console-synthesized channel, not a runtime gRPC stream.

Unblocking a refusal requires upstream action: creating the prerequisite checkpoint (for `requires_checkpoint`), resolving the prerequisite gate (for `requires_gate`), cancelling and relaunching with different context (for `budget_limited`), or retrying with the override flag (for `classification_gated`). The Console does not offer a "resolve refusal" button — its role is to explain the refusal and its unblock path.

- **Category:** native (derived from tool-layer policy)
- **Aliases to avoid:** tool denial, tool rejection, tool error, refusal (too generic on its own; use "tool-layer refusal" or "refusal event" explicitly in cross-spec references)
- **Relationships:** refusal is produced by a tool-layer policy; refusal blocks a workspace; refusal is resolved by meeting the policy's prerequisite; refusal is surfaced as a `RefusalEvent` on the `refusals` channel
- **Source:** this project

### refusal event (`RefusalEvent`)

The concrete data structure the Console uses to represent a tool-layer refusal. Defined in `wcon-sessions` §6.1 (in-memory form) and `wcon-highway` §4A.2 (wire format). Fields: `refusal_id`, `workspace_id`, `workspace_label`, `tool_name`, `tool_args_preview`, `policy_kind`, `error_code`, `reason`, `policy_reference` (with `vertical`), `unblock_hint`, `trail_entry_id`, `created_at`. Clients (the oversight dashboard) render refusal events in the refusal panel.

- **Category:** native (implementation term)
- **Aliases to avoid:** refusal (too generic), refusal notification
- **Relationships:** one RefusalEvent per refusal; constructed by the session monitor from a trail entry; emitted on the `refusals` WebSocket channel
- **Source:** this project

### role slot derivation (Mode A / Mode B)

The Console's strategy for mapping a workflow to the list of role slots the user must fill in the session launcher. Defined in `wcon-sessions` §2.4:

- **Mode A (stage-aware).** Each workflow stage becomes one role slot, carrying the stage name, the stage's role, and the stage's position. Applies when per-stage workflow metadata is available — a future upstream extension. Two stages with the same role produce two slots.
- **Mode B (role-aware fallback).** One slot per distinct role in the vertical's `VerticalEntry.roles`, without stage metadata. Applies today because the manifest's `WorkflowSummary` carries only counts, not per-stage detail. The same profile covers every stage that shares a role.

Mode selection is not user-visible; the wizard UI adapts transparently. Validation checks (`MISSING_ASSIGNMENT`, `ROLE_MISMATCH`) run against whichever mode the Console picked. The `session_assignments` table supports both modes via a nullable `stage_id` column and a mandatory `slot_position` ordering column (`wcon-data-model` §4.2).

- **Category:** native (implementation concept)
- **Aliases to avoid:** workflow mode, slot mode
- **Relationships:** derivation mode is determined per-workflow; informs the session launcher UI and the validation checks; preserved in `session_assignments` records
- **Source:** this project

### tool

A capability descriptor in a vertical's manifest. Contains a `name` (unique within the vertical) and a `description`. Tools are scoped to the vertical that declares them; per `wcon-discovery` §3.4, the Console treats all tools in a vertical as available to every role in that vertical (vertical-coarse mapping, because the upstream manifest does not provide per-role tool assignments). When a tool has a corresponding entry in `VerticalEntry.tool_policies`, it is **policy-gated** — the runtime enforces a prerequisite condition at invocation time (see "tool-layer policy").

The Console indexes tools in `ToolEntry` (`wcon-data-model` §6.1) with the owning vertical, the full policy (if any), and the list of associated roles. Tool input schemas and execution handlers live in the runtime — the manifest and the Console carry only the name and description.

- **Category:** inherited (from `wacp-ecosystem` / `wacp-taxonomy`)
- **Aliases to avoid:** capability (too abstract), function, API call
- **Relationships:** tool belongs to one vertical; tool may have a tool-layer policy; tool is referenced by profiles' allowlists/denylists
- **Source:** `wacp-ecosystem` (per-vertical `tools[]`); `wacp-taxonomy::ToolSummary`

### vertical-coarse tool mapping

A term of art for the fallback tool-role association strategy the Console uses while the upstream manifest lacks per-role tool metadata. Defined in `wcon-discovery` §3.4: every role in a vertical is considered to have access to every tool in the same vertical. Profile validation, discovery display, and session launch all operate on this assumption.

This is a deliberate relaxation — it gives up fine-grained tool/role authorization on the Console side and defers the authoritative check to runtime tool-layer refusal. When the upstream manifest is extended with per-role tool mappings, `wcon-discovery` §3.4 will be revised to restore a fine-grained mapping and the term "vertical-coarse" will become historical.

- **Category:** native (term coined to name the §3.4 fallback)
- **Aliases to avoid:** broad tool mapping, permissive tool mapping
- **Relationships:** drives `RoleEntry.tools` and `ToolEntry.roles` population; bounded by the vertical's boundary
- **Source:** this project

### vertical-specific checkpoint

A checkpoint type declared by a vertical (in `VerticalManifest.checkpoint_types`) with a structured field schema, used by the vertical's tool-layer policies or workflow enforcement. Examples: Finance `compliance_check` (pre-trade compliance verification), Healthcare `phi_access_grant` (consent or de-identified access authorization), MLOps `reproducibility_checkpoint` (training-run provenance), DataSci `declared_hypothesis` (pre-registered null/alternative), Analytics `data_snapshot` (query reproducibility anchor).

Distinct from protocol-level custom checkpoint types (`wacp-protocol` TAXONOMY), which are registered at the protocol layer. Vertical-specific checkpoints live at the ecosystem layer. The Console indexes them separately (`wcon-data-model` §6.1) and renders them with their structured field schemas in the trail stream (`wcon-highway` §8, `wcon-ui` §7.2).

- **Category:** native (term coined to distinguish from protocol-level custom checkpoint types)
- **Aliases to avoid:** domain checkpoint, vertical checkpoint, custom checkpoint (too ambiguous — overlaps with protocol-level custom types)
- **Relationships:** vertical-specific checkpoint is declared by a vertical; checkpoint type may be required by a tool-layer policy; checkpoint instance is recorded in the trail
- **Source:** this project (term); upstream `VerticalManifest.checkpoint_types` / `CheckpointSchema` (data)

## 5. Console-Native Concepts

### profile

A user-created agent configuration bundle that extends WACP's agent profile with additional operational parameters. Contains: a reference to a role (base or derived), LLM configuration (provider, model, temperature, max tokens), autonomy preset, tool allowlist and denylist, resource budget caps (cost, tokens, duration), and user-facing metadata (name, description, tags). Portable as YAML. Validated against the taxonomy on save — the referenced role must exist, and every allowed or denied tool must belong to the same vertical as the role (per `wcon-discovery` §3.4 vertical-coarse tool mapping; the upstream manifest does not provide per-role tool metadata). Policy-gated tools are saved with a non-blocking `TOOL_HAS_RUNTIME_POLICY` warning.

- **Category:** native (extends WACP's "agent profile")
- **Aliases to avoid:** config, template, preset, agent profile (use "profile" consistently in Console context; reserve "agent profile" for WACP upstream references)
- **Relationships:** profile references one role; profile specifies LLM configuration; profile defines budget; profile is assigned to a session slot; profile belongs to profile library
- **Source:** this project

### session

A user-initiated coordination run managed by the Console. A session binds a vertical, a workflow DAG, and a set of profile-to-role assignments into a launchable unit. Launching a session creates the corresponding WACP coordinator and workspaces. The session tracks lifecycle state independently from individual workspace states — a session is the user's view of a coordination run, not the runtime's.

- **Category:** native (wraps WACP's runtime session concept)
- **Aliases to avoid:** run, execution, job
- **Relationships:** session uses one vertical; session uses one workflow; session assigns profiles to roles; session maps to WACP coordinator + workspaces; session is monitored through oversight dashboard
- **Source:** this project

### discovery

The capability of browsing, searching, and inspecting the taxonomy's contents: available roles (base and derived), tools (grouped by the vertical that owns them, with policy indicators), protocol-level envelope and checkpoint types, and vertical definitions — each with its defining constraint, context schema, tool policies, vertical-specific checkpoint types, workflows, task types, and quality criteria. Discovery is read-only and always reflects the current state of the taxonomy index.

- **Category:** native
- **Aliases to avoid:** search, browse, catalog, exploration
- **Relationships:** discovery reads from taxonomy index; discovery is presented through discovery browser
- **Source:** this project

### taxonomy index

An in-memory, queryable representation of the WACP taxonomy rebuilt from two sources on Console startup (and on manual reload): protocol-taxonomy YAML files on the local filesystem (for base/derived roles, protocol-level envelope and checkpoint types) and vertical manifests fetched from the WACP runtime via `GET /v1/verticals[/{id}]` (for verticals, per ADR-001). Provides fast lookup by role, tool, type, or vertical. Read-only — the Console does not modify taxonomy files or mutate runtime state. Atomically rebuilt via `ArcSwap` (`wcon-data-model` §6.3) so readers never observe a partial index.

- **Category:** native
- **Aliases to avoid:** cache, registry, catalog
- **Relationships:** taxonomy index is built from protocol taxonomy files + runtime REST responses; taxonomy index serves discovery queries; vertical entries are projections of upstream `VerticalManifest`
- **Source:** this project

### profile library

The user's collection of saved profiles. Stored persistently (SQLite + filesystem YAML export). Supports CRUD operations, versioning (each save creates a new version), and import/export for sharing.

- **Category:** native
- **Aliases to avoid:** profile store, profile database, profile collection
- **Relationships:** profile library contains profiles; profile library persists to storage
- **Source:** this project

### vertical registry

The Console's in-memory catalog of available verticals, populated by calling `GET /v1/verticals` on the WACP runtime at startup and on taxonomy reload (ADR-001). Each entry is a full `VerticalEntry` — a projection of the upstream `VerticalManifest` with every field: defining_constraint, context_schema, tool_policies, checkpoint_types, quality_criteria, task_types, workflows, default_profiles, tools. The registry lives inside the taxonomy index (`wcon-data-model` §6.1 `verticals: HashMap<String, VerticalEntry>`); it is not a separate storage.

The registry is the authoritative vertical list the runtime advertises at the moment of the last successful fetch. Adding or removing a vertical is a runtime concern — the Console observes the change on its next reload. The Console does not scan a filesystem directory for verticals; earlier drafts called for `verticals.path` but that setting was removed per ADR-001.

- **Category:** native
- **Aliases to avoid:** vertical catalog, vertical store, marketplace
- **Relationships:** vertical registry is a slice of the taxonomy index; populated from REST at startup/reload; session launcher reads verticals from the registry; profile editor resolves role ownership through the registry
- **Source:** this project

### user

A human identity registered in the Console's local identity store. Has a username, a hashed password (Argon2id), and a console role (admin, operator, or viewer). Users are the unit of ownership — every profile and session is attributed to the user who created it. The identity store is Console-local; it has no corresponding WACP runtime concept.

- **Category:** native
- **Aliases to avoid:** account (too generic), operator (overloaded — also a console role level)
- **Relationships:** user owns profiles and sessions; user has one console role; user authenticates via browser session or API token
- **Source:** this project

### console role

The authorization level assigned to a Console user. Three hierarchical levels: `admin` ⊃ `operator` ⊃ `viewer`. Maps to the personas in `wcon-vision` §4: admin → Administrator, operator → Practitioner / Overseer, viewer → Explorer. Console roles govern what a user can do in the Console (CRUD profiles, launch sessions, manage users, view audit log). Unrelated to WACP protocol roles (coordinator, worker, observer) — a Console "operator" is not a WACP "observer."

- **Category:** native
- **Aliases to avoid:** role (ambiguous — see §8), permission level, access tier
- **Relationships:** console role is assigned to a user; console role determines authorization scope
- **Source:** this project

### browser session

The authenticated session between a user's browser and the Console backend. Cookie-based (HttpOnly, Secure, SameSite=Strict), rotated on login, CSRF-protected on all state-changing endpoints. Distinct from a Console "session" (a coordination run) and from a WACP runtime session. Stored in the `user_sessions` SQLite table.

- **Category:** native
- **Aliases to avoid:** session (ambiguous — see §8), login session, auth session
- **Relationships:** browser session authenticates a user; browser session is issued on login; browser session is stored server-side
- **Source:** this project

### API token

A named bearer credential for programmatic access to the Console's REST API. Scoped per user, revocable, hashed at rest (SHA-256), displayed exactly once at creation. Tokens carry the same console role as the owning user. Alternative to cookie-based browser sessions for automation and scripting.

- **Category:** native
- **Aliases to avoid:** API key (reserved for runtime auth in `wcon-architecture` §8), access token, secret
- **Relationships:** API token belongs to one user; API token authenticates REST requests; API token is managed through admin UI
- **Source:** this project

### audit log

An append-only record of every state-changing operation in the Console. Each entry captures: `user_id`, `timestamp`, `action`, `target_kind`, `target_id`, `ip`, `user_agent`. Stored in the `audit_log` SQLite table. Not to be confused with the WACP trail — the audit log records Console operations (profile edits, session launches, user management), not protocol events (workspace signals, checkpoint creation, envelope delivery).

- **Category:** native
- **Aliases to avoid:** trail (that is the protocol's immutable event record — see §8), log (too generic), event log
- **Relationships:** audit log is written on every Console mutation; audit log entries reference a user; audit log is viewable by admins
- **Source:** this project

### bootstrap credential

The one-time admin credential generated on the Console's first launch, printed to stdout or written to `$XDG_STATE_HOME/wacp-console/bootstrap-token`. Must be changed on first login. Ensures there is never a standing default credential in the system.

- **Category:** native
- **Aliases to avoid:** default password, initial password, setup token
- **Relationships:** bootstrap credential authenticates the first admin user; bootstrap credential is invalidated after first login
- **Source:** this project

### ownership

The association between a Console user and the resources they create. Every profile and session carries an `owner_user_id`. Ownership determines default access: owners have full control over their resources. Admins bypass ownership checks.

- **Category:** native
- **Aliases to avoid:** authorship, creatorship
- **Relationships:** ownership links a user to profiles and sessions; ownership interacts with visibility for profiles
- **Source:** this project

### visibility

A per-profile setting controlling who can access the profile beyond its owner. Two values: `private` (owner-only) and `shared` (readable and usable by all operators, editable by owner and admins). Default: `private`. Sessions do not have a visibility field — session access is governed by console role (operators see own sessions, admins see all).

- **Category:** native
- **Aliases to avoid:** access level, sharing mode, permission
- **Relationships:** visibility qualifies a profile; visibility interacts with console role to determine access
- **Source:** this project

## 6. Console UI Concepts

### discovery browser

The UI surface for exploring the taxonomy index. Displays roles (base and derived), tools (grouped by vertical with lock indicators for policy-gated tools), types (envelope types, protocol-level checkpoint types, and vertical-specific checkpoint types in three sections), and verticals (with per-vertical detail showing defining constraint, context schema, tool policies, checkpoint types, workflows, task types, quality criteria, and default profiles). Supports filtering, search, and detail views. Read-only — for inspection, not modification.

- **Category:** native
- **Aliases to avoid:** explorer, catalog view, taxonomy browser
- **Relationships:** discovery browser presents taxonomy index data; consumes `wcon-api` §6 endpoints
- **Source:** this project

### profile studio

The UI surface for creating, editing, cloning, and managing profiles. Includes the profile editor (form-based configuration) and the profile library view (list of saved profiles with versioning).

- **Category:** native
- **Aliases to avoid:** profile editor (that's a subcomponent), profile manager, config panel
- **Relationships:** profile studio operates on profile library; profile studio validates against taxonomy index
- **Source:** this project

### session launcher

The UI surface for configuring and starting a session. Six-step wizard: **Step 1** select vertical, **Step 2** select workflow, **Step 3** assign profiles to role slots (per `wcon-sessions` §2.4 Mode A stage-aware or Mode B role-aware slot derivation), **Step 4** supply vertical context (dynamically generated from the vertical's `context_schema`, automatically skipped for verticals with an empty schema like SWE), **Step 5** set budget overrides, **Step 6** review and launch.

- **Category:** native
- **Aliases to avoid:** run dialog, start wizard, launch panel
- **Relationships:** session launcher creates sessions; session launcher reads vertical registry and profile library; consumes `wcon-api` §8 endpoints
- **Source:** this project

### oversight dashboard

The UI surface for monitoring active sessions in real-time. Integrates trail streaming (with vertical-specific checkpoint rendering driven by indexed `CheckpointSchema.fields`), gate approval queue (with vertical rationale enrichment per `wcon-highway` §4.7), escalation inbox, refusal panel (for tool-layer refusals per `wcon-highway` §4A), directive injection, session header context badges (from `session.context`), and end-of-session quality report panel (from the vertical's `quality_criteria`). The Console's implementation of highway interaction. Seven WebSocket channels feed the dashboard: `trail`, `gates`, `escalations`, `refusals`, `workspaces`, `session`, `notification`.

- **Category:** native
- **Aliases to avoid:** monitor, control panel, highway view (highway is the protocol mechanism, not the UI)
- **Relationships:** oversight dashboard displays session state; presents gates, escalations, refusals; enables injection; consumes `wcon-api` §12 WebSocket protocol
- **Source:** this project

## 7. Console Internal Concepts

### console backend

The server-side Rust service that sits between the frontend SPA and the WACP runtime. Manages profiles, sessions, the taxonomy index, and proxies highway interactions. Connects to the WACP runtime over two transports: gRPC for sessions, agents, and highway (`AgentService`, `CoordinatorService`, `HighwayService`); REST for vertical manifest loading (`GET /v1/verticals[/{id}]`, per ADR-001).

- **Category:** internal
- **Aliases to avoid:** server, API layer, middleware
- **Relationships:** console backend connects to WACP runtime via gRPC and REST; serves frontend via REST and WebSocket; manages profile library and taxonomy index
- **Source:** this project

### console frontend

The browser-based SPA that presents the discovery browser, profile studio, session launcher, and oversight dashboard. Communicates with the console backend via REST and WebSocket.

- **Category:** internal
- **Aliases to avoid:** UI, client, web app (too generic)
- **Relationships:** console frontend consumes console backend API
- **Source:** this project

### authenticator

A pluggable backend trait that extracts a user identity from an incoming HTTP request. The Phase 1 implementation is `LocalAuthenticator` — validates cookie-based browser sessions and bearer API tokens against the local identity store. The trait shape accommodates a future `OidcAuthenticator` implementation without restructuring the request pipeline.

- **Category:** internal
- **Aliases to avoid:** auth middleware (too vague), login handler
- **Relationships:** authenticator produces a user identity from a request; authenticator is consumed by authorizer
- **Source:** this project

### authorizer

A pluggable backend trait that determines whether an authenticated user may perform a requested action. Receives the user identity (from the authenticator) and the action. The Phase 1 implementation enforces the three-level console role hierarchy (admin ⊃ operator ⊃ viewer).

- **Category:** internal
- **Aliases to avoid:** permission checker, access control
- **Relationships:** authorizer consumes authenticated identity; authorizer gates every API endpoint
- **Source:** this project

## 8. Flagged Ambiguities

### "session" — three-way overlap

Three distinct concepts share the word "session":

| Concept | Meaning | Storage |
|---------|---------|---------|
| **session** (Console-native, §5) | A user-initiated coordination run — vertical + workflow + profile assignments | `sessions` table |
| **runtime session** | WACP's execution container for a workflow (`wacp-local`) | Runtime memory |
| **browser session** (Console-native, §5) | The authenticated connection between a user's browser and the Console backend | `user_sessions` table |

**Resolution:** within Console specs, unqualified "session" always means the Console-native coordination run. Use "runtime session" for the WACP construct and "browser session" for the authentication construct. Never use bare "session" when the auth meaning is intended.

### "role" — two-way overlap

Two distinct concepts share the word "role":

| Concept | Meaning | Values |
|---------|---------|--------|
| **protocol role** (§2) | An agent's function within WACP — determines workspace permissions and capabilities | coordinator, worker, observer, derived roles |
| **console role** (§5) | A Console user's authorization level — determines what they can do in the Console | admin, operator, viewer |

**Resolution:** within Console specs, use "console role" when referring to admin/operator/viewer authorization. Use "role" (unqualified) only for protocol roles — this preserves consistency with §2 and the upstream WACP glossary. Never use bare "role" when the Console authorization meaning is intended.

### "audit log" vs "trail"

The Console's **audit log** (§5) and the WACP **trail** (§1) are both append-only event records, but they cover different domains:

| Concept | Records | Written by |
|---------|---------|------------|
| **trail** | Protocol events — signals, envelope deliveries, checkpoint creation, state transitions | WACP runtime |
| **audit log** | Console operations — profile edits, session launches, user management, login attempts | Console backend |

**Resolution:** never use "audit log" to refer to the trail, and never use "trail" or "log" to refer to the audit log. The trail is the protocol's record; the audit log is the Console's record.

### "profile" vs "agent profile"

WACP's ecosystem layer defines "agent profile" as system prompt + tool whitelist + autonomy level. The Console's "profile" is a superset that adds LLM configuration, budget caps, and user metadata. **Resolution:** within Console specs, "profile" always means the Console-native concept. When referring to the upstream WACP construct, use "WACP agent profile" or "ecosystem agent profile" explicitly.

### "tool" — no ambiguity

Both WACP and the Console use "tool" to mean the same thing: a capability descriptor with JSON Schema validation and an execution handler. No disambiguation needed — the Console inherits this term directly.

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wacp-protocol | WACP Protocol Specification | source of all inherited terms |
| wacp-taxonomy | WACP Taxonomy | source of taxonomy, derived role definitions |
| wacp-ecosystem/swe | SWE Vertical | source of vertical, agent profile, autonomy definitions |
| wcon-auth | Authentication & Authorization | defines auth terms introduced in this revision (§5, §7, §8) |

*WACP Console -- authored by AAkil98*
