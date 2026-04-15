# SPEC_BUILD

## Spec Map

Generated: 2026-04-09
Project: WACP Console (wcon)
Type: application

### Inventory

Greenfield project — no existing specs. The upstream WACP project provides protocol specs (`wacp-protocol`), taxonomy (`wacp-taxonomy`), and 17 implementation specs. This project references them but does not duplicate them.

### Specs

| Order | ID | Title | Category | Depends On | Complexity |
|-------|-----|-------|----------|------------|------------|
| 1 | wcon-vision | Product Vision | vision | — | multi |
| 2 | wcon-glossary | Glossary | glossary | — | single |
| 3 | wcon-architecture | System Architecture | architecture | 1, 2 | multi |
| 4 | wcon-data-model | Data Model | design | 3 | multi |
| 5 | wcon-discovery | Agent & Role Discovery | design | 3, 4 | multi |
| 6 | wcon-profiles | Profile System | design | 4, 5 | multi |
| 7 | wcon-sessions | Session Lifecycle | design | 3, 6 | multi |
| 8 | wcon-highway | Highway Integration | design | 3, 7 | multi |
| 9 | wcon-api | API Surface | api | 5, 6, 7, 8 | multi |
| 10 | wcon-ui | UI Design | design | 9 | multi |
| 11 | wcon-test | Test Strategy | test | 3, 9 | single |
| 12 | wcon-auth | Authentication & Authorization | design | 1, 2, 3 | multi |

### Spec Summaries

**1. wcon-vision** — Problem statement, target users, goals and non-goals, success criteria. Defines what the Console is and isn't. Establishes the product boundary between Console (user-facing workbench) and WACP runtime (protocol engine).

**2. wcon-glossary** — Canonical terminology. Disambiguates Console-specific terms (profile, session, vertical browser) from WACP protocol terms (workspace, envelope, checkpoint, trail). Required gate before writing any other spec.

**3. wcon-architecture** — System components, layer boundaries, data flows, connection model to WACP runtime (gRPC client), backend services, frontend SPA structure, persistence layer, authentication and authorization model. The load-bearing spec — everything else derives from decisions made here.

**4. wcon-data-model** — Schema for profiles, sessions, user accounts, vertical registry entries, and settings. Storage engine choice (SQLite), versioning strategy for profiles, import/export format (YAML). Defines the entities the application manages.

**5. wcon-discovery** — How the Console queries and indexes the WACP taxonomy: available roles (base + derived), tools per role, envelope types, checkpoint types, vertical definitions. Covers the taxonomy index (in-memory, rebuilt on startup), browsing/search UX model, and how taxonomy changes are detected.

**6. wcon-profiles** — Profile schema and lifecycle: create, edit, clone, delete, version, import/export. How profiles map to WACP agent configurations (LLM provider, model, temperature, autonomy level, tool allowlist, budget caps). Profile validation against taxonomy (role must exist, tools must be available for role).

**7. wcon-sessions** — Session lifecycle: configure (pick vertical, workflow, assign profiles to roles), launch (create WACP workspaces, bind profiles), monitor (track workspace states, task progress), teardown. Mapping from Console sessions to WACP coordinator sessions.

**8. wcon-highway** — How the Console integrates with WACP's human oversight: real-time trail streaming, gate approval queue, escalation inbox, directive injection. Builds on the existing HighwayService gRPC API. Defines the UX for human-in-the-loop coordination.

**9. wcon-api** — Backend API contract consumed by the frontend SPA. REST endpoints for CRUD operations, WebSocket channels for real-time events (trail stream, gate notifications, session state changes). Request/response schemas, error model, pagination, filtering.

**10. wcon-ui** — Screen inventory, navigation structure, interaction patterns, component hierarchy. Discovery browser, profile studio (editor + library), session launcher, live oversight dashboard. Responsive design constraints.

**11. wcon-test** — Testing strategy across layers: unit tests (backend services, frontend components), integration tests (backend ↔ WACP runtime), E2E tests (full user flows). Covers test data management (mock taxonomy, fixture profiles) and CI gating.

**12. wcon-auth** — Identity model (local users, Argon2id), authentication (browser sessions via cookies, API tokens via bearer), authorization (admin ⊃ operator ⊃ viewer permission matrix), ownership and visibility model, bootstrap flow, password policy, CSRF protection, rate limiting and account lockout, audit log, threat model. Canonical reference for all auth-related behavior. Added by the multi-user auth revision campaign (Q2).

