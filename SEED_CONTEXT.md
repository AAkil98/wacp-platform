# WACP Console — Seed Context

> Compressed summary of the full design for implementation. Read this before writing code.
> For detail on any topic, follow the spec references.

## What This Is

A full-stack coordination workbench for the WACP ecosystem. Users discover agent roles and capabilities, create and manage agent profiles, launch coordination sessions against a live WACP runtime, and oversee agent work in real-time. The Console is a **client** of the runtime — it connects via gRPC and REST, never modifies protocol behavior, never executes LLM calls.

**Spec:** `wcon-vision`

## Four Surfaces

| Surface | What it does | Spec |
|---------|-------------|------|
| **Discovery Browser** | Browse taxonomy: roles, tools, verticals, types. Read-only, search, filter, drill-down. | `wcon-discovery`, `wcon-ui` §4 |
| **Profile Studio** | Create/edit/clone/delete agent profiles. YAML import/export. Validates against taxonomy. | `wcon-profiles`, `wcon-ui` §5 |
| **Session Launcher** | 6-step wizard: vertical → workflow → assign profiles → vertical context → budgets → review & launch. | `wcon-sessions` §2, `wcon-ui` §6 |
| **Oversight Dashboard** | Real-time: trail stream, gate queue, escalation inbox, refusal panel, workspace tree, injection bar. 7 WebSocket channels. | `wcon-highway`, `wcon-ui` §7 |

## Architecture

Two tiers: Rust backend + React SPA. Backend owns persistence, taxonomy index, session orchestration, highway bridge. Frontend is a rendering layer consuming REST + WebSocket.

**Spec:** `wcon-architecture`

### Backend Components

| Component | Responsibility |
|-----------|---------------|
| **Taxonomy Index** | In-memory, built from protocol taxonomy YAML + runtime REST (`GET /v1/verticals[/{id}]`). Atomic swap via `ArcSwap`. |
| **Profile Store** | CRUD, versioning (append-only), soft delete, validation, import/export. SQLite. |
| **Session Manager** | Lifecycle from configuring → active → terminal. Translates UI config into gRPC calls. Monitors 4 streams. |
| **Highway Bridge** | Proxies trail/gates/escalations/workspace changes from gRPC to 7 WebSocket channels. Synthesizes `refusals`, `session`, `notification` channels. |
| **Auth Service** | Users, browser sessions, API tokens, login attempts. `LocalAuthenticator` + `RoleAuthorizer`. |
| **Audit Service** | Append-only mutation log. |

### Runtime Connection

Four endpoints, three independent Tonic channels + one REST client:

| Service | Default | Config key |
|---------|---------|------------|
| AgentService (gRPC) | `[::1]:9090` | `runtime.agent_address` |
| HighwayService (gRPC) | `[::1]:9091` | `runtime.highway_address` |
| CoordinatorService (gRPC) | `[::1]:9092` | `runtime.coordinator_address` |
| REST gateway | `http://[::1]:9093` | `runtime.rest_address` |

**NOT multiplexed.** Per-service reconnection and health tracking.

**Spec:** `wcon-architecture` §1, §4, §7; ADR-003

## Data Model

**Storage:** SQLite (`console.db`), WAL mode, single file.

### Tables

| Table | Purpose | Spec |
|-------|---------|------|
| `profiles` | Agent config bundles. Versioned (append-only), soft-delete. PK: `(id, version)`. | `wcon-data-model` §3 |
| `sessions` | Coordination runs. Optional `name`. State machine: configuring → validating → launching → active → completed/failed/cancelled. Cancel from any non-terminal state. | `wcon-data-model` §4 |
| `session_assignments` | Profile-to-role-slot bindings per session. Supports Mode A (stage-aware) and Mode B (role-aware fallback). | `wcon-data-model` §4.2 |
| `settings` | Key-value config. JSON-encoded values. | `wcon-data-model` §5.1 |
| `users` | Local identity store. Argon2id hashing. Console roles: admin ⊃ operator ⊃ viewer. Never deleted, only disabled. | `wcon-data-model` §5.3 |
| `user_sessions` | Cookie-based browser sessions. SHA-256 hashed token. 24h TTL. | `wcon-data-model` §5.4 |
| `api_tokens` | Bearer tokens for programmatic access. SHA-256 hashed. | `wcon-data-model` §5.5 |
| `audit_log` | Append-only mutation record. Admin-readable. | `wcon-data-model` §5.6 |
| `login_attempts` | Rate-limit tracking. GC'd after 24h. | `wcon-data-model` §5.7 |

### In-Memory

