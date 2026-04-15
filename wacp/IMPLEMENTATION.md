# WACP — Forward Strategy (Implementation Plan)

```yaml
id: wacp-impl
type: impl
status: active
created: 2026-04-01
revised: 2026-04-13
authors:
  - Akil Abderrahim
  - Claude Opus 4.6
tags: [strategy, roadmap, productionization, console, phase-28, phase-29]
```

This document is forward-looking. For the current-state snapshot — what's built, what's tested, what's deployed — see `SEED.md`.

---

## Table of Contents

1. Where we are
2. Where we're going
3. Console integration analysis (Phase 29 Dashboard)
4. Runtime productionization (Stream A)
5. wacp-console build (Phase 29.2)
6. Phase 28 — IDE + chat bridge (Stream B)
7. Push strategy
8. Task inventory
9. Phase history (compressed)
10. Open questions and risks

---

## 1. Where we are

Protocol spec complete (20 protocol specs + `PROTOCOL.md` + `TAXONOMY.md`). Runtime complete through **Phase 27S** — 15 Rust crates, 1,340 runtime tests (Rust), all 7 verticals wired end-to-end via `packages/wacp-cli/src/ecosystem.ts`, and a REST/WebSocket transport layer that serves 16 `/v1/*` endpoints plus `/v1/ws`. `wacp-taxonomy::VerticalManifest` is the canonical vertical schema, and `GET /v1/verticals[/{id}]` is the Console-facing discovery path. **Stream A (A1–A9) is complete** — all runtime productionization tasks have landed on `dev`. The `dev` branch is **CI-clean** as of 2026-04-13: `cargo fmt --check --all`, `cargo clippy --workspace -- -D warnings`, and `cargo test --workspace` all pass with zero failures (see `AUDIT-2026-04-12.md`).

Two external repositories share this forward plan:

| Repo | Purpose | Status |
|---|---|---|
| `wacp` (this repo) | Protocol, runtime, middleware, CLI, verticals | Phase 27S complete; Stream A (29.1) complete; Phase 28/29.2 pending |
| `wacp-console` (sibling) | User-facing workbench — profile studio, session launcher, live oversight dashboard | 11-spec set drafted, tech-stack proposition drafted, code not started |

`wacp-console` is functionally the **Phase 29 Dashboard** deliverable from the original plan. This strategy treats it as the canonical Dashboard and folds Phase 29.1 (public API surface) back into this repo as runtime work that unblocks Console development.

## 2. Where we're going

Three concrete deliverables remain:

1. **Runtime in production.** The runtime binary is built, tested, released, and installable outside this workspace. Today it exists only in a local checkout — this is the largest gap and the prerequisite for everything that runs against the runtime in a non-developer environment.
2. **`wacp-console` (Phase 29 Dashboard).** Browser-based workbench consuming the runtime REST + gRPC surface. Soft-blocked on (1) for a stable build target.
3. **Phase 28 applications.** VS Code / JetBrains IDE extension and Slack / Discord chat bridge. Both connect to a running runtime via existing transport. Structurally independent of the Console and of each other.

All three share a single productionization workstream on the runtime side. §4 is that workstream.

## 3. Console integration analysis (Phase 29 Dashboard)

### 3.1 How the Console talks to WACP

The Console is a Rust/Axum backend + React/TypeScript SPA (see `wacp-console/TECH_STACK_PROPOSAL.md`). Its shape is:

```
WACP runtime  ──(gRPC + REST + WebSocket)──▶  Console backend (Axum)  ──(REST + WebSocket)──▶  Console SPA (React)
```

The Console never reads `vertical.yaml` from disk. Per `wacp-console/SPEC_BUILD.md` ADR-001 (2026-04-11), the runtime is the vertical registry and the Console queries `GET /v1/verticals` (list) + `GET /v1/verticals/{id}` (detail) at startup. The Console additionally git-deps on `wacp-taxonomy` to get a strictly-typed `VerticalManifest` deserializer without duplicating the schema.

Runtime surface that the Console consumes:

| Runtime surface | Console use | Source |
|---|---|---|
| `GET /v1/verticals` | Discovery browser startup — list all verticals | `wcon-discovery` §2.2 |
| `GET /v1/verticals/{id}` | Discovery browser detail — per-vertical `context_schema`, `tool_policies`, etc. | `wcon-discovery` §2.2 |
| `GET /v1/health` | Doctor command — runtime reachability check | `wcon-architecture` §8 |
| `CoordinatorService` gRPC (15 RPCs) | Session lifecycle — `SubmitGoal`, `Dispatch`, `Abort`, `Suspend`, `Resume`, `TriggerIntegration`, signal stream | `wcon-sessions`, `wcon-api` |
| `HighwayService` gRPC (12 RPCs) | Oversight dashboard — trail, gates, escalations, directives | `wcon-highway` |
| `AgentService` gRPC (8 RPCs) | Session detail — workspace and checkpoint browsing | `wcon-sessions` §4 |
| `wacp-taxonomy::VerticalManifest` (Rust type) | Strict deserialization of vertical manifests in the Console backend | `wcon-data-model` §6.1 |

### 3.2 What's already there

The Console can start building today against:

