# WACP Platform — Seed Context

> Compressed summary of the full design, current implementation state, and next steps for the unified `wacp-platform` monorepo.
> For detail on any topic, follow the spec references. For task-level implementation detail, see `wacp/IMPLEMENTATION.md` (runtime) and `wacp-console/IMPLEMENTATION.md` (console).
> For the wiring strategy (runtime ↔ console integration plan), see `impl/wiring-strategy.md`.
> For the monorepo merge procedure (M0–M7), see `impl/merge-plan.md` and `impl/merge-execution-log.md`.
> For the latest codebase health + testing strategy, see `AUDIT-2026-04-15.md`.

## What This Is

The `wacp-platform` monorepo houses two binaries that ship together:

- **WACP runtime (`wacp/`)** — the protocol reference implementation. Rust workspace (15 crates) + TypeScript CLI/SDK/verticals + Python SDK. Serves gRPC (Agent, Highway, Coordinator) + REST + WebSocket for the 7 verticals.
- **WACP Console (`wacp-console/`)** — the operator workbench. Rust/Axum backend (6 crates) + React 19 SPA. Discovers agent roles, manages agent profiles, launches coordination sessions, and oversees agent work in real time. The Console is a **client** of the runtime — it connects via gRPC and REST, never modifies protocol behavior, never executes LLM calls.

They are shipped as two binaries with gRPC between them; the monorepo exists for development ergonomics (shared proto codegen, version-locked types, unified CI/fmt/clippy).

**Specs:** runtime protocol lives in the sibling `wacp-protocol` repo (CC BY-SA 4.0). Console design specs under `wacp-console/specs/` (12 finalized). Anchor spec for the Console: `wcon-vision`.

## Current State (Post M0–M7 merge + W1–W7 wiring + post-audit workstream)

**Branch:** `dev` @ `d8ca8ff` (ahead of `main` @ `b061c71` by 14 commits). Working tree clean. CI green on all five workflows (`ci-lint`, `ci-wacp`, `ci-console`, `release-runtime`, `release-console`) plus the new `coverage` workflow.

**Since the 2026-04-15 audit (from `a6773d6`)**, these have landed in order — first eleven on `main`, subsequent fourteen on `dev` (pending a batched merge):

| SHA | Subject | Audit anchor |
|-----|---------|--------------|
| `71b44b6` | docs(audit-04-15): land audit, move prior audit, fix Phase 4.6 step names | §11 #4, §11 #5 |
| `60d2245` | ci(deny): add cargo-deny workspace gate | §11 #1 |
| `7ba4ae8` | ci(release): SBOM (CycloneDX) + Trivy scan on published OCI images | §11 #2 |
| `d10ed29` | feat(transport): re-key auth HashMaps by SHA-256 digest, constant-time ws check | §11 #3 |
| `7f0736b` | ci(coverage): land cargo-llvm-cov, vitest v8, coverage.py + Codecov | §12.1 |
| `840450a` | test(coverage): Rust branch-coverage sweep — §12.2 T1–T10 | §12.2 T1–T10 |
| `82a4213` | test(frontend): add isolated per-file runner + tighten V8 heap cap | new §13.6 |
| `d71c4fe` | fix(frontend): a11y label bindings + client double-read | test-adjacent prod fixes |
| `fe48c7b` | test(frontend): F1–F6/F9 RTL suite + ProfilesPage monolith split | §12.3 F1–F6, F9 |
| `a510249` | docs(audit-04-15): append §13 post-audit progress | audit update |
| `5e5733b` | docs(audit-04-15): expand §13.7 — task breakdown with deliverables | §13.7 task packages |
| `b061c71` | docs(seed): refresh post-audit progress + point at §13.7 packages | seed refresh |
| `d63648a` | fix(frontend): ProfilesPage.actions render loop + global RTL cleanup | §13.7.1 |
| `b17ae49` | docs(audit-04-15): close §13.7.1 — actions.test render loop resolved | §13.7.1 |
| `8487f2f` | docs(wacp-console): performance-optimization notes from §13.7.1 work | perf-opt doc seeded |
| `e870018` | test(frontend): F7 Wizard.test.tsx — 41 tests across 6 steps + cross-cutting | §13.7.2 |
| `19e4203` | docs(audit-04-15): close §13.7.2 — F7 Wizard tests landed | §13.7.2 |
| `78512c1` | docs(wacp-console): fold §13.7.2 learnings into performance-optimization notes | perf-opt update |
| `92b3ddb` | test(frontend): F8 rest-of-oversight — 52 tests across 4 surfaces | §13.7.3 |
| `ef7940d` | docs(audit-04-15): close §13.7.3 — F8 oversight tests landed | §13.7.3 |
| `eec5486` | docs(wacp-console): §2.5 — spec-vs-impl drift lesson from F8 | perf-opt update |
| `543c295` | test(frontend): F10 Notifications.test.tsx — 16 tests, drift documented | §13.7.4 |
| `c519201` | docs: close §13.7.4 + fold F10 Notifications stub finding into §2.5 | §13.7.4 |
| `2fdf191` | test(console-db): §13.7.5 — fault-injection harness + 83 coverage tests | §13.7.5 / §12.2 T11 |
| `f75a2a7` | docs(wacp-console): perf-opt §9 — backend drifts surfaced by §13.7.5 | perf-opt update |
| `d8ca8ff` | docs(audit-04-15): close §13.7.5 — console-db T11 landed | §13.7.5 / §12.2 T11 |

