# WACP Console — Seed Context

> Compressed summary of the full design, current implementation state, and next steps.
> For detail on any topic, follow the spec references. For task-level implementation detail, see `IMPLEMENTATION.md`.
> For the wiring strategy (cross-cutting, intended to sit at the monorepo root post-merge), see `../impl/archive/wiring-strategy.md`.
> For the monorepo merge procedure (W0 detail, 7 milestones, decisions still open), see `impl/merge-plan.md`.

## What This Is

A full-stack coordination workbench for the WACP ecosystem. Users discover agent roles and capabilities, create and manage agent profiles, launch coordination sessions against a live WACP runtime, and oversee agent work in real-time. The Console is a **client** of the runtime — it connects via gRPC and REST, never modifies protocol behavior, never executes LLM calls.

**Spec:** `wcon-vision`

## Current State (Post Phase 6)

**Backend:** 66 REST endpoints across 12 tags, 99 unit tests, zero clippy warnings. Rust workspace with 6 crates (`console`, `console-api`, `console-core`, `console-db`, `console-runtime`, `console-test-support`).

**Frontend:** 37 TypeScript files, 9,367 lines. React 19 + Vite 6 + TanStack Query + Zustand. Builds to 376KB JS / 14KB CSS.

**What works end-to-end (no runtime needed):** Auth (login, logout, session, CSRF, API tokens, bootstrap, rate limiting), user management, audit log, settings, taxonomy discovery (if runtime REST is reachable), profile CRUD with validation/versioning/export/import/clone, health checks.

**What is structurally complete but hollow (needs runtime wiring):** Session launch (transitions state in SQLite, doesn't create workspaces), gate/escalation/inject actions (write audit log, don't forward to runtime), WebSocket (accepts connection, sends welcome, then idle), cross-session pending endpoints (return empty arrays), session cancellation cleanup (empty match arms), startup recovery (query exists, not wired).

**What doesn't exist yet:** Session monitor (Tokio task per session, 4 gRPC stream subscribers), refusal synthesis, event enrichment, notification synthesis.

### Phase evaluations

All six phases passed evaluation. Reports at `impl/phase-{1..6}-eval.md`.

| Phase | Tests | Endpoints | Key deliverables |
|-------|-------|-----------|-----------------|
| 1 | 51 → | 21 | Auth, users, tokens, audit, settings, health, bootstrap, rate limiting |
| 2 | 71 → | 40 | Taxonomy index, discovery endpoints, search, reload, pagination, OpenAPI |
| 3 | 87 → | 50 | Profile validation (14 codes + 2 warnings), CRUD, versioning, YAML export/import |
| 4 | 99 | 66 | Session state machine, validation (12 codes), gRPC pool, CRUD, highway actions, WebSocket |
| 5 | +3 FE | — | API types codegen, Zustand stores, app shell, login, discovery browser (4 tabs), profiles, settings, admin |
| 6 | — | — | WebSocket hook, 6-step wizard, oversight dashboard (8 panels), notifications |

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

## Next Step: Wiring Strategy

**Documents:** `../impl/archive/wiring-strategy.md` (cross-cutting plan — intended to sit at the monorepo root post-merge) and `impl/merge-plan.md` (W0 detail — `status: final`, 12 decisions resolved, 6 pre-M0 open items resolved with decisions recorded in §10).

Phase 7 (distribution) is postponed. The next work is wiring the Console to the real WACP runtime. The strategy has 7 steps:

| Step | What | Effort |
|------|------|--------|
| **W0** | Merge `wacp/` and `wacp-console/` into one workspace (see `impl/merge-plan.md`) | 1–2d |
| **W1** | gRPC pool → AppState (instantiate, connect, inject) | ~2h |
| **W2** | Real launch flow (CreateSession → SubmitGoal → Dispatch → SendEnvelope) | ~1d |
| **W3** | Session monitor (4 gRPC stream subscribers, event aggregation, WebSocket broadcast) | ~2d |
| **W4** | Highway forwarding (gate/escalation/inject → real gRPC calls) | ~4h |
| **W5** | Cancellation cleanup + startup recovery | ~4h |
| **W6** | Cross-session pending endpoints from monitor state | ~2h |

**Critical path:** W3 (session monitor) — the hardest piece. Everything else is mechanical wiring.