- **All three gRPC services.** Server-side handlers for all 35 RPCs are wired post-26R.
- **REST gateway — 16 endpoints live:**
  - `/v1/health`, `/v1/goals`, `/v1/tasks`, `/v1/budget`, `/v1/trail`
  - `/v1/workspaces/{id}` + `/dispatch`, `/abort`, `/suspend`, `/resume`, `/inject`, `/integrate`
  - `/v1/gates/{id}/respond`, `/v1/escalations/{id}/respond`
  - `/v1/verticals`, `/v1/verticals/{id}`
- **WebSocket** — `/v1/ws`, JSON-RPC 2.0 framing, `GatewayBackend`-trait-based, already authenticated.
- **Auth** — API key, session token, and OAuth (OIDC/JWT) authenticators in `wacp-transport`. Sufficient for Console Q2 Phase 1: the Console backend authenticates to the runtime via `ApiKeyAuthenticator`; the Console's user-facing multi-user auth (Argon2id + cookies + tokens + role hierarchy) lives inside the Console backend and is independent of runtime auth.
- **TLS** — rustls on gRPC services via `wacp-runtime::tls`; supports plaintext, system trust store, and custom-CA / pinned-cert / client-cert modes. Aligns with Console Q3 (rustls-everywhere, no OpenSSL). The Console's three-mode trust boundary is Console-side connection logic; no runtime changes required.
- **`wacp-taxonomy`** — `VerticalManifest` and sub-types are `Serialize + Deserialize`, forward-compatible via `#[serde(default)]`, authoritative for vertical schema evolution.
- **Runtime CLI** — `wacp-runtime {serve,validate,defaults}` with TOML config + env overrides.
- **Dockerfile** — multi-stage build producing a ~20–40 MB `debian:bookworm-slim` image (but see G1 below).

### 3.3 What's missing — concrete gaps

> **Status (2026-04-13): All 8 gaps resolved.** Stream A commits A1–A9 landed on `dev` between 2026-04-11 and 2026-04-12. The gap table below is retained as historical context; every item is now closed. See §8.1 for the per-task completion ledger.

Gaps that block or bruise the Console build, ordered by severity:

| # | Gap | Impact | Fix |
|---|---|---|---|
| **G1** | **Port configuration is internally inconsistent.** `crates/wacp-runtime/src/config.rs` defaults to `agent_listen=[::1]:9090`, `highway_listen=[::1]:9091`, `coordinator_listen=[::1]:9402`. `Dockerfile` sets `agent=0.0.0.0:9090`, `highway=0.0.0.0:9091`, and reserves 9092 for metrics and 9093 for health — it never sets `coordinator_listen` and never EXPOSEs 9402, so `CoordinatorService` is **unreachable in the Docker image**. `SEED.md` claims `9400/9401/9402` which matches neither. | Hard-blocks a clean Console setup flow. Every new integrator has to reverse-engineer the correct addresses. `wcon-doctor` cannot emit correct defaults. | Canonicalize the port map (§4.1). Apply to `config.rs`, `Dockerfile`, `deploy/wacp-runtime.service`, and the Console default-runtime-address setting. |
| **G2** | **No release pipeline.** `ci.yml` runs build/clippy/test/fmt but produces no artifacts. There is no `curl \| sh` installer, no published binary, no `cargo install wacp-runtime`, no published Docker image. | Hard-blocks Console dev on a fresh machine; blocks runtime deployment to anything that isn't a local checkout. | Add a tag-triggered release workflow (§4.2). |
| **G3** | **CI ignores all packages added in Phases 25–27S.** The TypeScript job runs only `highway-ui` and ignores `packages/wacp-cli/` (132 tests), `packages/wacp-local/` (86 tests), and every `ecosystem/<id>` package (459 vertical tests + 35 cross-vertical integration tests in `packages/wacp-cli/tests/ecosystem.test.ts`). | Regressions in the CLI or any vertical are not caught by CI. Every Phase-27S-style schema refactor can silently break downstream consumers — including the Console. | Extend `ci.yml` to matrix-build every `packages/*` and `ecosystem/*` workspace (§4.2). |
| **G4** | **`wacp-taxonomy` has no stable version.** The Console's tech-stack proposal git-deps on it to get `VerticalManifest`. Without a pinned tag or crates.io release, every Console build floats against `main` and every schema refactor here breaks the Console unpredictably. | Brittle Console CI; frequent breakages on unrelated WACP commits. | Publish `wacp-types` and `wacp-taxonomy` to crates.io as `0.1.0` (§4.3). |
| **G5** | **No OpenAPI spec on the runtime REST surface.** The Console must hand-maintain request/response shapes for the 16 existing endpoints and every new one. Duplicated work and silent drift. | Every endpoint has to be re-typed in the Console backend. No machine-verified contract at the Console/runtime boundary. | Annotate the 16 REST handlers with `utoipa::path`, derive `ToSchema` on request/response types, emit `openapi.yaml` at build time, CI-check for drift (§4.4). |
| **G6** | **Workspace listing is limited to get-by-id.** The Console's session detail view (`wcon-ui` §6, §7) needs to enumerate the workspace tree for an active session. `/v1/workspaces/{id}` exists but there is no list/parent-filter endpoint. | The Console backend must call gRPC directly from its session module, bypassing the REST pattern Phase 27S established. Workable but inconsistent. | Add `GET /v1/sessions/{id}/workspaces` (or equivalently `GET /v1/workspaces?parent_id=…`) as a thin wrapper over `coordinator.list_workspaces(filter)` (§4.4). |
| **G7** | **Session-scoped trail streaming is undocumented.** The Console's oversight dashboard needs push-based trail updates per session. `StreamSignals` exists over gRPC and `/v1/ws` is generic, but no named session-trail channel is documented for the Console to subscribe to. | The Console backend reinvents a translation layer from gRPC streams to per-session WebSocket channels, duplicating logic the runtime could provide once. | Document a `subscribe_session_trail` RPC on `/v1/ws` or a scoped SSE endpoint. Implementation is a filter over the existing `HighwayService::stream_signals` (§4.4). |
| **G8** | **No mock runtime binary.** `wacp-console/TECH_STACK_PROPOSAL.md` §7.3 specifies an in-process Tonic + Axum mock for integration tests. If Console-land builds its own, the mock will drift from the real runtime. | Console E2E ends up maintaining a parallel implementation of the runtime surface. Mock fidelity — an invariant the Console spec set relies on — is on the honor system. | Ship a `wacp-runtime --mock` mode (or a dedicated `wacp-mock-runtime` binary) that starts the full gRPC + REST stack against an in-memory backend seeded with fixture manifests (§4.5). |