What this delivered, in English: supply-chain scanning (cargo-deny, SBOM, Trivy) is in CI; runtime auth is constant-time via SHA-256 digest rekey; the full coverage-tooling stack (cargo-llvm-cov, Vitest v8, coverage.py, Codecov with per-component flags) is wired; Rust branch-coverage tests landed for T1–T11 (~11,900 lines; T11 `console-db` brought that crate from 55.6 % → 98.3 % region coverage via a new `src/testing.rs` fault-injection harness and 83 tests); frontend RTL tests landed for F1–F10 save F9 which was already green (~5,200 + ~2,100 additional lines for F7/F8/F10); a per-file isolated vitest runner plus a 1536 MB V8 heap cap keeps the now-much-larger frontend suite from crashing WSL; and `wacp-console/performance-optimization.md` aggregates the frontend-side `useEffect`-dep + spec-vs-impl drifts (§2.5) and the backend-side schema-vs-struct drifts (§9) that each session surfaces.

**Runtime (`wacp/`).** 15 Rust crates, ~1,280 Rust tests + TS matrix (10 packages + 7 verticals, ~1,000 tests) + Python SDK (104 tests across 3.11–3.13). All 35 gRPC RPCs fully wired across `AgentService`, `HighwayService`, `CoordinatorService`. REST gateway exposes 16 `/v1/*` endpoints + `/v1/ws`. OpenAPI drift-checked in CI. Stream A (A1–A9) closed all 8 Console-facing integration gaps; the 17 runtime-side stub/placeholder gaps identified in the subsequent implementation audit are all resolved. Port map canonicalized to `9090/9091/9092/9093/9094/9095`.

**Console (`wacp-console/`).** 6 Rust crates, 66+ REST endpoints, 99+ backend unit tests. React 19 + Vite + TanStack Query + Zustand frontend (37 TS files, 9,367 lines). Now fully wired to the real runtime after W1–W7:

- **W1** gRPC pool in `AppState`; `/api/health` queries live pool + REST gateway (not mocks).
- **W2** Launch flow: real `CoordinatorService` sequence (`SubmitGoal` → `Decompose` → `Dispatch×N` → envelope send), rollback via `AbortWorkspace` on partial failure.
- **W3** `SessionMonitor`: one Tokio task per session, bounded `broadcast` fan-out (cap 256), four stream drivers (Trail, Gates, Escalations, WorkspaceChanges).
- **W4** Highway forwarding: gate resolve, escalation respond, directive inject all hit real `HighwayService` gRPC.
- **W5** Cancel calls `AbortWorkspace`; startup recovery scans `state='active'` and respawns monitors.
- **W6** Cross-session pending endpoints (`/api/gates/pending`, `/api/escalations/pending`, `/api/refusals/pending`) aggregate from live monitor state.
- **W7** Integration harness (`integration/tests/`) — `lifecycle` and `cross_session` passing; `chaos` covers broadcast backpressure + reconnect. Two scenarios (`T7.2`, `T7.3`) `#[ignore]`-ed pending an LLM stub on the runtime side.

**Working end-to-end against a live runtime:** discovery (roles, tools, verticals, types, search), profile CRUD with validation/versioning/export/import/clone, multi-user auth (Argon2id, CSRF double-submit, rate limiting, 256-bit bootstrap credential at 0o600), session launch + oversight (trail stream, gate queue, escalation inbox, refusal panel, workspace tree, injection bar across 7 WebSocket channels), startup recovery, cross-session pending aggregation.

**Not yet present (tracked, not regressions):** Playwright E2E suite (Phase 7.6–7.10, audit §13.7.7), LLM-stub-dependent integration scenarios (T7.2/T7.3, §13.7.6), the five new Rust integration + chaos suites I1–I5 (§13.7.8; I6 folds into §13.7.6), mutation-testing workflow (§13.7.9), and Codecov monthly ratchet (§13.7.10, deferred until the new baseline settles). All broken out with deliverables in `AUDIT-2026-04-15.md` §13.7. Supply-chain scanning, the F-series frontend sweep, and the Rust branch-coverage sweep (T1–T11) are all landed.

### Milestone history

All milestones passed evaluation. Reports at `wacp-console/impl/phase-{1..6}-eval.md`, `wacp-console/impl/wiring-eval.md` (per-phase W1–W7), and `impl/merge-execution-log.md` (M0–M7).

| Track | Phases | Outcome |
|---|---|---|
| Console design | 0–4 (backend) + 5–6 (frontend) | 99 backend tests, 66 endpoints, full SPA |
| Runtime Stream A | A1–A9 | Port map, CI matrix, cargo-dist, crates.io metadata, OpenAPI, workspace listing, mock runtime |
| Monorepo merger | M0–M7 | Umbrella workspace, `wacp-proto` extraction, unified CI, hoisted tooling, docker-compose |
| Wiring | W1–W7 | Real gRPC+REST integration, session monitor, cancel + recovery, cross-session pending, integration tests |

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

## Next Steps: Close §13.7 work packages

The merger (M0–M7) and wiring (W1–W7) are done. The 2026-04-15 audit's §11 pre-release punch list (items 1–5) is landed, along with §12.1 tooling, §12.2 T1–T10, and §12.3 F1–F6/F9. Remaining work is tracked as ten numbered packages in `AUDIT-2026-04-15.md` §13.7, each with status, blockers, effort, deliverables, and acceptance criterion.

### Pre-`v0.1.0` punch list (from `AUDIT-2026-04-15.md` §11)

| # | Item | Status |
|---|---|---|
| 1 | ~~CI — add `cargo-deny` to `ci-lint.yml` + `deny.toml` at platform root~~ | **done** — `60d2245` |
| 2 | ~~CI — add SBOM + Trivy OCI scanning to `release-runtime.yml` + `release-console.yml`~~ | **done** — `7ba4ae8` |
| 3 | ~~Runtime auth — wrap post-lookup equality with `subtle::ConstantTimeEq` + re-key HashMap by hashed-token~~ | **done** (exceeded spec) — `d10ed29` |
| 4 | ~~Doc fix — `wacp-console/IMPLEMENTATION.md` Phase 4.6 step names~~ | **done** — `71b44b6` |
| 5 | ~~Move/cross-link `wacp/AUDIT-2026-04-12.md`~~ | **done** — `71b44b6` (now at `AUDIT-2026-04-12.md`) |
| 6 | Schedule the LLM stub that unblocks W7 T7.2/T7.3 | open — audit §13.7.6 |
| 7 | Plan the frontend test build-out (Phase 7.5–7.10 Playwright E2E) | F-series **complete** — `d63648a` + `e870018` + `92b3ddb` + `543c295`; Playwright open — audit §13.7.7 |