**Before writing any wiring code:** Start the real runtime (`cd ../wacp && cargo run --bin wacp-runtime -- serve --config dev/runtime.yaml` pre-merge; `cargo run -p wacp-runtime -- serve --config wacp/dev/runtime.yaml` post-merge), verify REST taxonomy loading works, confirm auth/profiles/settings work standalone.

### Monorepo Decision (Merge Plan in Review)

The two repos are not independent. They share proto contracts, type crates, and must version-lock. Merge is **decided** — full plan in `impl/merge-plan.md` (`status: final`). Architectural boundary stays: two binaries, gRPC between them. Merge is about development ergonomics.

**12 decisions resolved** (§2 of merge-plan):

| # | Area | Value |
|---|------|-------|
| D1 | Merge direction | New umbrella repo `wacp-platform/` |
| D2 | Git history | `git subtree add` from both — preserves history |
| D3 | Frontends | Independent at M0–M7 (`highway-ui` + `console/frontend` untouched); B/C revisit post-M7 |
| D4 | Cargo workspace | Single unified workspace, 22 crates as members |
| D5 | Proto codegen | New `wacp-proto` crate owns `tonic_build`; both consumers depend on it |
| D6 | CI | Per-project workflows (`ci-wacp.yml`, `ci-console.yml`) with `paths:` filters + shared `ci-lint.yml` |
| D7 | Specs/docs | Per-subdir trees preserved; `wiring-strategy.md` promotes to umbrella root at M5 |
| D8 | Branch naming | `main` (protected) + `dev` (integration) |
| D9 | Remote | `github.com/Madahub-dev/wacp-platform` |
| D10 | Release tagging | Per-subproject tags: `wacp-runtime-v*` + `wacp-console-v*`, independent cadence |
| D11 | Console distribution | **OCI image only (ADR-009 supersedes ADR-004); cargo-dist deferred** |
| D12 | Docker | Per-subdir Dockerfiles (runtime exists; console written at M5); root `docker-compose.yml` for dev |

**Sibling repo (out of scope):** `../wacp-protocol/` holds protocol specs (deliberately separate). Spec IDs are path-independent, so cross-references work unchanged.

**8 execution milestones:** M0 pre-flight → M1 umbrella + subtree import → M2 Cargo workspace union → M3 `wacp-proto` extraction → M4 path-dep flip → M5 tooling + Dockerfiles + ADR-009 + wiring-strategy relocation → M6 CI rewrite → M7 validate + tag `monorepo-v0`.

**6 pre-M0 open items resolved** (§10 of merge-plan). Decision summary:

1. **§10.1** — Commit `wacp/.cargo/config.toml` (WSL2 OOM mitigation: `jobs=1` + mold linker); hoist to umbrella root at M5. Done on `wacp` dev branch (commit `d010336`).
2. **§10.2** — Console currently CLI-flag-only (no config-file, no env). Add clap `env = "WACP_RUNTIME_*_ADDRESS"` attributes on five flags at M5, bundled with compose + Dockerfile. Also adds `WACP_CONSOLE_LISTEN` since `[::1]:8080` is container-hostile.
3. **§10.3** — `rust-embed` is a declared-but-unused dep (Phase 7 integration postponed). Implement at M5: new `crates/console/src/assets.rs` with `#[folder = "$CARGO_MANIFEST_DIR/../../frontend/dist/"]`; wire `.fallback_service` with SPA history fallback; honour the currently-discarded `--frontend-path` dev override.
4. **§10.4** — Real scratch-tag push to ghcr.io + cleanup runbook. `*-test` `if:` guards skip the 4-target matrix on `release-runtime.yml`, so validation runs docker-only (~5 min vs ~40 min). `act` rejected — can't test OIDC/ghcr-login step.
5. **§10.5** — Confirmed live: `cd wacp/packages/wacp-cli && pnpm install --frozen-lockfile` → 8 `file:` deps, 825 ms. Two unrelated `.gitignore` anomalies surfaced: `wacp/.gitignore:5` ignores `Cargo.lock` (drop at M5 union); `node_modules/` relies on user's global gitignore in both repos (add explicit `**/node_modules/` at M5).
6. **§10.6** — Hybrid port: TS/Python/proto job bodies copy verbatim with mechanical `wacp/` path-prefix updates; `rust` job rewrites for `-p wacp-*` scoping; workflow trigger adds `paths:` filter. Post-M4, `ci-console.yml` loses its cross-repo checkout dance — ~100-line simplification.