### Dependency Graph

```
 wcon-vision ─────┐
                  ├──▶ wcon-architecture ──┬──▶ wcon-data-model ──┬──▶ wcon-discovery
 wcon-glossary ───┘     │                  │                      │         │
                        │                  │                      ▼         │
                        │                  │               wcon-profiles ◀──┘
                        │                  │                      │
                        │                  └──────▶ wcon-sessions ◀┘
                        │                                │
                        └──────────▶ wcon-highway ◀──────┘
                                          │
 wcon-vision ─────┐                       │
 wcon-glossary ───┤                       │
 wcon-architecture┘                       │
       └──▶ wcon-auth ──────────────┐     │
                                    │     │
                        wcon-auth ──┤     │
                        wcon-discovery ───┤
                        wcon-profiles ────┼──▶ wcon-api ──▶ wcon-ui
                        wcon-sessions ────┤         │
                        wcon-highway ─────┘         │
                                                    │
                        wcon-architecture ──┬──▶ wcon-test
                        wcon-api ───────────┘
```

**`wcon-auth` dependency edges:** depends on `wcon-vision` (personas), `wcon-glossary` (terminology), `wcon-architecture` (trait slots). Consumed by `wcon-data-model` (physical schemas), `wcon-api` (auth endpoints), `wcon-profiles` (ownership), `wcon-sessions` (ownership), `wcon-ui` (auth screens).

### Writing Sequence

The topological order above is the writing sequence. Practical grouping:

1. **Foundation** (write first, gate on glossary before proceeding):
   - `wcon-vision` + `wcon-glossary` — can be written in parallel

2. **Core design** (write after foundation gate):
   - `wcon-architecture` — write first, everything else depends on it
   - `wcon-data-model` — immediately after architecture

3. **Feature specs** (write after core design, partially parallelizable):
   - `wcon-discovery` — can start once data-model is done
   - `wcon-profiles` — needs discovery (profiles reference roles)
   - `wcon-sessions` — needs profiles (sessions assign profiles to roles)
   - `wcon-highway` — needs sessions (highway observes sessions)

4. **Derived specs** (write after all feature specs):
   - `wcon-api` — synthesizes the API from all feature specs
   - `wcon-ui` — designs the interface against the API

5. **Post-design**:
   - `wcon-test` — testing strategy informed by architecture and API

### Gap Analysis

**Covered:**
- Every feature the user described (discovery, profiles, sessions, oversight)
- System boundaries (Console ↔ WACP runtime via gRPC)
- Data persistence (profiles, sessions, settings)
- Real-time capabilities (trail streaming, gate notifications)
- External interface (API spec for frontend consumption)

**Deferred to architecture sections (not standalone specs):**
- **Security/Auth** — user authentication and authorization model. Lives in `wcon-architecture` §security. If multi-tenancy becomes a requirement, promote to standalone spec `wcon-security`.
- **Performance** — no standalone spec. Budget constraints live in `wcon-architecture`. The Console is a standard web application; exotic performance requirements are unlikely.
- **Deployment** — deferred to implementation phase. Not a design concern.

**Upstream dependencies (not owned by this project):**
- WACP runtime must be running for sessions to function
- WACP taxonomy files must be accessible for discovery to work
- WACP gRPC services (Agent, Highway, Coordinator) are the integration surface

**No gaps identified.** All stated goals are addressed. No circular dependencies.

## Architectural Decision Records

### ADR-001 — Runtime is the vertical registry (REST as source of truth)

**Status:** accepted — 2026-04-11

**Context.**
The `wcon-discovery` draft (2026-04-09) postulated that the Console's Rust backend would read each vertical's `vertical.yaml` manifest directly from the filesystem under a configured `verticals.path` setting. At that point SWE was the only upstream vertical, no `vertical.yaml` files existed, and §2.2 of the draft explicitly deferred manifest generation to the vertical author. A spec review on 2026-04-11 flagged this as a blocker for the vertical expansion (SWE → seven verticals) and enumerated four candidate resolutions: upstream manifest mandate, Node-adapter subprocess, Console-side generator, or Markdown parsing.

