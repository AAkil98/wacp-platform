# WACP Console — Implementation Plan

**Created:** 2026-04-14
**Author:** AAkil98
**Design baseline:** `main` @ `21df8c5` (12 specs final, 8 ADRs accepted)
**Tech stack:** `TECH_STACK_PROPOSAL.md` (promoted to ADR-003)

---

## Overview

Eight phases (0–7). Phases 0–4 build the backend. Phases 5–6 build the frontend. Phase 7 ships the binary.

Each phase has:
- **Goal** — what becomes usable at the end
- **Modules** — which crates/packages gain code
- **Gate** — the conditions that must pass before the next phase starts
- **Deliverables** — checkable list of concrete outputs; every item is production code, not a stub or placeholder
- **Depends on** — which earlier phases must be complete

```
Phase 0: Scaffold + Mock Runtime
  │
Phase 1: Auth + Database Foundation
  │
Phase 2: Taxonomy + Discovery API
  │
Phase 3: Profiles API
  │
Phase 4: Sessions + Highway Backend
  │
Phase 5: Frontend — Shell + Auth + Discovery + Profiles
  │
Phase 6: Frontend — Session Launcher + Oversight Dashboard
  │
Phase 7: Distribution + E2E + Polish
```

No phase runs in parallel — each builds on the previous. Within a phase, tasks may be parallelized where dependencies allow.

**No stubs, no placeholder endpoints, no mock data layers.** Every phase delivers fully functional, tested, production-grade code for its scope. The only "mock" in the project is `console-test-support` — a test fixture server that implements the WACP gRPC and REST interfaces for integration and E2E testing. It is a real Tonic/Axum server, not a simulation.

---

## Phase 0 — Scaffold + Mock Runtime

**Goal:** A compilable, buildable, testable project. `cargo check`, `pnpm build`, `sqlx migrate`, and CI all pass. The mock runtime serves fixture responses.

**Depends on:** nothing (greenfield)

### Tasks

| # | Task | Crate/Package | Spec reference |
|---|------|---------------|----------------|
| 0.1 | Rust workspace: root `Cargo.toml` with `[workspace.dependencies]`, `rust-toolchain.toml` (pin stable), 6 crates with real `Cargo.toml` manifests and dependency declarations (`console`, `console-api`, `console-core`, `console-db`, `console-runtime`, `console-test-support`) | workspace root | ADR-003, `TECH_STACK_PROPOSAL.md` §2.9 |
| 0.2 | Proto codegen: `tonic-build` build script in `console-runtime` consuming `../wacp/proto/*.proto` (agent.proto, coordinator.proto, highway.proto, primitives.proto, taxonomy.proto) — produces compiled Rust client types | `console-runtime` | ADR-003 §2.1 |
| 0.3 | Git dependency on `wacp-taxonomy` crate: `VerticalManifest`, `VerticalSummary`, `ToolSummary`, `ContextField`, `ToolPolicy`, `CheckpointSchema`, `QualityCriterion`, `TaskTypeDescriptor`, `WorkflowSummary`, `ProfileSummary` — all importable and usable | `console-runtime` | ADR-003 §2.3, `wcon-data-model` §6.1 |
| 0.4 | SQLite migrations: complete DDL for all 9 tables (`profiles`, `sessions`, `session_assignments`, `settings`, `users`, `user_sessions`, `api_tokens`, `audit_log`, `login_attempts`) with all columns, constraints, indexes, and CHECK clauses exactly as specified | `console-db` / `migrations/` | `wcon-data-model` §3–§5 |
| 0.5 | Frontend project: Vite 6 + React 19 + TypeScript 5 strict (`noUncheckedIndexedAccess: true`) + Tailwind 4 + shadcn/ui initialized + React Router 7 with route declarations for all screens from `wcon-ui` §3 + pnpm lockfile | `frontend/` | ADR-003 §3 |
| 0.6 | Mock runtime: real Tonic gRPC server implementing `AgentService`, `HighwayService`, `CoordinatorService` interfaces + real Axum REST server implementing `GET /v1/verticals` and `GET /v1/verticals/{id}`. Serves `fixture-simple` (SWE-like: empty context_schema, no tool_policies) and `fixture-complex` (Finance-like: required context fields, 4 tool policies, 3 checkpoint types, quality criteria). Fixture data built from the actual `wacp-taxonomy::VerticalManifest` type — no hand-rolled JSON | `console-test-support` | `wcon-test` §5.2, §7.1 |
| 0.7 | CI pipeline: GitHub Actions with 4 stages — lint (`cargo fmt --check`, `cargo clippy -- -D warnings`, `pnpm lint`, `pnpm typecheck`), unit (`cargo test`, `pnpm test`), integration (reserved), OpenAPI drift (reserved). Cache layers for Cargo registry, target/, node_modules, pnpm store | `.github/workflows/` | `wcon-test` §8, `TECH_STACK_PROPOSAL.md` §8.3 |
| 0.8 | Clap CLI with derive API: `serve` subcommand (starts Axum server with config loading, database initialization, taxonomy index build, and graceful shutdown via `CancellationToken`), `migrate` subcommand (runs sqlx migrations and exits), `reset-admin-password` subcommand (recovery tool per `wcon-auth` §6.4) | `console` | `TECH_STACK_PROPOSAL.md` §2.8, `wcon-auth` §6.4 |
| 0.9 | `directories` crate integration: XDG-aware default data directory for `console.db`, export directory, bootstrap token file | `console` | `TECH_STACK_PROPOSAL.md` §2.8, `wcon-auth` §6 |

### Gate

- `cargo check --workspace` — zero errors
- `cargo test --workspace` — all tests pass (migration application tests, proto codegen import tests, fixture manifest construction tests)
- `pnpm build` — produces `frontend/dist/` with no TS errors
- `sqlx migrate run` — applies all 9 tables to a fresh `console.db`, schema matches `wcon-data-model` exactly
- Mock runtime starts on random ports, serves both fixture verticals via gRPC and REST
- CI pipeline runs green on all stages
- `wacp-console migrate` runs and exits cleanly
- `wacp-console serve` starts, binds HTTP, logs structured output, shuts down on SIGINT

### Deliverables

- [ ] `Cargo.toml` (workspace root) with all 6 member crates and shared dependencies
- [ ] `rust-toolchain.toml` pinning stable Rust
- [ ] `crates/console/` — binary crate with Clap CLI (`serve`, `migrate`, `reset-admin-password`), config loading, graceful shutdown
- [ ] `crates/console-api/` — Axum router skeleton (compiles, no endpoints yet)
- [ ] `crates/console-core/` — domain type definitions (compiles, no logic yet)
- [ ] `crates/console-db/` — sqlx pool initialization, migration runner, connection options (WAL, FK, busy timeout)
- [ ] `crates/console-runtime/` — compiled proto types, `wacp-taxonomy` re-exports, gRPC client type stubs
- [ ] `crates/console-test-support/` — mock runtime binary with fixture-simple + fixture-complex manifests, gRPC service implementations, REST handlers
- [ ] `migrations/` — SQL files for all 9 tables with full DDL
- [ ] `frontend/` — buildable React project with route declarations, Tailwind configured, shadcn/ui initialized
- [ ] `.github/workflows/ci.yml` — 4-stage pipeline

---

## Phase 1 — Auth + Database Foundation

**Goal:** A running Axum server with complete authentication, user management, settings, health, and audit. Every auth flow from `wcon-auth` works end-to-end. Testable via curl.

**Depends on:** Phase 0

### Tasks