### Testing coverage initiative (audit §12) — progress snapshot

| Weeks | Phase | Status |
|---|---|---|
| 1–2 | Tooling — `cargo-llvm-cov`, Vitest v8, Python `coverage --branch`, Codecov | **landed** — `7f0736b` |
| 3–4 | Rust branch gap — T1–T11 | **landed** — T1–T10 in `840450a` (+~9,800 lines); T11 `console-db` in `2fdf191` (+2,102 lines; region 55.6 → 98.3 %, line 63.4 → 99.1 %). |
| 5–6 | Frontend + E2E | F-series **complete** — `fe48c7b` (F1–F6/F9, ~5,200 lines) + `e870018` (F7, 41 tests) + `92b3ddb` (F8, 52 tests) + `543c295` (F10, 16 tests). Playwright scenarios not started — §13.7.7. Blocked partly on LLM stub (§13.7.6) for golden-path + multi-user. |

Mutation testing (`cargo-mutants`, `stryker`) still pending — audit §13.7.9. Codecov monthly ratchet deferred until baseline stabilizes — audit §13.7.10. Integration + chaos additions I1–I5 ready (§13.7.8); I6 waits on the LLM stub.

**End-state targets unchanged:** Rust 95% branch, Console frontend 95% branch, TS verticals 95% branch, Python SDK 95% branch.

### New workstream added this session — frontend test-execution stability

Not in the original audit. Running the newly-written F-series RTL suite under `pool: "forks"` with `maxWorkers: 1` and `--max-old-space-size=6144` climbed one worker's RSS past 5 GB across files and killed WSL twice. Two fixes landed:

- `wacp-console/frontend/vitest.config.ts` — `execArgv` lowered from 6144 MB to 1536 MB. Forces V8 GC pressure early; single-file leaks OOM cleanly inside vitest instead of escaping into system memory.
- `wacp-console/frontend/scripts/run-tests-isolated.sh` + `npm run test:isolated` — per-file process isolation via a shell loop. Commits `82a4213`, `d71c4fe`, `fe48c7b` (suite itself).

One outstanding known issue from that work — `ProfilesPage.actions.test.tsx` OOMing inside its own per-file cap — was **resolved in `d63648a` (§13.7.1)**: root cause was an infinite render loop driven by a `mockImplementation` returning a fresh object per call, not a memory leak. The RTL `cleanup()` hook was also missing from `test-setup.ts` and was added globally in the same commit. Full post-mortem + extended pattern library live at `wacp-console/performance-optimization.md`. `npm run test:isolated` now exits 0 across all 17 files with session peak ~291 MB RSS and walltime ~62 s.

### Resumption Point

**M0–M7 merger, W1–W7 wiring, runtime implementation audit, §11 pre-release punch list (1–5), §12.1 tooling, §12.2 T1–T11, §12.3 F1–F10, and audit §13.7.1–§13.7.5 all complete.** HEAD at `d8ca8ff` on `dev` (14 commits ahead of `main`). Working tree clean.

