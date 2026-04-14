---
id: wcon-data-model
type: design
status: final
created: 2026-04-09T00:00:00
revised: 2026-04-14T00:00:00
authors: [AKIL Abderrahim, Claude Opus 4.6]
tags: [data-model, persistence, schema, core, verticals, auth]
depends_on: [wcon-architecture, wcon-auth]
---

# WACP Console — Data Model

## Table of Contents

1. Overview
2. Storage Architecture
3. Profile Schema
4. Session Schema
5. Settings & Auth Schema
6. Taxonomy Index Schema
7. Profile Versioning
8. Import/Export Format
9. Migrations and Lifecycle
10. Invariants

---

## 1. Overview

This spec defines the entities the Console manages, their schemas, storage locations, and lifecycle rules. The Console owns five categories of persistent data:

1. **Profiles** — user-created agent configuration bundles, stored in SQLite with optional YAML export
2. **Session records** — historical records of coordination runs, stored in SQLite
3. **Settings** — Console-level configuration, stored in SQLite
4. **Auth entities** — users, browser sessions, API tokens, audit log, login attempts, stored in SQLite (`wcon-auth`)
5. **Taxonomy index** — an in-memory projection of two upstream sources: protocol-taxonomy YAML files from the local filesystem (base/derived roles, protocol-level envelope and checkpoint types) and vertical manifests fetched from the WACP runtime REST API (`GET /v1/verticals[/{id}]`, per ADR-001). Rebuilt on startup and on manual reload.

The storage engine decisions are inherited from `wcon-architecture` §6: SQLite for relational data, filesystem for YAML exports and protocol-taxonomy source files, REST for vertical manifests, in-memory for the taxonomy index and active session state.

This spec does not define the WACP runtime's data model. The Console reads runtime data through gRPC — it never accesses runtime storage directly. Where Console entities reference runtime concepts (workspace IDs, task IDs, trail entries), they store opaque identifiers, not denormalized copies.

## 2. Storage Architecture

### 2.1 SQLite Configuration

The Console uses a single SQLite database file (`console.db`) in a configured data directory.

| Setting | Value | Rationale |
|---------|-------|-----------|
| Journal mode | WAL | Concurrent reads during writes; required by the async backend (`wcon-architecture` §7) |
| Foreign keys | ON | Enforce referential integrity between profiles, sessions, and assignments |
| Busy timeout | 5000ms | Tolerate brief write contention from concurrent API requests |
| Page size | 4096 | Default; no large-blob workloads that would benefit from larger pages |

### 2.2 Storage Boundaries

| Data | Storage | Lifetime | Owner |
|------|---------|----------|-------|
| Profiles (current + history) | SQLite `profiles` table | Permanent until user-deleted | Console |
| Session records | SQLite `sessions` table | Permanent (historical record) | Console |
| Session role assignments | SQLite `session_assignments` table | Permanent (part of session record) | Console |
| Settings | SQLite `settings` table | Permanent until changed | Console |
| Users | SQLite `users` table | Permanent (disabled, never deleted) | Console |
| Browser sessions | SQLite `user_sessions` table | 24h TTL (configurable) | Console |
| API tokens | SQLite `api_tokens` table | Until revoked or expired | Console |
| Audit log | SQLite `audit_log` table | Permanent (append-only) | Console |
| Login attempts | SQLite `login_attempts` table | 24h (garbage-collected) | Console |
| Profile YAML exports | Filesystem (configured directory) | User-managed | Console |
| Protocol taxonomy YAML files | Filesystem (configured path) | External — read-only | WACP upstream |
| Vertical manifests | REST API (`GET /v1/verticals[/{id}]`) | External — fetched on startup/reload | WACP runtime (ADR-001) |
| Taxonomy index | In-memory | Process lifetime (rebuilt on startup/reload) | Console |
| Active session state | In-memory | Session lifetime | Console |

## 3. Profile Schema

A profile is the Console's central user-created entity. It bundles a role reference with LLM configuration, autonomy settings, tool permissions, and resource budgets into a reusable, portable unit.

### 3.1 SQLite Schema

```sql
CREATE TABLE profiles (
    -- Identity
    id          TEXT    NOT NULL,   -- UUID v4, stable across versions
    version     INTEGER NOT NULL,   -- monotonically increasing per id
    
    -- User-facing metadata
    name        TEXT    NOT NULL,   -- human-readable display name
    description TEXT,               -- optional user notes
    tags        TEXT,               -- JSON array of strings: ["swe", "fast", "budget"]
    
    -- Role binding
    role_ref    TEXT    NOT NULL,   -- taxonomy role identifier (e.g., "swe:implementer")
    
    -- LLM configuration
    llm_provider   TEXT    NOT NULL,   -- provider key (e.g., "anthropic", "openai")
    llm_model      TEXT    NOT NULL,   -- model identifier (e.g., "claude-sonnet-4-20250514")
    llm_temperature REAL,              -- 0.0–2.0; NULL means provider default
    llm_max_tokens  INTEGER,           -- max output tokens; NULL means provider default
    
    -- Autonomy
    autonomy    TEXT    NOT NULL DEFAULT 'assisted',
        -- CHECK (autonomy IN ('autonomous', 'assisted', 'supervised'))
    
    -- Tool permissions
    tool_allowlist TEXT,   -- JSON array of tool IDs; NULL means "all role-available tools"
    tool_denylist  TEXT,   -- JSON array of tool IDs; NULL means "deny none"
    
    -- Resource budget
    budget_max_cost_micros    INTEGER,   -- max cost in microdollars; NULL means no limit
    budget_max_tokens         INTEGER,   -- max total tokens (input + output); NULL means no limit
    budget_max_wall_time_ms   INTEGER,   -- max wall-clock duration in ms; NULL means no limit
    budget_warning_threshold  REAL DEFAULT 0.8,   -- fraction (0.0–1.0) at which to warn
    
    -- Ownership & visibility (wcon-auth §5)
    owner_user_id TEXT NOT NULL,    -- FK → users(id); the user who created this profile
    visibility    TEXT NOT NULL DEFAULT 'private',
        -- CHECK (visibility IN ('private', 'shared'))
    
    -- Lifecycle
    is_current  INTEGER NOT NULL DEFAULT 1,   -- 1 if this is the latest version, 0 otherwise
    created_at  TEXT    NOT NULL,   -- ISO 8601 timestamp
    deleted_at  TEXT,               -- ISO 8601 timestamp; NULL while the profile is live
    
    PRIMARY KEY (id, version),
    FOREIGN KEY (owner_user_id) REFERENCES users (id),
    CHECK (autonomy IN ('autonomous', 'assisted', 'supervised')),
    CHECK (visibility IN ('private', 'shared')),
    CHECK (budget_warning_threshold >= 0.0 AND budget_warning_threshold <= 1.0)
);

CREATE INDEX idx_profiles_current ON profiles (id) WHERE is_current = 1 AND deleted_at IS NULL;
CREATE INDEX idx_profiles_role ON profiles (role_ref) WHERE is_current = 1 AND deleted_at IS NULL;
CREATE INDEX idx_profiles_name ON profiles (name) WHERE is_current = 1 AND deleted_at IS NULL;
CREATE INDEX idx_profiles_owner ON profiles (owner_user_id) WHERE is_current = 1 AND deleted_at IS NULL;
```