| # | Task | Crate | Spec reference |
|---|------|-------|----------------|
| 1.1 | sqlx query layer: compile-time-verified typed queries for all 9 tables — CRUD operations, filtered list queries with cursor pagination, aggregate queries (count, exists). Every query from `wcon-data-model` §3–§5 | `console-db` | `wcon-data-model` §3–§5 |
| 1.2 | `Authenticator` trait + `LocalAuthenticator`: extract user identity from `wcon_sid` cookie (SHA-256 lookup in `user_sessions`) or `Authorization: Bearer wcon_t_...` header (SHA-256 lookup in `api_tokens`). Returns `AuthenticatedUser { user_id, username, console_role }` | `console-core` | `wcon-architecture` §8.2, `wcon-auth` §3 |
| 1.3 | `Authorizer` trait + `RoleAuthorizer`: three-level hierarchy (`admin` ⊃ `operator` ⊃ `viewer`), action-based authorization with ownership context. Full permission matrix from `wcon-auth` §4.2 (32 actions) | `console-core` | `wcon-architecture` §8.3, `wcon-auth` §4 |
| 1.4 | Auth middleware stack: Axum layers for authentication → CSRF double-submit validation (cookie-auth only, bearer exempt) → authorization. 401 on unauthenticated, 403 on unauthorized, constant-time token comparison via `subtle` | `console-api` | `wcon-architecture` §8.1, §8.4, `wcon-auth` §8 |
| 1.5 | Auth endpoints: `POST /api/auth/login` (create session, set cookie, rotate old session), `POST /api/auth/logout` (delete session, clear cookie), `GET /api/auth/whoami` (return authenticated user), `POST /api/auth/change-password` (verify current, hash new, clear `must_change_password`) | `console-api` | `wcon-api` §3.5, `wcon-auth` §3 |
| 1.6 | User management endpoints: `GET /api/users` (admin, paginated), `POST /api/users` (admin, create with Argon2id hash), `GET /api/users/:id`, `PATCH /api/users/:id` (disable, change role — with LAST_ADMIN guard), `POST /api/users/:id/reset-password` (admin, set `must_change_password`), `POST /api/users/:id/unlock` (admin, clear lockout) | `console-api` | `wcon-api` §3.6, `wcon-auth` §2 |
| 1.7 | API token endpoints: `GET /api/tokens` (own tokens), `POST /api/tokens` (create, display once, hash and store), `DELETE /api/tokens/:id` (revoke), admin variant: `GET /api/users/:id/tokens`, `DELETE /api/users/:id/tokens/:tid` | `console-api` | `wcon-api` §3.7, `wcon-auth` §3.3–3.4 |
| 1.8 | Rate limiting middleware: per-IP sliding window (20 attempts / 15 min → 429), per-account sliding window (5 failed / 15 min → 401 ACCOUNT_LOCKED with auto-unlock). Applied before password verification on login endpoint only | `console-api` | `wcon-auth` §9 |
| 1.9 | Bootstrap flow: on `serve` startup, detect empty `users` table → generate 256-bit random credential → create admin user with `must_change_password = 1` → print credential to stdout AND write to `$XDG_STATE_HOME/wacp-console/bootstrap-token`. No default credentials, ever | `console-core` | `wcon-auth` §6 |
| 1.10 | Audit service: writes to `audit_log` on every state-changing operation. 23 action types from `wcon-auth` §10.2. Captures `user_id`, `timestamp`, `action`, `target_kind`, `target_id`, `detail` (JSON), `ip`, `user_agent`. Append-only — no UPDATE or DELETE through application code | `console-core` + `console-db` | `wcon-auth` §10 |
| 1.11 | Audit log endpoint: `GET /api/audit-log` (admin only, paginated, filterable by user/action/target_kind/date_range) | `console-api` | `wcon-api` §3.8 |
| 1.12 | Settings service: `GET /api/settings` (all known keys with defaults materialized), `GET /api/settings/:key`, `PUT /api/settings/:key` (type-validate known keys, accept unknown keys), `DELETE /api/settings/:key` (reset to default). Authorization: view = operator+, modify = admin only | `console-core` + `console-api` | `wcon-data-model` §5.1–5.2, `wcon-api` §10 |
| 1.13 | Health endpoint: `GET /api/health` (unauthenticated) — checks `database: ok/error`, returns `{ status, checks, version }` | `console-api` | `wcon-api` §11 |
| 1.14 | Argon2id implementation: OWASP-recommended parameters (m=19456 KiB, t=2, p=1), 16-byte random salt per password, PHC-format output string | `console-core` | `wcon-auth` §7 |
| 1.15 | Structured logging: `tracing` + `tracing-subscriber` — JSON format in production (`RUST_LOG`-controlled), pretty console in development. Per-request spans via `tracing::instrument` on handlers | `console` | `TECH_STACK_PROPOSAL.md` §2.6 |
| 1.16 | Error model: `thiserror` enum implementing the full error taxonomy from `wcon-api` §4.3 — serializes to `{ error, code, message, detail }` JSON. Includes `UNAUTHENTICATED`, `FORBIDDEN`, `PASSWORD_TOO_WEAK`, `ACCOUNT_LOCKED`, `CSRF_VALIDATION_FAILED`, `PASSWORD_CHANGE_REQUIRED`, `LAST_ADMIN` | `console-api` | `wcon-api` §4 |

### Gate

- Bootstrap: fresh database → credential printed to stdout → first login with credential → 403 PASSWORD_CHANGE_REQUIRED → change password → full admin session
- Login/logout: cookie set on login, cleared on logout, `GET /api/auth/whoami` returns identity
- Bearer token: create token → use in `Authorization` header → access works → revoke → access fails
- CSRF: `POST /api/settings/ui.theme` without CSRF token → 403; with token → 200
- Rate limiting: 6 failed logins → ACCOUNT_LOCKED; wait 15 min (or admin unlock) → login works
- RBAC: viewer 403 on `POST /api/profiles`, operator 403 on `GET /api/users`, admin 200 on all
- Audit: every mutation produces exactly one `audit_log` row with correct action and actor
- Settings: `GET /api/settings` returns all known keys with defaults; `PUT` validates types; `DELETE` resets
- Health: `GET /api/health` returns `{ status: "healthy", checks: { database: "ok" } }`
- `cargo test` — all unit tests pass, including auth edge cases (expired session, disabled user, timing-safe comparison)

### Deliverables

- [ ] `console-db`: complete sqlx query module — every SQL query for all 9 tables, compile-time verified
- [ ] `console-core`: `Authenticator` trait + `LocalAuthenticator` implementation
- [ ] `console-core`: `Authorizer` trait + `RoleAuthorizer` implementation (32-action permission matrix)
- [ ] `console-core`: `AuditService` — append-only write for 23 action types
- [ ] `console-core`: `SettingsService` — known-key registry, defaults, type validation
- [ ] `console-core`: `PasswordHasher` — Argon2id with OWASP parameters
- [ ] `console-core`: `BootstrapService` — first-launch credential generation
- [ ] `console-api`: auth middleware stack (authenticate → CSRF → authorize)
- [ ] `console-api`: rate limiting middleware on login endpoint
- [ ] `console-api`: 4 auth endpoints (`login`, `logout`, `whoami`, `change-password`)
- [ ] `console-api`: 6 user management endpoints (list, create, get, patch, reset-password, unlock)
- [ ] `console-api`: 3 API token endpoints (list, create, revoke) + admin variants
- [ ] `console-api`: audit log endpoint with filtering and pagination
- [ ] `console-api`: settings CRUD endpoints
- [ ] `console-api`: health endpoint (database check)
- [ ] `console-api`: error model (`thiserror` enum, JSON serialization)
- [ ] `console`: structured logging configuration
- [ ] Unit tests: bootstrap flow, auth pipeline, RBAC matrix, rate limiting, audit writes, CSRF validation