| Structure | Contents |
|-----------|----------|
| `TaxonomyIndex` | Roles, tools, envelope types, checkpoint types, verticals (with full `VerticalEntry` including context_schema, tool_policies, checkpoint_types, quality_criteria, task_types, workflows). |
| Active session state | Workspace states, task states, trail buffer, pending gates/escalations/refusals per session. |

**Spec:** `wcon-data-model`

## Taxonomy & Discovery

The taxonomy index is built from two sources:
1. **Protocol taxonomy** — YAML files on disk (base/derived roles, protocol-level types). Path: `taxonomy.path` setting.
2. **Vertical manifests** — fetched from runtime REST API (ADR-001). Seven verticals: SWE, DevOps, MLOps, Finance, Healthcare, Analytics, DataSci.

Each vertical has: `defining_constraint`, `context_schema`, `tool_policies`, `checkpoint_types`, `quality_criteria`, `task_types`, `workflows`, `default_profiles`, `tools`.

**Tool-role mapping is vertical-coarse** — every role in a vertical lists every tool in that vertical. Fine-grained per-role tool mappings deferred until upstream manifest extends.

**Stub entries:** `VerticalEntry.load_error: Option<String>` for verticals whose manifest failed to load.

**Spec:** `wcon-discovery`, `wcon-data-model` §6

## Profiles

A profile bundles: role reference, LLM config (provider, model, temperature, max_tokens), autonomy preset (autonomous/assisted/supervised), tool allowlist/denylist, budget caps, user metadata.

**Validation** on every write: role must exist, tools must belong to same vertical as role (vertical-coarse), effective tool set non-empty. Policy-gated tools save with non-blocking `TOOL_HAS_RUNTIME_POLICY` warning.

**Name uniqueness:** per-user. Two different users can have identically-named profiles. Shared profiles display as `"{owner}'s {name}"` (derived at read time, not stored).

**Ownership:** `owner_user_id` set at creation, immutable. `visibility`: private (default) or shared.

**Export:** YAML with `format_version: 1` (ADR-007). No `id`, `version`, `owner_user_id`, or `visibility` in export.

**Spec:** `wcon-profiles`, `wcon-data-model` §3

## Sessions

**States:** configuring → validating → launching → active → completed/failed/cancelled. Cancel from any non-terminal state.

**Launch flow:** CreateSession → SubmitGoal → per-slot Dispatch + SendEnvelope (directive with LLM config, tools, system_prompt, context passthrough) → subscribe to 4 gRPC streams.

**Slot derivation:** Mode A (stage-aware, future) or Mode B (role-aware fallback, current). Mode B: one slot per distinct role in the vertical.

**Vertical context:** session-level JSON from the vertical's `context_schema`. Supplied at step 4 of wizard, stored in `sessions.context`, delivered to runtime at dispatch.

**Cancellation cleanup:** from configuring/validating = no-op; from launching = best-effort workspace abort; from active = CoordinatorService.AbortWorkspace.

**Recovery:** on backend restart, query active sessions, call CoordinatorService.GetWorkspace + GetTaskGraph, re-subscribe streams.

**Spec:** `wcon-sessions`, `wcon-data-model` §4

## Highway (Oversight)

Four gRPC streams per session → session monitor → 7 WebSocket channels to frontend:

| Channel | Source | Synthesized? |
|---------|--------|-------------|
| `trail` | StreamTrail | No |
| `gates` | StreamGates | No |
| `escalations` | StreamEscalations | No |
| `refusals` | Trail entries with refusal codes | Yes |
| `workspaces` | StreamWorkspaceChanges | No |
| `session` | Aggregated workspace states | Yes |
| `notification` | Cross-cutting events | Yes |

**Tool-layer refusals:** runtime-enforced, arrive as trail entries. Console synthesizes `RefusalEvent` with policy metadata from taxonomy index. Four kinds: requires_checkpoint, requires_gate, budget_limited, classification_gated.

**Authorization scoping:** cross-session endpoints filtered by session ownership. Operators see own sessions only. Admins see all.

**Spec:** `wcon-highway`

## Authentication & Authorization

**Identity:** local users, Argon2id, SQLite. No external IdP in Phase 1.

**Browser auth:** cookie (`wcon_sid`), HttpOnly, Secure, SameSite=Strict, rotated on login.

**API auth:** bearer token (`Authorization: Bearer wcon_t_...`), hashed at rest.

**Roles:** admin ⊃ operator ⊃ viewer. Personas: admin → Administrator, operator → Practitioner/Overseer, viewer → Explorer.

**Bootstrap:** first launch generates one-time credential → forced password change → no default credentials ever.

**CSRF:** double-submit cookie on all state-changing cookie-authenticated requests. API tokens exempt.

**Rate limiting:** per-IP (20/15min) + per-account (5 failed/15min) with auto-unlock.

