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

**Branch:** `dev`, 21 commits ahead of `main` @ `743c9bd`; **`ci/cleanup-2.7`** branched from `dev` tip `f6efc32` carries nine §2.7 commits — Phases A+B+C+D+E of the full-fix plan, all nine drift items + both debt items closed. Branch pushed to `aakil98/ci/cleanup-2.7` but **not yet ff'd to `dev`**. The dev→main batched merge ran 2026-04-17 morning (fast-forward, 21 commits); since then dev has accumulated §13.7.6b WA1/WA2/WA3 + strategy-doc + seed refresh + WA3.5/WA3.6 + WA5 + T7.3 (with WorkspaceState fix) + un-ignore sweep + closure docs + §13.7.9 mutation-testing pipeline + §13.7.7 D1 (Playwright tooling) + inter-deliverable drift-filing + §13.7.7 D2 (five E2E spec files + three backend-drift fixes) + **§2.1–§2.6 CI-pipeline cleanup** (setup-mold, tsconfig.build split, lint fixes, cargo fmt, deny.toml tune-up, clippy ws.rs fix) + **§2.7 full-fix plan** at `impl/ci-cleanup-2.7-plan.md`. **§13.7.6b + §13.7.9 fully closed; §13.7.7 D1+D2 landed, D3/D4/D5 remain; §2.1–§2.6 CI cleanup landed; §2.7 Phases A+B+C+D+E landed on `ci/cleanup-2.7` (all nine drift items closed: §2.7.1/2.7.2/2.7.3/2.7.4/2.7.5/2.7.6/2.7.7/2.7.8/2.7.9; both debt items closed: D.1 `wildcards=deny` restored via path+version pattern, D.2 `eslint-plugin-react-hooks` reinstated).** Working tree clean locally on `ci/cleanup-2.7`.

**CI state on `dev`** (not yet merged to main): §2.1–§2.6 landed 2026-04-17 evening (commits `efd23e9..0845acd`; doc refresh `e380f04`). Rust surface is **green** across all four triggered workflows — Build / Clippy / Test / Integration / Coverage-Rust + fmt + deny + Frontend-Lint + Protobuf. Nine pre-existing drift items (§2.7.1–§2.7.9) blocked Frontend Typecheck, TS ecosystem × 9, Python × 3, OpenAPI drift × 2, and Vitest-picks-up-e2e. **All nine are now fixed on `ci/cleanup-2.7`** (verified locally — `pnpm typecheck` exits 0 from 54 errors; `pnpm test:isolated` 22 files all green; `pnpm lint` 0 errors; full ecosystem typecheck, wacp-cli typecheck, wacp-local 86 tests, wacp-cli 132 tests, sdk-python 104 tests all green). Next step: **ff `ci/cleanup-2.7` → `dev`, push `dev`** — `push: branches: [main, dev]` trigger fires all four workflows (draft PR is not required; both triggers cover the same matrix). Target: 20 CI-triggered jobs, 20 green.

**§13.7.7 D3 (Playwright CI stage) can now branch off a green Rust+Integration base** without waiting for §2.7 (D3's job scope doesn't intersect the broken TS/Python legs); the original "cleanup prerequisite" blocker is lifted.