Phase 27S (upstream, 2026-04-11) resolved the blocker on the runtime side before any of those options was adopted:
- `packages/wacp-cli/scripts/generate-manifests.ts` now emits a deterministic `ecosystem/{id}/vertical.yaml` from each `LoadedVertical`. All seven verticals (swe, devops, mlops, finance, healthcare, analytics, datasci) ship a committed manifest.
- `wacp-taxonomy::VerticalManifest` (Rust) and the matching TypeScript `LoadedVertical` interface were extended with `defining_constraint`, `context_schema`, `tool_policies`, `checkpoint_types`, `quality_criteria`, and `task_types`.
- `wacp-transport::rest_gateway` serves `GET /v1/verticals` (returns `VerticalSummary[]`) and `GET /v1/verticals/{id}` (returns the full `VerticalManifest`, 404 on miss). The runtime loads manifests from `taxonomy.verticals_dir` at startup and holds them in an `Arc<Vec<VerticalManifest>>`.

**Decision.**
The Console consumes vertical manifests through the WACP runtime's REST API rather than reading `vertical.yaml` files from the filesystem. The runtime is the vertical registry; the Console is a consumer.

Concretely:
- `wcon-discovery` §2.2 is rewritten around `GET /v1/verticals` and `GET /v1/verticals/{id}` instead of filesystem parsing.
- The Console no longer exposes a `verticals.path` setting. Vertical location is a runtime concern (configured by `taxonomy.verticals_dir` upstream), not a Console concern.
- Adding, removing, or modifying a vertical requires a runtime restart (so the runtime reloads its registry). The Console picks up the change on its next taxonomy reload — no Console redeploy, no Console-local cache refresh beyond the existing reload mechanism.
- The extended manifest schema (`context_schema`, `tool_policies`, `checkpoint_types`, `quality_criteria`, `task_types`, `defining_constraint`) is written into the Console specs against the actual upstream field shapes documented in `wacp-taxonomy/src/vertical.rs`, not postulated ones.

**Consequences.**
- *Single source of truth.* The runtime already enforces tool policies and checkpoint requirements; having the Console read the same manifests the runtime reads eliminates the class of bug where the Console UI permits something the runtime refuses at execution time.
- *No Console-side YAML parser for verticals.* The Console's taxonomy index builder gains a REST client path for verticals instead of a filesystem+YAML-parsing path. It still reads the protocol taxonomy from the filesystem (derived roles, envelope/checkpoint types) — that path is unchanged.
- *Runtime must be reachable at startup to populate the vertical registry.* Startup failure behavior (`wcon-discovery` §8.1) is rewritten: runtime-unreachable-at-startup is a warning (Console starts with empty vertical registry, session launcher disabled) rather than a fatal.
- *Forward compatibility.* New manifest fields added upstream are served automatically by the REST endpoints. The Console tolerates unknown fields (it deserializes into its own `VerticalEntry` struct and ignores extras), so upstream schema evolution does not require lockstep Console releases.
- *Offline development.* Developing the Console without a running runtime is now harder — the vertical registry is empty. Mitigated in `wcon-test` via the mock runtime, which serves canned `VerticalManifest` responses for fixture verticals.

**Alternatives considered.**
1. *Console-side filesystem reader.* Rejected — duplicates YAML parsing logic that already exists in the runtime, creates two places where manifest schema evolution must be tracked, and tempts a pattern where the Console's view of a vertical diverges from the runtime's enforcement.
2. *Node-adapter subprocess.* Rejected — introduces a Node runtime dependency on the Console host, which conflicts with `wcon-vision` §6's "no external service dependencies" boundary criterion.
3. *Console-side TypeScript-to-YAML generator.* Made moot by Phase 27S shipping the generator upstream.
4. *Parsing `<VERTICAL>.md` spec documents.* Rejected as brittle — format drift in an upstream author's Markdown breaks Console discovery.

**References.**
- `wacp-taxonomy/src/vertical.rs` — authoritative `VerticalManifest` definition
- `wacp-transport/src/rest_gateway.rs` — REST handlers for `/v1/verticals[/{id}]`
- `wcon-discovery` §2.2 — consumer specification
- `wcon-data-model` §6.1 — `VerticalEntry` in-memory projection

### ADR-002 — Multi-user authentication in scope for Phase 1

**Status:** accepted — 2026-04-14

**Context.** `TECH_STACK_PROPOSAL.md` §10 Q2 asked whether to ship multi-user auth in Phase 1 or stub it. Shipping single-user first and retrofitting multi-user later is strictly worse: every table migration grows an `owner_user_id`, every API contract changes under live users, the frontend gets a login screen grafted onto single-user assumptions, and the audit trail has a gap.