**G1–G5 block a clean Console build.** G6–G8 are soft gaps the Console can work around but at higher maintenance cost. *(All resolved — see status note above.)*

## 4. Runtime productionization (Stream A) — COMPLETE

This workstream closes G1–G8 and delivers the runtime as a versioned, installable artifact. It also completes the spirit of original Phase 29.1 ("public API surface") as a set of upstream changes. **All sub-tasks (§4.1–§4.5, commits A1–A9) landed on `dev` as of 2026-04-12.** A codebase health audit on 2026-04-12 confirmed zero new failures introduced by Stream A; pre-existing CI failures (107 files of fmt drift, 1 test race, 24 clippy warnings) were resolved separately on 2026-04-13. A subsequent **runtime implementation audit** (2026-04-13 → 2026-04-14) found and resolved 17 additional stub/placeholder gaps across the runtime event loop, gRPC handlers, and coordinator path — all 35 RPC handlers are now fully wired with no `unimplemented!()` or `Default::default()` responses remaining.

### 4.1 Canonical port map (G1)

Propose this allocation, contiguous and non-overlapping:

| Service | Bind | Exposed in image |
|---|---|---|
| `AgentService` (gRPC) | `0.0.0.0:9090` | yes |
| `HighwayService` (gRPC) | `0.0.0.0:9091` | yes |
| `CoordinatorService` (gRPC) | `0.0.0.0:9092` | yes |
| REST gateway + WebSocket (HTTP) | `0.0.0.0:9093` | yes |
| Health (`/healthz`, `/readyz`) | `0.0.0.0:9094` | yes |
| Metrics (Prometheus) | `0.0.0.0:9095` | optional |

Fixes required:

- `crates/wacp-runtime/src/config.rs` — change `default_coordinator_listen` from `[::1]:9402` to `[::1]:9092`. Add a doc-comment block above the port defaults pointing at this table.
- Verify and (if necessary) document the REST gateway's current bind — it is not in `rest_gateway.rs::Router::new()`; confirm via `grpc_server.rs` where it is mounted and which `listen` config drives it.
- `Dockerfile` — `EXPOSE 9090 9091 9092 9093 9094` (drop 9092-for-metrics assumption; move metrics to 9095 and gate on an opt-in env var). Set `ENV WACP_SERVER__COORDINATOR_LISTEN=0.0.0.0:9092` and `ENV WACP_SERVER__REST_LISTEN=0.0.0.0:9093`.
- `deploy/wacp-runtime.service` — mirror the env vars.
- `SEED.md` — correct the Architecture Summary line that claims `9400/9401/9402` (informational fix; does not affect code).
- `wacp-console/TECH_STACK_PROPOSAL.md` Q3 ("cryptographic trust boundary") — the default runtime address in Console settings should match this map.

**Exit criteria.** `cargo run --bin wacp-runtime -- defaults` emits the canonical ports. `docker run <image>` binds all five services on the canonical ports. `grep -R '9090\|9091\|9092\|9093\|9094\|9095\|9402' crates Dockerfile deploy` returns only references that match the map.

### 4.2 CI expansion + release pipeline (G2, G3)

**Expand `.github/workflows/ci.yml`:**

- Replace the single `typescript` job with a matrix over `highway-ui`, `packages/wacp-cli`, `packages/wacp-local`, and each `ecosystem/<id>`. Each runs `pnpm install --frozen-lockfile && pnpm typecheck && pnpm test`.
- Add an `integration` job: boot the runtime binary, run `pnpm -C packages/wacp-cli test -- tests/ecosystem.test.ts` against it.
- Extend the `python` job to cover any vertical-specific test suites that land in `sdk-python/`.

**Add `.github/workflows/release.yml`:**

