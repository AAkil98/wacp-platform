# WACP Platform — Seed Context

> Compressed summary of the full design, current implementation state, and next steps for the unified `wacp-platform` monorepo.
> For detail on any topic, follow the spec references. For task-level implementation detail, see `wacp/IMPLEMENTATION.md` (runtime) and `wacp-console/IMPLEMENTATION.md` (console).
> For the wiring strategy (runtime ↔ console integration plan), see `impl/archive/wiring-strategy.md`.
> For the monorepo merge procedure (M0–M7), see `impl/merge-plan.md` and `impl/merge-execution-log.md`.
> For the latest codebase health + testing strategy, see `AUDIT-2026-04-15.md`.

## What This Is

The `wacp-platform` monorepo houses two binaries that ship together:

- **WACP runtime (`wacp/`)** — the protocol reference implementation. Rust workspace (15 crates) + TypeScript CLI/SDK/verticals + Python SDK. Serves gRPC (Agent, Highway, Coordinator) + REST + WebSocket for the 7 verticals.
- **WACP Console (`wacp-console/`)** — the operator workbench. Rust/Axum backend (6 crates) + React 19 SPA. Discovers agent roles, manages agent profiles, launches coordination sessions, and oversees agent work in real time. The Console is a **client** of the runtime — it connects via gRPC and REST, never modifies protocol behavior, never executes LLM calls.

They are shipped as two binaries with gRPC between them; the monorepo exists for development ergonomics (shared proto codegen, version-locked types, unified CI/fmt/clippy).

**Specs:** runtime protocol lives in the sibling `wacp-protocol` repo (CC BY-SA 4.0). Console design specs under `wacp-console/specs/` (12 finalized). Anchor spec for the Console: `wcon-vision`.

## Current State (Post M0–M7 merge + W1–W7 wiring + post-audit workstream)