When resuming:
1. Read `AUDIT-2026-04-15.md` §13 (~10 min) — the appendix is the authoritative record of what has landed since the audit and what remains. The §13.8 tracking table is the fastest index; §13.7.1–§13.7.5 rows are all struck through with commit SHAs. Skip §1–§12 unless you need the underlying rationale; those sections describe the 2026-04-15 snapshot and are largely fossilized now.
2. Read `wacp-console/performance-optimization.md` (~5 min) — working document aggregating findings across sessions. §2–§3 cover frontend `useEffect`-dep + StrictMode + render-radius patterns and the spec-vs-impl drifts surfaced by writing tests. §9 covers the `console-db` spec-vs-schema drifts from the §13.7.5 sweep. Any new performance-adjacent observation this session should extend it.
3. `cd /home/aakil98/mada/wacp-platform` and pick a package from §13.7. Recommended next ordering (per §13.8 blockers):
   - **Start with §13.7.6 (LLM stub)** — 3–4 h, ready, no blockers. It is the gate for §13.7.7's golden-path + multi-user Playwright scenarios and §13.7.8's I6 integration suite, so landing it first unblocks the most follow-on work.
   - **In parallel** (independent): **§13.7.9 (mutation testing workflow)** — 2 h setup + ongoing triage, ready; gives high-signal regression coverage on the four critical modules (`wacp-transport` auth, `wacp-tools` execution, `console-core::session_launcher` + `session_monitor`).
   - **Then §13.7.7 (Playwright E2E tooling + first two scenarios)** — 4–6 h, tooling can start without the LLM stub; `golden-path.spec.ts` + `multi-user.spec.ts` need it.
   - **Then §13.7.8 (Rust integration I1–I5)** — 4–6 h, ready independently; I6 folds into §13.7.6.
   - **§13.7.10 (Codecov monthly ratchet)** — deferred until 2–3 `main` merges with §13.7.1–§13.7.5 so the new baseline settles.
4. Before doing any of (3), the dev→main batched merge may be worth doing first so CI has a clean snapshot of the landed work. 14 commits land cleanly if merged as a fast-forward or a single merge commit — no conflicts expected since all work has been on `dev`.
5. Each §13.7 package's "Acceptance criterion" is what closes it. Update the §13 status tables as items land; keep the appendix current the way §11 items got crossed off above.
6. Tag `wacp-runtime-v0.1.0` and `wacp-console-v0.1.0` independently once the Rust branch-coverage floor clears 85 % (already exceeded for `console-db` at 98.3 %; re-confirm workspace-wide via `cargo-llvm-cov` after `2fdf191` merges to `main`) and the Playwright golden-path + auth scenarios (§13.7.7 minimum) are green.

### Hollow Code Inventory — closed

All eight hollow components identified pre-wiring are now real. For the record:

| Component | Pre-W state | Post-W state (file:line proof) |
|---|---|---|
| gRPC pool | Built, never instantiated | Instantiated + connected in `wacp-console/crates/console/src/main.rs:150–162`; injected into `AppState` |
| Launch flow | SQLite state transitions only | `SubmitGoal → Decompose → Dispatch` sequence in `wacp-console/crates/console-core/src/session_launcher.rs:154–347`; rollback at `:375–396` |
| Session monitor | Didn't exist | Tokio task with 4 stream drivers in `wacp-console/crates/console-core/src/session_monitor.rs` (drivers at `:644/675/706/737`) |
| Gate resolution | Audit log only | `HighwayService::respond_to_gate` at `wacp-console/crates/console-api/src/routes/highway.rs:73–120` |
| Escalation response | Audit log only | `HighwayService::respond_to_escalation` at `…/highway.rs:260–335` |
| Directive injection | Audit log only | `HighwayService::send_envelope` at `…/highway.rs:409–500` |
| Cancel cleanup | Empty match arms | `CoordinatorService::AbortWorkspace` at `wacp-console/crates/console-api/src/routes/sessions.rs:687` |
| Startup recovery | Query exists, not wired | Scan + probe + respawn at `wacp-console/crates/console-core/src/recovery.rs:67–119` |

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
| 009 | OCI-only console distribution; cargo-dist deferred; `rust-embed` retained inside Dockerfile cargo build stage |

**Full ADR text:** `wacp-console/SPEC_BUILD.md` + `impl/adr-009-oci-only-console.md`.