- **Tool:** **`cargo-dist`**. This aligns with Console Q1 (`wacp-console/TECH_STACK_PROPOSAL.md` §10), which committed the Console binary to cargo-dist across five channels. The WACP runtime uses the same tool and the same target matrix so operators see a consistent install story across both binaries.
- **Trigger:** push of a tag matching `v*` (`v0.1.0`, `v0.2.0-rc.1`, …).
- **Target matrix** (mirrors Console §5.2):
  - Tier 1: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`
  - Tier 2: `x86_64-pc-windows-msvc`, `aarch64-unknown-linux-gnu`
- **Five channels** (mirrors Console Q1):
  1. GitHub Release — prebuilt binaries per target with `tar.gz` / `zip` archives + SHA256 sums
  2. Shell installer — `curl https://.../install.sh | sh`, cargo-dist-generated
  3. Homebrew tap — cargo-dist-generated formula
  4. Windows — MSI installer or winget manifest (cargo-dist supports both)
  5. Docker image on GHCR — `ghcr.io/Madahub-dev/wacp-runtime:<tag>` via the fixed-ports `Dockerfile` from §4.1
- **Also builds** `wacp-mock-runtime` (once §4.5 lands) as a second binary artifact on the same target matrix.
- **Optional on the same trigger:** publish `wacp-types` + `wacp-taxonomy` to crates.io (see §4.3).
- **Deferred:** deb / rpm packages. Rationale matches Console Q1 — each distro package carries real per-version maintenance cost (systemd units, FHS conventions, signing, postinst scripts) not served by the current operator audience. `nfpm` / `cargo-deb` / `cargo-rpm` can generate packages later without rewriting the pipeline.

**Exit criteria.** Tagging `v0.1.0` produces: a GitHub Release with Tier-1 + Tier-2 binaries + shell installer + Homebrew formula + (if configured) Windows installer + Docker image on GHCR; and (if §4.3 is adopted) a crates.io publication. The CI matrix runs green on every PR for every package.

### 4.3 `wacp-taxonomy` stability (G4)

Two viable paths. **Option A is recommended.**

**Option A — crates.io publication.**
- Add `description`, `license = "Apache-2.0"`, `repository`, `keywords`, `readme` to both `crates/wacp-types/Cargo.toml` and `crates/wacp-taxonomy/Cargo.toml`. License is already set at workspace level post-Q7 resolution.
- Publish in dependency order: `cargo publish -p wacp-types` → `cargo publish -p wacp-taxonomy`.
- Start at `0.1.0`. Bump to `0.2.0` on any `VerticalManifest` field removal or rename. Patch bumps for additive fields.
- Consumer (Console `Cargo.toml`): `wacp-taxonomy = "0.1"` instead of `git = "…"`.

**Option B — git tag pinning.**
- Introduce an annotated tag `wacp-taxonomy-v0.1.0`.
- Consumer: `wacp-taxonomy = { git = "https://github.com/.../wacp", tag = "wacp-taxonomy-v0.1.0" }`.
- Breaking changes → new tag. No publication workflow.

Option A is cheaper for downstream consumers (standard registry resolution, caching, `cargo update` semantics, docs.rs), slightly more expensive here (one-time publication setup + license check). Option B is cheaper here, more expensive for everyone consuming. Prefer A.

**Name collision check.** Before publishing, verify `wacp-taxonomy` and `wacp-types` are available on crates.io. If taken, fall back to `wacp-protocol-taxonomy` / `wacp-protocol-types`.

**Exit criteria.** Console repo `Cargo.toml` pins a registry version (or tag) rather than a floating `git = "..."`.

### 4.4 REST surface audit + OpenAPI (G5, G6, G7)

**OpenAPI generation (G5):**

- Add `utoipa = "5"` (and `utoipa-axum` for automatic route registration) to `crates/wacp-transport/Cargo.toml`.
- Annotate each of the 16 handlers in `rest_gateway.rs` with `#[utoipa::path(...)]` including request params, response types, status codes, and the endpoint's error shapes.
- Derive `ToSchema` on every request and response struct in `crates/wacp-transport/src/messages.rs`.
- Add `crates/wacp-transport/src/bin/gen_openapi.rs` — a small binary that builds the `OpenApi` doc from `utoipa` and prints it. Commit the generated `openapi.yaml` at the repo root.
- CI step: `cargo run -p wacp-transport --bin gen_openapi > openapi.yaml && git diff --exit-code openapi.yaml` — fails the PR if the checked-in spec and the derived spec diverge.
- The Console consumes `openapi.yaml` via `openapi-typescript` (per its tech stack §4) to generate the frontend client.

**Workspace listing (G6):**

- New handler: `GET /v1/sessions/{id}/workspaces` → `Vec<WorkspaceSummary>`.
- Wire to `coordinator::Coordinator::list_workspaces(filter)` — already implemented for the in-process path.
- Response: `[{ id, parent_id, state, role, role_derived_from, created_at, last_signal_type, budget_consumed }]`.
- `WorkspaceSummary` lives in `wacp-transport::messages`, derives `Serialize + ToSchema`.

**Session-scoped trail streaming (G7):**