**Soft-delete rationale.** Historical session assignments (`session_assignments`, §4.2) pin profiles by `(profile_id, profile_version)` via a foreign key. Hard-deleting a profile whose versions are referenced by any session (active, completed, failed, or cancelled) would fail the foreign key constraint — or, if the FK were cascaded, would destroy the immutable history of sessions that ran against the profile. Soft delete resolves the tension: the rows survive, the foreign key stays valid, and queries filter by `deleted_at IS NULL` to hide deleted profiles from the live library.

### 3.2 Field Semantics

**Identity fields:**

- `id` — a UUID v4 assigned at profile creation. Stable across all versions of the same profile. Two rows with the same `id` are versions of the same profile.
- `version` — integer starting at 1, incremented on each save. The pair `(id, version)` is the primary key.

**Role binding:**

- `role_ref` — a taxonomy role identifier using the namespaced format defined by WACP taxonomy (e.g., `"swe:implementer"`, `"swe:reviewer"`). Must resolve to a valid role in the taxonomy index at save time. Base roles use the unnamespaced protocol names: `"coordinator"`, `"worker"`, `"observer"`.

**LLM configuration:**

- `llm_provider` and `llm_model` — identify the LLM the runtime should use when executing this agent. The Console does not validate these against the runtime's available providers — that validation happens at session launch when the runtime receives the configuration.
- `llm_temperature` and `llm_max_tokens` — optional overrides. NULL means the provider or model default applies.

**Autonomy:**

- `autonomy` — one of three presets inherited from WACP (`wcon-glossary` §4): `autonomous` (no gates), `assisted` (selective gates), `supervised` (all gates). Determines gate activation behavior when this profile is used in a session.

**Tool permissions:**

- `tool_allowlist` — JSON array of tool identifiers. When non-NULL, only these tools are available to the agent. Each tool ID must exist in the taxonomy and belong to the same vertical as the profile's role (per `wcon-discovery` §3.4, the upstream manifest does not provide per-role tool mappings, so tool availability is vertical-coarse).
- `tool_denylist` — JSON array of tool identifiers. These tools are removed from the agent's available set. Applied after the allowlist.
- When both are NULL, the agent gets all tools in the role's vertical (base roles — `coordinator`/`worker`/`observer` — have no tools, because tools are vertical-scoped).
- When both are non-NULL, the effective set is: `(allowlist) - (denylist)`.

**Ownership & visibility (`wcon-auth` §5):**

- `owner_user_id` — the `users.id` of the user who created this profile. Set at creation, immutable across all versions. When a profile is versioned (§7), the new version row carries the same `owner_user_id`.
- `visibility` — `"private"` (default) or `"shared"`. Private profiles are visible only to the owner and admins. Shared profiles are readable and usable by all authenticated users, but editable only by the owner and admins. Changing visibility creates a new version (it is a profile edit). See `wcon-auth` §5.2 for the full access matrix.

**Resource budget:**

- Budget fields map to WACP's `ResourceBudget` message fields. The Console stores these as profile-level defaults. Sessions may override them (see §4).
- `budget_warning_threshold` — fraction of each budget limit at which the oversight dashboard shows a warning. Default 0.8 (80%).

**Lifecycle:**

- `is_current` — denormalized flag for query performance. For a live profile (not deleted), exactly one row per `id` has `is_current = 1`. Updated atomically when a new version is inserted (see §7).
- `created_at` — ISO 8601 timestamp of when this version was created.
- `deleted_at` — ISO 8601 timestamp of when the profile was soft-deleted. NULL for live profiles. When set, every version row for that `id` carries the same `deleted_at` (soft delete marks all versions uniformly). Queries that return "live" profiles filter `WHERE deleted_at IS NULL`. Historical session assignments that reference deleted versions remain valid — the FK still resolves because the parent row still exists.

### 3.3 Derived Fields (Not Stored)

These fields are computed at read time, not stored:

| Field | Derivation |
|-------|-----------|
| `display_name` | For private profiles or profiles viewed by their owner: `name`. For shared profiles viewed by other users: `"{owner_display_name}'s {name}"` where `owner_display_name` is looked up from `users(owner_user_id).display_name`. The stored `name` field is never prefixed — the display name is a read-time concern |
| `role_name` | Looked up from taxonomy index by `role_ref` |
| `vertical` | Looked up from `RoleEntry(role_ref).vertical` — the owning vertical of the role (if any) |
| `available_tools` | Computed from the role's vertical tool set (all tools in the owning vertical, per `wcon-discovery` §3.4), filtered by allowlist then denylist |
| `policy_gated_tools` | Subset of `available_tools` whose `ToolEntry.policy` is non-empty — used by `wcon-profiles` §3.2 to emit `TOOL_HAS_RUNTIME_POLICY` warnings at save time |
| `version_count` | `COUNT(*) WHERE id = ?` |
| `latest_version` | `MAX(version) WHERE id = ?` |

## 4. Session Schema

A session record captures the full configuration and lifecycle of a coordination run. Active sessions are also held in memory for real-time state tracking; the SQLite record is the durable historical artifact.

### 4.1 SQLite Schema

```sql
CREATE TABLE sessions (
    -- Identity
    id          TEXT    NOT NULL PRIMARY KEY,   -- UUID v4
    
    -- User-facing metadata
    name        TEXT,               -- optional user-assigned label; NULL → derived as
                                    --   "{vertical} / {workflow}" for display
    
    -- Ownership (wcon-auth §5)
    owner_user_id TEXT NOT NULL,    -- FK → users(id); the user who created this session
    
    -- Configuration (immutable after launch)
    vertical    TEXT    NOT NULL,   -- vertical identifier from registry
    workflow    TEXT    NOT NULL,   -- workflow identifier within the vertical
    context     TEXT,               -- JSON object: vertical-specific context tags
                                    --   (typed per VerticalEntry.context_schema)
                                    --   NULL for verticals with empty context_schema (e.g., SWE)

    -- Runtime mapping
    coordinator_workspace_id TEXT,   -- WACP workspace ID; set at launch
    
    -- Lifecycle
    state       TEXT    NOT NULL DEFAULT 'configuring',
    created_at  TEXT    NOT NULL,   -- ISO 8601
    launched_at TEXT,               -- ISO 8601; set when state → launched
    closed_at   TEXT,               -- ISO 8601; set when state → completed|failed|cancelled
    
    -- Budget overrides (session-level, applied over profile defaults)
    budget_max_cost_micros  INTEGER,   -- NULL means use profile defaults
    budget_max_tokens       INTEGER,
    budget_max_wall_time_ms INTEGER,
    
    FOREIGN KEY (owner_user_id) REFERENCES users (id),
    CHECK (state IN (
        'configuring',   -- user is setting up the session
        'validating',    -- backend is validating configuration
        'launching',     -- backend is creating WACP workspaces
        'active',        -- session is running, trail is streaming
        'completed',     -- all tasks completed successfully
        'failed',        -- session terminated due to failure
        'cancelled'      -- user cancelled the session
    ))
);

CREATE INDEX idx_sessions_state ON sessions (state);
CREATE INDEX idx_sessions_created ON sessions (created_at);
CREATE INDEX idx_sessions_owner ON sessions (owner_user_id);
```