**Branches.** `main` and `dev` both at `633493b` — aligned by the 2026-04-20 16:38 UTC ff. Use `git log --oneline main..dev | wc -l` for the live divergence count (0 at ff close; grows with each topic branch that ff's to dev).

Since the prior SEED pass (`ce4a3a2`, 2026-04-20 morning), four batches have landed and propagated to main:

- **v0.1.0-readiness Tracks A + B + C** (range `f0527d7..c20df35`, 9 commits) — Track A keyboard-nav a11y sweep across three surfaces; Track B spec-harmonization (deleted Notifications stub, reshaped RefusalPanel read-only semantics, filtered InjectionBar to ACTIVE sessions); Track C Criterion benchmark harness + wacp-llm single-call / lazy-streaming refactors + console-db migration baseline (5.78 ms). Plans `impl/archive/v0.1.0-readiness-plan.md` + `impl/archive/backend-perf-baseline-plan.md`. Closed 2026-04-20 AM.
- **Benchmark rustfmt fix** (`4027e80`, single commit) — stub_bench.rs import ordering + multi-line ternary formatting. Own its batch; no plan doc.
- **CI-mutation pipeline fix** (range `22cb0f9..89e0e1d`, 3 commits) — §15.1 workflow `--file` paths rewritten for post-M0–M7 nested crate layout; §15.2 `mutation-score.py` + `mutation-summary.py` updated to parse cargo-mutants v27's `CaughtMutant` / `MissedMutant` / wrapped-scenario output shape. First scheduled cron (2026-04-20 04:00 UTC) had surfaced both bugs — pipeline now operational. Triggered a new tier-3 plan doc for session_monitor triage. Details in HEALTH-LOG §15.1 + §15.2.
- **Session-monitor mutation triage** (range `c960741..633493b`, 8 commits) — 5-phase batch on `testing/session-monitor-mutants`: classify 15 survivors → 4 stream-driver tests → handle ops + spawn_drivers + `process_workspace_view` extract → run_stream_driver Fatal + fractional-second timestamp tests → fixup (kill L662 attempts counter + delete equivalent trail arm) → closeout + archive. `console-core/session_monitor` mutation score 72.7 % → **98.2 %** (54/55). Plan `impl/archive/session-monitor-mutation-triage-plan.md`. Also captured the side lesson that cargo-mutants' `// mutants:skip` comment form isn't recognised — only `#[mutants::skip]` attributes work.

**CI state.** Four workflows (`ci-lint`, `ci-wacp`, `ci-console`, `coverage`) run on push to `main` + `dev`. Path filters narrow docs-only commits to `ci-lint` + `coverage`, and wacp-only commits to `ci-wacp` + `ci-lint`. Both `dev` and `main` at `633493b` green on all three applicable workflows (ci-wacp path-filtered out — no `wacp/**` changes in the session-monitor batch). `ci-console` runs Playwright `e2e` + 50-test integration suite; `ci-lint` `file-size` job passes (6 warnings / 0 fails). Branch protection active on `main` + `dev` (linear-history + no force-push + no deletion; `enforce_admins=false`).

**Mutation pipeline (`.github/workflows/ci-mutation.yml`).** Monday 04:00 UTC cron + `workflow_dispatch`. Post-triage run (2026-04-20 15:20 UTC on `testing/session-monitor-mutants`) scored all four targets above threshold: wacp-transport/auth **92.3 %** ✅ (48/52), wacp-tools/execution **91.7 %** ✅ (11/12), console-core/session_launcher **92.9 %** ✅ (13/14), console-core/session_monitor **98.2 %** ✅ (54/55, up from 72.7 % baseline). session_monitor triage closed via `impl/archive/session-monitor-mutation-triage-plan.md` (5 phases `c960741..deefc6c` + §E closeout). Three passing targets retain 1–4 non-blocking survivors (boundary-operator mutants in auth rate-limiting, timeout arithmetic, build_directive equality check) — worth triaging in a follow-up batch but not blocking.

**Known CI flake (open).** `account_lockout_after_five_failed_logins` hit a `RuntimeHarness::pick_port` TOCTOU on one run 2026-04-20 (two of five ephemeral ports collided); `gh run rerun --failed` passed cleanly. HEALTH-LOG §14.1 has the holder-listener fix sketch. Other outstanding CI tasks: Playwright `--coverage` → frontend lcov merge, Codecov monthly ratchet.

**Since the 2026-04-15 audit (from `a6773d6`)**, the following have landed in order — all on `main` at `89e0e1d`. For live `git log` slice: `git log --oneline a6773d6..main`.

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

**§2.7 Phases A+B+C+D+E — on `main` at `89e0e1d`:**

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

**closeout-plan P1 — landed on `dev` via ff of `ci/pre-launch-closeout` on 2026-04-19** (range `6197263..c07af7f`, 7 commits). Per-phase SHAs + scope in `impl/archive/closeout-plan.md` §7 execution log — not duplicated here per the new `seed-refresh` batch-boundary convention. Delivers: Vite sourcemap off, `wacp/highway-ui/` deleted + impl spec archived, `.file-size-allowlist` + `scripts/check-file-sizes.sh` + `ci-lint.yml` `file-size` job (Rust 1000/1500, TS 500/1000), `.gitmessage` template, opt-in `.githooks/pre-push` + `scripts/install-hooks.sh`, `tech-debt §7` + `git-strategy §13` closeouts, SEED 15th-pass refresh.

**closeout-plan P2 — branch protection, landed on `dev` 2026-04-20** (`338e4f2`, single docs commit after the `gh api` configuration). Both `main` and `dev` now have `required_linear_history=true`, `allow_force_pushes=false`, `allow_deletions=false`. `enforce_admins=false` — sole-maintainer bypass retained; tighten when a second maintainer lands. Closes `impl/git-strategy.md` §11.5 risk class.

**closeout-plan P3 — dev → main ff on 2026-04-20** via `ff-main` skill. Range `d0be941..338e4f2`, 11 commits (P1 batch + 2 post-batch SEED refreshes + P2). Tested the new P2 linear-history rule: accepted ff-only push to both branches. Superseded by subsequent ffs — main advanced to `4027e80` during the Tracks A+B+C / benchmark work, then to `89e0e1d` at the 2026-04-20 14:18 UTC ff carrying the CI-mutation fix batch.

**v0.1.0-readiness Tracks A+B+C + benchmark rustfmt — on `main`, range `f0527d7..4027e80`** (10 commits). Track A: a11y keyboard-nav sweep. Track B: spec-harmonization (InjectionBar, RefusalPanel, Notifications stub delete). Track C: Criterion bench harness (`c5149af`) + wacp-llm single-call serialize + lazy streaming refactor (`4b735b0`) + console-db migration baseline (`8e117e4`). Plans landed and archived at `impl/archive/v0.1.0-readiness-plan.md` + `impl/archive/backend-perf-baseline-plan.md`. `4027e80` was a same-session rustfmt fix on the bench harness. Full per-phase detail lives in the archived plans — not duplicated here.

**CI-mutation pipeline first-run fix — on `main`, range `22cb0f9..89e0e1d`** (3 commits). 2026-04-20 04:00 UTC Monday cron fired all four mutation jobs red with 0 mutants tested, exposing two companion bugs: §15.1 workflow `--file src/foo.rs` patterns assumed pre-M0–M7 crate layout (cargo-mutants matches path-separator globs from workspace root), §15.2 `mutation-score.py` + `mutation-summary.py` looked for legacy `Caught`/`Missed` in `outcomes.json::summary` when v27 emits `CaughtMutant`/`MissedMutant` with scenarios wrapped as `{"Mutant": {...}}`. Both fixed; post-fix `workflow_dispatch` run scored 3 of 4 targets ≥85 %, with console-core/session_monitor at 72.7 % (15 survivors). Full finding at HEALTH-LOG §15.1 + §15.2.

**Session-monitor mutation triage — on `main`, range `c960741..633493b`** (8 commits). Five-phase plan on topic branch `testing/session-monitor-mutants`: classify 15 survivors (13 real gaps + 2 equivalent) → 4 driver tests (`Err("highway unavailable")` against disconnected pool) → handle ops + spawn_drivers + `process_workspace_view` extract → run_stream_driver Fatal + fractional-seconds rfc3339 tests → fixup (kill attempts-counter via `lag_attempts == vec![0,1,2]` assertion + delete redundant `"trail" => vec![]` match arm) → §E closeout (AUDIT + HEALTH-LOG strikes) → archive. `console-core/session_monitor` mutation score 72.7 % → 98.2 % (54/55). One documented-equivalent mutant remaining (L356 `seed_workspace_labels → ()` — early-returns under disconnected unit-test pool; real path covered by integration suite). Three other targets retain 1–4 non-blocking survivors each (boundary-operator mutants). Full detail in `impl/archive/session-monitor-mutation-triage-plan.md` §7 execution log. Side lesson: cargo-mutants' `// mutants:skip` comment form is NOT honoured — only `#[mutants::skip]` attributes work (see `cargo-mutants/book/src/attrs.md`). With this batch, **every mutation target on `main` now clears the 85 % threshold**, closing AUDIT §13.7.9's first-run triage.

What this delivered, in English: supply-chain scanning (cargo-deny, SBOM, Trivy) is in CI; runtime auth is constant-time via SHA-256 digest rekey; the full coverage-tooling stack (cargo-llvm-cov, Vitest v8, coverage.py, Codecov with per-component flags) is wired; Rust branch-coverage tests landed for T1–T11 (~11,900 lines; T11 `console-db` brought that crate from 55.6 % → 98.3 % region coverage via a new `src/testing.rs` fault-injection harness and 83 tests); frontend RTL tests landed for F1–F10 save F9 which was already green (~5,200 + ~2,100 additional lines for F7/F8/F10); a per-file isolated vitest runner plus a 1536 MB V8 heap cap keeps the now-much-larger frontend suite from crashing WSL; `HEALTH-LOG.md` aggregates the frontend-side `useEffect`-dep + spec-vs-impl drifts (§2.5) and the backend-side schema-vs-struct drifts (§9) that each session surfaces; and the §2.1–§2.6 CI cleanup restored the Rust + fmt + deny + Frontend-Lint + Integration surface by installing `rui314/setup-mold@v1` in every Rust-compiling job (10 jobs across 4 workflows), splitting `tsconfig.build.json` for `pnpm build`, fixing 9 lint drifts across 3 frontend files, allowing `MIT-0` + `CDLA-Permissive-2.0` in `deny.toml`, reshaping the wildcards check, and working around a new-in-rust-1.95.0 `clippy::collapsible_match` lint with an explicit `#[allow]` + comment (the match-guard fix fails because `axum::body::Bytes` isn't `Copy`).

**Runtime (`wacp/`).** 15 Rust crates, ~1,280 Rust tests + TS matrix (10 packages + 7 verticals, ~1,000 tests) + Python SDK (104 tests across 3.11–3.13). All 35 gRPC RPCs fully wired across `AgentService`, `HighwayService`, `CoordinatorService`. REST gateway exposes 16 `/v1/*` endpoints + `/v1/ws`. OpenAPI drift-checked in CI. Stream A (A1–A9) closed all 8 Console-facing integration gaps; the 17 runtime-side stub/placeholder gaps identified in the subsequent implementation audit are all resolved. Port map canonicalized to `9090/9091/9092/9093/9094/9095`.

**Console (`wacp-console/`).** 6 Rust crates, 66+ REST endpoints, 99+ backend unit tests. React 19 + Vite + TanStack Query + Zustand frontend (37 TS files, 9,367 lines). Now fully wired to the real runtime after W1–W7:

- **W1** gRPC pool in `AppState`; `/api/health` queries live pool + REST gateway (not mocks).
- **W2** Launch flow: real `CoordinatorService` sequence (`SubmitGoal` → `Decompose` → `Dispatch×N` → envelope send), rollback via `AbortWorkspace` on partial failure.
- **W3** `SessionMonitor`: one Tokio task per session, bounded `broadcast` fan-out (cap 256), four stream drivers (Trail, Gates, Escalations, WorkspaceChanges).
- **W4** Highway forwarding: gate resolve, escalation respond, directive inject all hit real `HighwayService` gRPC.
- **W5** Cancel calls `AbortWorkspace`; startup recovery scans `state='active'` and respawns monitors.
- **W6** Cross-session pending endpoints (`/api/gates/pending`, `/api/escalations/pending`, `/api/refusals/pending`) aggregate from live monitor state.
- **W7** Integration harness (`integration/tests/`) — `lifecycle`, `cross_session`, `chaos`, `llm_stub_e2e`, plus **§13.7.8's five new suites** (`launch_failure_matrix`, `recovery_matrix`, `auth_matrix`, `ws_chaos`, `taxonomy_reload`) + `mock_coordinator_smoke`. **50/50 green**, zero `#[ignore]`d tests. Shared `InjectableCoordinator` mock at `integration/src/mock_coordinator.rs` drives failure injection across the launcher + recovery suites.

**Working end-to-end against a live runtime:** discovery (roles, tools, verticals, types, search), profile CRUD with validation/versioning/export/import/clone, multi-user auth (Argon2id, CSRF double-submit, rate limiting, 256-bit bootstrap credential at 0o600), session launch + oversight (trail stream, gate queue, escalation inbox, refusal panel, workspace tree, injection bar across 7 WebSocket channels), startup recovery, cross-session pending aggregation.

**Playwright E2E (§13.7.7) — fully landed:** D1 `03d0411`, D2 `385ba71`, D3 `3508941`, D4 + D5 docs in `9fee339`. Remaining: Playwright `--coverage` → frontend lcov merge (deferred, not blocked).

**Rust integration + chaos (§13.7.8) — fully landed:** P0 `78a7fab` (shared `InjectableCoordinator` mock generalizing WA5's FailureProxy — per-RPC `VecDeque<Option<Status>>` queues + `pass_*()` helpers for "forward first K, fail K+1" patterns), I1 `bd86754` (10 launch-failure scenarios), I2 `baef3c2` (7 recovery-matrix scenarios), I3 `60e49cc` (12 auth wiring tests — scope re-sized from 45-cell matrix to integration-level wiring proof after confirming `authorizer.rs::tests` covers the role-matrix exhaustively), I4 `62295a0` (3 WS-chaos tests), I5 `f5b4879` (4 taxonomy-reload tests + `mock_rest::RestState` ArcSwap hot-swap upgrade + `ConsoleHarness::spawn_with_db_and_rest` variant). **50/50 integration green.** Four sub-scenarios deferred with in-file notes → `HEALTH-LOG.md` §13: workspace-`Failed`-state probe (mock highway), DB-degraded-read path (FaultyDb::drop_reads), gap-fill REST replay (endpoint doesn't exist — recommended strike), context_schema evolution.

CI pipeline cleanup (§2.1–§2.6 + §2.7 full-fix + §2.7.1b + protoc rate-limit hardening all landed on `dev`) — see `impl/ci-health-2026-04-17.md` + `impl/archive/ci-cleanup-2.7-plan.md`. §2.7 plan executed across five clusters in ~175 min vs the 5.5–7.5 h estimate. Full-fix principle held throughout: no `#[allow]` bandaids, no tsconfig exclusions as deferrals, no config relaxations.

**Not yet present (tracked, not regressions):** Codecov monthly ratchet (§13.7.10, deferred until the new baseline settles), Playwright `--coverage` merge, and four §13.7.8 deferred sub-scenarios (each tied to an absent infrastructure piece — mock highway, FaultyDb mode, REST replay endpoint, schema-evolution fixture). Supply-chain scanning, the F-series frontend sweep, the Rust branch-coverage sweep (T1–T11), the deterministic LLM stub provider + I6 integration test, §13.7.6b in its entirety, the §13.7.9 mutation-testing pipeline (awaiting first scheduled run), §2.1–§2.6 + §2.7 CI cleanup, §13.7.7 Playwright E2E in full, and §13.7.8 Rust integration + chaos in full are all landed.

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

## Doc Tiers & Audit Process

Three-tier system for how findings, plans, and snapshots flow through the repo:

| Tier | Location | Lifecycle | Who writes |
|---|---|---|---|
| **1. Living health log** | `HEALTH-LOG.md` (platform root) | Append-only, every session that surfaces drift | Any session that discovers a spec-vs-impl drift, schema-vs-struct drift, useEffect leak, coincidental-green test, or other health signal. One new `## N.M` subsection per package. |
| **2. Dated snapshots** | `AUDIT-YYYY-MM-DD.md`, `tech-debt-YYYY-MM-DD.md` at platform root | Frozen when written; superseded by later-dated snapshots | Written periodically when findings accumulate enough to warrant triage + work packages. Consolidates HEALTH-LOG entries + fresh measurements into §13.7-style tables with status / effort / acceptance criteria. |
| **3. In-flight plans** | `impl/{scope}-plan.md` | Lives while the plan is executing; moves to `impl/archive/` once all its commits land | Written when a work package needs more than ~3 steps to execute. Deleted (not archived) if abandoned. |

**Also in `impl/`:** active strategy references (`git-strategy.md`) and historical execution logs (`merge-execution-log.md`, `ci-health-2026-04-17.md`) — neither planning nor finding, these stay where they are. ADRs live in `adr/` at platform root.

**Flow.** A finding lands in HEALTH-LOG first. When a cluster accumulates (or a fresh baseline is wanted), a dated snapshot consolidates them into numbered work packages. Packages execute on `{scope}/{slug}` topic branches, with a plan in `impl/` if complex enough. When the package closes, its plan graduates to `impl/archive/` and the HEALTH-LOG entry is struck through or marked resolved.

**What goes where — worked examples:**
- Discovering a new `useEffect` dep leak → new `## N.M` in HEALTH-LOG.
- Finding that 11 files exceed 1500 lines → new `tech-debt-YYYY-MM-DD.md` at root (dated snapshot).
- Planning a 5-phase CI cleanup → `impl/ci-cleanup-N.N-plan.md`; archive on close.
- Deciding OCI-only distribution → `adr/adr-NNN-*.md`.
- Recording that M0–M7 merge happened → `impl/merge-execution-log.md` (historical log, permanent).

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

One outstanding known issue from that work — `ProfilesPage.actions.test.tsx` OOMing inside its own per-file cap — was **resolved in `d63648a` (§13.7.1)**: root cause was an infinite render loop driven by a `mockImplementation` returning a fresh object per call, not a memory leak. The RTL `cleanup()` hook was also missing from `test-setup.ts` and was added globally in the same commit. Full post-mortem + extended pattern library live at `HEALTH-LOG.md`. `npm run test:isolated` now exits 0 across all 17 files with session peak ~291 MB RSS and walltime ~62 s.

### Resumption Point

**Every AUDIT §13.7 work package except 13.7.10 is closed — mutation pipeline now runs cleanly + session_monitor triage is the one open follow-up.** M0–M7 merger, W1–W7 wiring, runtime implementation audit, §11 pre-release punch list (1–5), §12.1 tooling, §12.2 T1–T11, §12.3 F1–F10, audit §13.7.1–§13.7.5, §13.7.6, §13.7.6b, §13.7.7 Playwright E2E D1–D5, §13.7.8 Rust integration + chaos I1–I5 + P0 infra, §13.7.9 mutation pipeline including the 2026-04-20 first-run workflow-path + script-parser fixes, §2.1–§2.6 CI cleanup, §2.7 full-fix, §2.7.1b highway-ui strict-mode, protoc rate-limit hardening, closeout-plan P1–P5, v0.1.0-readiness Tracks A+B+C — **all on `main` at `89e0e1d`**. Test totals unchanged from prior pass: `wacp-coordinator` 387, `wacp-workspace` 65, `wacp-runtime` 109, `wacp-types` 45, `console-integration` 50 passing / 0 ignored, `pnpm test:e2e` 7 pass / 9 skip / 0 fail, frontend `pnpm typecheck` 0 errors, `pnpm test:isolated` 22 files green, `pnpm lint` 0 errors, `wacp-local` 86 tests, `wacp-cli` 132 tests, all 7 ecosystem packages typecheck, `sdk-python` 104 tests. Workspace clippy + fmt clean.

**CI state on `dev` and `main`:** aligned at `89e0e1d` (ff'd 2026-04-20 14:18 UTC). Post-ff push-trigger CI on `main` was in_progress at refresh time; `dev` at the same SHA was green on all applicable workflows. `ci-console` runs Rust + Integration (50 tests) + Frontend + E2E (Playwright). Mutation pipeline has its own `ci-mutation.yml` workflow — schedule + `workflow_dispatch`, does not block PRs.

**Key docs:**
- `impl/ci-health-2026-04-17.md` — historical record of the §2.1–§2.7 cleanup.
- `impl/archive/ci-cleanup-2.7-plan.md` — five-phase CI-drift plan; documents four plan deviations that needed adapting.
- `impl/archive/audit-13-7-8-plan.md` — seven-phase integration plan with per-suite deliverables, scope adjustments, and drift-filing discipline captured.
- `wacp-console/frontend/e2e/README.md` — local-run recipe for Playwright.
- `HEALTH-LOG.md` §13 — integration + chaos findings per suite (all five subsections + P0 infra documented); §12.5 `ProfilesPage` Create-New click unmounts React still open (30–60 min bisect).
- `HEALTH-LOG.md` §11.4 — **P0 pass executed 2026-04-18**, full table of seven fixed sites; one recovery-matrix test exposed as coincidentally-green, renamed + flipped, Closed-terminal → COMPLETED branch coverage flagged as follow-up.
- `tech-debt-2026-04-18.md` — baseline survey (file sizes, binary sizes, dep graph, disk footprint) + 3-bucket triage (A pre-launch deletion / B post-v0.1 refactor / C CI prevention). User-facing footprint is fine; contributor-facing is the problem. **§7 open questions answered 2026-04-19** — Q1 delete / Q2 agree / Q3 single PR; execution tracked in `impl/archive/closeout-plan.md` (archived 2026-04-20).
- `impl/git-strategy.md` — branching (main / dev / `{scope}/{slug}` topic branches), commit conventions extending CLAUDE.md with §X.Y scopes, fast-forward merges, draft-PR-for-CI pattern, release tagging per binary, failure-handling recipes. §13 tooling: all four items closed (`.gitmessage` + rerere + opt-in pre-push hook landed 2026-04-19 at `3364098` + `7e83da5`; branch protection landed 2026-04-20 via `gh api`).
- `impl/archive/closeout-plan.md` — **archived 2026-04-20** after all five phases closed. P1 `6197263..c07af7f` (2026-04-19); P2 `338e4f2` repo admin + doc; P3 ff dev→main `d0be941..338e4f2`; P4 `b076711..55c29ab` (13 commits on `refactor/file-splits`); P5 `7c4ee1d` (ProfilesPage PaginatedResponse fix); B.4+B.5e follow-ups `27789ff..b9dfaf1` shrank `.file-size-allowlist` Rust production section to empty. §7 execution log has per-phase SHAs.
- `impl/archive/bucket-b-refactor-plan.md` — **archived 2026-04-20** after both B.4/B.5e follow-ups landed (`27789ff` config_env extract; `b9dfaf1` highway_pending extract). `.file-size-allowlist` Rust production section is now empty. Per-file deviation rationale preserved in the doc (B.1 keeps `init.rs` name; B.2/B.3 skip the plan's driver/stage split in favor of test-extraction; B.4+B.5e produce production splits via `#[path]`-based submodule extraction).
- `impl/archive/session-monitor-mutation-triage-plan.md` — **archived 2026-04-20** after all five phases landed on `testing/session-monitor-mutants` (range `c960741..deefc6c`). session_monitor mutation score 72.7 % → 98.2 % (54/55). Plan's §7 execution log preserves per-phase SHAs; §4 acceptance criteria all ticked. One documented-equivalent mutant remains at L356 (`seed_workspace_labels → ()` early-returns under disconnected unit-test pool) with in-file explanatory comment. Side lesson captured: `// mutants:skip` comment form is NOT recognised by cargo-mutants — only `#[mutants::skip]` attributes work.
- `HEALTH-LOG.md` §15 — new top-level section this session: CI workflow drift after the M0–M7 monorepo nesting. §15.1 documents the `--file` path bug, §15.2 documents the v27 output-shape parser bug. Both fixed in the same branch (`22cb0f9` + `a941dfe`). §15.2 carries the post-fix per-target scores + 15-survivor triage-follow-up block.

**When resuming:**

**Branch state to start:** `git checkout dev`. Confirm tips with `git rev-parse dev main` and divergence with `git log --oneline main..dev | wc -l` (0 at SEED-write time). Check CI on tip with `gh run list --branch dev --limit 6` or `--branch main --limit 6`. No live topic branches at refresh time; remote copies of prior-closed branches (`aakil98/ci/pre-launch-closeout`, `aakil98/refactor/file-splits`, `aakil98/fix/ci-mutation-paths`) can be deleted when convenient — they're all fully-merged.

**Primary tracks — in rough ROI order:**
1. ~~**session_monitor mutation triage**~~ — **closed 2026-04-20** at 98.2 % via `impl/archive/session-monitor-mutation-triage-plan.md` (5 phases, `c960741..deefc6c` on `testing/session-monitor-mutants`).
2. **Playwright `--coverage` merge** — 1–2 h: wire Playwright's `--coverage` → `wacp-console/frontend/coverage/lcov.info`. Deferred during §13.7.7 D3; pick up when baseline numbers settle.
3. **`pick_port` TOCTOU fix** — ~2–3 h: replace listener-close-then-bind with holder-listener pattern (requires `--listen-fd` CLI mode on `wacp-runtime`) or deterministic port-range partitioning. Known flake documented in HEALTH-LOG §14.1.
4. **Mutation-pipeline hardening** — ~1 h: pin `cargo-mutants` to a specific major version in `ci-mutation.yml` to prevent another v27-style format-drift; optionally add a fixture-based regression test for `mutation-score.py` / `mutation-summary.py`. Preventive, not urgent. HEALTH-LOG §15.2 "Prevention" block.
5. **Closed-terminal → COMPLETED branch coverage** — follow-up from §11.4 P0 sweep; small.
6. **Three mutation-pipeline single-survivor targets** — each of wacp-transport/auth (4 survivors), wacp-tools/execution (1), console-core/session_launcher (1) has triage worth ~30–60 min apiece. Not blocking; can be batched together.

**Independent / scheduled:**
7. **§13.7.10 (Codecov monthly ratchet)** — deferred until 2–3 `main` merges settle the baseline. Today's 14:18 UTC ff counts as one.
8. **§13.7.9 Monday cron** — now delivers real signal. Next run 2026-04-27 04:00 UTC unless triggered earlier by `workflow_dispatch`. Post-triage ideal: all four targets green.
9. **Four §13.7.8 deferred sub-scenarios** — each tied to absent infrastructure (mock highway, FaultyDb::drop_reads, REST replay endpoint, schema-evolution fixture). In-file notes in the relevant tests.

**Merge strategy:** dev and main aligned at `89e0e1d` after today's ff. Next natural cut point comes whenever the session_monitor triage batch or the Playwright `--coverage` work lands. Tag `wacp-runtime-v0.1.0` / `wacp-console-v0.1.0` once Rust branch-coverage floor clears 85 % and all four mutation targets hit ≥85 % (both post-v0.1 gates, not pre-merge). See `impl/git-strategy.md` §8 for full tagging ceremony + v0.1.0 gates.

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

**Full ADR text:** `wacp-console/SPEC_BUILD.md` + `adr/adr-009-oci-only-console.md`.

## Workspace Layout

```
wacp-platform/
├── Cargo.toml                  # umbrella workspace (all Rust crates from both trees)
├── Cargo.lock                  # unified lockfile
├── rust-toolchain.toml         # pin Rust stable
├── docker-compose.yml          # dev stack: runtime + console + postgres
├── SEED.md                     # this file
├── HEALTH-LOG.md               # living drift/health log — append per session (tier 1)
├── AUDIT-2026-04-15.md         # dated snapshot — comprehensive audit + §13.7 work packages (tier 2)
├── tech-debt-2026-04-18.md     # dated snapshot — file-size + dep baseline + 3-bucket triage (tier 2)
├── adr/                        # architecture decision records (adr-009-oci-only-console)
├── impl/                       # active strategy + historical logs (git-strategy, merge-execution-log, ci-health-2026-04-17)
│   └── archive/                # executed plans (wiring-*, ci-cleanup-2.7-plan, audit-13-7-8-plan, notes/)
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

*WACP Platform — authored by Akil Abderrahim and Claude Opus 4.6. Refreshed 2026-04-16 with post-audit progress and §13.7 task packages; refreshed again same-day by Claude Opus 4.7 (1M context) after §13.7.1–§13.7.5 landed on `dev`; refreshed 2026-04-17 after §13.7.6 (stub provider + I6) landed and §13.7.6b (runtime wiring follow-up) was carved out; refreshed again same-day after dev→main fast-forward and §13.7.6b WA1/WA2/WA3 landed, with WA3.5 + WA3.6 carved out as the remaining gate-fan and auto-integration pieces; refreshed 2026-04-17 (third pass) after §13.7.6b WA3.5 + WA3.6 landed in the working tree; refreshed 2026-04-17 (fourth pass) after §13.7.6b was fully closed via WA5 + T7.3 + WorkspaceState fix + un-ignore sweep; refreshed 2026-04-17 (fifth pass) after §13.7.9 mutation-testing pipeline wired (weekly Monday cron, 4 targets, ≥85% threshold); refreshed 2026-04-17 (sixth pass) after §13.7.7 D1 + D2 landed; refreshed 2026-04-17 (seventh pass) after §2.1–§2.6 CI-pipeline cleanup landed on `dev` (`efd23e9..0845acd` plus doc refresh `e380f04` — five commits restoring Rust/fmt/deny/Frontend-Lint/Integration/Coverage-Rust to green) and the §2.7 full-fix plan was drafted at `impl/archive/ci-cleanup-2.7-plan.md` (nine drift items + two carried-forward debt items, 5.5–7.5 h estimate, no-technical-debt principle); refreshed 2026-04-18 (eighth pass) after §2.7 Phases A+B+C executed on a new `ci/cleanup-2.7` branch (six commits `d8e90fb..4916e90` closing eight of nine drift items in ~80 min vs the planned ~3.0–3.7 h for those phases); refreshed 2026-04-18 (ninth pass) after Phases D + E landed on `ci/cleanup-2.7` (`7e52bfb` §2.7.1 strict-mode, `f12d390` D.1 `wildcards=deny`, `3b23e85` D.2 `eslint-plugin-react-hooks`) in ~85 min vs the planned ~140–170 min — closing all 9 drift items + both debt items; refreshed 2026-04-18 (tenth pass, this session) after the §2.7 branch ff'd to `dev` and CI verification cycle closed out: `2c57a59` §2.7.1b (14 highway-ui strict-mode errors unmasked by §2.7.4's `pnpm install` fix), `d97a6ca` protoc rate-limit hardening (12 `arduino/setup-protoc@v3` usages across 6 workflows now pass `repo-token: ${{ secrets.GITHUB_TOKEN }}` — 60 → 1000 req/h quota), and **§13.7.7 D3+D4+D5 fully landed**: `3508941` (Playwright `e2e` job in `ci-console.yml` — green first try, 3m25s cold-cache, 4 parallel jobs in the console workflow), new `wacp-console/frontend/e2e/README.md`, and this SEED + AUDIT §13.2/§13.5/§13.7.7/§13.8 refresh. Branch state: dev now 26 commits ahead of main, every CI check green. §13.7.7 Playwright E2E fully closed; only Playwright `--coverage` → lcov merge deferred; refreshed 2026-04-18 (eleventh pass) after **§13.7.8 Rust integration + chaos fully closed**: seven commits `78a7fab..f5b4879` across P0 shared infrastructure (`InjectableCoordinator` generalizing WA5's FailureProxy) + I1 launch_failure_matrix (10 tests) + I2 recovery_matrix (7 tests) + I3 auth_matrix (12 tests, scope re-sized from 45-cell to integration-wiring proof) + I4 ws_chaos (3 tests, gap-fill-replay endpoint flagged as non-existent) + I5 taxonomy_reload (4 tests + `RestState` ArcSwap hot-swap upgrade + `ConsoleHarness::spawn_with_db_and_rest` variant). 50/50 integration green, ~5 h vs 7–9 h plan estimate, four sub-scenarios deferred with in-file notes pointing at `HEALTH-LOG.md` §13; refreshed 2026-04-18 (twelfth pass, this session) after **§11.4 P0 enum-offset sweep fully closed** (commit `56085c8` — 4 new `_to_proto` helpers in `wacp-runtime/src/init.rs`, 7 broken internal-enum casts rerouted, hand-rolled `TaskStatus` match folded into helper, one recovery_matrix test renamed + flipped after being exposed as coincidentally-green under the bug, Closed-terminal→COMPLETED branch coverage flagged as a follow-up), and **two planning docs landed** at the user's request in response to the 2200-line-file concern surfaced by §11.4: `tech-debt-2026-04-18.md` (baseline survey of file sizes + binaries + dep graph + disk footprint; 3-bucket triage — A pre-launch deletion / B post-v0.1 refactor / C CI prevention; user-facing footprint is fine, contributor-facing is the problem) and `impl/git-strategy.md` (codifies branching / commit / merge / CI / release tagging / failure-handling conventions in use since M0). Three open tech-debt questions pending user decision per tech-debt §7. Branch state: dev and main both at `d0be941`; **ff'd 2026-04-18** — ff merge carried 48 commits on dev forward to main (which itself had been locally 21 commits ahead of `aakil98/main` from a prior ff never pushed; so `aakil98/main` advanced 69 commits total, `b061c71 → d0be941`). Post-ff CI on main triggered, all four workflows in_progress at refresh time. **Every AUDIT §13.7 package except 13.7.10 (Codecov ratchet — awaiting baseline settle) is closed.***; refreshed 2026-04-18 (thirteenth pass, same session) after the ff — doc realignment plus a note that the prior SEED count was off-by-12 (fix-forwarded in `d0be941`); refreshed 2026-04-18 (fourteenth pass, same session) after the **doc-tier reorganization**: renamed `wacp-console/performance-optimization.md` → `HEALTH-LOG.md` at platform root (the log had already outgrown its console-only scope, covering both binaries in §9/§11/§12/§13); moved `impl/tech-debt-2026-04-18.md` → root (dated snapshot tier); moved `impl/adr-009-oci-only-console.md` → `adr/` (new top-level); archived executed plans to `impl/archive/` (`wiring-strategy.md`, `wiring-strategy-b.md`, `wiring-phases.md`, `ci-cleanup-2.7-plan.md`, `audit-13-7-8-plan.md`, `notes/`); kept `impl/git-strategy.md` + `impl/merge-execution-log.md` + `impl/ci-health-2026-04-17.md` in place as active references / historical logs. New **## Doc Tiers & Audit Process** section (tier 1 living log / tier 2 dated snapshots / tier 3 in-flight plans) codifies the flow so future sessions don't re-derive it. All cross-references updated across SEED.md, AUDIT-2026-04-15.md, README.md, `wacp-console/SEED.md`, `wacp-console/impl/merge-plan.md`, `wacp-console/specs/coding/*.md`, `wacp/impl/wa*.md`, integration test modules, and `session_launcher.rs` / `session_monitor.rs` module comments; refreshed 2026-04-19 (fifteenth pass) after **closeout-plan P1 landed locally** on topic `ci/pre-launch-closeout` (6 commits, pushed to `aakil98` without PR per user directive): Vite sourcemap off, `wacp/highway-ui/` deleted (subtree + impl spec archived to `wacp/impl/archive/highway-ui.md` `status: superseded`), file-size guardrail (`scripts/check-file-sizes.sh` + `.file-size-allowlist` + `ci-lint.yml` `file-size` job), `.gitmessage` template + Development Setup section in README, opt-in `.githooks/pre-push` + `scripts/install-hooks.sh`, tech-debt §7 + git-strategy §13 closeouts. Per-phase SHAs live in `impl/closeout-plan.md` §7, not duplicated here; refreshed 2026-04-19 (sixteenth pass, same session) after the P1 batch ff'd to `dev` (`d0be941..c07af7f`, 9 commits) + pushed to `aakil98/dev`, + the `seed-refresh` skill SKILL.md was updated locally to codify two batch-boundary invariants (SEED refreshes only at ff / package-close / milestone; in-flight SHAs live in tier-3 plan docs, SEED references them). Batch-boundary + SHA-deferral rule explicitly replaces the prior per-phase SEED-update pattern that created the "commit → stale → recommit" loop; refreshed 2026-04-20 (seventeenth pass) after **closeout-plan P2 + P3 landed**: P2 configured branch protection on `main` + `dev` via `gh api` (`required_linear_history=true`, `allow_force_pushes=false`, `allow_deletions=false`, `enforce_admins=false`) with doc updates at `338e4f2`; P3 ff'd `dev` → `main` via the `ff-main` skill (range `d0be941..338e4f2`, 11 commits — the P1 batch + two SEED refreshes + P2). Post-ff both branches align at `338e4f2`; `main` CI in flight at refresh time, `dev` CI green on tip. First push under P2's linear-history rule succeeded for both branches. Remaining closeout-plan work: P4 (Bucket B refactor) + P5 (§12.5 ProfilesPage bisect); refreshed 2026-04-20 (eighteenth pass, same session) after **closeout-plan P4 + P5 landed** — closing every remaining item in the plan. P4 Bucket-B refactor executed as 13 commits on `refactor/file-splits` (B.1 `b9b9eb6..61afe2b` — four-way split of `init.rs` 2139 → 819 across `init.rs` + `agent_service.rs` + `highway_service.rs` + `coordinator_service.rs` + `conversions.rs` with private-field visibility widened to `pub(crate)`; B.2/B.3/B.5 `a6af818`/`747a991`/`34f29c0..0e286e3` — six test-extractions via `#[cfg(test)] #[path="X_tests.rs"] mod tests;` on files that were 50–86% tests; B.4 `8b9df1f` — top-level test extraction on `routes/highway.rs`; `55c29ab` bucket-b plan closure). Ff'd to dev; 1939 workspace tests still pass, fmt + clippy (`--workspace -- -D warnings`) clean. `.file-size-allowlist` shrunk from 9 production entries pre-B to 2 warn-only entries post-B. Production-split follow-ups for `routes/highway.rs` (still 1302 lines) and `config.rs` (still 1038 lines) filed in `impl/bucket-b-refactor-plan.md`. P5 §12.5 fix at `7c4ee1d` — root cause identified by static analysis without needing a git-bisect: `/api/roles` returns `PaginatedResponse<RoleEntry>` = `{items, cursor, has_more}` (see `console-api/src/pagination.rs:37`), but the frontend cast `rolesQuery.data as RoleSummary[]` and called `.map` inside the `<select>` at `ProfilesPage.tsx:400`. Crash deferred until the form mounted (Create-New click). Fix unwraps `.items` with a defensive `Array.isArray` branch so any future endpoint-shape change can't relapse. Golden-path Playwright spec un-skipped and now asserts the Create-New → form-render path; passes in 968 ms. HEALTH-LOG §12.5 marked RESOLVED with full root-cause + fix + preventive-pattern notes. Also filed HEALTH-LOG §14.1 after one flaky `ci-console` run exposed a `RuntimeHarness::pick_port` TOCTOU window (two of five ephemeral ports collided mid-harness-spawn; `gh run rerun --failed` passed cleanly on retry). Dev now at `ce4a3a2`, 15 commits ahead of main; next natural ff-main cut point carries the Bucket-B + P5 + closeout batches forward; refreshed 2026-04-20 (nineteenth pass) after the **CI-mutation pipeline first-run fix** (HEALTH-LOG §15.1 + §15.2) — two companion bugs surfaced by the 2026-04-20 04:00 UTC Monday cron fired all four mutation jobs red with 0 mutants tested. §15.1: `cargo mutants --file src/foo.rs` path-separator globs match from the workspace root per [skip_files.md](https://github.com/sourcefrog/cargo-mutants/blob/main/book/src/skip_files.md), so after the M0–M7 nesting the 7 `--file` args in `.github/workflows/ci-mutation.yml` matched zero files. Fix: rewrite to full `wacp/crates/…` / `wacp-console/crates/…` paths + mirror in spec §4.1–§4.4 + §7 triage instructions. §15.2: cargo-mutants v27 renamed `outcomes.json::summary` values `Caught` → `CaughtMutant`, `Missed` → `MissedMutant`, wrapped scenarios as `{"Mutant": {...}}` with `span.start.line` — `mutation-score.py` / `mutation-summary.py` still looked for the legacy shape and reported 0/0 for every target. Fix: `STATUS_ALIASES` map at parse time + baseline `Success` skip + survivor extraction rewritten. Post-fix run (2026-04-20 13:37 UTC, `workflow_dispatch` on `fix/ci-mutation-paths`) scored wacp-transport/auth 92.3 % (48/52, 4 boundary-operator survivors), wacp-tools/execution 91.7 % (11/12, 1 timeout `> → >=` survivor), console-core/session_launcher 92.9 % (13/14, 1 `build_directive == → !=` survivor), console-core/session_monitor 72.7 % ❌ (40/55, 15 real survivors across stream drivers, handle no-ops, seed_workspace_labels, lag_refresh_hint arms, run_stream_driver counters, and `event_enricher_util::timestamp_rfc3339` nanosecond math). The 15 session_monitor survivors motivated a new tier-3 plan at `impl/session-monitor-mutation-triage-plan.md` (draft) with phases A classify / B stream-driver killers / C handle+init observability / D arithmetic+timestamp / E verify+close, acceptance gate ≥85 %, estimate 3–5 h on topic branch `testing/session-monitor-mutants`. Three-commit batch `22cb0f9..89e0e1d` ff'd dev → main 2026-04-20 14:18 UTC (`4027e80..89e0e1d`); both branches at `89e0e1d`, main CI in_progress at refresh time. Noted during refresh that main had already advanced past the eighteenth-pass `338e4f2` anchor to `4027e80` via the interim v0.1.0-readiness Tracks A+B+C + backend-perf-baseline landings (`f0527d7..c20df35`, 9 commits) and the `4027e80` rustfmt fix — that full span was absent from the prior SEED but is now on `main` and ff'd through to the tip; refreshed 2026-04-20 (twentieth pass, same session) after the **session-monitor mutation triage batch ff'd dev → main** at `633493b`. 8 commits on topic branch `testing/session-monitor-mutants` (range `c960741..633493b`) closed the five-phase plan — classify → driver killers → handle ops + `process_workspace_view` extract → run_stream_driver Fatal + fractional-second timestamp → fixup (attempts counter via `lag_attempts == vec![0,1,2]`, plus redundant trail-arm deletion) → closeout + archive. `console-core/session_monitor` mutation score lifted from 72.7 % to **98.2 %** (54/55); one documented-equivalent mutant remains at L356 (`seed_workspace_labels → ()` early-returns under the disconnected unit-test pool; real path covered by integration tests via `MockHighwayService`). Plan archived to `impl/archive/session-monitor-mutation-triage-plan.md`; §7 execution log has per-phase SHAs. With this batch, all four AUDIT §13.7.9 mutation targets clear the 85 % threshold on `main`: wacp-transport/auth 92.3 %, wacp-tools/execution 91.7 %, console-core/session_launcher 92.9 %, console-core/session_monitor 98.2 %. Three of those four still carry 1–4 non-blocking boundary-operator survivors — triaging them is a medium-priority follow-up (~1.5–3 h total) but unblocks nothing. Surprise side lesson captured in HEALTH-LOG §15.2 + §15.1's companion fix-commit notes: cargo-mutants' `// mutants:skip` comment syntax is NOT honoured — only `#[mutants::skip]` attributes or `#[cfg_attr(any(), mutants::skip)]` work (per `cargo-mutants/book/src/attrs.md`). Dev+main aligned at `633493b`; ci-lint + ci-console + coverage green on both; ci-wacp path-filtered out since the batch touched no `wacp/**`.