**Decision.** Multi-user authentication, authorization, ownership, and audit ship in Phase 1. The full specification lives in `wcon-auth`. Implementation: local users with Argon2id, cookie-based browser sessions, bearer API tokens, three-level console role hierarchy (`admin` ⊃ `operator` ⊃ `viewer`), per-resource ownership, append-only audit log, bootstrap credential on first launch. OIDC deferred to a future `OidcAuthenticator` implementation.

**Spec revision campaign** (completed 2026-04-14): `wcon-auth` drafted; `wcon-architecture` §8, `wcon-data-model` §5, `wcon-api`, `wcon-profiles`, `wcon-sessions`, `wcon-ui` revised. All 12 specs grilled and finalized.

**Stack additions:** `argon2`, `tower-sessions` + `tower-sessions-sqlx-store`, CSRF double-submit middleware, `subtle`.

### ADR-003 — Tech stack: Rust/Axum/Tonic backend + React/Vite/TypeScript frontend

**Status:** accepted — 2026-04-14

**Context.** The Console is a two-tier application: Rust backend + browser SPA. The upstream WACP runtime is Rust/Tonic/Axum/Tokio. The frontend needs real-time dashboards, dynamic forms, and rich tables.

**Decision.** Full stack documented in `TECH_STACK_PROPOSAL.md` §1–§4, §11. Key choices:

| Layer | Choice |
|-------|--------|
| Backend runtime | Tokio + Axum 0.8 + Tonic 0.12 |
| Database | SQLite via sqlx 0.8 (compile-time query verification) |
| Taxonomy index | `arc-swap` for atomic swap |
| Upstream types | Git dep on `wacp-taxonomy` (no schema duplication) |
| Frontend | React 19 + TypeScript 5 strict + Vite 6 |
| UI components | shadcn/ui + Radix + Tailwind 4 |
| Server state | TanStack Query v5 |
| Tables | TanStack Table v8 + Virtual |
| Forms | React Hook Form + Zod |
| E2E tests | Playwright |

**gRPC client architecture.** The upstream runtime runs three separate gRPC services on three Tonic servers (`AgentService` on `[::1]:9090`, `HighwayService` on `[::1]:9091`, `CoordinatorService` on `[::1]:9092`). The Console maintains three independent Tonic channels — one per service — with per-service reconnection and health tracking. This corrects the earlier assumption of single-channel multiplexing.

**Rust workspace layout:** `crates/console` (binary), `console-api` (Axum routes), `console-core` (pure domain logic, no I/O), `console-db` (sqlx), `console-runtime` (gRPC/REST clients to WACP), `console-test-support` (fixtures).

### ADR-004 — Single binary distribution with embedded frontend

**Status:** accepted — 2026-04-14

**Context.** `wcon-vision` §6 requires zero external service dependencies. The Console must be self-contained.

**Decision.** `rust-embed` embeds the Vite-built `frontend/dist/` into the binary at compile time. Axum serves static assets at `/`. A `--frontend-path <dir>` CLI flag overrides to serve from disk during development. Distribution via `cargo-dist`: GitHub Releases with prebuilt binaries, shell installer, Homebrew tap, Windows MSI, Docker image. deb/rpm deferred.

### ADR-005 — TLS trust boundary: three modes

**Status:** accepted — 2026-04-14

**Context.** The Console connects to the runtime over gRPC and REST. The TLS posture varies by deployment (localhost dev, internal CA, public CA, air-gapped).

**Decision.** Three modes, selected by address scheme, fail-closed by default:
- **Plaintext** — allowed implicitly on loopback; non-loopback requires `WCON_ALLOW_INSECURE_TLS=1`.
- **System trust store** — `https://`/`grpcs://` scheme, OS CA bundle via `rustls-native-certs`.
- **Explicit CA / pinned cert** — `runtime.tls_ca_pem` or `runtime.tls_pinned_cert_sha256` settings.

`rustls` everywhere; no OpenSSL dependency.

### ADR-006 — Apache-2.0 license

**Status:** accepted — 2026-04-14

**Decision.** Apache-2.0, matching the upstream `wacp` repository. Provides explicit patent grant for a coordination-protocol component. Not dual-licensed — the Console ships as a binary, not a library crate.

### ADR-007 — Profile YAML format versioning

