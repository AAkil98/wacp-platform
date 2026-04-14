---
id: wcon-discovery
type: design
status: final
created: 2026-04-09T00:00:00
revised: 2026-04-11T00:00:00
authors: [AAkil98]
tags: [discovery, taxonomy, indexing, search, verticals]
depends_on: [wcon-architecture, wcon-data-model]
---

# WACP Console — Agent & Role Discovery

## Table of Contents

1. Overview
2. Taxonomy Sources
3. Source Parsing
4. Index Construction
5. Query Model
6. Search Model
7. Browsing UX Model
8. Change Detection and Reload
9. Error Handling
10. Invariants

---

## 1. Overview

Discovery is how users explore what the WACP ecosystem offers before they create profiles or launch sessions. The Console presents the taxonomy — roles, tools, types, and verticals — as a browsable, searchable catalog rather than a collection of source files.

Discovery is strictly read-only. The Console never modifies taxonomy files or vertical definitions (`wcon-vision` §3, NG2/NG5). It builds an in-memory index from upstream sources, serves queries against that index, and rebuilds the index on demand when sources change.

The taxonomy index is the shared data structure that powers discovery. Its schema is defined in `wcon-data-model` §6. This spec defines how the index is populated (source parsing and construction), how it is queried (query and search models), how changes are detected (reload), and how the discovery browser presents the data to users.

Three principles govern discovery:

1. **Complete** — every entity that upstream sources expose appears in the index. Every role in the parsed protocol taxonomy, every vertical returned by the runtime's `GET /v1/verticals`, and every field of every manifest fetched via `GET /v1/verticals/{id}` is indexed. If a vertical exists upstream, the Console shows it.
2. **Consistent** — cross-references within the index are resolved. A vertical's `tool_policies[T]` entry is mirrored into `ToolEntry(T).policy`. A vertical's roles appear in the roles map with `vertical` set. Cross-reference bidirectionality is vertical-coarse after the §3.4 relaxation: any role in a vertical lists every tool in that vertical, and vice versa, because the upstream manifest does not carry per-role tool mappings.
3. **Current on demand** — the index reflects the state of upstream sources at the time of its last build. Users trigger rebuilds explicitly; the Console does not watch for file changes or poll the runtime REST endpoints for changes.

## 2. Taxonomy Sources

The Console reads from two categories of upstream source, obtained in two different ways:

| Category | Transport | Configured via |
|----------|-----------|----------------|
| Protocol taxonomy (base-role extensions, custom envelope types, protocol-level custom checkpoint types) | Local filesystem — YAML files under a configured path | `taxonomy.path` setting (`wcon-data-model` §5.2) |
| Vertical manifests (roles, tools, task types, workflows, context schemas, tool policies, vertical-specific checkpoint types, quality criteria, default profiles) | WACP runtime REST API | `runtime.rest_address` setting (`wcon-data-model` §5.2) |

The split is deliberate: the protocol taxonomy changes rarely and lives in the source tree alongside the protocol itself; vertical manifests are owned and versioned by the runtime which already has a mechanism to serve them (ADR-001 in `SPEC_BUILD.md`). Both sources are read-only as far as the Console is concerned.

### 2.1 Protocol Taxonomy

**Source path:** configured via `taxonomy.path` setting (default: `../wacp/protocol/taxonomy`)

The protocol taxonomy defines the extensible type registries: derived roles, custom envelope types, and custom checkpoint types. The canonical schema is specified in `wacp-protocol` TAXONOMY and uses YAML format:

```yaml
taxonomy:
  id: "string"
  version: "string"
  protocol_version: "string"
  roles:
    - name: "string"
      extends: "worker" | "observer"
      add: ["capability"]
      remove: ["capability"]
      override:
        checkpoint_types: ["string"]
  envelope_types:
    - name: "string"
      description: "string"
      permissions:
        - sender_role: "string"
          receiver_role: "string"
  checkpoint_types:
    - name: "string"
      description: "string"
      permitted_roles: ["string"]
      required_fields: ["string"]
```

Base roles (`coordinator`, `worker`, `observer`) are not defined in the taxonomy file — they are protocol constants. The Console hardcodes their definitions (capabilities, permissions) from the protocol specification.

### 2.2 Vertical Definitions

**Source:** the running WACP runtime's REST API.

Verticals are loaded from the runtime via two endpoints:

| Endpoint | Returns | Purpose |
|----------|---------|---------|
| `GET /v1/verticals` | `VerticalSummary[]` | Enumerate available verticals |
| `GET /v1/verticals/{id}` | `VerticalManifest` | Full manifest for one vertical |

The runtime loads manifests at startup from files under its configured `taxonomy.verticals_dir` and holds them in an `Arc<Vec<VerticalManifest>>`. These manifests are the authoritative source — the Console is a read-only consumer. The Console does not read `vertical.yaml` files directly, does not know or care where they live on disk, and does not require a filesystem path to them.

This section specifies the contract the Console consumes. The upstream schema is defined by `wacp-taxonomy::VerticalManifest` (Rust, at `crates/wacp-taxonomy/src/vertical.rs`) and mirrored in the TypeScript `LoadedVertical` interface (`packages/wacp-cli/src/ecosystem.ts`). The REST handlers live in `wacp-transport::rest_gateway`. The decision to treat the runtime as the vertical registry is recorded as ADR-001 in `SPEC_BUILD.md`.

#### 2.2.1 Vertical Summary (list endpoint)

`GET /v1/verticals` returns a JSON array of summaries, one per registered vertical:

```json
[
  {
    "id": "finance",
    "name": "Finance",
    "defining_constraint": "Regulatory pre-check + fiduciary duty — trade_execute refuses without an approved compliance_check checkpoint for the same trade_id (expires after 5 minutes); classifyForbiddenPattern() hard-blocks insider/wash/spoofing/layering/front-running/churning/painting-the-tape.",
    "task_type_count": 9,
    "workflow_count": 4,
    "tool_count": 10
  }
]
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Stable vertical identifier (e.g., `"finance"`, `"healthcare"`) |
| `name` | string | Human-readable name |
| `defining_constraint` | string | One-sentence description of the distinctive rule this vertical enforces |
| `task_type_count` | integer | Number of task types defined by this vertical |
| `workflow_count` | integer | Number of workflows |
| `tool_count` | integer | Number of tools |

The summary is sufficient to render the discovery browser's vertical list (§6) and the session launcher's step-1 cards (`wcon-ui` §6.2 / `wcon-sessions` §2.1). Full manifest detail is fetched on demand from the detail endpoint.

Empty registry returns `200 OK` with `[]` — the runtime has no verticals loaded (see §8.1 for the Console's behavior in this case).

#### 2.2.2 Vertical Manifest (detail endpoint)

`GET /v1/verticals/{id}` returns the full `VerticalManifest`. `404 Not Found` is returned if `{id}` does not match a loaded vertical.

```
VerticalManifest
├── id                   : string
├── name                 : string
├── defining_constraint  : string
├── context_schema       : map<string, ContextField>       (may be empty)
├── tool_policies        : map<string, ToolPolicy>         (may be empty)
├── checkpoint_types     : map<string, CheckpointSchema>   (may be empty)
├── quality_criteria     : QualityCriterion[]
├── task_types           : TaskTypeDescriptor[]
├── workflows            : WorkflowSummary[]
├── profiles             : ProfileSummary[]
└── tools                : ToolSummary[]
```

All collection fields default to empty — a vertical with no context schema (e.g., SWE) serves `"context_schema": {}`, not a missing key.

**ContextField** — declaration of one context tag the vertical requires (or optionally accepts) at session launch time.

| Field | Type | Description |
|-------|------|-------------|
| `type` | `"string" \| "number" \| "boolean" \| "enum"` | Field type (determines the widget in the launch wizard) |
| `required` | boolean | If true, the field must be supplied or session launch is rejected |
| `description` | string | Human-readable description |
| `enum_values` | string[]? | Choices when `type == "enum"`; absent otherwise |
| `default` | any? | Pre-filled default value when absent |

Example (Finance): `context_schema.jurisdiction = { type: "enum", required: true, description: "Regulatory jurisdiction governing trades in this session.", enum_values: ["SEC", "FINRA", "MiFID II", "FCA", "other"] }`.

**ToolPolicy** — a runtime-enforced rule attached to a specific tool. Discriminated by `kind`.

| Field | Type | Present when kind is | Description |
|-------|------|---------------------|-------------|
| `kind` | `"requires_checkpoint" \| "requires_gate" \| "budget_limited" \| "classification_gated"` | always | Enforcement type |
| `description` | string | always | Human-readable rule |
| `checkpoint_type` | string? | `requires_checkpoint` | Checkpoint type that must exist in the trail |
| `matching_field` | string? | `requires_checkpoint` | Tool-arg field that must match the checkpoint's recorded value |
| `expires_after_ms` | integer? | `requires_checkpoint` | Freshness window; checkpoint older than this is stale |
| `gate_condition` | string? | `requires_gate` | Human-readable condition that activates the gate |
| `budget_field` | string? | `budget_limited` | Tool-arg field carrying the requested amount |
| `blocked_classifications` | string[]? | `classification_gated` | Classification values blocked by default |
| `override_flag` | string? | `classification_gated` | Boolean arg flag that bypasses the block (with gate clearance) |

Examples:
- Finance `trade_execute`: `{ kind: "requires_checkpoint", checkpoint_type: "compliance_check", matching_field: "trade_id", expires_after_ms: 300000 }`
- MLOps `train_launch`: `{ kind: "budget_limited", budget_field: "max_hours" }`
- Healthcare `clinical_report_generate`, `lab_interpret`, `risk_score`: three `requires_checkpoint` policies, all gated on `phi_access_grant`

**CheckpointSchema** — describes the shape of one vertical-specific checkpoint type.

| Field | Type | Description |
|-------|------|-------------|
| `description` | string | What this checkpoint represents |
| `fields` | CheckpointField[] | Field list (`name`, `type`, `description`, `enum_values` when `type == "enum"`) |

Vertical-specific checkpoint types observed in the current ecosystem (non-exhaustive — the set is owned upstream):

| Checkpoint type | Vertical | Example fields |
|-----------------|----------|----------------|
| `compliance_check` | finance | `trade_id`, `instrument`, `side`, `quantity`, `status`, `regulation_cited`, `forbidden_pattern_screened`, `suitability_verified`, `kyc_current`, `expires_at` |
| `phi_access_grant` | healthcare | consent-basis variant: `patient_id`, `consent_id`, `consent_scope[]`, `expires_at`; de-identified variant: `deidentification_method`, `deidentified_data_hash`, `expires_at` |
| `reproducibility_checkpoint` | mlops | `data_hash`, `code_version`, `random_seed`, `hyperparameters` |
| `declared_hypothesis` | datasci | (domain-specific — see upstream manifest) |
| `data_snapshot` | analytics | (domain-specific — see upstream manifest) |

**QualityCriterion** — one weighted dimension in the vertical's quality rubric.

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Stable identifier (e.g., `"regulatory_compliance"`) |
| `name` | string | Human-readable label |
| `description` | string | What this criterion measures |
| `weight` | number | Relative weight; `1.0` = equal weight |

**TaskTypeDescriptor** — one task type with its target workflow and detection keywords.

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Namespaced identifier (e.g., `"finance:trade"`) |
| `name` | string | Human-readable label |
| `description` | string | What this task type represents |
| `workflow_id` | string | Default workflow for this task type |
| `keywords` | string[] | Representative keywords for search and CLI task-type detection (not regex) |

**WorkflowSummary** — one workflow with counts.

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Namespaced workflow identifier |
| `name` | string | Human-readable name |
| `description` | string | Short description of the workflow's purpose |
| `stage_count` | integer | Number of stages in the DAG |
| `gated_stage_count` | integer | Number of stages whose transition is gated |

Per-stage detail (role, dependencies, gated flag) is **not** included in the manifest. The Console shows workflow cards from the summary alone (`wcon-ui` §4.5, §6.2). When per-stage detail is required (workflow detail view, session launcher stage-by-stage assignment), the Console obtains it from the upstream TypeScript source or a future per-workflow endpoint.

**ProfileSummary** — one default agent profile shipped with the vertical.

| Field | Type | Description |
|-------|------|-------------|
| `role_id` | string | Role this profile targets |
| `autonomy` | `"gated" \| "autonomous"` | Default autonomy mode for this role in this vertical |

Default profiles are informational — they populate the vertical detail view and suggest autonomy defaults in the session launcher. They are not Console profiles (`wcon-data-model` §3 profiles are user-created).

**ToolSummary** — one tool.

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Tool identifier (e.g., `"code_edit"`, `"trade_execute"`) |
| `description` | string | Human-readable description |

Tool input schemas and execution handlers live in the runtime; the Console displays names and descriptions.

#### 2.2.3 Forward Compatibility

The manifest is the upstream runtime's contract. The Console's `VerticalEntry` (`wcon-data-model` §6.1) is a local projection that tolerates unknown fields — the deserializer ignores extras. New manifest fields added by the upstream runtime appear in JSON responses as soon as the runtime is upgraded; the Console's typed projection can catch up in a later release without breaking startup.

The Console must not enforce stricter validation than the runtime on manifest content. If the runtime serves a manifest, the Console indexes it. Fields the Console does not yet understand are preserved as opaque JSON in the index (`wcon-data-model` §6.1) so the frontend can display them even if the Console's typed code path has not caught up.

#### 2.2.4 No Filesystem Access

The Console does not read `vertical.yaml` files directly. Any former `verticals.path` setting is removed from `wcon-data-model` §5.2. The runtime is the registry; the filesystem path it reads from is a runtime concern configured via the runtime's `taxonomy.verticals_dir` key.

This decision is recorded as ADR-001 in `SPEC_BUILD.md`.

## 3. Source Parsing

### 3.1 Parse Order

Index construction proceeds in a fixed order to resolve cross-references:

1. **Base roles** — hardcoded protocol constants (coordinator, worker, observer) with their capabilities and permissions.
2. **Protocol taxonomy** — parse the taxonomy YAML to extract derived roles, custom envelope types, and custom checkpoint types.
3. **Vertical manifests** — fetch `GET /v1/verticals` from the WACP runtime to enumerate the registry, then fetch `GET /v1/verticals/{id}` for each entry and project the manifest into index entries (§3.3). No filesystem access.

Steps 1 and 2 still read from the local filesystem (base roles are compiled-in constants; the protocol taxonomy path is unchanged, see §2.1). Only step 3 was reshaped by ADR-001.

### 3.2 Protocol Taxonomy Parsing

The parser reads all YAML files under the configured `taxonomy.path`. For each file containing a `taxonomy` root key:

| Source field | Index target | Processing |
|-------------|-------------|------------|
| `roles[].name` | `RoleEntry.name` | Prepended with taxonomy namespace if present |
| `roles[].extends` | `RoleEntry.base_role` | Must be `"worker"` or `"observer"` |
| `roles[].add` | `RoleEntry.capabilities_added` | Capability identifiers |
| `roles[].remove` | `RoleEntry.capabilities_removed` | Capability identifiers |
| `envelope_types[].name` | `EnvelopeTypeEntry.name` | Registered in envelope types map |
| `envelope_types[].permissions` | `EnvelopeTypeEntry.sender_roles`, `receiver_roles` | Flattened from permission pairs |
| `checkpoint_types[].name` | `CheckpointTypeEntry.name` | Registered in checkpoint types map |
| `checkpoint_types[].permitted_roles` | `CheckpointTypeEntry.allowed_roles` | Direct mapping |

### 3.3 Vertical Manifest Ingestion

For each vertical returned by `GET /v1/verticals`, the builder issues `GET /v1/verticals/{id}` and projects the manifest into index entries:

| Source field | Index target | Processing |
|--------------|-------------|------------|
| `id` | `VerticalEntry` key | Uniqueness enforced by the runtime |
| `name` | `VerticalEntry.name` | Direct |
| `defining_constraint` | `VerticalEntry.defining_constraint` | Direct |
| `context_schema` | `VerticalEntry.context_schema` | Preserved as typed map; each `ContextField` deserialized into `wcon-data-model` §6.1 form |
| `tool_policies` | `VerticalEntry.tool_policies` | Preserved as typed map keyed by tool name; `ToolPolicy.kind` discriminates variant |
| `checkpoint_types` | `VerticalEntry.checkpoint_types` | Preserved as typed map keyed by checkpoint type name |
| `quality_criteria[]` | `VerticalEntry.quality_criteria` | Direct mapping of `(id, name, description, weight)` |
| `task_types[]` | `VerticalEntry.task_types` | Direct mapping including `workflow_id` and `keywords[]` |
| `workflows[]` | `VerticalEntry.workflows` | Summary only (`stage_count`, `gated_stage_count`) — per-stage detail is not present in the manifest |
| `profiles[]` | `VerticalEntry.default_profiles` | Informational only — default autonomy per role, not indexed as Console profiles |
| `tools[]` | `ToolEntry` additions | Each tool's `name`/`description` added to the global tools map; the tool's owning vertical is recorded |

**Tool-layer policy cross-references.** For every entry in `VerticalEntry.tool_policies`, the builder populates two sides of the same edge:

- `ToolEntry(T).policy` — a full `ToolPolicy` object (kind, description, and kind-specific fields: `checkpoint_type`, `matching_field`, `expires_after_ms` for `requires_checkpoint`; `gate_condition` for `requires_gate`; `budget_field` for `budget_limited`; `blocked_classifications`, `override_flag` for `classification_gated`). This is the same object structurally — policies are stored by value, not by pointer.
- For `requires_checkpoint` policies specifically: if the named `checkpoint_type` appears in the same vertical's `checkpoint_types` map, `CheckpointSchema(checkpoint_type).required_by` (on the vertical-scoped side — see `wcon-data-model` §6.1) accumulates the set of tools that require it.

**Scoping rule.** A policy's `checkpoint_type` reference names a type declared in the **same vertical's** `checkpoint_types` map. Cross-vertical references are not supported by the upstream schema (the upstream `ToolPolicy.checkpoint_type` is a bare string without a vertical prefix) and the Console does not attempt to resolve them. A reference that does not resolve within the same vertical is recorded as unresolved (below).

Unresolved references (a checkpoint type named by a policy but not declared in the same manifest's `checkpoint_types` map) are logged as a warning and stored with `checkpoint_type` flagged as unresolved. The runtime is authoritative, so missing metadata in the manifest does not prevent indexing or block Console startup.

**Ingestion failure modes.** Individual manifest deserialization errors (a new field the Console does not understand, an enum value outside the known set) are tolerated wherever possible: unknown fields are ignored, unknown enum values are preserved as opaque strings. If a manifest is structurally unparseable (not valid JSON, missing required fields), the builder logs the error, skips that vertical, and continues ingesting the rest. Startup does not fail on a single bad manifest.

### 3.4 Role-Tool Resolution

The upstream manifest does not currently carry per-role tool mappings — `ProfileSummary` in the manifest exposes only `role_id` and `autonomy`, not the per-role tool list. The full `LoadedVertical.profiles[].tools` exists in the upstream TypeScript source but is not projected into the manifest Phase 27S ships.

Given this, the Console does not attempt to compute a precise `RoleEntry.tools → ToolEntry.roles` bidirectional mapping for vertical-scoped roles. Instead:

1. `RoleEntry.tools` for a vertical role is populated with **every** tool in the vertical. This is a permissive, worst-case view — it lets the profile editor surface the full tool set for each role and defers the exact authorization decision to runtime tool-layer refusal (§3.5).
2. `ToolEntry.roles` is the set of vertical roles that share the tool's vertical.
3. Base protocol roles (coordinator, worker, observer) have no inherent tool bindings — tools are a vertical-level concern.

The former `tool_access` levels (`"read-only"`, `"read-write"`, `"read-write-test"`) are not used. They were postulated before the upstream schema landed and never corresponded to a real field in any vertical's source. The Console does not expose them in the query model, the index, or the API.

This is a deliberate relaxation: rather than encoding tool-role authorization in the Console's index (where it would need to be kept in sync with runtime enforcement), the Console surfaces availability ("this tool exists in this vertical") and defers authorization to the runtime ("the runtime will refuse the call if it is not permitted"). The profile editor surfaces policy constraints via §3.5 so users are not surprised at runtime.

When the upstream manifest is later extended with per-role tool mappings, §3.4 will be revised to restore the bidirectional index. That revision is tracked in `SPEC_BUILD.md` as an open question.

### 3.5 Tool Policy Surfacing

Even without per-role tool mappings, the ingestion layer preserves enough metadata to surface runtime policy constraints in Console UIs. For each tool that has an entry in `VerticalEntry.tool_policies`, the builder records a `ToolEntry.policy` reference containing the policy kind and the fields relevant to that kind (see §2.2.2 `ToolPolicy`).

Consumers of this metadata:

- **Profile editor** (`wcon-profiles` §3.2, `wcon-ui` §5.2) — displays a lock indicator next to tools with a non-empty `policy`, tooltip summarizing the policy ("Requires a prior approved compliance_check checkpoint with matching trade_id within 5 minutes"), and flags the profile with a non-blocking warning on save.
- **Session launcher** (`wcon-sessions` §2.1 / `wcon-ui` §6.2) — step-1 vertical card shows the `defining_constraint`; step-4 context step is populated from `context_schema`.
- **Oversight dashboard** (`wcon-highway` §8, `wcon-ui` §7) — when a tool refusal event arrives through the trail stream with a policy-violation status code, the dashboard renders the refusal with a reference to the policy metadata the Console already has indexed.

Policy metadata is a display and hinting layer. It is never enforced by the Console — enforcement is exclusively the runtime's responsibility.

## 4. Query Model

The taxonomy index serves queries through the backend's REST API. All queries are read-only, served from the in-memory index with no database or filesystem access.

### 4.1 Entity Queries

Each indexed entity type supports list and detail queries. Entities with global scope (roles, tools, protocol-level envelope and checkpoint types) have top-level endpoints. Entities with per-vertical scope (workflows, task types, context fields, tool policies, vertical-specific checkpoint types, quality criteria) are accessed via sub-endpoints under `/api/verticals/:id/`.

**Global entities:**

| Entity | List endpoint | Detail endpoint | Filter fields |
|--------|--------------|----------------|---------------|
| Roles | `GET /api/roles` | `GET /api/roles/:id` | `base_role`, `vertical` |
| Tools | `GET /api/tools` | `GET /api/tools/:name` | `vertical`, `has_policy` |
| Envelope types (protocol-level) | `GET /api/envelope-types` | `GET /api/envelope-types/:name` | `sender_role`, `receiver_role` |
| Checkpoint types (protocol-level) | `GET /api/checkpoint-types` | `GET /api/checkpoint-types/:name` | `allowed_role` |
| Verticals | `GET /api/verticals` | `GET /api/verticals/:id` | — |

**Per-vertical entities** (scoped under `/api/verticals/:id/`):

| Entity | List endpoint | Detail endpoint |
|--------|--------------|----------------|
| Workflows | `GET /api/verticals/:id/workflows` | `GET /api/verticals/:id/workflows/:wf_id` |
| Task types | `GET /api/verticals/:id/task-types` | — (list returns full descriptors) |
| Context schema | `GET /api/verticals/:id/context-schema` | — (returns the full `context_schema` map) |
| Tool policies | `GET /api/verticals/:id/tool-policies` | — (returns the full `tool_policies` map) |
| Checkpoint types (vertical-specific) | `GET /api/verticals/:id/checkpoint-types` | — (returns the full `checkpoint_types` map) |
| Quality criteria | `GET /api/verticals/:id/quality-criteria` | — (returns the full list) |

The protocol-level `Checkpoint types` endpoint (`/api/checkpoint-types`) and the per-vertical one (`/api/verticals/:id/checkpoint-types`) are distinct: the former returns protocol-registered types like `artifact`, the latter returns vertical-specific types like Finance's `compliance_check`. Vertical-specific checkpoint types have a field schema; protocol-level ones do not.

There is no global cross-vertical list endpoint for vertical-scoped entities (workflows, task types, etc.). Cross-vertical queries are served by the search endpoint (§5). This keeps the API's scope structure mirroring the index structure (`wcon-data-model` §6.1) and avoids two places the same data can be filtered differently.

### 4.2 Pagination

List queries support cursor-based pagination:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `limit` | integer | 50 | Max items per page (cap: 200) |
| `cursor` | string | — | Opaque cursor from previous response |

Response envelope:

```json
{
  "items": [],
  "cursor": "opaque-next-page-cursor",
  "has_more": true
}
```

Cursor encoding: the cursor is a base64-encoded string containing the sort key of the last item in the current page. The sort key is the entity's primary identifier (role ID, tool name, etc.), giving stable, alphabetical ordering.

### 4.3 Filtering

Filters are passed as query parameters. Multiple filters are combined with AND logic.

Examples:
- `GET /api/roles?base_role=worker` — all worker-derived roles
- `GET /api/roles?vertical=finance` — all roles defined by the Finance vertical
- `GET /api/tools?vertical=finance` — all tools in Finance
- `GET /api/tools?has_policy=true` — all tools with a non-empty `ToolEntry.policy` (i.e., runtime-enforced tool-layer policy); `false` returns the complement

Invalid filter values (e.g., a nonexistent vertical) return an empty result set, not an error.

### 4.4 Detail Responses

Detail queries return the full entity with resolved cross-references:

**Role detail** includes:
- Role definition fields (name, base_role, extends, capabilities)
- Resolved tool list (full tool entries, not just IDs — drawn from the owning vertical per §3.4)
- Vertical membership (if any)
- Envelope types this role can send/receive
- Checkpoint types this role can create

**Tool detail** includes:
- Tool definition fields (name, description)
- Owning vertical (if any)
- Resolved role list (which roles can use this tool — at minimum, all roles in the owning vertical per §3.4)
- `policy` — resolved `ToolPolicy` when the owning vertical declares one for this tool (§3.5)

**Vertical detail** includes:
- Vertical metadata: `name`, `defining_constraint`
- Role summaries (ID + name, not full entries — follow links for detail)
- Context schema (typed fields with descriptions, required flags, enum values, defaults)
- Tool policies (keyed by tool name with kind-specific fields)
- Vertical checkpoint types (with field schemas)
- Task type list (with keywords for search)
- Workflow summaries (ID + name + description + stage count + gated stage count)
- Quality criteria list (with weights)
- Default profiles (role_id + autonomy)

## 5. Search Model

### 5.1 Full-Text Search

The discovery browser provides a single search box that queries across all entity types.

**Endpoint:** `GET /api/search?q=<query>&type=<entity_type>`

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `q` | string | yes | Search query (min 2 characters) |
| `type` | string | no | Filter to a single entity type — see §5.2 for the full list |
| `vertical` | string | no | Scope results to entities owned by one vertical (only meaningful for vertical-scoped types) |
| `limit` | integer | no | Max results per type (default 10, cap 50) |

Valid `type` values: `role`, `tool`, `envelope_type`, `checkpoint_type`, `vertical_checkpoint_type`, `vertical`, `workflow`, `task_type`, `context_field`, `tool_policy`. These mirror the rows of §5.2 exactly. Omitting `type` searches across all categories.

### 5.2 Search Fields

Each entity type has designated searchable fields:

| Entity | Searchable fields |
|--------|------------------|
| Role | `name`, `base_role`, capability names |
| Tool | `name`, `description`, owning vertical |
| Envelope type | `name`, `description` |
| Checkpoint type (protocol-level) | `name`, `description` |
| Checkpoint type (vertical-specific) | `name`, `description`, owning vertical, field names |
| Vertical | `name`, `defining_constraint` |
| Workflow | `name`, `description` |
| Task type | `name`, `description`, `keywords[]` (manifest field) |
| Context field | field name, field `description` (scoped by owning vertical) |
| Tool policy | tool name + policy `description` (scoped by owning vertical) |

### 5.3 Search Implementation

The search uses case-insensitive substring matching against the designated fields. The taxonomy index is small enough (hundreds to low thousands of entries) that a linear scan with string matching is sufficient — no inverted index or external search engine is needed.

Results are grouped by entity type and ranked by match quality:
1. Exact name match (highest)
2. Name prefix match
3. Name substring match
4. Description/field match (lowest)

### 5.4 Search Response

```json
{
  "query": "compliance",
  "results": {
    "roles": [
      { "id": "finance:compliance_officer", "name": "Compliance Officer", "match_field": "name", "vertical": "finance" }
    ],
    "tools": [
      { "name": "compliance_check", "description": "Pre-trade compliance check...", "match_field": "name", "vertical": "finance", "has_policy": false },
      { "name": "trade_execute", "description": "REQUIRES a prior approved compliance_check...", "match_field": "description", "vertical": "finance", "has_policy": true }
    ],
    "task_types": [
      { "id": "finance:compliance_check", "name": "Compliance Check", "match_field": "name", "vertical": "finance" }
    ],
    "workflows": [],
    "envelope_types": [],
    "checkpoint_types": [],
    "vertical_checkpoint_types": [
      { "name": "compliance_check", "vertical": "finance", "description": "Pre-trade compliance verification...", "match_field": "name" }
    ],
    "verticals": [
      { "id": "finance", "name": "Finance", "match_field": "defining_constraint" }
    ]
  }
}
```

Each result includes enough fields for the discovery browser to render a preview (name, description snippet, match field, parent vertical, policy indicator) and link to the detail view. Vertical-specific checkpoint types are a distinct result category because they are scoped to a vertical and have their own detail view (§4.4 Vertical detail section).

## 6. Browsing UX Model

The discovery browser (`wcon-architecture` §4.2) presents the taxonomy index through five browsing modes, each corresponding to an entity type.

### 6.1 Navigation Structure

```
Discovery Browser
├── Roles
│   ├── Base Roles (coordinator, worker, observer)
│   └── Derived Roles (grouped by vertical)
│       ├── SWE            (swe:planner, swe:implementer, swe:tester, swe:reviewer)
│       ├── DevOps         (devops:*)
│       ├── MLOps          (mlops:*)
│       ├── Finance        (finance:analyst, finance:portfolio_manager, finance:risk_officer,
│       │                   finance:compliance_officer, finance:auditor)
│       ├── Healthcare     (health:*)
│       ├── Analytics      (analytics:*)
│       └── DataSci        (datasci:*)
├── Tools
│   ├── [grouped by vertical, with lock indicator for policy-gated tools (§3.5)]
│   └── [each tool shows its vertical and, if applicable, its tool-layer policy summary]
├── Types
│   ├── Envelope Types            (protocol-level)
│   ├── Checkpoint Types          (protocol-level)
│   └── Vertical Checkpoint Types (grouped by vertical — compliance_check, phi_access_grant,
│                                  reproducibility_checkpoint, declared_hypothesis, data_snapshot, ...)
├── Verticals
│   └── [each vertical expandable to: header (name + defining_constraint),
│       roles, task types, workflows, context schema, tool policies,
│       vertical-specific checkpoint types, quality criteria, tools, default profiles
│       — matches the per-section layout in `wcon-ui` §4.5]
└── Search (global, cross-entity — searches names, descriptions, keywords, defining constraints)
```

The role tab groups derived roles by their owning vertical. When seven verticals are present, the sidebar uses collapsible headers — clicking a vertical name collapses/expands its role group. The default state is "collapsed except the currently selected group" when there are more than three vertical groups.

The Tools tab shows tools grouped by vertical with a visual lock badge next to any tool that has a `policy` entry (§3.5). The badge tooltip shows a one-line summary of the policy.

The Types tab splits envelope and checkpoint types from the protocol layer and vertical-specific checkpoint types. Vertical checkpoint types (e.g., Finance's `compliance_check`) are shown under their owning vertical with their field schemas.

### 6.2 Browsing Patterns

**List → Detail.** The primary pattern. The user sees a filtered list of entities (e.g., all worker-derived roles) and clicks one to see its full definition, including resolved cross-references.

**Drill-down.** The user starts at a vertical, sees its roles, workflows, tools, tool policies, and checkpoint types. Clicking a role opens the role detail, which links to every tool in the role's vertical (per §3.4's vertical-coarse mapping) along with each tool's policy metadata. Clicking a tool opens its detail showing description, owning vertical, and — if applicable — its `ToolPolicy`. Each step narrows focus and adds context.

The drill-down does not surface per-tool JSON Schema (input parameters) — the upstream manifest only carries tool `name` and `description`. Tool input schemas live in the runtime's handlers, not in the manifest, and are therefore not part of discovery.

**Cross-reference navigation.** Detail views contain links to related entities:
- Role detail → list of tools in the role's vertical
- Tool detail → owning vertical, roles associated with the vertical, and (if present) the `ToolPolicy` with a link to the checkpoint type it requires (for `requires_checkpoint` policies)
- Vertical checkpoint type detail (within vertical detail) → list of tools whose policy names this checkpoint type in `required_by`
- Task type detail → the workflow it targets (via `workflow_id`)

The user follows connections without returning to the list.

### 6.3 Interaction States

| State | Display |
|-------|---------|
| Loading | Skeleton placeholders in list and detail panels |
| Empty (no results for filter) | Empty state message with suggestion to clear filters |
| Error (index unavailable) | Error banner with "Reload taxonomy" action |
| Stale (user triggered reload) | Loading overlay on existing content; old data visible until rebuild completes |

### 6.4 Layout

The discovery browser uses a two-panel layout:

- **Left panel:** entity list with filter controls and search. Scrollable, with pagination loading.
- **Right panel:** detail view of the selected entity. Shows all fields, resolved cross-references as clickable links, and related entity previews.

The left panel retains scroll position and filter state when the user navigates between detail views. Selecting a different entity type (roles → tools) resets filters but preserves the search query.

## 7. Change Detection and Reload

### 7.1 No Automatic Watching

The Console does not watch taxonomy files or vertical directories for changes. File-watching introduces complexity (platform differences, race conditions during multi-file updates, performance with large directory trees) without proportionate value — taxonomy changes are infrequent (vertical authoring events, not runtime events).

### 7.2 Manual Reload

The user triggers a taxonomy reload through:

1. **UI action:** a "Reload taxonomy" button in the discovery browser header.
2. **API endpoint:** `POST /api/taxonomy/reload` — returns immediately with a reload status.

### 7.3 Reload Process

1. The backend spawns a background task to build a new taxonomy index from the current state of both upstream sources:
   a. Re-parse protocol-taxonomy YAML files under `taxonomy.path`.
   b. Re-fetch `GET /v1/verticals` from the runtime, then for each listed vertical, fetch `GET /v1/verticals/{id}`.
2. During the build, the existing index remains active and serves all queries. Readers are unaffected.
3. On successful build, the new index is swapped in atomically via `ArcSwap` (`wcon-architecture` §7, `wcon-data-model` §6.3). Subsequent queries see the new data.
4. On partial failure (some individual vertical manifests failed to fetch or deserialize, but the protocol taxonomy parsed and `GET /v1/verticals` succeeded), the build completes with stub entries for the affected verticals (§9.1) and swaps in normally. The reload response carries `"status": "partial"` with warnings.
5. On total build failure (protocol taxonomy YAML parse error, or `GET /v1/verticals` itself unreachable), the existing index is retained. The failure is reported via the reload response and a notification in the discovery browser.

Reloading the protocol taxonomy and vertical manifests is always atomic on the Console side — either the new index fully replaces the old one, or nothing changes. The Console does not support "reload only protocol taxonomy" or "reload only verticals" as separate operations; both are refetched together, because downstream consumers (profile validation, session launch) rely on consistent cross-references between roles from the protocol taxonomy and tools from vertical manifests.

### 7.4 Reload Response

```json
{
  "status": "success" | "partial" | "failed",
  "duration_ms": 142,
  "counts": {
    "roles": 34,
    "tools": 67,
    "envelope_types": 3,
    "checkpoint_types": 2,
    "vertical_checkpoint_types": 6,
    "verticals": 7
  },
  "warnings": [
    "Vertical 'experimental' — manifest failed to deserialize, skipped"
  ],
  "errors": []
}
```

`"partial"` status indicates the index was rebuilt but some sources were skipped (e.g., a vertical whose detail endpoint returned a manifest the Console could not deserialize). The index contains everything that loaded successfully.

Counts shown above are illustrative of the current ecosystem (seven verticals); actual counts depend on what the runtime has loaded. `vertical_checkpoint_types` counts checkpoint types registered by verticals (e.g., `compliance_check`, `phi_access_grant`), distinct from protocol-level custom checkpoint types.

### 7.5 Impact on Active Sessions

Taxonomy reload does not affect active sessions. Sessions pin their configuration at launch time (`wcon-data-model` §4.1, §10.2): vertical, workflow, profile-to-role assignments, and `context` are all immutable once the session leaves the `configuring` state. A taxonomy reload may cause the discovery browser and profile studio to surface different roles, tools, context schemas, tool policies, or checkpoint types, but running sessions continue with their original bindings. The runtime's own copy of the manifest (which the session was launched against) is the authority while the session runs.

Profiles saved before a reload may become stale in several ways after a reload:

| Stale because | Behavior |
|---------------|----------|
| Role no longer exists in the taxonomy | Profile marked invalid (`wcon-data-model` §10.1 inv. 3); editing requires fixing the role reference |
| Tool in allowlist/denylist no longer exists | Profile marked invalid; editing requires removing the reference |
| Tool acquires a new `policy` that wasn't there at save time | Profile remains valid but saves with a new non-blocking warning (`wcon-profiles` §3.2 `TOOL_HAS_RUNTIME_POLICY`) |
| Tool loses a `policy` it had at save time | Profile remains valid; the warning previously displayed is gone |

Profiles pinned to sessions by `(profile_id, profile_version)` are not affected by these re-validations — the pinned row is immutable and the session uses it regardless of whether the latest version passes current validation.

**Session-level impact.** A vertical whose `context_schema` changed between a session's launch and a reload has no effect on the session itself (the session's context was captured at launch and delivered to the runtime already), but the *clone* of a running session (`wcon-sessions` §9.5) uses the current `context_schema` — cloning may require the user to re-enter context that was valid at the original launch but no longer matches the schema's type or enum values.

## 8. Error Handling

### 8.1 Startup Failures

| Failure | Behavior |
|---------|----------|
| Taxonomy path not found | Fatal — Console refuses to start. The protocol taxonomy is required for base-role capabilities and custom envelope/checkpoint types. |
| Taxonomy path found but empty | Warning — Console starts with base roles only (coordinator, worker, observer). No derived roles, no custom protocol types. |
| Parse error in taxonomy YAML | Fatal — Console refuses to start. A corrupt taxonomy could allow invalid profiles. |
| Runtime unreachable at startup (cannot call `GET /v1/verticals`) | Warning — Console starts with an empty vertical registry. Discovery browser shows base roles and protocol types only. Session launcher is disabled (no verticals to select). Startup banner prompts the user to configure the runtime address or run a taxonomy reload once the runtime is up. |
| Runtime reachable but `GET /v1/verticals` returns empty array | Warning — same as above. The runtime has no verticals loaded; the Console reflects that accurately. |
| `GET /v1/verticals/{id}` fails for one vertical (network error, 404, deserialization error) | Warning — that vertical is skipped. The discovery browser shows a stub entry ("manifest load failed — retry on reload") for the missing vertical. Other verticals load normally. |
| `GET /v1/verticals/{id}` returns a manifest with unresolved tool-policy cross-references | Warning only — the vertical is still indexed, the affected tool's `policy` metadata is stored with `checkpoint_type` marked unresolved. The runtime is authoritative; the Console does not block on incomplete metadata. |

The distinction: protocol taxonomy errors are fatal (the taxonomy is the foundation for base roles and profile validation), but runtime-side vertical loading errors are non-fatal (the Console degrades gracefully — discovery works for base roles, session launch is disabled until verticals are available).

**Runtime reachability is a runtime concern, not a startup gate.** The Console is designed to come up before the runtime if necessary. Once the runtime becomes reachable, the user triggers a taxonomy reload (§7.2) and the vertical registry is populated. Session launch becomes available as soon as at least one vertical is successfully ingested.

### 8.2 Reload Failures

Reload failures never crash the Console or invalidate the current index. The reload response carries error details for the user.

| Failure | Behavior | Reload status |
|---------|----------|---------------|
| Protocol-taxonomy YAML parse error | Reload fails entirely — existing index retained | `failed` |
| Protocol-taxonomy path inaccessible (permissions, deleted) | Reload fails entirely — existing index retained, error reported | `failed` |
| Runtime REST endpoint unreachable (`GET /v1/verticals` fails with connection error, 5xx, timeout) | Reload fails entirely — existing index retained | `failed` |
| `GET /v1/verticals` succeeds but returns an empty array | Reload succeeds with an empty vertical registry (protocol taxonomy portion still rebuilt) | `success` or `partial` depending on other sources |
| Single vertical detail endpoint fails (`GET /v1/verticals/{id}` returns 404, deserialization error, or timeout) | Partial reload — stub entry for that vertical (§9.1), other verticals included, swap succeeds | `partial` |
| Single vertical manifest contains unresolved tool-policy cross-references | Partial reload — vertical indexed normally, unresolved references flagged (§3.3) | `partial` (warning only) |
| Runtime REST authentication fails | Reload fails entirely — existing index retained, error prompts the user to check credentials | `failed` |

The rule: **protocol-taxonomy failures** and **runtime enumeration failures** (`GET /v1/verticals`) are total — they prevent any new index from being built. **Individual vertical failures** are partial — the new index is built with fewer or stub verticals.

### 8.3 Validation Failures

When profile validation (`wcon-data-model` §10.1) fails because a role or tool no longer exists in the taxonomy index (after a reload or on a fresh start with different taxonomy sources):

- The profile remains in the library with its stored data intact.
- The profile is marked as invalid in the UI (visual indicator).
- Editing and re-saving the profile requires fixing the invalid reference.
- The profile cannot be assigned to a session slot.

## 9. Invariants

### 9.1 Index Completeness

Every entity present in a successfully ingested source appears in the index. The index never silently drops entities. Specifically:

- Every role, envelope type, and checkpoint type in a successfully parsed protocol-taxonomy YAML file is indexed.
- Every vertical returned by `GET /v1/verticals` that the builder successfully fetches via `GET /v1/verticals/{id}` is indexed, and every `task_type`, `tool`, `workflow`, `quality_criterion`, `context_schema` entry, `tool_policy`, and `checkpoint_type` in that manifest is indexed.
- Verticals that the runtime advertises in the list endpoint but whose detail endpoint fails are recorded as stub entries with an error marker, not silently dropped. Users can see which verticals failed to load.
- A manifest with empty `context_schema`, empty `tool_policies`, and empty `checkpoint_types` (e.g., SWE) is complete — empty is not a failure state.

"Complete" means every entry the runtime served is present in the index, not every entry a user might expect. If the runtime's registry is empty, the index is complete-and-empty; that is a valid state.

### 9.2 Cross-Reference Integrity

All cross-references within the index are resolved. The bidirectional role-tool mapping is vertical-coarse (every role in a vertical lists every tool in that vertical — see §3.4); within that constraint, the following hold:

- If `RoleEntry.tools` contains tool T, then `ToolEntry(T).roles` contains the role. (Vertical-coarse after §3.4 — this reduces to "T is in the same vertical as the role.")
- If `VerticalEntry.roles` contains role R, then `RoleEntry(R).vertical` points to the vertical.
- If `EnvelopeTypeEntry.sender_roles` contains role R, the role R exists in the roles map.
- If a tool T has a `ToolEntry(T).policy`, there exists a vertical V such that `VerticalEntry(V).tool_policies[T]` is the same `ToolPolicy` value. Conversely, every `VerticalEntry(V).tool_policies[T]` is mirrored into `ToolEntry(T).policy`.
- For each `requires_checkpoint` policy whose `checkpoint_type` resolves within the same vertical, `CheckpointSchema(checkpoint_type).required_by` (scoped to that vertical) contains the tool name T. Conversely, every tool name in a `required_by` list has a corresponding `tool_policies[T]` entry in the same vertical referencing that checkpoint type.

These bidirectional mirrors are built at ingestion time (§3.3) and held invariant for the lifetime of the index. Unresolved references (a policy naming a checkpoint type that does not exist in the same vertical) are recorded with an unresolved marker and are explicitly **not** expected to satisfy the bidirectional invariants — they are the documented failure mode, not an invariant violation.

### 9.3 Atomic Visibility

The index is either fully built and visible, or the previous index is visible. There is no partially-built state observable by any reader. This is guaranteed by the `ArcSwap` mechanism — the swap is a single atomic pointer exchange.

### 9.4 Read-Only Source Relationship

The Console never writes to, modifies, or deletes any file under the protocol taxonomy path, and never issues a non-GET request to the runtime's REST endpoints during ingestion. Protocol taxonomy files are opened in read-only mode; runtime REST calls are strictly `GET /v1/verticals[/{id}]`. The one action that mutates runtime state (session launch, gate resolution, directive injection) is out of scope for the discovery spec and uses gRPC, not the discovery REST endpoints.

### 9.5 Base Role Presence

The three base roles (`coordinator`, `worker`, `observer`) are always present in the index regardless of what the protocol taxonomy files contain or what the runtime's REST endpoint returns. They are protocol constants, not taxonomy-registered entities, and they are inserted into the index before either upstream source is consulted. Base roles have `vertical: None` and empty `tools: []` per `wcon-data-model` §10.4 inv 6.

### 9.6 Deterministic Builds

Given the same protocol-taxonomy source files and the same REST responses from the runtime, two index builds produce identical indexes. The Console sorts the vertical list by `id` (lexicographic) before ingestion — determinism does not depend on the runtime's response order. Protocol-taxonomy file parse order within a directory is lexicographic by filename. No randomness, no dependency on system state beyond the two authoritative sources.

### 9.7 Runtime-as-Registry

The Console does not maintain its own vertical registry. The authoritative list of verticals is whatever `GET /v1/verticals` returns at the time of index build. Adding or removing a vertical is exclusively a runtime concern (restart the runtime with a different `taxonomy.verticals_dir`). The Console observes the change on its next taxonomy reload.

This invariant enforces ADR-001: one source of truth, no filesystem path to vertical manifests in Console configuration, no Console-local copy of vertical state.

### 9.8 Vertical-Scoped Checkpoint Type Names

Checkpoint type names declared by a vertical are scoped to that vertical. Two verticals may declare `checkpoint_types` entries with the same name (e.g., two different `compliance_check` types) without conflict — the index stores them under `VerticalEntry(V1).checkpoint_types[name]` and `VerticalEntry(V2).checkpoint_types[name]` separately. Cross-vertical lookups (search, aggregated views) always carry the owning vertical ID alongside the checkpoint type name.

Protocol-level checkpoint types (`CheckpointTypeEntry` at the top level of the index) occupy a distinct namespace. A vertical's `checkpoint_types` map entry named `artifact` does not shadow the protocol-level `artifact` checkpoint type, and vice versa — they coexist and are queried via different endpoints (§4.1).

### 9.9 Tool Policy Fidelity

For every `VerticalEntry(V).tool_policies[T]`, there is exactly one `ToolEntry(T).policy` with identical content — same `kind`, same `description`, same kind-specific fields. The mirror is maintained at ingestion time and is not recomputed during queries. If a tool appears in multiple verticals (currently not supported by the upstream schema, which scopes tools to one vertical), the mirror becomes undefined — the Console logs a warning and uses the first-seen policy. This is a documented degenerate case, not a supported configuration.

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-architecture | System Architecture | defines taxonomy index component (§4.1), concurrency model (§7), frontend discovery browser (§4.2) |
| wcon-data-model | Data Model | defines taxonomy index schema (§6), `VerticalEntry` projection (§6.1), protocol taxonomy path and runtime address settings (§5.2) |
| wcon-profiles | Profile System | consumes policy metadata (§3.5) for tool validation warnings |
| wcon-sessions | Session Lifecycle | consumes `context_schema` (§2.2) for session launch validation |
| wcon-highway | Highway Integration | consumes `tool_policies` and `checkpoint_types` (§2.2) for refusal event rendering |
| wcon-glossary | Glossary | defines discovery, taxonomy index, taxonomy, vertical, derived role, tool-layer policy, workspace context tag |
| wcon-vision | Product Vision | establishes discovery as a core capability (§2, G1), read-only relationship with taxonomy (BC1), vertical-agnosticism (BC4) |
| wacp-protocol | WACP Protocol Specification | TAXONOMY section defines protocol-taxonomy YAML schema |
| wacp-taxonomy | WACP Taxonomy crate | defines `VerticalManifest` struct and nested types served over REST |
| wacp-transport | WACP Transport crate | defines REST handlers for `GET /v1/verticals[/{id}]` |

*WACP Console -- authored by AAkil98*