---

## Phase 2 — Taxonomy + Discovery API

**Goal:** The Console loads the full taxonomy from filesystem (protocol) + runtime REST (verticals) and serves all discovery queries. Every endpoint from `wcon-api` §6–§7 works. The OpenAPI spec is generated and CI-gated.

**Depends on:** Phase 1

### Tasks

| # | Task | Crate | Spec reference |
|---|------|-------|----------------|
| 2.1 | Protocol taxonomy YAML parser: read all `.yaml` files under `taxonomy.path`, extract derived roles (name, extends, add/remove capabilities), custom envelope types (name, permissions), custom checkpoint types (name, permitted_roles). Handle parse errors (fatal per `wcon-discovery` §8.1) | `console-core` | `wcon-discovery` §2.1, §3.2 |
| 2.2 | REST client: `reqwest` client for `GET /v1/verticals` (returns `VerticalSummary[]`) and `GET /v1/verticals/{id}` (returns `VerticalManifest`). Per-vertical error tolerance (skip + stub on failure). Runtime auth credential attachment when configured | `console-runtime` | `wcon-discovery` §2.2, ADR-001, `wcon-architecture` §8.6 |
| 2.3 | `TaxonomyIndex` builder: hardcoded base roles (coordinator, worker, observer) → protocol taxonomy parse → vertical manifest ingestion → cross-reference resolution: tool-role vertical-coarse bidirectional mapping, `ToolEntry.policy` mirroring from `VerticalEntry.tool_policies`, `CheckpointSchema.required_by` population. Deterministic builds (sort by ID) | `console-core` | `wcon-discovery` §3–§4, `wcon-data-model` §6.1, §10.4 |
| 2.4 | `ArcSwap` atomic index management: initial build at startup, `POST /api/taxonomy/reload` triggers background rebuild, atomic swap on success, retain old index on failure. Reload response with `status` (success/partial/failed), `counts`, `warnings` | `console-core` | `wcon-data-model` §6.3, `wcon-discovery` §7.3–7.4 |
| 2.5 | Failed vertical stub entries: `VerticalEntry.load_error: Option<String>` populated when `GET /v1/verticals/{id}` fails. Stubs appear in vertical list with error message, excluded from session launcher and profile validation | `console-core` | `wcon-data-model` §6.1, `wcon-discovery` §9.1 |
| 2.6 | Discovery API — global entity endpoints: `GET /api/roles` (filter: base_role, vertical), `GET /api/roles/:id` (full detail with resolved tools, envelope types, checkpoint types), `GET /api/tools` (filter: vertical, has_policy), `GET /api/tools/:name` (detail with policy, roles, vertical), `GET /api/verticals` (list with defining_constraint), `GET /api/verticals/:id` (full detail), `GET /api/envelope-types`, `GET /api/envelope-types/:name`, `GET /api/checkpoint-types`, `GET /api/checkpoint-types/:name` | `console-api` | `wcon-discovery` §4, `wcon-api` §6 |
| 2.7 | Discovery API — per-vertical sub-endpoints: `GET /api/verticals/:id/workflows`, `GET /api/verticals/:id/workflows/:wf_id`, `GET /api/verticals/:id/task-types`, `GET /api/verticals/:id/context-schema`, `GET /api/verticals/:id/tool-policies`, `GET /api/verticals/:id/checkpoint-types`, `GET /api/verticals/:id/quality-criteria` | `console-api` | `wcon-discovery` §4.1, `wcon-api` §6 |
| 2.8 | Search endpoint: `GET /api/search?q=<query>&type=<filter>&vertical=<filter>&limit=<n>`. Cross-entity substring matching across 10 entity types. Results grouped by type, ranked by match quality (exact > prefix > substring > description). Minimum 2-character query | `console-api` | `wcon-discovery` §5, `wcon-api` §7 |
| 2.9 | Taxonomy reload endpoint: `POST /api/taxonomy/reload` (operator+). Background rebuild, atomic swap, response with status/counts/warnings | `console-api` | `wcon-discovery` §7.2 |
| 2.10 | Cursor-based pagination: base64-encoded sort key cursor, `limit` parameter (default 50, cap 200), `{ items, cursor, has_more }` response envelope. Applied to all list endpoints (roles, tools, verticals, envelope-types, checkpoint-types, audit-log, profiles, sessions) | `console-api` | `wcon-discovery` §4.2, `wcon-api` §5.3 |
| 2.11 | Health endpoint expansion: add per-service runtime checks — `runtime_agent` (ping `[::1]:9090`), `runtime_highway` (ping `[::1]:9091`), `runtime_coordinator` (ping `[::1]:9092`), `runtime_rest` (HEAD `[::1]:9093`). Any unreachable → `status: "degraded"` | `console-api` | `wcon-api` §11 |
| 2.12 | `utoipa` annotations on all Phase 1 + Phase 2 endpoints. `gen-openapi` binary that writes `openapi.yaml`. CI stage: `cargo run --bin gen-openapi && git diff --exit-code` | `console-api` | ADR-008 |

### Gate

- Startup against mock runtime: taxonomy index contains base roles + derived roles from protocol taxonomy + all fixture vertical roles/tools/types
- `GET /api/verticals` returns fixture-simple and fixture-complex with correct counts
- `GET /api/verticals/fixture-complex` returns full detail (defining_constraint, context_schema with typed fields, tool_policies with kind-specific fields, checkpoint_types with field schemas, quality_criteria, task_types with keywords, workflows with stage/gate counts)
- `GET /api/roles?vertical=fixture-complex` returns all fixture-complex roles
- `GET /api/tools?has_policy=true` returns only policy-gated tools with resolved `ToolPolicy`
- `GET /api/search?q=compliance` returns results across roles, tools, checkpoint types, verticals
- `POST /api/taxonomy/reload` → background rebuild → new index swapped in → response with counts
- Startup without runtime → warning logged, empty vertical registry, base roles present, `GET /api/health` returns `degraded`
- `openapi.yaml` generated, CI gate passes
- `cargo test` — all discovery unit tests pass (parser, builder, cross-references, deterministic builds, stub entries)

### Deliverables

- [ ] `console-core`: protocol taxonomy YAML parser (derives roles, envelope types, checkpoint types)
- [ ] `console-runtime`: REST client for `GET /v1/verticals[/{id}]` with auth credential, error tolerance
- [ ] `console-core`: `TaxonomyIndex` — complete in-memory index with all entity types and cross-references
- [ ] `console-core`: `TaxonomyIndexBuilder` — deterministic build from two sources with `ArcSwap` swap
- [ ] `console-api`: 10 global discovery endpoints (roles, tools, verticals, envelope-types, checkpoint-types — list + detail)
- [ ] `console-api`: 7 per-vertical sub-endpoints (workflows, task-types, context-schema, tool-policies, checkpoint-types, quality-criteria, workflow detail)
- [ ] `console-api`: search endpoint with 10-type cross-entity matching
- [ ] `console-api`: taxonomy reload endpoint
- [ ] `console-api`: cursor-based pagination module (reusable across all list endpoints)
- [ ] `console-api`: per-service health checks (4 runtime endpoints)
- [ ] `console-api`: `gen-openapi` binary + `openapi.yaml` + CI drift gate
- [ ] Unit tests: YAML parsing, index construction, cross-reference integrity, reload atomicity, stub entries, search ranking