**Status:** accepted — 2026-04-14

**Decision.** All profile YAML exports include a required `format_version: 1` top-level key. Importers reject unknown major versions with `INVALID_FORMAT_VERSION`. Within a known version, unknown fields are preserved with import warnings (forward-compat). Ensures future format evolution has a migration hook.

### ADR-008 — OpenAPI as the shared contract between backend and frontend

**Status:** accepted — 2026-04-14

**Decision.** Rust types → `utoipa` annotations → `openapi.yaml` → `openapi-typescript` → TypeScript types. Single source of truth: adding an endpoint is a one-file Rust change; TypeScript regenerates automatically. CI gates: `cargo run --bin gen-openapi && git diff --exit-code` and `pnpm gen:api && git diff --exit-code` fail on stale types.

## Completed Work

### Multi-User Authentication Spec Revision Campaign (Q2) — completed 2026-04-14

All 7 items landed, grilled, and finalized. See ADR-002.

### Spec Grill Campaign — completed 2026-04-14

All 12 design specs grilled. 20 HIGH, 11 MEDIUM, 7 LOW findings identified and resolved. All specs promoted to `status: final`.

### Upstream WACP License Clarification (resolved 2026-04-11)

**Source:** discovered 2026-04-11 while finalizing `TECH_STACK_PROPOSAL.md` §10 Q7.

**Original issue.** The upstream `wacp` repository carried a three-way license inconsistency:
- `../wacp/LICENSE` declared **CC BY-SA 4.0** (a Creative Commons content license).
- `../wacp/Cargo.toml` workspace package declared `license = "Apache-2.0"`.
- 10 TypeScript `package.json` files (`packages/wacp-cli`, `packages/wacp-local`, `highway-ui`, and 7 `ecosystem/*` packages) declared no license field at all, silently inheriting the root `LICENSE` (CC BY-SA 4.0) under npm conventions — a latent legal booby-trap for any downstream consumer, and a direct share-alike infection risk for the Console's `wacp-taxonomy` git-dep.

**Likely root cause.** WACP began as a protocol specification repository (CC BY-SA 4.0 is appropriate for written specs) and code was added later with Apache-2.0 in Cargo metadata. The root `LICENSE` file was never split or updated. TypeScript packages were created fresh and the license field was simply omitted.

**Resolution.** Upstream commit `ef20421` (2026-04-11) performed a **physical repo split**:

- **`github.com/Madahub-dev/wacp`** — reference implementation, uniformly **Apache-2.0**. Root `LICENSE` replaced with the standard Apache-2.0 text. `NOTICE` file added with attribution and pointer to the sibling spec repo. All 10 TypeScript `package.json` files now declare `"license": "Apache-2.0"` explicitly. 15 markdown cross-references in `impl/*.md` updated from relative `../protocol/...` paths to absolute URLs in the new sibling repo.
- **`github.com/Madahub-dev/wacp-protocol`** — new sibling repo, **CC BY-SA 4.0**. Contains only the protocol specification: `PROTOCOL.md`, `TAXONOMY.md`, and the primitive/foundation/mechanism/topology spec folders (20 specs total). Extracted via `git filter-repo --path protocol/ --path-rename protocol/:` from a fresh clone. The pre-existing `LICENSE` (CC BY-SA 4.0) was preserved by copying it in before the rewrite.

**Why split rather than an in-repo multi-license layout.** The project is spec-first by self-description (see `wacp-protocol/README.md` §Overview). Apache-2.0 carries the explicit patent grant meaningful for a coordination-protocol implementation; CC BY-SA 4.0 preserves share-alike protection on the written specification. The two licenses are compatible — implementations of the protocol are independent works, not derivative works of the specification in the share-alike sense. A physical split gives each artifact its natural license, a separately citable home, and a clean path for multi-implementation reuse without the ongoing discipline cost of managing two licenses inside one repository.

**Impact on the Console.** The original advisory's core concern — that the Console's `wacp-taxonomy` git-dep could be infected by CC BY-SA 4.0's share-alike clause — is fully resolved. The git-dep now resolves against an Apache-2.0 crate in an Apache-2.0 repository. No further Console-side action.

**Status:** closed. Retained in this document as historical record of the pre-resolution state and the reasoning behind the chosen fix.

## Next Step

`/impl-plan` — create the phased implementation plan in `IMPLEMENTATION.md`.