**`context` column** — JSON-encoded map of vertical-specific context tag values, keyed by the field names defined in `VerticalEntry.context_schema` (§6.1) for the session's vertical. Example for a Finance session:

```json
{
  "compliance_scope": "equities",
  "jurisdiction": "SEC"
}
```

Example for an MLOps session:

```json
{
  "compute_budget": 50
}
```

The column is nullable — verticals with empty `context_schema` (e.g., SWE) store NULL. Validation rules for required/optional fields and type/enum/range constraints live in `wcon-sessions` §3.1 (`MISSING_CONTEXT`, `INVALID_CONTEXT`). The schema is typed per-vertical, so a single context column with JSON is a better fit than a normalized sibling table — queryability of individual context fields is not a requirement and normalization would force a wide null-heavy schema.

Rationale for folding MLOps compute budget into `context` instead of extending the budget columns: MLOps compute budget is GPU-hours keyed to the `train_launch` tool's `max_hours` argument (see `wcon-data-model` §4.2 note), not a Console-enforced resource limit like cost/tokens/wall-time. It flows through session context to the runtime for tool-layer refusal, not through `ResourceBudget`.

### 4.2 Session Assignments

Each session assigns profiles to role slots defined by the workflow. This is a separate table because a session has multiple role assignments.

```sql
CREATE TABLE session_assignments (
    -- Identity
    id          TEXT    NOT NULL PRIMARY KEY,   -- UUID v4
    session_id  TEXT    NOT NULL,
    
    -- Assignment
    role_ref        TEXT    NOT NULL,   -- role bound to this slot
    stage_id        TEXT,               -- stage identifier when operating in Mode A
                                        --   (per-stage slot derivation, `wcon-sessions` §2.4);
                                        --   NULL in Mode B (per-role slot derivation)
    slot_position   INTEGER NOT NULL,   -- order within the session's assignment list;
                                        --   used to distinguish two assignments for the same
                                        --   role in Mode B or two slots with the same role_ref
                                        --   across different stages in Mode A
    profile_id      TEXT    NOT NULL,   -- profile assigned to this slot
    profile_version INTEGER NOT NULL,   -- pinned version at assignment time
    
    -- Runtime mapping
    workspace_id TEXT,   -- WACP workspace ID; set at launch
    
    -- Per-assignment budget overrides
    budget_max_cost_micros  INTEGER,
    budget_max_tokens       INTEGER,
    budget_max_wall_time_ms INTEGER,
    
    FOREIGN KEY (session_id) REFERENCES sessions (id) ON DELETE CASCADE,
    FOREIGN KEY (profile_id, profile_version) REFERENCES profiles (id, version)
);

CREATE INDEX idx_assignments_session ON session_assignments (session_id);
CREATE UNIQUE INDEX idx_assignments_slot ON session_assignments (session_id, slot_position);
```

**Slot position.** `slot_position` is a zero-based index into the session's assignment list, establishing a stable order for Mode A stage iteration (workspaces are created in stage order at launch) and for Mode B deterministic iteration. Two assignments in the same session cannot share a `slot_position`; the unique index enforces this.

**`stage_id` nullability.** In Mode A (per-stage slot derivation), `stage_id` is set from the workflow's stage list. In Mode B (per-role fallback), `stage_id` is NULL — the slot is identified by `role_ref` + `slot_position` alone. `wcon-sessions` §2.4 describes when each mode applies. Queries that need to render stage-aware assignment UIs must tolerate NULL `stage_id` and fall back to the role-aware display per `wcon-ui` §6.2 step 3.

### 4.3 Session State Machine

```
configuring ──▶ validating ──▶ launching ──▶ active ──┬──▶ completed
      │              │              │           │      ├──▶ failed
      │              │              │           │      └──▶ cancelled
      │              ▼              │           │
      │     (back to configuring    │           │
      │      on validation failure) │           │
      │              │              │           │
      ▼              ▼              ▼           ▼
  cancelled      cancelled    cancelled /  cancelled
                                failed
```

Allowed transitions:

| From | To | Trigger |
|------|----|---------|
| configuring | validating | User clicks "launch" |
| configuring | cancelled | User discards the session before launch |
| validating | configuring | Validation fails — user returns to editor |
| validating | launching | Validation passes |
| validating | cancelled | User cancels during validation |
| launching | active | All WACP workspaces created, streams subscribed |
| launching | failed | Runtime unreachable or workspace creation fails |
| launching | cancelled | User cancels during launch (best-effort cleanup of partially-created workspaces) |
| active | completed | All tasks in the workflow reach `COMPLETED` or `INTEGRATED` status |
| active | failed | Unrecoverable failure (coordinator crash, runtime disconnect) |
| active | cancelled | User cancels the session |

Terminal states: `completed`, `failed`, `cancelled`. No transitions out of terminal states. A session can be cancelled from any non-terminal state — pre-launch cancellation is a discard (no runtime cleanup needed); post-launch cancellation sends a cancel signal to the coordinator.

### 4.4 Field Semantics

**Configuration fields:**

- `vertical` — identifier of the vertical from the vertical registry. Immutable after launch — the vertical defines the available roles, workflows, context schema, tool policies, and checkpoint types.
- `workflow` — identifier of the specific workflow within the vertical. Immutable after launch.
- `context` — JSON-encoded map of vertical-specific context tag values (see §4.1). Required for verticals with a non-empty `context_schema`; NULL for verticals without one. Mutable during `configuring` state via `PATCH /api/sessions/:id` (`wcon-sessions` §2.2). Immutable after launch — the runtime pins context at dispatch time.

**Runtime mapping:**

- `coordinator_workspace_id` — the WACP workspace ID of the coordinator workspace created at launch. Set once during the `launching` transition. Used to correlate Console session with runtime state.
- `workspace_id` on assignments — the WACP workspace ID of each worker workspace. Set during launch.

**Budget overrides:**

- Session-level budgets override profile defaults for all assignments in the session. Per-assignment budgets override both profile defaults and session-level budgets. The precedence is: assignment override > session override > profile default.
- `ResourceBudget` covers cost (micros), total tokens, and wall-clock duration — the three limits WACP enforces at the workspace level. It does **not** cover vertical-specific compute metrics such as MLOps GPU-hours (`compute_budget`). Those are session context (see `context` column above) and are enforced by tool-layer refusal in the runtime (`tool_policies[train_launch].kind == "budget_limited"`).