---

## Phase 3 — Profiles API

**Goal:** Complete profile lifecycle — create, edit, version, validate, soft-delete, import, export, clone. Full ownership and visibility model. Every validation error code fires correctly.

**Depends on:** Phase 2 (profiles validate against the taxonomy index)

### Tasks

| # | Task | Crate | Spec reference |
|---|------|-------|----------------|
| 3.1 | Profile validation engine: `UNKNOWN_ROLE` (role must exist in index), `UNKNOWN_TOOL` (tool must exist), `TOOL_NOT_IN_ROLE_VERTICAL` (tool must belong to same vertical as role), `EMPTY_TOOL_SET` (effective set non-empty for vertical roles, skipped for base roles), `DUPLICATE_NAME` (per-user uniqueness among own live profiles), `INVALID_NAME`/`INVALID_PROVIDER`/`INVALID_MODEL`/`INVALID_TEMPERATURE`/`INVALID_MAX_TOKENS`/`INVALID_AUTONOMY`/`INVALID_THRESHOLD`/`INVALID_BUDGET`/`INVALID_TAGS` (field-level). Non-blocking warnings: `TOOL_HAS_RUNTIME_POLICY` (policy-gated tool in allowlist), autonomous-worker with write-capable tools | `console-core` | `wcon-profiles` §3, `wcon-data-model` §10.1 |
| 3.2 | Profile CRUD endpoints: `POST /api/profiles` (create, validate, set owner), `GET /api/profiles` (list with ownership/visibility filtering — operator sees own + shared, admin sees all, viewer sees own + shared read-only), `GET /api/profiles/:id` (detail with derived fields: display_name, role_name, vertical, available_tools, policy_gated_tools), `PUT /api/profiles/:id` (update = new version), `DELETE /api/profiles/:id` (soft delete with active-session guard + non-terminal session warnings + WebSocket notification to affected session owners) | `console-api` | `wcon-profiles` §2, `wcon-api` §7 |
| 3.3 | Profile versioning: append-only — each update creates new row with `version + 1`, toggles `is_current`, previous versions retained. Rollback-to-version creates a new version with old content. `GET /api/profiles/:id/versions` returns version history | `console-core` + `console-db` | `wcon-data-model` §7 |
| 3.4 | Per-user name uniqueness: `DUPLICATE_NAME` checks only `owner_user_id = authenticated_user AND is_current = 1 AND deleted_at IS NULL`. Derived `display_name`: private profiles = `name`, shared profiles viewed by non-owners = `"{owner_display_name}'s {name}"` | `console-core` | `wcon-profiles` §3.1, `wcon-data-model` §3.3 |
| 3.5 | YAML export: `GET /api/profiles/:id/export` → `application/x-yaml` response. `format_version: 1`, excludes `id`, `version`, `is_current`, `created_at`, `owner_user_id`, `visibility`. NULL fields omitted. Tags as YAML array | `console-core` | `wcon-data-model` §8, ADR-007 |
| 3.6 | YAML import: `POST /api/profiles/import` (multipart). Parse YAML, check `format_version`, generate new UUID, set `owner_user_id` = importer, `visibility` = private, `version` = 1. Full validation against taxonomy. Per-user name uniqueness check. Return created profile + warnings | `console-core` + `console-api` | `wcon-profiles` §7.3 |
| 3.7 | Clone: `POST /api/profiles/:id/clone` — copies current version with new UUID, new name (`"{name} (copy)"`), owner = authenticated user. Full validation against current taxonomy | `console-api` | `wcon-profiles` §6 |
| 3.8 | utoipa annotations on all Phase 3 endpoints → `openapi.yaml` updated | `console-api` | ADR-008 |

### Gate

- Create → validate → save → `GET /api/profiles` (appears in list) → `GET /api/profiles/:id` (full detail with derived fields)
- Update → `version` incremented, old version in `GET /api/profiles/:id/versions`
- Rollback → new version created with old content
- Delete → soft-deleted, filtered from list, still visible in session assignment history
- Delete with non-terminal session → 200 with `warnings` array listing affected sessions
- Export → YAML with `format_version: 1`, no internal fields, round-trip import produces identical profile
- Import → new UUID, validated, per-user name uniqueness, warnings for policy-gated tools
- Clone → new UUID, `"(copy)"` suffix, owner = cloner
- Every validation error code fires on the correct input
- Two users create profiles with same name → both succeed
- Shared profile viewed by non-owner shows `"{owner}'s {name}"`
- Operator: CRUD own + read shared; admin: CRUD all; viewer: read own + shared
- `cargo test` — all profile tests pass

### Deliverables

- [ ] `console-core`: `ProfileValidationEngine` — all 14 error codes + 2 warning types
- [ ] `console-core`: `ProfileService` — create, update, delete, clone, versioning, rollback
- [ ] `console-core`: `ProfileExporter` — YAML serialization with `format_version: 1`
- [ ] `console-core`: `ProfileImporter` — YAML parsing, validation, new-id generation
- [ ] `console-api`: 7 profile endpoints (list, create, get, update, delete, export, import) + versions + clone
- [ ] `console-db`: profile queries — CRUD, version history, ownership-filtered lists, per-user uniqueness check
- [ ] Unit tests: every validation code, versioning lifecycle, soft-delete FK integrity, import round-trip, per-user uniqueness, display_name derivation

---

## Phase 4 — Sessions + Highway Backend

**Goal:** Sessions launch end-to-end against the real runtime (validated via mock runtime in integration tests), stream real-time events via WebSocket, handle gates/escalations/refusals, and cancel cleanly. The full backend API is complete.

**Depends on:** Phase 3 (sessions reference profiles)

### Tasks