- Extend `/v1/ws` with a `subscribe_session_trail` method. The request carries `{ session_id, from_hlc? }`.
- Implementation: subscribe to `HighwayService::stream_signals`, filter by session ancestry in the workspace tree, relay as WebSocket messages.
- Document the message schema (event types, payload shape) in `openapi.yaml` via a WebSocket-channels section (OpenAPI 3.1 + AsyncAPI reference, or an ad-hoc sibling `openapi-ws.yaml` if the tooling path is too heavy).

**Exit criteria.** `openapi.yaml` is committed and CI-gated. Console backend consumes `wacp-taxonomy` + `openapi.yaml` — no hand-maintained runtime types. `GET /v1/sessions/{id}/workspaces` returns a workspace tree. `/v1/ws subscribe_session_trail` pushes scoped trail updates.

### 4.5 Mock runtime binary (G8)

- Add `crates/wacp-runtime/src/bin/mock.rs` (or, cleaner, a new crate `crates/wacp-mock-runtime/`) that:
  - Starts the full gRPC + REST + WebSocket stack on the canonical ports.
  - Uses an in-memory `GatewayBackend` impl — the existing `wacp-transport::in_process::InProcessTransport` is the scaffold.
  - Loads fixture manifests from an embedded asset directory. Two profiles: `--fixtures simple` (1 vertical, 2 tools) and `--fixtures complex` (all 7 verticals).
  - Exposes a deterministic signal script: `--script tests/fixtures/signals/session_complete.yaml` replays pre-recorded signals on demand.
- Ships as a release artifact alongside `wacp-runtime` (§4.2).
- Console's E2E harness (`wacp-console/tests/e2e/`) boots this as a sidecar.

**Exit criteria.** `wacp-mock-runtime --fixtures complex serve` starts in under a second on developer hardware, serves `GET /v1/verticals` with all 7 manifests, and responds to `SubmitGoal` with a synthetic session identifier.

## 5. wacp-console build (Phase 29.2)

`wacp-console` is built in its own repository. This repo does not own its code. But Console buildability depends on Stream A landing §4.1–§4.4 cleanly:

| Stream A output | wacp-console consumer |
|---|---|
| §4.1 port map | Default runtime endpoints in `runtime.{grpc,rest,ws}_address` settings |
| §4.2 release binary | `wcon-doctor` can locate and validate a runtime binary on PATH |
| §4.3 stable `wacp-taxonomy` | `Cargo.toml` dep resolution in `console-db` / `console-core` stops floating against `main` |
| §4.4 OpenAPI + REST expansion | `openapi-typescript` codegen for the Console frontend; workspace tree endpoint for session detail |
| §4.5 mock runtime binary | Console E2E test sidecar |

**Recommended Console kickoff order** (tracked in `wacp-console/SPEC_BUILD.md` and its own `IMPLEMENTATION.md`):