## Workspace Layout

```
wacp-platform/
├── Cargo.toml                  # umbrella workspace (all Rust crates from both trees)
├── Cargo.lock                  # unified lockfile
├── rust-toolchain.toml         # pin Rust stable
├── docker-compose.yml          # dev stack: runtime + console + postgres
├── SEED.md                     # this file
├── AUDIT-2026-04-15.md         # post-wiring health audit + testing strategy
├── impl/                       # merge-plan, merge-execution-log, wiring-strategy, ADR-009
├── .github/workflows/          # ci-lint / ci-wacp / ci-console / release-runtime / release-console
│
├── wacp/                       # runtime subtree
│   ├── crates/                 # 15 Rust crates
│   │   ├── wacp-proto/         # shared tonic_build codegen (M3)
│   │   ├── wacp-runtime/       # binary — gRPC server, config, TLS
│   │   ├── wacp-coordinator/   # orchestrator: workspace tree, task graph
│   │   ├── wacp-transport/     # gRPC/REST/WS gateway, auth
│   │   ├── wacp-highway/       # signals, gates, escalations
│   │   ├── wacp-tools/         # tool execution, cancellation, retries
│   │   ├── wacp-llm/           # provider adapters (Anthropic, OpenAI)
│   │   ├── wacp-security/      # PII filter, policies
│   │   ├── wacp-taxonomy/      # VerticalManifest + derived roles
│   │   ├── wacp-types/         # primitives
│   │   └── …                   # fsm, clock, permissions, trail, workspace, sdk, coordinator-sdk
│   ├── packages/               # @wacp/cli, @wacp/local (TypeScript)
│   ├── ecosystem/              # 7 verticals: swe, devops, mlops, finance, healthcare, analytics, datasci
│   ├── sdk-python/             # Python bindings
│   ├── highway-ui/             # legacy highway webapp
│   ├── openapi.yaml            # runtime REST spec (16 endpoints)
│   ├── proto/                  # .proto definitions
│   ├── impl/                   # 17 impl specs + phase evals
│   └── IMPLEMENTATION.md       # runtime forward strategy
│
└── wacp-console/               # console subtree
    ├── crates/
    │   ├── console/            # binary — CLI, tracing, startup, taxonomy build, rust-embed assets
    │   ├── console-api/        # Axum routes, handlers, OpenAPI, pagination, WebSocket
    │   ├── console-core/       # auth, profiles, sessions, taxonomy, session_launcher, session_monitor, recovery
    │   ├── console-db/         # sqlx types, queries, migrations
    │   ├── console-runtime/    # GrpcPool, REST client, upstream re-exports
    │   └── console-test-support/ # mock runtime (real Tonic/Axum), fixtures
    ├── migrations/             # 9 SQL tables
    ├── frontend/               # Vite + React 19 + TypeScript SPA
    │   ├── src/api/            # types.ts (generated), client.ts, hooks/
    │   ├── src/store/          # auth.ts, ui.ts, session.ts (Zustand)
    │   ├── src/components/     # Layout, Sidebar, AdminGuard, Notifications
    │   ├── src/realtime/       # useSessionStream.ts (WebSocket hook)
    │   └── src/surfaces/       # auth, discovery, profiles, sessions, oversight, settings, admin
    ├── integration/            # W7 cross-binary integration + chaos tests
    ├── openapi.yaml            # console REST spec (66 operations, 12 tags)
    ├── specs/                  # 12 finalized design specs + 7 wiring coding specs
    ├── impl/                   # phase evals + wiring evals
    └── IMPLEMENTATION.md       # console forward strategy
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

*WACP Platform — authored by Akil Abderrahim and Claude Opus 4.6. Refreshed 2026-04-16 with post-audit progress and §13.7 task packages; refreshed again same-day by Claude Opus 4.7 (1M context) after §13.7.1–§13.7.5 landed on `dev`.*