| # | Task | Crate | Spec reference |
|---|------|-------|----------------|
| 4.1 | Session state machine: all transitions from `wcon-data-model` §4.3 — `configuring → validating → launching → active → completed/failed/cancelled`. Cancel from any non-terminal state with per-state cleanup (configuring/validating = no-op, launching = best-effort abort, active = CoordinatorService.AbortWorkspace) | `console-core` | `wcon-data-model` §4.3, `wcon-sessions` §7.3 |
| 4.2 | Session CRUD: `POST /api/sessions` (create in configuring state, set owner, optional name), `GET /api/sessions` (ownership-filtered list), `GET /api/sessions/:id` (detail with assignments, context, state), `PATCH /api/sessions/:id` (update config in configuring state only — 409 after launch), `PUT /api/sessions/:id/assignments` (replace assignments, pin profile versions), `POST /api/sessions/:id/launch` (validate + launch), `POST /api/sessions/:id/cancel`, `POST /api/sessions/:id/clone` | `console-api` | `wcon-sessions` §2, `wcon-api` §8 |
| 4.3 | Session validation: all 12 checks — `UNKNOWN_VERTICAL`, `UNKNOWN_WORKFLOW`, `MISSING_ASSIGNMENT`, `UNKNOWN_PROFILE`, `DELETED_PROFILE_IN_ASSIGNMENT`, `UNKNOWN_VERSION`, `ROLE_MISMATCH`, `INVALID_PROFILE`, `MISSING_CONTEXT`, `INVALID_CONTEXT`, `INVALID_BUDGET`, `RUNTIME_UNREACHABLE`. Context validation: strict type matching (no coercion), enum membership, required field enforcement | `console-core` | `wcon-sessions` §3.1 |
| 4.4 | Mode B slot derivation: one slot per distinct role in the vertical's `VerticalEntry.roles` (synthesized from profiles). No per-stage metadata | `console-core` | `wcon-sessions` §2.4 |
| 4.5 | gRPC client pool: three independent Tonic channels (`runtime.agent_address`, `runtime.highway_address`, `runtime.coordinator_address`). Per-service exponential backoff reconnection (100ms → 5s cap, 30 attempts → session failed). Per-service health tracking. Shared runtime auth credential interceptor | `console-runtime` | `wcon-architecture` §4.1, §7, §8.6 |
| 4.6 | Launch flow: `CoordinatorService.CreateSession` → `CoordinatorService.SubmitGoal` → per-assignment `CoordinatorService.Dispatch` (role, budget, task) + `AgentService.SendEnvelope` (directive payload: `llm`, `tools`, `system_prompt`, `context` passthrough). Record `coordinator_workspace_id` and per-assignment `workspace_id` | `console-core` + `console-runtime` | `wcon-sessions` §4, `wcon-architecture` §5.3 |
| 4.7 | Session monitor: one Tokio task per active session. Subscribes to 4 gRPC streams (`StreamTrail`, `StreamGates`, `StreamEscalations`, `StreamWorkspaceChanges` via HighwayService). Aggregates events into in-memory session state. Updates SQLite on state transitions. Drives session lifecycle (`active → completed` when all tasks done) | `console-core` | `wcon-architecture` §7, `wcon-sessions` §6 |
| 4.8 | Refusal synthesis: session monitor detects trail entries with refusal status codes (`COMPLIANCE_NOT_APPROVED`, `PHI_ACCESS_NOT_GRANTED`, `COMPUTE_BUDGET_EXCEEDED`, `ENVIRONMENT_GATE_REQUIRED`, `CLASSIFICATION_BLOCKED`). Constructs `RefusalEvent` with policy metadata resolved from taxonomy index. Maintains `pending_refusals` list per session. Clears on: prerequisite checkpoint created, workspace transitions out of BLOCKED, tool retry succeeds, session cancelled | `console-core` | `wcon-highway` §4A, `wcon-sessions` §6.3 |
| 4.9 | WebSocket server: `GET /api/sessions/:id/ws` → upgrade. 7 channels: `trail` (enriched trail entries), `gates` (with vertical rationale), `escalations`, `refusals` (synthesized `RefusalEvent`), `workspaces` (state changes), `session` (lifecycle events: `session_active`, `session_completed`, `session_failed`, `session_cancelled`), `notification` (cross-cutting alerts). Per-session `tokio::sync::broadcast` fan-out. Slow consumers dropped with warning | `console-api` | `wcon-api` §12, `wcon-highway` §2.2 |
| 4.10 | Highway action endpoints: `POST /api/sessions/:id/gates/:gid` (approve/reject with reason), `POST /api/sessions/:id/gates/batch-resolve` (batch with partial failure), `POST /api/sessions/:id/escalations/:eid` (respond), `POST /api/sessions/:id/inject` (directive injection with workspace target validation). Authorization: owner + admin only | `console-api` | `wcon-highway` §4–§6, `wcon-api` §9 |
| 4.11 | Event enrichment: workspace ID → label mapping, vertical-specific checkpoint field schema rendering, gate rationale from recent trail entries (`wcon-highway` §4.7), tool-layer refusal policy reference resolution | `console-core` | `wcon-highway` §7 |
| 4.12 | Backend restart recovery: on startup, query `sessions` table for `state = 'active'`. Per session: `CoordinatorService.GetWorkspace` for each assignment's `workspace_id`, `CoordinatorService.GetTaskGraph`, re-subscribe to 4 streams. Fail → mark session `failed` with `recovery_failed` reason | `console-core` | `wcon-sessions` §8.2 |
| 4.13 | Cross-session endpoints: `GET /api/gates/pending` (all pending gates, ownership-scoped — operators see own sessions, admins see all), analogous for escalations and refusals. Powers the nav badge | `console-api` | `wcon-highway` §8.4 |
| 4.14 | Notification synthesis: gate arrival → `notification` channel event, gate timeout < 20% remaining → high-priority notification, new escalation → high-priority, new refusal → normal, runtime disconnect → high-priority | `console-core` | `wcon-highway` §9 |
| 4.15 | Session clone: `POST /api/sessions/:id/clone` — copies configuration (vertical, workflow, assignments, context), resets state to `configuring`, new UUID, new owner = cloner. Context uses current `context_schema` (may require re-entry if schema changed) | `console-api` | `wcon-sessions` §9.5 |
| 4.16 | utoipa annotations on all Phase 4 endpoints | `console-api` | ADR-008 |

### Gate

- Create session → configure (set vertical, workflow, assignments, context) → launch against mock runtime → gRPC workspace creation confirmed
- 4 gRPC streams established → trail entries appear on WebSocket `trail` channel
- Mock runtime emits gate event → appears on `gates` channel with vertical rationale → approve via API → workspace resumes (visible on `workspaces` channel)
- Mock runtime emits refusal trail entry → `RefusalEvent` on `refusals` channel with policy kind, error code, unblock hint
- Cancel from configuring (instant, no gRPC), from active (AbortWorkspace called), from launching (best-effort cleanup)
- `DELETED_PROFILE_IN_ASSIGNMENT` fires when assigned profile is soft-deleted before launch
- All 12 validation checks fire on the correct inputs
- Ownership: operator sees own sessions only; admin sees all; cross-session endpoints respect ownership
- Multiple concurrent sessions: each has independent streams, monitor, state
- WebSocket reconnection: client disconnects → reconnects → resumes receiving events
- Backend restart: active sessions recovered from database, streams re-subscribed
- `cargo test` — all integration tests pass against mock runtime
- `openapi.yaml` covers all endpoints (Phases 1–4)

### Deliverables

- [ ] `console-core`: `SessionStateMachine` — all transitions, cancel from any non-terminal state
- [ ] `console-core`: `SessionValidationEngine` — 12 validation checks + context type validation
- [ ] `console-core`: `SessionLauncher` — full gRPC launch sequence (CreateSession → SubmitGoal → Dispatch → SendEnvelope)
- [ ] `console-core`: `SessionMonitor` — Tokio task, 4 stream subscribers, event aggregation, lifecycle derivation
- [ ] `console-core`: `RefusalSynthesizer` — detect, classify, construct `RefusalEvent`, manage pending list, clearance
- [ ] `console-core`: `EventEnricher` — workspace labels, checkpoint field schemas, gate rationale
- [ ] `console-core`: `NotificationSynthesizer` — cross-cutting event generation
- [ ] `console-core`: `RecoveryService` — startup recovery for active sessions
- [ ] `console-runtime`: gRPC client pool — 3 Tonic channels, per-service reconnection, auth interceptor
- [ ] `console-api`: 8 session endpoints (list, create, get, patch, assignments, launch, cancel, clone)
- [ ] `console-api`: 4 highway action endpoints (gate resolve, batch resolve, escalation respond, inject)
- [ ] `console-api`: 3 cross-session endpoints (pending gates, escalations, refusals)
- [ ] `console-api`: WebSocket upgrade endpoint with 7-channel multiplexed JSON protocol
- [ ] Integration tests: full session lifecycle against mock runtime, WebSocket streaming, gate approval, refusal synthesis, cancel, recovery

---

## Phase 5 — Frontend: Shell + Auth + Discovery + Profiles

**Goal:** A fully functional SPA with login, taxonomy browsing (all 4 tabs), profile management, settings, and admin screens. No real-time features yet.