1. **Finish spec set.** §10 of `wacp-console/TECH_STACK_PROPOSAL.md` is now answered (Q1–Q7; Q7 is resolved by this repo's split in commit `ef20421`). **But Q2's Phase-1 multi-user auth commitment requires 7 Console-side spec revisions before `/impl-plan` can proceed:** a new `wcon-auth` spec (identity, sessions, authorization, bootstrap, password policy, audit schema, threat model) plus revisions to `wcon-architecture` §8, `wcon-data-model` §5 (new tables `users`, `user_sessions`, `api_tokens`, `audit_log`, `login_attempts` + `owner_user_id`/`visibility` columns on profile and session records), `wcon-api` (`/v1/auth/*`, `/v1/users/*`, `/v1/tokens/*`, `/v1/audit-log` endpoint families), `wcon-profiles` (ownership + visibility + ACL), `wcon-sessions` (launch identity + stream authorization), and `wcon-ui` (login screen, user menu, admin pages, audit log viewer). This is the critical-path blocker on the Console side and is independent of Stream A — the Console cannot write code against a data model that has no `owner_user_id` column until the spec revisions land.
2. **Bridge phase.** `/tech-stack` → `/impl-plan` → `/setup-dev` inside `wacp-console` after step 1.
3. **Code phase.** After §4.1–§4.4 land on the runtime side. Console specs and scaffolding can proceed in parallel with Stream A; only integration testing and release require Stream A complete.

### 5.1 Console tech-stack decisions (Q1–Q7) — reference

Recorded here for cross-session lookup. Decisions come from `wacp-console/TECH_STACK_PROPOSAL.md` §10. The last column records where each decision affects runtime-side work in this repo.

| Q | Topic | Decision | Runtime-side impact |
|---|---|---|---|
| **Q1** | Distribution | `cargo-dist`, five channels (GitHub Release, shell installer, Homebrew tap, Windows MSI/winget, Docker image). Tier 1: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`. Tier 2: `x86_64-pc-windows-msvc`, `aarch64-unknown-linux-gnu`. deb/rpm deferred. | **§4.2** — runtime uses the same tool, matrix, and channel set. Operators install both binaries the same way. Docker base image note: Console ships distroless; WACP currently ships `debian:bookworm-slim`. Alignment optional, not blocking. |
| **Q2** | Auth scope | Multi-user in Phase 1. Local users, Argon2id password hashing, SQLite storage. Cookie sessions (HttpOnly, Secure, SameSite=Strict, CSRF-protected). Named API tokens (bearer, scoped, revocable, hashed at rest). Three-role hierarchy (`admin` ⊃ `operator` ⊃ `viewer`). Ownership/visibility on profiles and sessions. First-launch bootstrap admin credential. Audit log + rate limiting. Future OIDC path. | Runtime **unchanged** — `wacp-transport::ApiKeyAuthenticator` already covers the Console backend → runtime channel. The Console's user-facing auth lives inside the Console backend. **Unblocks**: nothing on runtime side. **Blocks**: Console `/impl-plan` until 7 spec revisions land (see §5 step 1). |
| **Q3** | gRPC TLS trust | Three modes, selected by address scheme: plaintext on loopback (implicit), system trust store (`https://` / `grpcs://`), explicit CA / pinned cert (`runtime.tls_ca_pem` or `runtime.tls_pinned_cert_sha256`). Fails closed on non-loopback plaintext unless `WCON_ALLOW_INSECURE_TLS=1`. rustls everywhere, no OpenSSL. | Runtime **unchanged** — `wacp-runtime::tls` already supports all three modes. The Console's connect-time logic and `runtime` table columns (`tls_mode`, `tls_ca_pem`, `tls_pinned_cert_sha256`, `tls_server_name`, `tls_client_cert_pem`, `tls_client_key_pem`) are Console-side implementation. |
| **Q4** | Frontend bundle | Embed by default via `rust-embed`. `--frontend-path <dir>` override for dev convenience. Single release-build path, single dev-build path, no dual-code-path drift. | Console-internal. No runtime impact. |
| **Q5** | Telemetry | Never phones home. `settings.telemetry.enabled` off by default. When enabled, exports OTLP to an *operator-configured* endpoint only — no Anthropic/WACP-owned URL is ever hard-coded. | Console-internal policy. The runtime follows the same pattern: `tracing-opentelemetry` export is opt-in, endpoint is operator-configured (see §4.2 observability mention). Aligned. |
| **Q6** | Profile YAML versioning | `format_version: 1` required top-level integer key in all profile exports. Unknown major version → new `INVALID_FORMAT_VERSION` error code. Unknown fields within a known version → import warning (forward-compat). | Console-internal (profiles are Console entities, not runtime entities). **Follow-up consideration**: the runtime's `ecosystem/*/vertical.yaml` manifests currently have no `format_version` key; adopting the same convention would improve forward-compat symmetry. Out of scope for Q6 but worth revisiting as a Phase 27S follow-up. |
| **Q7** | License | Apache-2.0 for reference implementation; CC BY-SA 4.0 for protocol spec in sibling `wacp-protocol` repo. | **Resolved in commit `ef20421`** — this repo fully relicensed, protocol spec extracted to sibling repo, 15 cross-references updated to GitHub URLs, 10 TypeScript `package.json` files given explicit `"license": "Apache-2.0"`. Remaining manual step: push `wacp-protocol` to GitHub (see `ef20421` commit body). |

## 6. Phase 28 — IDE + chat bridge (Stream B)

### 6.1 Independence analysis

Phase 28 produces two applications. Both connect to a running `wacp-runtime` via existing transport:

| Component | Runtime dependency | Independent of |
|---|---|---|
| VS Code / JetBrains extension | `HighwayService` gRPC for trail + gates + signals; `AgentService` for workspace state; `/v1/verticals` for context hints | Verticals (27A–G), `wacp-console`, each other |
| Slack / Discord chat bridge | `CoordinatorService.SubmitGoal`; `HighwayService.StreamSignals`; `/v1/gates/{id}/respond` for approvals | Verticals, `wacp-console`, IDE extension |

Neither requires anything from `wacp-console`. Both benefit from Stream A's §4.1 (ports) and §4.2 (release binary) but do not strictly block on them — a developer running the runtime from `cargo run` locally can exercise both in dev.

**Conclusion:** **Stream B is independent of Stream A and of `wacp-console`.** It can ship in parallel. The only shared dependency is a reachable runtime, which already exists for local-loopback use.

### 6.2 Scope

- **28.1 IDE extension.** VS Code first; JetBrains as a follow-on. Features: workspace panel (live workspace tree of the active session), signal stream (tail of signals emitted by child workspaces), inline checkpoints (click a checkpoint in the trail to jump to the referenced file), gate approval UI (respond to a gate inline without leaving the IDE).
- **28.2 Chat bridge.** Slack first; Discord as a follow-on. Features: `/wacp goal …` slash command → `CoordinatorService.SubmitGoal`; signal stream posted as threaded replies; gate approvals via emoji reaction or button press. Runs as a small standalone service a team self-hosts.

Both reuse existing gRPC transport. No new protocol surface is required.

## 7. Push strategy

### 7.1 Stream map

```
Stream A — Runtime productionization + Console API surface
  §4.1 port alignment            ──┐
  §4.2 CI + release pipeline       │
  §4.3 wacp-taxonomy on crates.io  │──▶  wacp-console integration-testable
  §4.4 OpenAPI + REST expansion    │
  §4.5 mock runtime binary         ──┘

Stream B — Phase 28 applications (parallel to Stream A)
  §6.2 IDE extension (VS Code)   ──▶ IDE GA
  §6.2 Chat bridge (Slack)       ──▶ Chat bridge GA

Stream A outputs ──▶ wacp-console (separate repo)
  Spec finalization + tech-stack answers
  Scaffolding + backend services
  Frontend surfaces
  E2E against mock runtime (from §4.5)
  Release against real runtime (from §4.2)
```

### 7.2 Sequencing within Stream A

The five sub-streams of §4 have a soft dependency order:

1. **§4.1** first — touches `Dockerfile`, `config.rs`, systemd, SEED-CONTEXT. Everything downstream references the canonical port map.
2. **§4.2** second — guards every subsequent PR. Landing §4.2 before §4.3/§4.4 means the expansion PRs are CI-gated from the first commit.
3. **§4.3** third — atomic publication step. Independent of §4.4, so can float.
4. **§4.4** fourth — the largest single work item. Proceeds in parallel with §4.5.
5. **§4.5** fifth — or concurrent with §4.4. Blocks Console E2E but nothing else.

### 7.3 Cross-stream dependencies

- **Stream A → `wacp-console`:** Hard dependency for release. Soft for spec and scaffolding.
- **Stream A → Stream B:** Soft. Phase 28 apps *run* against a local `cargo run`; they *release* against §4.2's binary distribution.
- **Stream B ↔ `wacp-console`:** None. Parallel peers with different client shapes.

### 7.4 Suggested execution order

**If one workstream at a time:** §4.1 → §4.2 → §4.3 → §4.4 → §4.5 → `wacp-console` core → Stream B.

**If two workstreams in parallel:**
- Main: §4.1 → §4.2 → §4.3 → §4.4 → `wacp-console`
- Side: §4.5 → Stream B (start with the chat bridge — smaller scope, validates Phase-28 assumptions before committing to the IDE extension)

## 8. Task inventory

Executable tasks, ordered by stream. Each is small enough to land in one focused session with tests.

### 8.1 Stream A

| # | Task | Files / crates | Blocker |
|---|---|---|---|
| **A1** | ~~Canonicalize port map~~ | `config.rs`, `grpc_server.rs`, `Dockerfile`, `deploy/`, `runtime-manager.ts`, 5 impl specs, `SEED.md` | **Done** |
| **A2** | ~~CI matrix for `packages/wacp-cli`, `packages/wacp-local`, `ecosystem/*`~~ | `.github/workflows/ci.yml` | **Done** |
| **A3** | ~~`release.yml` — tag-triggered matrix build + GitHub Release + GHCR image~~ | `.github/workflows/release.yml` | **Done** |
| **A4** | ~~Prepare `wacp-types` + `wacp-taxonomy` for crates.io~~ | `crates/wacp-types/Cargo.toml`, `crates/wacp-taxonomy/Cargo.toml` — metadata ready, `cargo publish` pending | **Done** (metadata) |
| **A5** | ~~Annotate 16 REST handlers with `utoipa::path`; derive `ToSchema` on request/response types~~ | `crates/wacp-transport/src/rest_gateway.rs`, `Cargo.toml` | **Done** |
| **A6** | ~~`gen_openapi` binary; commit `openapi.yaml`; CI drift check~~ | `gen_openapi.rs`, root `openapi.yaml`, `ci.yml` drift step | **Done** |
| **A7** | ~~`GET /v1/sessions/{id}/workspaces`~~ | `rest_gateway.rs` — handler, backend trait, mock, test, OpenAPI | **Done** |
| **A8** | ~~`subscribe_session_trail` method on `/v1/ws`~~ | `websocket.rs` — JSON-RPC method + 2 tests | **Done** |
| **A9** | ~~Mock runtime binary with fixture loader~~ | `crates/wacp-runtime/src/bin/mock.rs` — gRPC + REST + WS, simple/complex fixtures | **Done** |

### 8.2 Stream B

| # | Task | Repo / crate | Blocker |
|---|---|---|---|
| **B1** | IDE extension scaffolding (VS Code) | new `apps/vscode-extension/` or separate repo | A1 |
| **B2** | Workspace panel + signal stream | same | B1 |
| **B3** | Inline checkpoints + gate approval UI | same | B2 |
| **B4** | Chat bridge scaffolding (Slack) | new `apps/chat-bridge/` or separate repo | A1 |
| **B5** | `SubmitGoal` slash command + signal relay | same | B4 |
| **B6** | Gate approval via reaction / button | same | B5 |

### 8.3 `wacp-console` (tracked in its own repo)

Executed via the Console's own `/tech-stack` → `/impl-plan` → `/setup-dev` chain after A1–A6 land. Not tracked in detail here.

## 9. Phase history (compressed)

All phases below are complete. Line items are pointers; full detail lives in commit messages and spec files.

| Phase | Name | Status | Key artifact |
|---|---|---|---|
| 0–19 + T1–T5 | Runtime core (12 Rust crates) | **Complete** | 947 Rust tests, gRPC services wired |
| 20 | Tool framework (`wacp-tools`) | **Complete** | 124 tests |
| 21 | LLM adapters (`wacp-llm`) | **Complete** | 134 tests, Anthropic + OpenAI |
| 22 | Agent SDK v2 + coordinator SDK client | **Complete** | 69 tests |
| 23 | Security + transport extensions (API key, session token) | **Complete** | 136 tests |
| 24 | Local SDK (`@wacp/local`) | **Complete** | 86 tests |
| 25 | CLI agent (`@wacp/cli` base) | **Complete** | 70 tests |
| 26 | SWE vertical (`@wacp/swe`) | **Complete** | 57 tests |
| 26R | Remediation: `CoordinatorService` server, orchestrator, protocol-aware CLI, REST handlers, WebSocket, Python bindings, OAuth | **Complete** | 8 architectural gaps closed |
| 27A | DevOps vertical | **Complete** | 73 tests |
| 27B | MLOps vertical | **Complete** | 67 tests |
| 27C | Finance vertical | **Complete** | 83 tests |
| 27D | Healthcare vertical | **Complete** | 90 tests |
| 27F | Data Analytics vertical | **Complete** | 73 tests |
| 27G | Data Science vertical | **Complete** | 73 tests |
| 27R | Vertical wiring remediation — multi-vertical ecosystem loader, cross-vertical router, tool dispatch, end-to-end constraint enforcement | **Complete** | 35 cross-vertical tests |
| 27S | Vertical surfacing — enriched `*_VERTICAL` descriptors, manifest generator, runtime manifest loader, `GET /v1/verticals[/{id}]` | **Complete** | 9 new transport + taxonomy tests |
| **28** | IDE + chat bridge | **Pending** (Stream B) | — |
| **29.1** | Runtime productionization + public API surface | **Complete** (Stream A, A1–A9) | 9 commits, CI-clean |
| — | Runtime implementation audit (17 gaps: C1–C8, K1–K6, A1–A3) | **Complete** (5 phases, `c875804`–`4c42173`) | All stubs/placeholders resolved |
| **29.2** | `wacp-console` Dashboard | **Pending** (separate repo) | — |

## 10. Open questions and risks

### 10.1 Console backend vs. gRPC-Web direct-to-runtime

The Console's tech-stack proposal is firm: the Console backend talks gRPC to the runtime, and the Console frontend talks REST/WebSocket to the Console backend. An alternative — gRPC-Web browser client straight to the runtime — was considered and rejected for auth scope and frontend-aggregation reasons. Stream A §4.4's OpenAPI is therefore consumed by the Console *backend*, not the Console *frontend*; the frontend consumes a separate Console-backend OpenAPI. No conflict with any Stream A decision.

### 10.2 `wacp-taxonomy` name on crates.io

The `wacp-taxonomy` crate name may or may not be available. Verify before §4.3 lands; fall back to `wacp-protocol-taxonomy` if taken. Same consideration for `wacp-types`.

### 10.3 Mock runtime scope creep

The mock must remain an adapter, not a second runtime. Mitigation: every mock handler MUST construct its response from a `wacp-taxonomy::VerticalManifest` or from `in_process::InMemoryBackend`. No handwritten response bodies. Enforced by a mock-fidelity test that cross-checks mock responses against the real runtime for a shared fixture set.

### 10.4 Phase 28 platform priorities

VS Code before JetBrains. Slack before Discord. Subjective — revisit if stakeholders explicitly want otherwise. Chat bridge likely ships first (smaller scope, validates the runtime-as-service model with fewer moving parts).

### 10.5 Release cadence

Not defined. Suggested default: tag on any change that is visible in `openapi.yaml` or `VerticalManifest` (new endpoint, new vertical, new manifest field, auth provider). `0.1.x` for additive; `0.2.0` for breaking. Revisit once Console is consuming releases.

### 10.6 ~~Port 9402 → 9092 migration risk~~ (resolved by A1)

Canonical port map is now `9090/9091/9092/9093/9094/9095` across all code, deployment files, and docs. Zero references to the old `9400/9401/9402` range remain in code or impl specs.

### 10.7 ~~SEED.md port drift~~ (resolved by A1)

Architecture Summary in `SEED.md` now documents the canonical port map. No drift.

---

## References

| ID / Path | Title | Relationship |
|---|---|---|
| `SEED.md` | WACP session primer | current state (authoritative for what is built) |
| `wacp-console/TECH_STACK_PROPOSAL.md` | Console tech stack proposition | consumer of Stream A §4 |
| `wacp-console/SPEC_BUILD.md` | Console spec build + ADR-001 | consumer of `GET /v1/verticals` |
| `crates/wacp-transport/src/rest_gateway.rs` | 16 REST handlers | modified in Stream A §4.4 |
| `crates/wacp-transport/src/websocket.rs` | `/v1/ws` JSON-RPC handler | modified in Stream A §4.4 (G7) |
| `crates/wacp-runtime/src/config.rs` | Runtime config + port defaults | modified in Stream A §4.1 |
| `.github/workflows/ci.yml` | Existing CI | extended in Stream A §4.2 |
| `Dockerfile` | Runtime image | modified in Stream A §4.1 |
| `deploy/wacp-runtime.service` | systemd unit | modified in Stream A §4.1 |
| `LAYER-MAPPING.md` | Historical map of mada-os layers → WACP | kept for spec lineage references |
| `README.md` | Public protocol README | independent — may need a separate accuracy pass |

---

*WACP implementation strategy — Akil Abderrahim and Claude Opus 4.6*