**Since the 2026-04-15 audit (from `a6773d6`)**, these have landed in order — all 25 through `743c9bd` now on `main`; the last 4 (WA1–WA3 + strategy update) still on `dev`:

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
| `abfbb99` | feat(wacp-llm): §13.7.6 — stub LlmAdapter + fixture + build_adapter factory | §13.7.6 |
| `afe98f6` | test(console-integration): §13.7.6 — I6 llm_stub_e2e + tighten T7.* ignore reasons | §13.7.6 / §12.5 I6 |
| `140fcc2` | docs(wacp-console): §13.7.6 — wcon-llm-stub spec + W7 deviation note + perf-opt §10 | §13.7.6 |
| `2f7b7ae` | docs(impl): §13.7.6b — wiring-strategy-b for runtime agent-service wiring | §13.7.6b |
| `743c9bd` | docs(audit-04-15): close §13.7.6 partial; carve out §13.7.6b; seed refresh | §13.7.6 |
| `b01757c` | feat(wacp-runtime): §13.7.6b WA1 — Bind projects WorkspaceConfig | §13.7.6b / WA1 |
| `69bcde0` | feat(wacp-runtime+wacp-sdk): §13.7.6b WA2 — EmitSignal drives workspace FSM | §13.7.6b / WA2 |
| `822674c` | feat(wacp-runtime): §13.7.6b WA3 — CreateCheckpoint forwards to workspace actor | §13.7.6b / WA3 |
| `7782d78` | docs(impl): wiring-strategy-b — carve WA3.5 + WA3.6; revise total effort | §13.7.6b / strategy |
| `d97ba7a` | feat(§13.7.6b): WA3.5 checkpoint-approval gates + WA3.6 auto-integration; defer WA5 + un-ignore sweep | §13.7.6b / WA3.5 / WA3.6 |
| `6d964b5` | docs(seed): fix dev-ahead-of-main count after WA3.5/WA3.6 commit | docs |
| `dec6385` | feat(console-integration): §13.7.6b WA5 — dispatch-failure proxy + T7.5 un-ignore | §13.7.6b / WA5 |
| `8ce249a` | feat(console-integration+wacp-runtime): §13.7.6b T7.3 un-ignore + fix WorkspaceState enum-offset | §13.7.6b / T7.3 |
| `efaa6d3` | test(console-integration): §13.7.6b un-ignore sweep — T7.2 + T7.7 + T7.8 + T7.10 | §13.7.6b / un-ignore |
| `ebd9eff` | docs: close §13.7.6b — refresh AUDIT/SEED/wiring-strategy-b/perf-opt | §13.7.6b / docs |
| `6f32ade` | ci(mutation): §13.7.9 — wire ci-mutation.yml + score/summary scripts + spec | §13.7.9 |
| `03d0411` | test(frontend): §13.7.7 D1 — Playwright tooling + `wacp-mock-runtime` bin | §13.7.7 / D1 |
| `1f1e25a` | docs: §13.7.7 D1 inter-deliverable — file findings to perf-opt §12 + new `impl/ci-health-2026-04-17.md` | §13.7.7 / drift-filing |
| `385ba71` | test(frontend): §13.7.7 D2 — five E2E spec files + prereq backend fixes | §13.7.7 / D2 |
| `efd23e9` | ci: restore green — setup-mold, tsconfig.build split, lint drifts, cargo fmt | §2.1–§2.4 |
| `eeda70e` | ci(deny): allow MIT-0 + workspace-path deps | §2.5 step 1 |
| `9056b8a` | fix(console-api): clippy collapsible_match in ws.rs — allow-attr | §2.6 |
| `0845acd` | ci(deny): allow CDLA-Permissive-2.0 + relax wildcards to warn | §2.5 step 2 |
| `e380f04` | docs(impl): §2.7 full-fix plan + ci-health doc refresh post-merge | §2.7 plan |
| `f6efc32` | docs(seed): refresh — §2.1–§2.6 CI cleanup landed, §2.7 plan drafted | seed refresh |

**§2.7 Phases A+B+C+D+E — landed on `ci/cleanup-2.7` (branched from `f6efc32`), pushed to `aakil98/ci/cleanup-2.7`, not yet ff'd to `dev`:**