**Depends on:** Phase 4 (full backend API available); OpenAPI TypeScript types generated

### Tasks

| # | Task | Package | Spec reference |
|---|------|---------|----------------|
| 5.1 | OpenAPI TypeScript codegen: `openapi-typescript` reads `openapi.yaml` → generates `src/api/types.ts` + typed fetch wrapper. `pnpm gen:api` script. CI gate: `pnpm gen:api && git diff --exit-code` | `frontend/` | ADR-008 |
| 5.2 | TanStack Query hooks: typed hooks for every endpoint family — `useRoles()`, `useTools()`, `useVerticals()`, `useProfiles()`, `useUsers()`, `useSettings()`, `useAuditLog()`, `useHealth()`. Cache invalidation rules per `wcon-ui` §11.3 | `frontend/src/api/hooks/` | `wcon-ui` §11.2 |
| 5.3 | App shell: sidebar navigation with route links, collapsible groups, permission-gated items (admin-only: Users, Audit Log). Active route highlighting. Responsive sidebar collapse at breakpoints per `wcon-ui` §10 | `frontend/src/` | `wcon-ui` §2 |
| 5.4 | Login screen: username/password form, error display (invalid credentials, account locked, forced password change redirect), cookie-based session management in Zustand store. Forced password change screen. Logout from user menu | `frontend/src/surfaces/auth/` | `wcon-ui` §7A.1–7A.3 |
| 5.5 | Discovery browser — Roles tab: base roles section + derived roles grouped by vertical with collapsible headers. Role detail panel (capabilities, tools, envelope types, checkpoint types). Filter by base_role, vertical | `frontend/src/surfaces/discovery/` | `wcon-ui` §4, `wcon-discovery` §6 |
| 5.6 | Discovery browser — Tools tab: grouped by vertical, lock badge on policy-gated tools, tooltip with policy summary. Tool detail panel (description, vertical, roles, full policy details when applicable) | `frontend/src/surfaces/discovery/` | `wcon-ui` §4, `wcon-discovery` §6 |
| 5.7 | Discovery browser — Types tab: three sections — envelope types (protocol), checkpoint types (protocol), vertical-specific checkpoint types (grouped by vertical with field schemas) | `frontend/src/surfaces/discovery/` | `wcon-ui` §4 |
| 5.8 | Discovery browser — Verticals tab: vertical list with defining_constraint. Expandable detail: context_schema (typed fields), tool_policies, checkpoint_types (with field schemas), quality_criteria, task_types (with keywords), workflows (card with stage/gate counts), default_profiles, tools | `frontend/src/surfaces/discovery/` | `wcon-ui` §4.5 |
| 5.9 | Discovery search: global search box above tabs. `GET /api/search` integration. Results grouped by type with match highlighting. Click result navigates to detail view | `frontend/src/surfaces/discovery/` | `wcon-discovery` §5 |
| 5.10 | Profile studio — editor: form with role selector (populated from taxonomy), LLM fields, autonomy radio, tool allowlist/denylist (filtered to role's vertical tools, lock badges), budget fields. Real-time validation feedback. Policy warning banner on save | `frontend/src/surfaces/profiles/` | `wcon-ui` §5 |
| 5.11 | Profile studio — library: TanStack Table with sort, filter (by role, vertical, tag), cursor pagination. Version history panel. Clone, delete (with confirmation + warning display), export (YAML download), import (file upload with preview) | `frontend/src/surfaces/profiles/` | `wcon-ui` §5 |
| 5.12 | Settings screen: 4 runtime address fields, taxonomy path, export directory, theme selector (light/dark/system), trail buffer size. Test Connection button. Save per field | `frontend/src/surfaces/settings/` | `wcon-ui` §8 |
| 5.13 | Admin — User management: TanStack Table listing users. Create user dialog. Edit user (disable, change role with LAST_ADMIN guard). Reset password. Unlock account | `frontend/src/surfaces/admin/` | `wcon-ui` §7A.4 |
| 5.14 | Admin — Audit log viewer: TanStack Table with filters (user, action, target type, date range), cursor pagination. Detail row expansion | `frontend/src/surfaces/admin/` | `wcon-ui` §7A.5 |
| 5.15 | Theme implementation: Tailwind CSS variables for light/dark, `system` default respects `prefers-color-scheme`. Persisted via `PUT /api/settings/ui.theme` | `frontend/` | `wcon-data-model` §5.2 |

### Gate

- Login → sidebar visible → navigate all routes → logout
- Forced password change: bootstrap credential → login → redirected to change password → complete → full access
- Discovery: browse all 4 tabs, expand vertical detail, navigate role → tools → policy drill-down, search "compliance" → cross-entity results
- Profiles: create → edit → validation errors shown inline → save with policy warning → list → version history → clone → export YAML → delete (confirmation + warning if session affected) → import YAML file
- Shared profile shows `"{owner}'s {name}"` in library
- Admin screens: user management (create, disable, change role, reset password, unlock) + audit log (filter, paginate)
- Non-admin: admin nav items hidden, direct URL → 403 screen
- Settings: edit 4 runtime addresses → save → test connection → status indicator
- Theme: toggle light/dark/system → UI updates → persists on reload
- `pnpm test` — Vitest + RTL tests pass for all components
- `pnpm build` — production build succeeds with zero TS errors

### Deliverables

- [ ] `frontend/src/api/types.ts` — generated from OpenAPI
- [ ] `frontend/src/api/hooks/` — TanStack Query hooks for all endpoint families
- [ ] `frontend/src/surfaces/auth/` — login screen, forced password change, user menu, Zustand auth store
- [ ] `frontend/src/surfaces/discovery/` — 4-tab browser (roles, tools, types, verticals) + search + detail panels
- [ ] `frontend/src/surfaces/profiles/` — editor form with validation + library table with version history, clone, delete, export, import
- [ ] `frontend/src/surfaces/settings/` — settings form with test connection
- [ ] `frontend/src/surfaces/admin/` — user management table + audit log viewer
- [ ] `frontend/src/components/` — shared components: Sidebar, ContextBadge, LockBadge, PolicyDetail, Pagination
- [ ] `frontend/src/store/` — Zustand slices: auth, ui (sidebar, theme)
- [ ] Theme: light/dark/system via CSS variables
- [ ] Vitest + RTL test files for all surfaces and components

---

## Phase 6 — Frontend: Session Launcher + Oversight Dashboard

**Goal:** Full E2E flow in the browser: login → discover → create profile → launch session → oversee with real-time trail/gates/escalations/refusals → approve gate → cancel session.

**Depends on:** Phase 5

### Tasks

| # | Task | Package | Spec reference |
|---|------|---------|----------------|
| 6.1 | `useSessionStream` WebSocket hook: connect to `/api/sessions/:id/ws`, parse JSON frames into typed events per channel, dispatch to Zustand slices, exponential backoff reconnection (100ms → 5s). On reconnect: `GET /api/sessions/:id/state` for snapshot, `GET /api/sessions/:id/trail?since=<last_seen>` to fill gap | `frontend/src/realtime/` | `wcon-ui` §11.2, `wcon-sessions` §8.3 |
| 6.2 | Session launcher wizard — step 1 (select vertical): card grid from `useVerticals()`, each card shows name + defining_constraint + summary counts | `frontend/src/surfaces/sessions/` | `wcon-ui` §6.2 |
| 6.3 | Session launcher wizard — step 2 (select workflow): workflow cards from selected vertical's `WorkflowSummary` list, stage count + gated stage count | `frontend/src/surfaces/sessions/` | `wcon-ui` §6.2 |
| 6.4 | Session launcher wizard — step 3 (assign profiles): Mode B role slots (one per distinct role in vertical). Per-slot profile picker from library (filtered to role's vertical). Inline create option | `frontend/src/surfaces/sessions/` | `wcon-ui` §6.2, `wcon-sessions` §2.4 |
| 6.5 | Session launcher wizard — step 4 (vertical context): dynamic form generated from `VerticalEntry.context_schema`. String → text input, number → number input, boolean → toggle, enum → dropdown. Required field enforcement. Skip step when schema is empty (SWE) | `frontend/src/surfaces/sessions/` | `wcon-ui` §6.2 |
| 6.6 | Session launcher wizard — step 5 (budget overrides): session-level + per-assignment expandable overrides. Excludes vertical-specific compute metrics (those are step 4 context) | `frontend/src/surfaces/sessions/` | `wcon-ui` §6.2 |
| 6.7 | Session launcher wizard — step 6 (review + launch): summary of all config, optional session name field, Discard button, Launch button with runtime connectivity check. Launching state with progress. Error → return to relevant step | `frontend/src/surfaces/sessions/` | `wcon-ui` §6.2 |
| 6.8 | Discard button: present on every wizard step. Calls `POST /api/sessions/:id/cancel`, returns to session list | `frontend/src/surfaces/sessions/` | `wcon-ui` §6.2 |
| 6.9 | Oversight dashboard — session header: name (or derived `"{vertical} / {workflow}"`), state badge, elapsed timer, context badges from `session.context`, Cancel Session button (owner + admin, confirmation dialog) | `frontend/src/surfaces/oversight/` | `wcon-ui` §7.2 |
| 6.10 | Oversight dashboard — trail stream: TanStack Virtual for windowed scrolling (up to `ui.trail_buffer_size` entries). Per-entry rendering: timestamp, workspace label, event type, expandable detail. Vertical-specific checkpoint entries rendered with field schema table. Refusal entries with red left border. Filter by event type, workspace, severity | `frontend/src/surfaces/oversight/` | `wcon-ui` §7.2 |
| 6.11 | Oversight dashboard — workspace tree: visual workspace hierarchy with per-workspace state badge. Refusal badge on BLOCKED workspaces | `frontend/src/surfaces/oversight/` | `wcon-ui` §7.2 |
| 6.12 | Oversight dashboard — task view: task DAG visualization with status badges, dependencies, progress | `frontend/src/surfaces/oversight/` | `wcon-ui` §7.2 |
| 6.13 | Oversight dashboard — gate queue: ordered list (urgency then timeout). Per-gate: type, workspace label, summary, vertical rationale subtitle, timeout countdown, approve/reject buttons with reason field. Batch resolution UI (select multiple → batch approve) | `frontend/src/surfaces/oversight/` | `wcon-highway` §4 |
| 6.14 | Oversight dashboard — escalation inbox: list with workspace label, reason, age. Detail overlay (slide from right) with context JSON rendering, response form | `frontend/src/surfaces/oversight/` | `wcon-highway` §5 |
| 6.15 | Oversight dashboard — refusal panel: pending refusals list with workspace, tool name, error code, policy kind, unblock hint. Navigation links to related checkpoint/gate. No "resolve" button (Console surfaces, runtime enforces) | `frontend/src/surfaces/oversight/` | `wcon-highway` §4A |
| 6.16 | Oversight dashboard — injection bar: CodeMirror 6 editor for directive/feedback payload, workspace target dropdown (active workspaces only), Send button with confirmation | `frontend/src/surfaces/oversight/` | `wcon-highway` §6 |
| 6.17 | Oversight dashboard — quality report panel: visible when session reaches terminal state. Per-criterion pass/warn/fail badges from vertical's `quality_criteria` | `frontend/src/surfaces/oversight/` | `wcon-ui` §7.2 |
| 6.18 | Notification system: Sonner toasts (auto-dismiss 5s for normal, manual dismiss for high-priority). Nav badge with aggregated gate + escalation + refusal counts from cross-session endpoints. Browser notification API integration (permission request, focus check) | `frontend/src/` | `wcon-highway` §9, `wcon-ui` §2.1 |
| 6.19 | Session list: active sessions with state indicators + historical sessions. Session switcher in dashboard header (dropdown/tabs). Click → opens dashboard | `frontend/src/surfaces/sessions/` | `wcon-ui` §7, `wcon-highway` §8.4 |
| 6.20 | Session terminal state: dashboard header updates, trail shows final entry, gate/escalation/refusal panels cleared, injection disabled, workspace tree shows final states, summary banner (elapsed time, tokens, cost, tasks completed, per-metric totals for verticals with custom metrics) | `frontend/src/surfaces/oversight/` | `wcon-ui` §7.3 |
| 6.21 | Keyboard shortcuts: `G` (focus gate queue), `E` (focus escalation inbox), `R` (focus refusal panel), `F` (focus trail stream), `A` (approve selected gate), `Enter` (open selected item) via `react-hotkeys-hook` | `frontend/src/surfaces/oversight/` | `wcon-ui` §7.2 |

### Gate

- Full E2E flow: login → browse taxonomy → create profile → launch session wizard (all 6 steps) → session active → trail streaming in dashboard → gate appears → approve → workspace resumes → session completes → quality report shown
- Wizard step 4: fixture-complex shows enum dropdown for jurisdiction, required field blocks Next
- Wizard step 4: fixture-simple skips context step entirely
- Cancel from wizard (Discard on step 3) → session cancelled, return to list
- Cancel from dashboard (Cancel Session button) → confirmation → session cancelled → terminal state UI
- Refusal: mock runtime sends refusal trail entry → refusal panel shows entry with policy metadata and unblock hint
- Gate timeout: countdown visible, timeout warning notification fires
- Escalation: appears in inbox → open detail → respond → resolved
- Injection: type directive in CodeMirror → select workspace → send → trail entry appears
- Notification: gate toast fires, badge count updates, browser notification if tab unfocused
- Session switcher: switch between two active sessions → dashboard content swaps completely
- Trail virtualization: 500+ entries render smoothly without DOM bloat
- Keyboard shortcuts: `G` focuses gates, `A` approves, `F` focuses trail
- `pnpm test` — all component and hook tests pass
- `pnpm build` — production build succeeds

### Deliverables

- [ ] `frontend/src/realtime/useSessionStream.ts` — WebSocket hook with 7-channel parsing, reconnection, gap recovery
- [ ] `frontend/src/surfaces/sessions/Wizard.tsx` — 6-step session launcher with Discard
- [ ] `frontend/src/surfaces/sessions/ContextForm.tsx` — dynamic form from `context_schema`
- [ ] `frontend/src/surfaces/sessions/SessionList.tsx` — active + historical list with state badges
- [ ] `frontend/src/surfaces/oversight/Dashboard.tsx` — layout shell with session header, panels, session switcher
- [ ] `frontend/src/surfaces/oversight/TrailStream.tsx` — virtualized trail with checkpoint rendering and refusal styling
- [ ] `frontend/src/surfaces/oversight/WorkspaceTree.tsx` — hierarchical workspace state display
- [ ] `frontend/src/surfaces/oversight/TaskView.tsx` — task DAG visualization
- [ ] `frontend/src/surfaces/oversight/GateQueue.tsx` — ordered gate list with rationale, timeout, approve/reject, batch
- [ ] `frontend/src/surfaces/oversight/EscalationInbox.tsx` — list + detail overlay + response form
- [ ] `frontend/src/surfaces/oversight/RefusalPanel.tsx` — refusal list with policy metadata and hints
- [ ] `frontend/src/surfaces/oversight/InjectionBar.tsx` — CodeMirror editor + workspace selector
- [ ] `frontend/src/surfaces/oversight/QualityReport.tsx` — per-criterion verdicts
- [ ] `frontend/src/components/Notifications.tsx` — Sonner toasts + nav badge + browser notification
- [ ] `frontend/src/store/session.ts` — Zustand slice for active session state (trail, gates, escalations, refusals, workspaces)
- [ ] Vitest + RTL tests for wizard steps, dashboard panels, WebSocket hook, notification system

---

## Phase 7 — Distribution + E2E + Polish

**Goal:** A shippable single binary with automated release pipeline, Docker image, Playwright E2E suite, and verified performance.

**Depends on:** Phase 6

### Tasks

| # | Task | Location | Spec reference |
|---|------|----------|----------------|
| 7.1 | `rust-embed`: embed `frontend/dist/` into the binary at compile time. Axum serves at `/` (HTML) and `/assets/*` (JS/CSS). Content-type detection, gzip pre-compression, cache headers | `console` crate | ADR-004 |
| 7.2 | `--frontend-path <dir>` CLI flag: when present, serve from disk instead of embedded. Same router, different asset source. For development only — not documented as production use | `console` crate | ADR-004 |
| 7.3 | `cargo-dist` configuration: `dist-workspace.toml` with build targets (x86_64-linux-gnu, aarch64-apple-darwin, x86_64-windows-msvc, aarch64-linux-gnu), GitHub Releases, shell installer, Homebrew tap, Windows MSI | workspace root | ADR-004, `TECH_STACK_PROPOSAL.md` §5 |
| 7.4 | Docker multi-stage build: stage 1 (Rust builder with cargo-chef for dependency caching), stage 2 (pnpm frontend builder), stage 3 (distroless runtime with binary + no other files). Expose port 8080. Health check `CMD` | `Dockerfile` | `TECH_STACK_PROPOSAL.md` §5.3 |
| 7.5 | Playwright E2E harness: `console-e2e` crate starts the full binary with mock runtime sidecar, seeds fixture data (users, profiles), launches Playwright against `http://localhost:<port>` | `frontend/tests/` | `wcon-test` §6 |
| 7.6 | E2E: golden path — bootstrap → login → discover verticals → create profile → launch session → trail streams → approve gate → session completes → view quality report | `frontend/tests/` | `wcon-test` §6.3 |
| 7.7 | E2E: auth flows — login with bad password (error shown), account lockout (after 5 failures), forced password change (bootstrap credential), logout and re-login, API token create/use | `frontend/tests/` | `wcon-test` §6.3 |
| 7.8 | E2E: multi-user — admin creates operator and viewer. Operator: can create profiles, launch sessions, sees own only. Viewer: read-only, cannot create. Admin: sees all, manages users | `frontend/tests/` | `wcon-test` §6.3 |
| 7.9 | E2E: session cancel — cancel from wizard (Discard), cancel from dashboard (Cancel Session), verify terminal state UI | `frontend/tests/` | `wcon-test` §6.3 |
| 7.10 | E2E: profile import/export — export YAML → delete profile → import YAML → profile restored with new ID | `frontend/tests/` | `wcon-test` §6.3 |
| 7.11 | `cargo deny` configuration: license allow-list (Apache-2.0, MIT, BSD-2/3, ISC, Unicode-DFS-2016), advisory DB check, duplicate version detection. CI integration | CI | `TECH_STACK_PROPOSAL.md` §8.2 |
| 7.12 | `LICENSE` file (Apache-2.0 full text) + `NOTICE` file (attribution, pointer to upstream WACP) | workspace root | ADR-006 |
| 7.13 | Performance verification: taxonomy reload < 5s for 10 verticals + 100 tools (SC1), trail buffer at 1000 entries renders without jank (SC7 visual latency < 2s), WebSocket latency measurement | — | `wcon-vision` §7 |
| 7.14 | `README.md`: installation (5 channels), quickstart (bootstrap → first session), development setup, architecture overview, link to specs | workspace root | — |

### Gate

- Single binary: `./wacp-console serve` starts, serves frontend at `http://localhost:8080`, all API endpoints respond, WebSocket connects
- Docker: `docker build . && docker run -p 8080:8080 wacp-console` → same behavior
- Playwright: all 5 E2E scenarios pass against single binary + mock runtime (golden path, auth flows, multi-user, cancel, import/export)
- `cargo deny check` — zero license violations, zero advisories
- Release pipeline: create tag → GitHub Actions builds all 4 targets → artifacts appear on Releases page → shell installer works → Homebrew install works
- SC1: taxonomy reload completes in < 5s
- SC7: trail visual latency < 2s with 100 entries streaming
- README renders correctly on GitHub

### Deliverables

- [ ] `console` crate: `rust-embed` integration + `--frontend-path` override
- [ ] `dist-workspace.toml` — cargo-dist configuration for 4 build targets + 5 distribution channels
- [ ] `Dockerfile` — multi-stage build, distroless runtime
- [ ] `frontend/tests/e2e/` — 5 Playwright test suites (golden-path, auth, multi-user, cancel, import-export)
- [ ] `console-e2e` harness crate (binary + mock runtime + fixture seeding)
- [ ] `deny.toml` — cargo-deny configuration
- [ ] `LICENSE` (Apache-2.0) + `NOTICE`
- [ ] `README.md`
- [ ] CI: Playwright stage added, cargo-deny stage added, release workflow on tag push
- [ ] Performance: SC1 and SC7 verified with measurements logged

---

## Module → Spec Mapping

| Crate / Package | Primary specs |
|----------------|---------------|
| `console` | `wcon-architecture` §1 (binary wiring), ADR-004 (embedding) |
| `console-api` | `wcon-api` (all endpoints), `wcon-architecture` §4.2 (frontend surfaces) |
| `console-core` | `wcon-profiles` (validation, versioning), `wcon-sessions` (state machine, launch, monitor), `wcon-discovery` (index builder, parser), `wcon-highway` (bridge, refusal synthesis), `wcon-auth` (auth/authz logic) |
| `console-db` | `wcon-data-model` (all tables, queries, migrations) |
| `console-runtime` | `wcon-architecture` §4.1 (gRPC client pool, REST client), proto definitions |
| `console-test-support` | `wcon-test` §5.2 (mock runtime), §7 (fixture manifests) |
| `frontend/` | `wcon-ui` (all surfaces), `wcon-api` §12 (WebSocket) |

## Risk Register

| Risk | Mitigation | Phase |
|------|-----------|-------|
| Upstream proto changes break codegen | Pin `wacp` git dep to a tag/commit; CI detects drift | 0 |
| sqlx compile-time checking slows builds | Use `sqlx-data.json` offline mode in CI; live checking in dev | 0 |
| Mock runtime diverges from real runtime | Mock uses same `wacp-taxonomy` types (ADR-003); `wcon-test` §10.4 mock fidelity invariant | 0 |
| WebSocket fan-out under load (many concurrent sessions) | Bounded broadcast channels, slow-consumer drop (`wcon-architecture` §7) | 4 |
| Trail stream rendering with 1000+ entries | TanStack Virtual windowing (`TECH_STACK_PROPOSAL.md` §3.3) | 6 |
| OpenAPI drift between backend and frontend | CI gate: `gen-openapi && git diff --exit-code` (ADR-008) | 2+ |

---

*WACP Console — Implementation Plan — authored by AAkil98 and Claude Opus 4.6*