## 5. Settings & Auth Schema

This section covers Console-wide settings (§5.1–5.2) and auth entity tables (§5.3–5.7). The auth schemas implement the logical model defined in `wcon-auth`; this section defines the physical tables.

Console-wide configuration stored as key-value pairs.

### 5.1 SQLite Schema

```sql
CREATE TABLE settings (
    key         TEXT NOT NULL PRIMARY KEY,
    value       TEXT NOT NULL,   -- JSON-encoded value
    updated_at  TEXT NOT NULL    -- ISO 8601
);
```

### 5.2 Known Keys

| Key | Value type | Description | Default |
|-----|-----------|-------------|---------|
| `runtime.agent_address` | string | gRPC address of the runtime's AgentService | `"[::1]:9090"` |
| `runtime.highway_address` | string | gRPC address of the runtime's HighwayService | `"[::1]:9091"` |
| `runtime.coordinator_address` | string | gRPC address of the runtime's CoordinatorService | `"[::1]:9092"` |
| `runtime.rest_address` | string | REST base URL of the runtime gateway (used for `GET /v1/verticals[/{id}]`) | `"http://[::1]:9093"` |
| `runtime.auth_method` | string | Authentication method for runtime connection | `"none"` |
| `runtime.auth_credential` | string | Credential for runtime authentication (PSK, API key) | `""` |
| `taxonomy.path` | string | Filesystem path to protocol-level taxonomy YAML files (base/derived roles, protocol envelope and checkpoint types) | `"../wacp/protocol/taxonomy"` |
| `export.directory` | string | Filesystem path for profile YAML exports | `"./exports"` |
| `ui.theme` | string | Frontend theme preference | `"system"` |
| `ui.trail_buffer_size` | integer | Max trail entries held in the dashboard before eviction | `1000` |
| `auth.session_ttl_hours` | integer | Browser session expiry in hours (`wcon-auth` §3.2) | `24` |

**Note on `verticals.path` removal.** Earlier drafts included a `verticals.path` setting pointing at `../wacp/ecosystem` for direct filesystem reads. That setting is removed per ADR-001 (`SPEC_BUILD.md`). Vertical manifests are served by the runtime over REST — the Console has no configuration knob for where manifests live on disk, because it never reads them from disk.

**Note on per-service gRPC addresses.** The upstream runtime runs three gRPC services on three separate Tonic servers (see `wacp-runtime/src/config.rs`), so the Console configures three separate addresses. The defaults match the runtime's defaults. If the runtime later multiplexes all services on a single port, the Console operator sets all three keys to the same address — no Console code change required.

The settings table is extensible — new keys can be added without schema migration. Values are JSON-encoded to support strings, numbers, booleans, and arrays uniformly.

**Reading an absent key** returns the default shown in the table above (not `null`, not an error). The default is applied in application code when the SQL query returns no row. Settings endpoints (`wcon-api` §10) always return a value, so clients never need to handle "key not set" separately from "key set to default".

**Writing a known key** validates the new value against the key's expected type (string/integer/boolean) and rejects malformed JSON with `422 Unprocessable Entity`. Writing an unknown key succeeds without type validation — the extensibility trade-off accepts that unknown keys are only meaningful to code that knows how to interpret them.

### 5.3 Users

```sql
CREATE TABLE users (
    id                   TEXT    NOT NULL PRIMARY KEY,   -- UUID v4
    username             TEXT    NOT NULL,               -- 3–64 chars, [a-zA-Z0-9_.-]
    username_lower       TEXT    NOT NULL UNIQUE,         -- lowercased for case-insensitive lookup
    display_name         TEXT    NOT NULL,               -- 1–128 chars
    password_hash        TEXT    NOT NULL,               -- Argon2id PHC-format string
    console_role         TEXT    NOT NULL DEFAULT 'operator',
    must_change_password INTEGER NOT NULL DEFAULT 1,     -- boolean: 1 = forced change on next login
    disabled_at          TEXT,                            -- ISO 8601; NULL while active
    created_at           TEXT    NOT NULL,               -- ISO 8601
    updated_at           TEXT    NOT NULL,               -- ISO 8601
    
    CHECK (console_role IN ('admin', 'operator', 'viewer'))
);
```

See `wcon-auth` §2 for the identity model. Users are never deleted — only disabled (`disabled_at` set). The `username_lower` column enforces case-insensitive uniqueness without a custom collation.

### 5.4 Browser Sessions

```sql
CREATE TABLE user_sessions (
    id          TEXT    NOT NULL PRIMARY KEY,   -- UUID v4
    user_id     TEXT    NOT NULL,               -- FK → users(id)
    token_hash  TEXT    NOT NULL UNIQUE,        -- SHA-256 of cookie value
    ip          TEXT    NOT NULL,               -- client IP at login
    user_agent  TEXT    NOT NULL,               -- client User-Agent at login
    created_at  TEXT    NOT NULL,               -- ISO 8601
    expires_at  TEXT    NOT NULL,               -- ISO 8601; absolute expiry
    
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_user_sessions_user ON user_sessions (user_id);
CREATE INDEX idx_user_sessions_expires ON user_sessions (expires_at);
```

See `wcon-auth` §3.1–3.2 for browser session lifecycle. The `ON DELETE CASCADE` clause is defensive — users are never deleted in normal operation, but if the database is manually cleaned, orphan sessions should not survive.

### 5.5 API Tokens

```sql
CREATE TABLE api_tokens (
    id          TEXT    NOT NULL PRIMARY KEY,   -- UUID v4
    user_id     TEXT    NOT NULL,               -- FK → users(id)
    name        TEXT    NOT NULL,               -- user-assigned label
    token_hash  TEXT    NOT NULL UNIQUE,        -- SHA-256 of the full token (wcon_t_...)
    created_at  TEXT    NOT NULL,               -- ISO 8601
    expires_at  TEXT,                            -- ISO 8601; NULL means no expiry
    last_used_at TEXT,                           -- ISO 8601; updated on each successful auth
    revoked_at  TEXT,                            -- ISO 8601; when set, token is rejected
    
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_api_tokens_user ON api_tokens (user_id);
```

See `wcon-auth` §3.3–3.4 for API token lifecycle. Token names are not unique — a user may have multiple tokens with the same label. The full token value is never stored; only the SHA-256 hash is persisted.

### 5.6 Audit Log