| SHA | Subject | Drift item |
|-----|---------|------------|
| `d8e90fb` | fix(openapi): §2.7.2+§2.7.3 — console gen-openapi to stdout; regen wacp/openapi.yaml | §2.7.2 + §2.7.3 |
| `b44d097` | test(frontend): §2.7.9 — exclude playwright e2e specs from vitest | §2.7.9 |
| `852906b` | fix(highway-ui): §2.7.4 — move onlyBuiltDependencies into package.json | §2.7.4 |
| `6b50344` | build(wacp-local): §2.7.6 — compile to dist/; ecosystem + wacp-cli pick up emitted types | §2.7.6 |
| `e1b6f0c` | fix(wacp-cli): §2.7.5 — OperationType + Workflow narrowings via type-shape fixes | §2.7.5 |
| `4916e90` | feat(sdk-python): §2.7.7+§2.7.8 — generate wacp.v1 proto stubs via betterproto | §2.7.7 + §2.7.8 |
| `7e52bfb` | test(frontend): §2.7.1 — fix 54 strict-mode errors in tests at source | §2.7.1 |
| `f12d390` | ci(deny): D.1 — restore wildcards=deny via path+version pattern | D.1 |
| `3b23e85` | chore(frontend): D.2 — reinstate eslint-plugin-react-hooks | D.2 |