**Revised effort estimate:** 1–2 working days (not the original 4h).

### Resumption Point

Pre-flight complete: merge-plan `status: final`, §10 items resolved, `wacp/.cargo/config.toml` committed on wacp `dev` (commit `d010336`), `wacp-console/Cargo.lock` re-synced on console `dev` (commit `40457e5`).

When resuming:
1. Read `impl/merge-plan.md` §5 procedure and §10 decisions (~10 min refresh).
2. Finish M0 — commit outstanding docs (this SEED update, `../impl/archive/wiring-strategy.md`, `impl/merge-plan.md`) on `wacp-console/dev`, then tag `pre-monorepo-wacp` on wacp and `pre-monorepo-console` on wacp-console.
3. Execute M1 → M7 in one focused session (1–2 days).
4. Post-M7, begin W1 (gRPC pool → AppState) on umbrella `dev` branch.

**ADR-009 is new and supersedes ADR-004.** Update the ADR table below when M5 writes it.

### Hollow Code Inventory

8 scaffolded components that need real gRPC calls:

| Component | Current state | What it needs |
|-----------|--------------|---------------|
| gRPC pool | Built, never instantiated | Add to AppState, connect on startup |
| Launch flow | SQLite state transitions only | 5-step gRPC sequence |
| Session monitor | Doesn't exist | Tokio task with 4 stream subscribers |
| Gate resolution | Audit log only | `HighwayService::RespondToGate` |
| Escalation response | Audit log only | `HighwayService::RespondToEscalation` |
| Directive injection | Audit log only | `HighwayService::InjectEnvelope` |
| Cancel cleanup | Empty match arms | `CoordinatorService::AbortWorkspace` |
| Startup recovery | Query exists, not wired | Verify workspaces, re-subscribe streams |

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
| 004 | ~~Single binary with embedded frontend (rust-embed + cargo-dist)~~ — **superseded by ADR-009** |
| 005 | TLS trust boundary: three modes (plaintext loopback / system CA / explicit CA) |
| 006 | Apache-2.0 license |
| 007 | Profile YAML format versioning (format_version: 1) |
| 008 | OpenAPI as shared contract (utoipa → openapi-typescript) |
| 009 | *(pending — written at M5)* OCI-only console distribution; cargo-dist deferred; `rust-embed` retained inside Dockerfile cargo build stage |

**Full ADR text:** `SPEC_BUILD.md` (ADR-009 will live at umbrella `adr/adr-009-oci-only-console.md` post-merge).

## Workspace Layout

```
wacp-console/
├── Cargo.toml                  # workspace root (6 member crates)
├── rust-toolchain.toml         # pin Rust stable
├── openapi.yaml                # generated (66 operations, 12 tags)
├── crates/
│   ├── console/                # binary — CLI, tracing, startup, taxonomy build
│   ├── console-api/            # Axum routes, handlers, OpenAPI, pagination, WebSocket
│   ├── console-core/           # domain logic: auth, profiles, sessions, taxonomy, validation
│   ├── console-db/             # sqlx types, queries, migrations
│   ├── console-runtime/        # gRPC pool, REST client, proto codegen, upstream re-exports
│   └── console-test-support/   # mock runtime (gRPC + REST), fixtures
├── migrations/                 # sqlx SQL migration files (9 tables)
├── frontend/                   # Vite + React 19 + TypeScript SPA
│   ├── src/api/                # types.ts (generated), client.ts, hooks/
│   ├── src/store/              # auth.ts, ui.ts, session.ts (Zustand)
│   ├── src/components/         # Layout, Sidebar, AdminGuard, Notifications
│   ├── src/realtime/           # useSessionStream.ts (WebSocket hook)
│   └── src/surfaces/           # auth, discovery, profiles, sessions, oversight, settings, admin
├── specs/                      # 12 finalized design specs
└── impl/                       # phase evals, wiring strategy
```

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

*WACP Console -- authored by AKIL Abderrahim and Claude Opus 4.6*