```sql
CREATE TABLE audit_log (
    id          TEXT    NOT NULL PRIMARY KEY,   -- UUID v4
    user_id     TEXT    NOT NULL,               -- FK → users(id); the actor
    timestamp   TEXT    NOT NULL,               -- ISO 8601 UTC
    action      TEXT    NOT NULL,               -- machine-readable (e.g., "profile.create")
    target_kind TEXT    NOT NULL,               -- entity type: user, profile, session, token, settings
    target_id   TEXT    NOT NULL,               -- entity ID
    detail      TEXT,                            -- JSON; action-specific structured data
    ip          TEXT    NOT NULL,               -- client IP
    user_agent  TEXT    NOT NULL,               -- client User-Agent
    
    FOREIGN KEY (user_id) REFERENCES users (id)
);

CREATE INDEX idx_audit_log_timestamp ON audit_log (timestamp);
CREATE INDEX idx_audit_log_user ON audit_log (user_id);
CREATE INDEX idx_audit_log_action ON audit_log (action);
CREATE INDEX idx_audit_log_target ON audit_log (target_kind, target_id);
```

See `wcon-auth` §10 for the audit log specification and the full action list. The audit log FK does not cascade — if a user record is somehow removed, audit entries must survive (they are the forensic record). The `detail` column is nullable: some actions (e.g., `auth.logout`) carry no additional context.

### 5.7 Login Attempts

```sql
CREATE TABLE login_attempts (
    id          TEXT    NOT NULL PRIMARY KEY,   -- UUID v4
    ip          TEXT    NOT NULL,               -- client IP
    username    TEXT    NOT NULL,               -- attempted username (may not match a real user)
    attempted_at TEXT   NOT NULL,               -- ISO 8601
    success     INTEGER NOT NULL,               -- boolean: 1 = successful login, 0 = failed
    
    CHECK (success IN (0, 1))
);

CREATE INDEX idx_login_attempts_ip ON login_attempts (ip, attempted_at);
CREATE INDEX idx_login_attempts_username ON login_attempts (username, attempted_at);
```

See `wcon-auth` §9 for rate limiting and account lockout rules. This table exists solely for rate-limit/lockout decisions — it is not part of the audit log. Rows older than 24 hours are garbage-collected by a background task. The indexes support the two rate-limit dimensions: per-IP (20 attempts/15 min) and per-account (5 failed/15 min).

## 6. Taxonomy Index Schema

The taxonomy index is an in-memory data structure, not a SQLite table. It is built from two sources: protocol-taxonomy YAML files on the local filesystem (base/derived roles, protocol-level envelope and checkpoint types) and vertical manifests fetched from the WACP runtime via `GET /v1/verticals[/{id}]` (per ADR-001). The index is rebuilt on startup and on manual reload.

The `VerticalEntry` portion of the index is a one-to-one projection of `wacp-taxonomy::VerticalManifest` as served by `GET /v1/verticals/{id}`. Field names and shapes mirror the upstream struct so the Console and the runtime speak the same vocabulary. Fields the Console does not yet understand are preserved as opaque JSON to support forward compatibility (`wcon-discovery` §2.2.3).

### 6.1 Index Structure

```
TaxonomyIndex
├── roles: HashMap<String, RoleEntry>
│   ├── key: role identifier (e.g., "coordinator", "finance:portfolio_manager")
│   └── value: RoleEntry
│       ├── name: String
│       ├── base_role: "coordinator" | "worker" | "observer"
│       ├── extends: Option<String>          -- parent role if derived
│       ├── capabilities_added: Vec<String>
│       ├── capabilities_removed: Vec<String>
│       ├── tools: Vec<String>               -- tool IDs available to this role
│       │                                      (for vertical roles: every tool in the
│       │                                       vertical, per `wcon-discovery` §3.4)
│       └── vertical: Option<String>         -- which vertical defines this role, if any
│
├── tools: HashMap<String, ToolEntry>
│   ├── key: tool identifier
│   └── value: ToolEntry
│       ├── name: String
│       ├── description: String
│       ├── vertical: Option<String>         -- owning vertical (None for protocol-level tools, if any)
│       ├── roles: Vec<String>               -- roles associated with this tool (all vertical roles
│       │                                      in the tool's vertical, per `wcon-discovery` §3.4)
│       └── policy: Option<ToolPolicy>       -- present when tool_policies[name] is set in
│                                              the owning vertical's manifest
│
├── envelope_types: HashMap<String, EnvelopeTypeEntry>
│   ├── key: type name
│   └── value: EnvelopeTypeEntry
│       ├── name: String
│       ├── sender_roles: Vec<String>
│       └── receiver_roles: Vec<String>
│
├── checkpoint_types: HashMap<String, CheckpointTypeEntry>
│   ├── key: type name (protocol-level types only — e.g., "artifact")
│   └── value: CheckpointTypeEntry
│       ├── name: String
│       └── allowed_roles: Vec<String>
│
└── verticals: HashMap<String, VerticalEntry>
    ├── key: vertical identifier (e.g., "finance", "healthcare", "swe")
    └── value: VerticalEntry
        ├── id: String
        ├── name: String
        ├── load_error: Option<String>        -- None for successfully loaded verticals;
        │                                        Some(error_message) for stubs whose
        │                                        GET /v1/verticals/{id} failed. When set,
        │                                        all collection fields below are empty/default.
        │                                        The discovery browser renders the stub with the
        │                                        error message; session launcher and profile
        │                                        validation skip stubs.
        ├── defining_constraint: String       -- one-sentence description of the distinctive rule
        ├── roles: Vec<String>                -- role IDs defined by this vertical
        ├── context_schema: HashMap<String, ContextField>
        │   ├── key: context field name (e.g., "compliance_scope", "compute_budget")
        │   └── value: ContextField
        │       ├── field_type: "string" | "number" | "boolean" | "enum"
        │       ├── required: bool
        │       ├── description: String
        │       ├── enum_values: Option<Vec<String>>    -- for field_type == "enum"
        │       └── default: Option<serde_json::Value>  -- pre-fill for launch wizard
        ├── tool_policies: HashMap<String, ToolPolicy>
        │   ├── key: tool name (e.g., "trade_execute", "train_launch")
        │   └── value: ToolPolicy
        │       ├── kind: "requires_checkpoint" | "requires_gate"
        │       │       | "budget_limited" | "classification_gated"
        │       ├── description: String
        │       ├── checkpoint_type: Option<String>         -- requires_checkpoint
        │       ├── matching_field: Option<String>          -- requires_checkpoint
        │       ├── expires_after_ms: Option<u64>           -- requires_checkpoint
        │       ├── gate_condition: Option<String>          -- requires_gate
        │       ├── budget_field: Option<String>            -- budget_limited
        │       ├── blocked_classifications: Option<Vec<String>>  -- classification_gated
        │       └── override_flag: Option<String>           -- classification_gated
        ├── checkpoint_types: HashMap<String, CheckpointSchema>
        │   ├── key: checkpoint type name (e.g., "compliance_check", "phi_access_grant")
        │   └── value: CheckpointSchema
        │       ├── description: String
        │       ├── fields: Vec<CheckpointField>
        │       │   ├── name: String
        │       │   ├── field_type: "string" | "number" | "boolean" | "enum"
        │       │   ├── description: String
        │       │   └── enum_values: Option<Vec<String>>
        │       └── required_by: Vec<String>    -- tools whose policy references this type
        │                                          (populated by `wcon-discovery` §3.3 cross-linking)
        ├── quality_criteria: Vec<QualityCriterion>
        │   ├── id: String
        │   ├── name: String
        │   ├── description: String
        │   └── weight: f64             -- 1.0 = equal weight
        ├── task_types: Vec<TaskTypeDescriptor>
        │   ├── id: String
        │   ├── name: String
        │   ├── description: String
        │   ├── workflow_id: String     -- target workflow
        │   └── keywords: Vec<String>   -- representative keywords for search/detection
        ├── workflows: Vec<WorkflowSummary>
        │   ├── id: String
        │   ├── name: String
        │   ├── description: String
        │   ├── stage_count: u32
        │   └── gated_stage_count: u32  -- note: per-stage detail not in manifest
        ├── default_profiles: Vec<ProfileSummary>
        │   ├── role_id: String
        │   └── autonomy: "gated" | "autonomous"
        └── raw_manifest: serde_json::Value   -- full deserialized manifest, preserved verbatim
                                                (source for search indexing over unknown fields,
                                                 and for a "view raw manifest" debug affordance
                                                 in the vertical detail view — not part of the
                                                 standard vertical detail API response)
```