What this delivered, in English: supply-chain scanning (cargo-deny, SBOM, Trivy) is in CI; runtime auth is constant-time via SHA-256 digest rekey; the full coverage-tooling stack (cargo-llvm-cov, Vitest v8, coverage.py, Codecov with per-component flags) is wired; Rust branch-coverage tests landed for T1–T11 (~11,900 lines; T11 `console-db` brought that crate from 55.6 % → 98.3 % region coverage via a new `src/testing.rs` fault-injection harness and 83 tests); frontend RTL tests landed for F1–F10 save F9 which was already green (~5,200 + ~2,100 additional lines for F7/F8/F10); a per-file isolated vitest runner plus a 1536 MB V8 heap cap keeps the now-much-larger frontend suite from crashing WSL; `wacp-console/performance-optimization.md` aggregates the frontend-side `useEffect`-dep + spec-vs-impl drifts (§2.5) and the backend-side schema-vs-struct drifts (§9) that each session surfaces; and the §2.1–§2.6 CI cleanup restored the Rust + fmt + deny + Frontend-Lint + Integration surface by installing `rui314/setup-mold@v1` in every Rust-compiling job (10 jobs across 4 workflows), splitting `tsconfig.build.json` for `pnpm build`, fixing 9 lint drifts across 3 frontend files, allowing `MIT-0` + `CDLA-Permissive-2.0` in `deny.toml`, reshaping the wildcards check, and working around a new-in-rust-1.95.0 `clippy::collapsible_match` lint with an explicit `#[allow]` + comment (the match-guard fix fails because `axum::body::Bytes` isn't `Copy`).

**Runtime (`wacp/`).** 15 Rust crates, ~1,280 Rust tests + TS matrix (10 packages + 7 verticals, ~1,000 tests) + Python SDK (104 tests across 3.11–3.13). All 35 gRPC RPCs fully wired across `AgentService`, `HighwayService`, `CoordinatorService`. REST gateway exposes 16 `/v1/*` endpoints + `/v1/ws`. OpenAPI drift-checked in CI. Stream A (A1–A9) closed all 8 Console-facing integration gaps; the 17 runtime-side stub/placeholder gaps identified in the subsequent implementation audit are all resolved. Port map canonicalized to `9090/9091/9092/9093/9094/9095`.

**Console (`wacp-console/`).** 6 Rust crates, 66+ REST endpoints, 99+ backend unit tests. React 19 + Vite + TanStack Query + Zustand frontend (37 TS files, 9,367 lines). Now fully wired to the real runtime after W1–W7:

- **W1** gRPC pool in `AppState`; `/api/health` queries live pool + REST gateway (not mocks).
- **W2** Launch flow: real `CoordinatorService` sequence (`SubmitGoal` → `Decompose` → `Dispatch×N` → envelope send), rollback via `AbortWorkspace` on partial failure.
- **W3** `SessionMonitor`: one Tokio task per session, bounded `broadcast` fan-out (cap 256), four stream drivers (Trail, Gates, Escalations, WorkspaceChanges).
- **W4** Highway forwarding: gate resolve, escalation respond, directive inject all hit real `HighwayService` gRPC.
- **W5** Cancel calls `AbortWorkspace`; startup recovery scans `state='active'` and respawns monitors.
- **W6** Cross-session pending endpoints (`/api/gates/pending`, `/api/escalations/pending`, `/api/refusals/pending`) aggregate from live monitor state.
- **W7** Integration harness (`integration/tests/`) — `lifecycle`, `cross_session`, `chaos`, and `llm_stub_e2e` all passing. **Zero `#[ignore]`d tests** as of `efaa6d3`. The full §13.7.6b sweep landed all six T7.* tests (T7.2/T7.3/T7.5/T7.7/T7.8/T7.10) plus the surfaced WorkspaceState enum-offset fix.

**Working end-to-end against a live runtime:** discovery (roles, tools, verticals, types, search), profile CRUD with validation/versioning/export/import/clone, multi-user auth (Argon2id, CSRF double-submit, rate limiting, 256-bit bootstrap credential at 0o600), session launch + oversight (trail stream, gate queue, escalation inbox, refusal panel, workspace tree, injection bar across 7 WebSocket channels), startup recovery, cross-session pending aggregation.

**Partially landed:** Playwright E2E (§13.7.7) — D1 (tooling) and D2 (five spec files: 7 unskipped passing, 9 `test.skip` with substantive unskip notes) landed on `dev`. D3 (Playwright CI stage), D4 (e2e/README.md), and D5 (audit closure + SEED refresh) remain. **D3's former blocker (CI cleanup commit) is lifted** — §2.1–§2.6 landed. D3 can now add its Playwright stage against the green Rust/Integration base; the Frontend Typecheck and TS/Python legs staying red on §2.7 drift don't intersect D3's job scope.

CI pipeline cleanup (§2.1–§2.6 landed on `dev`; §2.7 Phases A+B+C+D+E all landed on `ci/cleanup-2.7`; ff-to-`dev` + CI matrix verification pending) — see `impl/ci-health-2026-04-17.md` + `impl/ci-cleanup-2.7-plan.md`. §2.7 plan executed across five clusters (A cheap wins ~35 min — **done**; B types ~90–135 min — **done in ~30 min**; C python codegen ~60–90 min — **done in ~25 min**; D mechanical strict-mode ~90–120 min — **done in ~60 min**; E debt ~50 min — **done in ~25 min**). Total wall: ~175 min vs plan's 5.5–7.5 h estimate. Full-fix principle held throughout: no `#[allow]` bandaids, no tsconfig exclusions as deferrals, no config relaxations.

**Not yet present (tracked, not regressions):** §13.7.7 D3/D4/D5, ff `ci/cleanup-2.7` → `dev` + CI matrix verification (target: 20/20 green), the five new Rust integration + chaos suites I1–I5 (§13.7.8; I6 landed via §13.7.6), and Codecov monthly ratchet (§13.7.10, deferred until the new baseline settles). All broken out with deliverables in `AUDIT-2026-04-15.md` §13.7. Supply-chain scanning, the F-series frontend sweep, the Rust branch-coverage sweep (T1–T11), the deterministic LLM stub provider + I6 integration test, §13.7.6b in its entirety, the §13.7.9 mutation-testing pipeline (awaiting first scheduled run), §13.7.7 D1+D2, §2.1–§2.6 CI cleanup, and §2.7 Phases A+B+C+D+E (nine commits on `ci/cleanup-2.7`) are all landed.

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
| 6 | Schedule the LLM stub that unblocks W7 T7.2/T7.3 | **stub landed + §13.7.6b WA1–WA3 landed** — `abfbb99` / `afe98f6` / `140fcc2` / `2f7b7ae` / `743c9bd` (§13.7.6) + `b01757c` / `69bcde0` / `822674c` (§13.7.6b WA1/WA2/WA3). Un-ignore of the six T7.* tests still blocked on WA3.5 (checkpoint gates) + WA3.6 (auto-integration) + WA5 (dispatch-failure harness) — ~9–11 h remaining |
| 7 | Plan the frontend test build-out (Phase 7.5–7.10 Playwright E2E) | F-series **complete** — `d63648a` + `e870018` + `92b3ddb` + `543c295`; Playwright D1+D2 **landed** (`03d0411` + `385ba71`) — 7 unskipped / 9 skipped / 0 fail; D3/D4/D5 remain |

### Testing coverage initiative (audit §12) — progress snapshot

| Weeks | Phase | Status |
|---|---|---|
| 1–2 | Tooling — `cargo-llvm-cov`, Vitest v8, Python `coverage --branch`, Codecov | **landed** — `7f0736b` |
| 3–4 | Rust branch gap — T1–T11 | **landed** — T1–T10 in `840450a` (+~9,800 lines); T11 `console-db` in `2fdf191` (+2,102 lines; region 55.6 → 98.3 %, line 63.4 → 99.1 %). |
| 5–6 | Frontend + E2E | F-series **complete** — `fe48c7b` + `e870018` + `92b3ddb` + `543c295`. Playwright **D1+D2 landed** — `03d0411` (tooling + `wacp-mock-runtime` bin) + `385ba71` (5 spec files: `auth-flows` 5/5 green, `golden-path` 2/3 green + 1 skip, `multi-user`/`cancel`/`profile-roundtrip` 3+2+1 skip). D3 (CI stage), D4 (README), D5 (audit closure) remain. |

Mutation testing (`cargo-mutants`, `stryker`) still pending — audit §13.7.9. Codecov monthly ratchet deferred until baseline stabilizes — audit §13.7.10. Integration + chaos additions I1–I5 ready (§13.7.8); I6 waits on the LLM stub.

**End-state targets unchanged:** Rust 95% branch, Console frontend 95% branch, TS verticals 95% branch, Python SDK 95% branch.

### New workstream added this session — frontend test-execution stability

Not in the original audit. Running the newly-written F-series RTL suite under `pool: "forks"` with `maxWorkers: 1` and `--max-old-space-size=6144` climbed one worker's RSS past 5 GB across files and killed WSL twice. Two fixes landed:

- `wacp-console/frontend/vitest.config.ts` — `execArgv` lowered from 6144 MB to 1536 MB. Forces V8 GC pressure early; single-file leaks OOM cleanly inside vitest instead of escaping into system memory.
- `wacp-console/frontend/scripts/run-tests-isolated.sh` + `npm run test:isolated` — per-file process isolation via a shell loop. Commits `82a4213`, `d71c4fe`, `fe48c7b` (suite itself).

One outstanding known issue from that work — `ProfilesPage.actions.test.tsx` OOMing inside its own per-file cap — was **resolved in `d63648a` (§13.7.1)**: root cause was an infinite render loop driven by a `mockImplementation` returning a fresh object per call, not a memory leak. The RTL `cleanup()` hook was also missing from `test-setup.ts` and was added globally in the same commit. Full post-mortem + extended pattern library live at `wacp-console/performance-optimization.md`. `npm run test:isolated` now exits 0 across all 17 files with session peak ~291 MB RSS and walltime ~62 s.

### Resumption Point

**M0–M7 merger, W1–W7 wiring, runtime implementation audit, §11 pre-release punch list (1–5), §12.1 tooling, §12.2 T1–T11, §12.3 F1–F10, audit §13.7.1–§13.7.5, §13.7.6, §13.7.6b in full, §13.7.9 (mutation-testing pipeline), §13.7.7 D1+D2 (Playwright tooling + 5 E2E spec files), §2.1–§2.6 CI-pipeline cleanup, and §2.7 Phases A+B+C+D+E (nine commits on `ci/cleanup-2.7`, all nine drift items + both debt items closed)** all complete. Test totals: `wacp-coordinator` 387, `wacp-workspace` 65, `wacp-runtime` 109, `wacp-types` 45, `console-integration` 12 passing + 0 ignored, `pnpm test:e2e` 7 pass / 9 skip / 0 fail; on `ci/cleanup-2.7` add: frontend `pnpm typecheck` 0 errors (was 54), `pnpm test:isolated` 22 files all green, `pnpm lint` 0 errors, `wacp-local` 86 tests, `wacp-cli` 132 tests, all 7 ecosystem packages typecheck, `sdk-python` 104 tests. Workspace clippy + fmt clean.

**§13.7.6b + §13.7.9 + §2.1–§2.6 CI cleanup — fully closed/wired. §13.7.7 — D1+D2 landed; D3/D4/D5 remain. §2.7 — Phases A+B+C+D+E all landed locally on `ci/cleanup-2.7` (all 9 drift items + both debt items closed: §2.7.1/.2/.3/.4/.5/.6/.7/.8/.9 + D.1 + D.2). Branch pushed to `aakil98/ci/cleanup-2.7`. Next step: ff `ci/cleanup-2.7` → `dev`, push `dev`, verify 20/20 CI jobs green.**

**Key new docs since last resumption:**
- `impl/ci-health-2026-04-17.md` — now at `status: partial`. Header note records the §2.1–§2.6 merge to dev (`efd23e9..0845acd`) and points at the sequel plan. §2.7 subsection enumerates 9 pre-existing drift items surfaced during push verification; §2.1–§2.6 subsections all marked "Applied" with per-item SHAs.
- **`impl/ci-cleanup-2.7-plan.md` (new)** — sequel plan covering §2.7 + two debt items (D.1 restore `wildcards = "deny"` via path+version pattern; D.2 reinstall `eslint-plugin-react-hooks`). Phased A/B/C/D/E, 5.5–7.5 h total. Every item has a local verification command and explicit root-fix (no bandaids).
- `wacp-console/performance-optimization.md` §12 — four drifts surfaced during §13.7.7 D1+D2; the three CI-tooling items (§12.3) now fully addressed by §2.1–§2.6 + §2.7 plan. §12.5 `ProfilesPage` Create-New click unmounts React remains open (30–60 min bisect recommended).
- §11.4 P0 audit pass on remaining Rust-enum-as-i32 sites is still open and high-ROI (~30–60 min).

**Ephemeral tracker:** `/tmp/13-7-7-progress.md` holds the live D1–D5 deliverables checklist with SHAs. Rebuilt each session (not committed).

**When resuming:**

**Branch state to start the session:** `git checkout ci/cleanup-2.7` (already exists locally; tip is `3b23e85`, 9 commits ahead of `dev`'s `f6efc32`, already pushed to `aakil98/ci/cleanup-2.7`). Working tree clean. All five phases done — all nine §2.7 drift items + both debt items closed locally.

**Primary track — finish §2.7 (per `impl/ci-cleanup-2.7-plan.md`):**
1. ~~Phase A (cheap wins)~~ — **done**. SHAs: `d8e90fb` (openapi), `b44d097` (vitest), `852906b` (highway-ui).
2. ~~Phase B (types)~~ — **done in ~30 min vs 90–135 min estimate**. SHAs: `6b50344` (wacp-local dist + ci-wacp.yml prebuild step), `e1b6f0c` (wacp-cli `OperationType` widened to `… | (string & {})`; `Workflow.stages`/`WorkflowStage.dependsOn`/`AgentProfile.tools`/protocol-client `decompose`+`dispatch` array params widened to `readonly`).
3. ~~Phase C (Python codegen)~~ — **done in ~25 min vs 60–90 min estimate**. SHA `4916e90`. Used `protoc + protoc-gen-python_betterproto` directly (buf isn't installed locally; same output as the buf remote plugin path the plan prescribed). Renamed namespace `wacp.proto.v1` → `wacp.v1` to align with the canonical proto package directive — 7 import sites updated.
4. ~~Phase D (strict-mode sweep)~~ — **done in ~60 min vs 90–120 min estimate**. SHA `7e52bfb`. Six clusters: `.mock.calls[0]!` non-null assertions (6×), destructure `!` (1×), `describeMutationErrors` generic widened to minimal structural shape `{ result: { current: { isError, error } } }` so hooks with diff TVariables assign (5×), `makeGate` fixture `subject: { detail } as Record<string, unknown>` (14×), `trail[0]!.id` post-`toHaveLength(1)` (11×), `vi.stubGlobal("location", …)` replacing `delete window.location` dance (2×), `_data` rename (1×), `fireEvent.click(buttons[N]!)` (14×).
5. ~~Phase E (debt)~~ — **done in ~25 min vs 50 min estimate**. SHAs: `f12d390` (D.1 `wildcards=deny` via path+version pattern on 20 internal deps; cargo-deny local install skipped per plan note that "CI verification on push is the backup") + `3b23e85` (D.2 `eslint-plugin-react-hooks@7.1.1` installed; `rules-of-hooks: error` + `exhaustive-deps: warn`; `SettingsPage.tsx:137` disable comment re-added with justification; one pre-existing warn at `Wizard.tsx:739` logged for future ratchet).
6. **ff + push + verify** — ff `ci/cleanup-2.7` → `dev`, push `dev` (per `push: branches: [main, dev]` trigger, this fires all four workflows; no draft-PR needed). Verify per `impl/ci-cleanup-2.7-plan.md` §5 matrix. Target: 20 CI-triggered jobs, 20 green. If red: revert the offending commit, fix forward on `ci/cleanup-2.7`, re-ff. When green, later ff `dev` → `main`.

**Plan deviations to note for future-you (so you don't re-discover them):**
- **§2.7.2 prescribed recipe was unsafe.** The console `gen-openapi` binary self-wrote to a hardcoded path AND printed status to stdout. The plan's `cargo run … > wacp-console/openapi.yaml` would have nuked the openapi (race between binary's own `File::write` and shell's stdout-redirect of "wrote NNNN bytes"). Fix shipped: change binary to write to stdout (matches `wacp-transport`'s `gen_openapi`); same recipe now works for both crates.
- **§2.7.5 closed-union OperationType was infeasible.** Verticals already use ops outside the canonical 9-literal set (finance: `data_read`/`trade_exec`, mlops: `compute_exec`, etc.). Could not type `BUILTIN_TOOL_OPERATIONS` as `Record<string, OperationType>` without dropping ops. Opened the union via the documented `(string & {})` pattern — preserves literal autocomplete, admits vertical extensions, no runtime change.
- **§2.7.5 had a hidden cascade.** Widening `Workflow.stages`/`WorkflowStage.dependsOn` to `readonly` to fix the `VerticalWorkflow → Workflow` mismatch surfaced two more readonly-vs-mutable mismatches: `AgentProfile.tools` (consumer of profile.tools never mutates) and `protocol-client.decompose`/`dispatch` task-array params. All widened to `readonly` for consistency.
- **§2.7.6 worked exactly as plan B prescribed.** Per-matrix-entry `Pre-build @wacp/local` step in `ci-wacp.yml` with `if: matrix.package != 'wacp/packages/wacp-local'`. pnpm symlinks `file:` deps so the rebuilt dist propagates to consumers when they `pnpm install` after the prebuild step.
- **§2.7.7+§2.7.8 path mismatch.** Plan said commit to `src/wacp/proto/v1/`. betterproto's protoc plugin emits at the proto package path (`wacp.v1`) as a flat module, not a directory. Renamed the SDK's import path from `wacp.proto.v1` → `wacp.v1` (7 sites) — aligns with what Rust + TS use, removes an unnecessary indirection.

**Parallel track — §13.7.7 D3/D4/D5:**
8. **§13.7.7 D3** — 1–2 h: add `e2e` job to `ci-console.yml` (build runtime + mock bin + console; `pnpm build`; `pnpm exec playwright test`; upload HTML report + Codecov with `playwright` flag). Former CI-cleanup prerequisite is lifted; D3 can start on current dev as-is (Rust base is green).
9. **§13.7.7 D4** — 30–60 min: `frontend/e2e/README.md` with local-run recipe, debug (`--ui`/`--debug`), snapshot updates, port map.
10. **§13.7.7 D5** — 30 min: update `AUDIT-2026-04-15.md` §13.2/§13.5/§13.8 + SEED (mark §13.7.7 landed); fold the ephemeral tracker's remaining notes.

**Independent tracks:**
11. **§13.7.8 (Rust integration I1–I5)** — 4–6 h. Fully independent of §13.7.7; can run in parallel.
12. **§13.7.10 (Codecov monthly ratchet)** — still deferred until 2–3 `main` merges land §13.7.6b + §13.7.7 so baseline settles.
13. **§13.7.9 first-run triage** — next Monday cron (surviving mutants → killer tests; equivalent → `// mutants:skip`).

**Merge strategy:** current dev lead = 21 commits; `ci/cleanup-2.7` adds 9 more on top (all 9 §2.7 drift items + both debt items closed locally; not yet on dev). Path forward: ff `ci/cleanup-2.7` → `dev`, push `dev` (`push: branches: [main, dev]` trigger fires the full CI matrix — draft PR is not required; both triggers cover the same workflows — see memory `reference_ci_trigger_scopes.md`). Verify 20/20 jobs green, then ff `dev` → `main` once §13.7.7 at least D3 also lands. Tag `wacp-runtime-v0.1.0` / `wacp-console-v0.1.0` once Rust branch-coverage floor clears 85%, first mutation run hits ≥85% per module, and §13.7.7 at least D3 is landed + green.

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

*WACP Platform — authored by Akil Abderrahim and Claude Opus 4.6. Refreshed 2026-04-16 with post-audit progress and §13.7 task packages; refreshed again same-day by Claude Opus 4.7 (1M context) after §13.7.1–§13.7.5 landed on `dev`; refreshed 2026-04-17 after §13.7.6 (stub provider + I6) landed and §13.7.6b (runtime wiring follow-up) was carved out; refreshed again same-day after dev→main fast-forward and §13.7.6b WA1/WA2/WA3 landed, with WA3.5 + WA3.6 carved out as the remaining gate-fan and auto-integration pieces; refreshed 2026-04-17 (third pass) after §13.7.6b WA3.5 + WA3.6 landed in the working tree; refreshed 2026-04-17 (fourth pass) after §13.7.6b was fully closed via WA5 + T7.3 + WorkspaceState fix + un-ignore sweep; refreshed 2026-04-17 (fifth pass) after §13.7.9 mutation-testing pipeline wired (weekly Monday cron, 4 targets, ≥85% threshold); refreshed 2026-04-17 (sixth pass) after §13.7.7 D1 + D2 landed; refreshed 2026-04-17 (seventh pass) after §2.1–§2.6 CI-pipeline cleanup landed on `dev` (`efd23e9..0845acd` plus doc refresh `e380f04` — five commits restoring Rust/fmt/deny/Frontend-Lint/Integration/Coverage-Rust to green) and the §2.7 full-fix plan was drafted at `impl/ci-cleanup-2.7-plan.md` (nine drift items + two carried-forward debt items, 5.5–7.5 h estimate, no-technical-debt principle); refreshed 2026-04-18 (eighth pass) after §2.7 Phases A+B+C executed on a new `ci/cleanup-2.7` branch (six commits `d8e90fb..4916e90` closing eight of nine drift items in ~80 min vs the planned ~3.0–3.7 h for those phases); refreshed 2026-04-18 (ninth pass, this session) after Phases D + E landed on `ci/cleanup-2.7` (`7e52bfb` §2.7.1 strict-mode, `f12d390` D.1 `wildcards=deny`, `3b23e85` D.2 `eslint-plugin-react-hooks`) in ~85 min vs the planned ~140–170 min — closing all 9 drift items + both debt items. Branch state: dev still 21 commits ahead of main; `ci/cleanup-2.7` now 9 commits ahead of dev tip `f6efc32`, pushed to `aakil98/ci/cleanup-2.7`. Next action: ff `ci/cleanup-2.7` → `dev`, push `dev` (`push: branches: [main, dev]` trigger covers all four workflows — draft PR is not required). See "When resuming" → "Plan deviations to note" for the four adaptation spots.*