**Audit:** append-only log of every mutation. 23 action types. Admin-readable.

**Spec:** `wcon-auth`

## API Surface

REST + WebSocket. All endpoints except `/api/health` and `POST /api/auth/login` require authentication.

**Key endpoint families:**

| Family | Path prefix | Spec section |
|--------|------------|-------------|
| Discovery | `/api/roles`, `/api/tools`, `/api/verticals`, `/api/search` | `wcon-api` §6–7 |
| Profiles | `/api/profiles` | `wcon-api` §7 |
| Sessions | `/api/sessions` | `wcon-api` §8 |
| Gates/Escalations/Injection | `/api/sessions/:id/gates`, `escalations`, `inject` | `wcon-api` §9 |
| Auth | `/api/auth/*` | `wcon-api` §3.5 |
| Users (admin) | `/api/users/*` | `wcon-api` §3.6 |
| Tokens | `/api/tokens/*` | `wcon-api` §3.7 |
| Audit log (admin) | `/api/audit-log` | `wcon-api` §3.8 |
| Settings | `/api/settings` | `wcon-api` §10 |
| Health | `/api/health` | `wcon-api` §11 |
| WebSocket | `/api/sessions/:id/ws` | `wcon-api` §12 |

**Pagination:** cursor-based. **Error model:** `{ error, code, message, detail }`.

**Health:** per-service checks — `runtime_agent`, `runtime_highway`, `runtime_coordinator`, `runtime_rest`.

**Spec:** `wcon-api`

## Testing Strategy

| Layer | Tool | Fixture |
|-------|------|---------|
| Backend unit | `cargo test` + rstest + insta | In-memory SQLite, inline taxonomy data |
| Frontend unit | Vitest + RTL + MSW | Mocked API responses |
| Integration | In-process Tonic/Axum mock runtime | `fixture-simple` (SWE-like) + `fixture-complex` (Finance-like) |
| E2E | Playwright | Full binary + mock runtime sidecar |

**Two fixture verticals:** `fixture-simple` (no context, no policies — SWE baseline) and `fixture-complex` (required context, tool policies, vertical checkpoints — Finance/Healthcare-like).

**Mock runtime** uses the same `wacp-taxonomy::VerticalManifest` struct as real runtime (compile-time schema fidelity).

**Spec:** `wcon-test`

## Key Invariants

1. **Console never modifies taxonomy files or runtime state outside of gRPC/REST** (`wcon-vision` BC1, BC2)
2. **Manifest-driven rendering** — no hardcoded per-vertical logic; new vertical works without code change (`wcon-vision` G7, `wcon-ui` §12.6)
3. **Taxonomy index is atomic** — fully built or previous visible, never partial (`wcon-data-model` §10.4)
4. **Profile versions are append-only** — existing rows never mutated except `is_current` and `deleted_at` (`wcon-data-model` §10.1)
5. **At least one active admin** always exists (`wcon-auth` §13)
6. **No default credentials** — bootstrap generates a one-time credential (`wcon-auth` §13)
7. **Audit log is append-only** — no UPDATE/DELETE through the application (`wcon-auth` §13)
8. **Tool-layer policies are never enforced by the Console** — surfaced as warnings and refusal events only (`wcon-discovery` §3.5)

## ADRs

| ADR | Decision |
|-----|----------|
| 001 | Runtime is the vertical registry (REST, not filesystem) |
| 002 | Multi-user auth in Phase 1 |
| 003 | Tech stack: Rust/Axum/Tonic + React/Vite/TS + SQLite/sqlx |
| 004 | Single binary with embedded frontend (rust-embed + cargo-dist) |
| 005 | TLS trust boundary: three modes (plaintext loopback / system CA / explicit CA) |
| 006 | Apache-2.0 license |
| 007 | Profile YAML format versioning (format_version: 1) |
| 008 | OpenAPI as shared contract (utoipa → openapi-typescript) |

**Full ADR text:** `SPEC_BUILD.md`

## Design Specs (all final)

| # | ID | Title |
|---|----|-------|
| 1 | `wcon-vision` | Product Vision |
| 2 | `wcon-glossary` | Glossary |
| 3 | `wcon-architecture` | System Architecture |
| 4 | `wcon-data-model` | Data Model |
| 5 | `wcon-discovery` | Agent & Role Discovery |
| 6 | `wcon-profiles` | Profile System |
| 7 | `wcon-sessions` | Session Lifecycle |
| 8 | `wcon-highway` | Highway Integration |
| 9 | `wcon-api` | API Surface |
| 10 | `wcon-ui` | UI Design |
| 11 | `wcon-test` | Test Strategy |
| 12 | `wcon-auth` | Authentication & Authorization |

*WACP Console -- authored by AAkil98*