**Cross-reference resolution.** The ingestion builder (`wcon-discovery` §3.3) populates:
- `ToolEntry.policy` from `VerticalEntry.tool_policies[tool_name]` (if present).
- `CheckpointSchema.required_by` from `VerticalEntry.tool_policies.values()` where `kind == "requires_checkpoint"` and `checkpoint_type` matches.
- `ToolEntry.vertical` and `ToolEntry.roles` from the tool's owning vertical.

**Note on `VerticalEntry.roles`.** The upstream `VerticalManifest` does not carry an explicit role list. The Console synthesizes `VerticalEntry.roles` from `profiles[].role_id` — one role per `ProfileSummary`, deduplicated. This matches the upstream convention: every vertical ships at least one default profile per role, so iterating the `profiles[]` list enumerates every role the vertical declares.

Limitation: a vertical that declares a role but ships no default profile for it would have that role missing from `VerticalEntry.roles`. This is not observed in any current ecosystem vertical (SWE, DevOps, MLOps, Finance, Healthcare, Analytics, DataSci all ship default profiles for every role). When the upstream manifest later adds an explicit role list (or the per-role tool mappings noted in `wcon-discovery` §3.4), this synthesis is replaced with a direct projection and the limitation disappears.

The synthesized list is sorted lexicographically by role ID for determinism (`wcon-discovery` §9.6).

**Note on missing workflow stage detail.** `WorkflowSummary` carries only counts. The Console cannot render a workflow-stage DAG from the manifest alone — it needs per-stage information (role, dependencies, gated flag) that the upstream TypeScript `VerticalWorkflow.stages` exposes but the manifest does not. `wcon-ui` §4.5 and §6.2 address this by showing a simplified card (name + description + stage/gate counts) and deferring full DAG rendering until the upstream manifest is extended or a supplementary endpoint is added.

### 6.2 Index Operations

| Operation | Input | Output | Complexity |
|-----------|-------|--------|------------|
| Get role | role ID | `Option<RoleEntry>` | O(1) |
| List roles | filter (base_role, vertical) | `Vec<RoleEntry>` | O(n) scan with filter |
| Get tool | tool name | `Option<ToolEntry>` | O(1) |
| List tools | filter (vertical, has_policy) | `Vec<ToolEntry>` | O(n) scan with filter |
| Tools for role | role ID | `Vec<ToolEntry>` | O(k) where k = tools in the role's vertical (per `wcon-discovery` §3.4) |
| Get vertical | vertical ID | `Option<VerticalEntry>` | O(1) |
| List verticals | — | `Vec<VerticalEntry>` | O(n) |
| Get context schema | vertical ID | `Option<HashMap<String, ContextField>>` | O(1) lookup |
| Get tool policies | vertical ID | `Option<HashMap<String, ToolPolicy>>` | O(1) lookup |
| Get vertical checkpoint types | vertical ID | `Option<HashMap<String, CheckpointSchema>>` | O(1) lookup |
| Get quality criteria | vertical ID | `Option<Vec<QualityCriterion>>` | O(1) lookup |
| Get task types | vertical ID | `Option<Vec<TaskTypeDescriptor>>` | O(1) lookup |
| Workflows for vertical | vertical ID | `Option<Vec<WorkflowSummary>>` | O(1) lookup + O(w) copy |
| Get workflow | vertical ID + workflow ID | `Option<WorkflowSummary>` | O(1) + O(w) scan within vertical |
| Policy for tool | tool name | `Option<ToolPolicy>` | O(1) via `ToolEntry.policy` — resolves cross-vertical lookups in constant time |
| Checkpoint schema | vertical ID + checkpoint type name | `Option<CheckpointSchema>` | O(1) + O(1) nested lookup |
| Search | query string, optional type filter | `Vec<SearchResult>` | O(n) scan; acceptable for taxonomy-scale data |
| Reload | — | new `TaxonomyIndex` | Full rebuild; swapped atomically via `ArcSwap` |

All operations are read-only against the current index snapshot. Concurrent reads require no synchronization; rebuilds happen against a fresh `TaxonomyIndex` instance that atomically replaces the old one (§6.3). Readers holding a reference to the old index continue using it until they drop their `Arc`.

### 6.3 Concurrency

The taxonomy index is immutable after construction. Read access requires no synchronization. Reload builds a new index on a background task and swaps it in atomically using `ArcSwap` (`wcon-architecture` §7). Readers holding a reference to the old index continue using it until they drop their `Arc`.

## 7. Profile Versioning

### 7.1 Strategy

Profile versioning uses an append-only model: every save creates a new row with an incremented `version` number. Previous versions are retained for historical reference and rollback.

### 7.2 Version Lifecycle

**Create:** Insert a new row with a fresh UUID and `version = 1`, `is_current = 1`, `deleted_at = NULL`.

**Update:** Only valid on a live profile (`deleted_at IS NULL`). Within a transaction:
1. Check that `deleted_at IS NULL` for the existing current row. If not, return 404 (`wcon-profiles` §2.2 — soft-deleted profiles cannot be updated; they are recoverable only via export/import).
2. Set `is_current = 0` on the existing current row for this `id`.
3. Insert a new row with the same `id`, `version = previous + 1`, `is_current = 1`, `deleted_at = NULL`.

**Delete:** Soft delete — set `deleted_at = now()` on every row for this `id`. The rows survive to preserve foreign-key integrity with `session_assignments` (§4.2). Deleted profiles are filtered from live library queries (§3.2 index predicates, `wcon-profiles` §8.1) but remain visible in historical session detail views (`wcon-sessions` §9.4). Undelete is not supported — a mistakenly deleted profile is reconstructed by import (the exported YAML is the operational backup).

**Rollback to version N:** Only valid on a live profile (`deleted_at IS NULL`). Within a transaction:
1. Verify the profile is not soft-deleted; return 404 if it is.
2. Set `is_current = 0` on the existing current row.
3. Copy the row for `(id, N)` with `version = current_max + 1`, `is_current = 1`, `deleted_at = NULL`, excluding `deleted_at` from the copied fields (`wcon-profiles` §5.3).

Rollback creates a new version rather than mutating the `is_current` flag on the old row. The version history is strictly append-only for all columns except `is_current` (toggled during transitions) and `deleted_at` (set uniformly during soft delete).

### 7.3 Session Pinning

When a profile is assigned to a session (`session_assignments`), the assignment records both `profile_id` and `profile_version`. This pins the session to the exact profile configuration at assignment time. Subsequent profile edits do not affect running or completed sessions.

## 8. Import/Export Format

Profiles are portable as YAML files. The YAML format is the external representation — SQLite is the internal representation. They carry the same information in different shapes.

### 8.1 YAML Structure

```yaml
# WACP Console Profile
profile:
  name: "Fast Implementer"
  description: "High-autonomy implementer with aggressive budget"
  tags: ["swe", "fast"]
  
  role: "swe:implementer"
  
  llm:
    provider: "anthropic"
    model: "claude-sonnet-4-20250514"
    temperature: 0.3
    max_tokens: 8192
  
  autonomy: "autonomous"
  
  tools:
    allow: ["code_edit", "file_read", "file_write", "terminal"]
    deny: ["browser"]
  
  budget:
    max_cost_micros: 500000
    max_tokens: 100000
    max_wall_time_ms: 300000
    warning_threshold: 0.8
```

### 8.2 Export Rules

- The exported YAML does not include `id`, `version`, `is_current`, `created_at`, `owner_user_id`, or `visibility`. Identity and lifecycle fields are internal; ownership and visibility are instance-specific (`wcon-auth` §5) — an imported profile is always owned by the importer with `visibility = "private"`.
- NULL values are omitted from the YAML (e.g., if `llm_temperature` is NULL, the `temperature` key is absent).
- Tags are exported as a YAML array. An empty tag list is omitted.
- Tool lists: if both allowlist and denylist are NULL, the `tools` key is omitted entirely.

### 8.3 Import Rules

- On import, a new `id` (UUID v4) is generated. The imported profile is a new entity, not a continuation of the original.
- `version` is set to 1, `is_current = 1`, `deleted_at = NULL`.
- `role_ref` is validated against the taxonomy index. If the role does not exist, import fails with `UNKNOWN_ROLE`.
- Tool IDs in allowlist/denylist are validated against the role's **owning vertical** (not the role itself, per `wcon-discovery` §3.4 vertical-coarse mapping). A tool that belongs to a different vertical than the role fails with `TOOL_NOT_IN_ROLE_VERTICAL` (violation on allowlist; non-blocking warning on denylist). A tool that does not exist in the taxonomy fails with `UNKNOWN_TOOL`.
- Name uniqueness is checked against **live** profiles only (`deleted_at IS NULL`). Reusing a soft-deleted profile's name is allowed.
- Missing optional fields (`description`, `tags`, `temperature`, `max_tokens`, budget fields) default to NULL.
- `autonomy` defaults to `"assisted"` if absent.
- Non-blocking warnings (`TOOL_HAS_RUNTIME_POLICY` for policy-gated tools, autonomous-worker warning) do not block import; they surface in the response body alongside the created profile (`wcon-profiles` §7.3).

### 8.4 Round-Trip Guarantee

Export → import produces a profile that is functionally identical to the original: same role binding, LLM configuration, autonomy, tool permissions, and budget. Only internal fields (`id`, `version`, timestamps) differ.

### 8.5 Vertical Context on Export

Profile YAML does **not** carry vertical-specific context tags (environment, compute_budget, compliance_scope, etc.). Those are session-level, not profile-level — they are supplied at session launch time and stored in `sessions.context` (§4.1). A profile is reusable across sessions with different context values.

Profile YAML also does **not** carry vertical-specific tool-policy metadata. Tool-layer policies are enforced by the runtime against the owning vertical's live manifest, not against anything the profile remembers. Importing a profile for `finance:portfolio_manager` on a Console whose runtime serves a Finance vertical with a differently-shaped `compliance_check` policy is expected to work — the runtime validates at session launch, not at profile import.

The one case where import might surface a surprise: if a profile's `tool_allowlist` contains a tool whose owning vertical exposes a policy that was not present when the profile was exported, the import succeeds (the tool exists) but the imported profile's save warning (`wcon-profiles` §3.2) surfaces the new policy. This is informational — it does not block import or session launch.

## 9. Migrations and Lifecycle

### 9.1 Schema Initialization

On first startup, the Console creates the SQLite database and applies the initial schema (tables from §3, §4, §5 including auth tables). Schema creation is idempotent — if the tables exist, no action is taken. After schema initialization, the bootstrap flow (`wcon-auth` §6) runs if the `users` table is empty.

### 9.2 Schema Versioning

The database carries a schema version in the `settings` table under the key `schema.version`. Each release that modifies the schema increments this version and includes a migration function that transforms `version N` to `version N+1`.

Migration sequence on startup:
1. Read `schema.version` from settings (default 0 if absent — fresh database).
2. Apply all pending migrations in order.
3. Update `schema.version` to the current application schema version.

Migrations are forward-only. Downgrade is not supported — restore from backup if needed.

### 9.3 Data Retention

| Data | Retention policy |
|------|-----------------|
| Profile current versions | Retained until user-deleted |
| Profile historical versions | Retained indefinitely; future cleanup policy TBD |
| Session records (all states) | Retained indefinitely; historical value |
| Settings | Retained until changed |
| Users | Permanent (disabled, never deleted) |
| Browser sessions | Expired sessions cleaned on access; 24h TTL |
| API tokens | Retained until revoked; revoked tokens retained for audit |
| Audit log | Permanent (append-only; operator may truncate offline) |
| Login attempts | Garbage-collected after 24h by background task |
| Taxonomy index | Rebuilt on each startup; no persistence needed |

## 10. Invariants

These rules must hold at all times. Violation of any invariant is a bug.

### 10.1 Profile Invariants

1. **Exactly one current version per live profile.** For every distinct `id` where `deleted_at IS NULL`, exactly one row has `is_current = 1`. Deleted profiles may have multiple rows (one per historical version) but no live `is_current` is expected to be queried.
2. **Monotonic versions.** For a given `id`, version numbers are strictly increasing with no gaps.
3. **Valid role reference.** A profile cannot be saved (created, updated, or imported) if `role_ref` does not resolve to a role in the taxonomy index.
4. **Valid tool references.** Every tool ID in `tool_allowlist` and `tool_denylist` must exist in the taxonomy index and must belong to the same vertical as the profile's `role_ref` (per §3.2 and `wcon-discovery` §3.4). Tool IDs outside the role's vertical are rejected regardless of whether they exist somewhere in the taxonomy.
5. **Non-empty effective tool set.** After applying allowlist and denylist, the resulting tool set must not be empty. A profile that disables all tools in its vertical is invalid.
6. **Immutable history.** Existing version rows are never mutated except for the `is_current` flag during version transitions and the `deleted_at` column during soft delete. Every other column of an existing `(id, version)` row is write-once.
7. **Uniform soft delete.** All version rows for a given `id` share the same `deleted_at` value (either all NULL or all set to the same timestamp). Partial deletion of some versions is not supported.
8. **Immutable ownership.** All version rows for a given `id` share the same `owner_user_id`. Ownership is set at creation and never changes (`wcon-auth` §5.1).
9. **Valid visibility.** `visibility` is `"private"` or `"shared"`. Access rules follow `wcon-auth` §5.2.

### 10.2 Session Invariants

1. **Valid state transitions.** Session state changes must follow the state machine in §4.3. No transition outside the defined edges.
2. **Immutable configuration after launch.** The `vertical`, `workflow`, `context`, and `session_assignments` are immutable once the session leaves the `configuring` state.
3. **Pinned profile versions.** Session assignments reference a specific `(profile_id, profile_version)` pair. The referenced version must exist in the `profiles` table (soft-deleted versions count as existing — see §10.1 inv 6 and §7.2 soft delete).
4. **Complete role coverage.** A session cannot transition from `validating` to `launching` unless every role slot defined by the workflow's stage-aware mode, or every role defined by the vertical in the role-aware fallback mode, has an assigned profile. See `wcon-sessions` §2.4 for the Mode A / Mode B distinction.
5. **Terminal state finality.** Sessions in `completed`, `failed`, or `cancelled` states cannot transition to any other state.
6. **Complete context coverage.** A session cannot transition from `validating` to `launching` unless every required field in the vertical's `context_schema` has a value in the session's `context` column that satisfies the field's type/enum/range constraints. Enforced by `wcon-sessions` §3.1 validation checks `MISSING_CONTEXT` and `INVALID_CONTEXT`.
7. **Immutable ownership.** `owner_user_id` is set at session creation and never changes (`wcon-auth` §5.1).

### 10.3 Settings Invariants

1. **Unique keys.** The `key` column is the primary key; duplicates are impossible at the schema level.
2. **Valid JSON values.** The `value` column contains valid JSON. Malformed JSON is rejected at the application layer before write.

### 10.4 Taxonomy Index Invariants

1. **Consistent after build.** The taxonomy index is either fully built and consistent, or the previous index is visible. There is no partially-built state visible to readers (guaranteed by `ArcSwap`, §6.3).
2. **Role inheritance resolved.** Every derived role in the index has its capabilities fully resolved against its base role. Consumers of the index do not need to walk inheritance chains.
3. **Tool-role consistency (vertical-coarse).** If a tool T appears in `RoleEntry(R).tools`, then `ToolEntry(T).roles` contains R, and vice versa. Per the `wcon-discovery` §3.4 relaxation, this bidirectionality collapses to "R and T share an owning vertical" — every role in a vertical lists every tool in that vertical. The invariant is still bidirectional but no longer fine-grained.
4. **Tool policy cross-references.** For every `VerticalEntry.tool_policies[T]`, the corresponding `ToolEntry(T).policy` is populated with the same policy content (same `kind`, `description`, and kind-specific fields). For every `requires_checkpoint` policy whose `checkpoint_type` resolves to a declared vertical checkpoint type in the same vertical, the bidirectional link to `CheckpointSchema.required_by` is populated. Unresolved checkpoint-type references are recorded with an unresolved marker but do not invalidate the vertical.
5. **Vertical manifest fidelity.** The `VerticalEntry` for a given vertical is a lossless projection of the manifest returned by `GET /v1/verticals/{id}` at the time of the last successful build — modulo forward-compatibility handling of unknown fields (`wcon-discovery` §2.2.3). The Console does not synthesize context fields, tool policies, or checkpoint types that were not in the upstream manifest.
6. **Base roles always present.** The three base roles (`coordinator`, `worker`, `observer`) are always in the index regardless of what the protocol taxonomy files contain or what the runtime returns. They are protocol constants. Base roles have `vertical: None` and empty `tools: []` — tools are vertical-scoped (per inv 3 and `wcon-discovery` §3.4), and base roles belong to no vertical.

### 10.5 Auth Invariants

These invariants supplement those in `wcon-auth` §13. The data-model-specific invariants:

1. **At least one active admin.** The `users` table must always contain at least one row where `console_role = 'admin'` and `disabled_at IS NULL`. Operations that would violate this (disabling or demoting the last admin) are rejected at the application layer.
2. **No plaintext secrets.** `users.password_hash` contains an Argon2id PHC string, never plaintext. `user_sessions.token_hash` and `api_tokens.token_hash` contain SHA-256 hex digests, never raw token values.
3. **Audit log append-only.** No UPDATE or DELETE is ever issued against the `audit_log` table through the application. Every state-changing API request inserts exactly one row.
4. **Ownership FK integrity.** Every `profiles.owner_user_id` and `sessions.owner_user_id` references a valid `users.id`. Since users are never deleted, FK violations cannot arise in normal operation.
5. **Login attempts are ephemeral.** Rows in `login_attempts` older than 24 hours are garbage-collected. The table is not an audit record and has no retention guarantee.

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-auth | Authentication & Authorization | defines identity model, session model, authorization — §5.3–5.7 implement its logical schema |
| wcon-architecture | System Architecture | constrains storage engine, persistence model, concurrency approach (§6, §7) |
| wcon-discovery | Agent & Role Discovery | defines how `VerticalEntry` is ingested from the runtime REST API (§2.2, §3.3) |
| wcon-sessions | Session Lifecycle | defines session `context` semantics, validation, and runtime delivery (§2.1, §3.1, §4.1) |
| wcon-profiles | Profile System | consumes `ToolEntry.policy` for policy-aware tool validation (§3.2) |
| wcon-highway | Highway Integration | consumes `VerticalEntry.checkpoint_types` for trail rendering (§8) and `tool_policies` for refusal events |
| wcon-glossary | Glossary | informs terminology for all entities |
| wcon-vision | Product Vision | informs profile portability requirement (G5), validation requirement (G6), vertical-agnosticism (BC4) |
| wacp-protocol | WACP Protocol Specification | defines runtime message types (ResourceBudget, workspace state, task status) |
| wacp-taxonomy | WACP Taxonomy crate | defines `VerticalManifest` struct and nested types (authoritative schema for §6.1) |

*WACP Console -- authored by AKIL Abderrahim and Claude Opus 4.6*
