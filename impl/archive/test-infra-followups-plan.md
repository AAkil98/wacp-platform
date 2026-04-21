---
id: wacp-test-infra-followups-plan
type: impl
status: draft
created: 2026-04-21T18:30:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, playwright, coverage, integration-harness, mutation-testing, flake]
depends_on: [wcon-test, wcon-mutation-testing]
---

# Test-Infra Follow-ups — Plan

> **Triggering findings:** four residual items from SEED "Resumption Point — Primary tracks" (post-`5adb8b6` dev tip): Playwright `--coverage` merge (deferred during §13.7.7 D3), `pick_port` TOCTOU flake (HEALTH-LOG §14.1), mutation-pipeline hardening (HEALTH-LOG §15.2 Prevention), and the three non-blocking single-survivor mutation targets carried over from AUDIT §13.7.9 first-run triage close.
> **Target branch:** one topic branch per phase — these are independent work items with no ordering dependency. Use `testing/{slug}` per `git-strategy.md` §4. Phase E is a docs-only close commit that can land on `dev` directly after the four phase branches ff.
> **Rough effort:** ~5.5–9.5 h across all phases — medium confidence on A/B/D, high on C. See §2 for per-phase estimates.
> **Not in scope:** §13.7.10 Codecov monthly ratchet (deferred until more merges settle the baseline), §11.4 P0 "Closed-terminal → COMPLETED branch coverage" follow-up (unrelated single-file tweak), broader mutation sweep beyond the three targets already passing at ≥85 %.

## 1. Goal & Motivation

These four items are each small enough on their own to not warrant a dedicated tier-3 plan, but large enough in aggregate (and stale enough) to be worth closing as a batch. All four share a rough theme — **test infrastructure that is already 85 %+ working but has a known gap, a known prevention opportunity, or a known flake** — and leaving them open for another week risks forgetting the context that makes each one cheap to fix.

The cost of not fixing:

- **Playwright `--coverage`**: the frontend lcov shipped to Codecov under-counts wizard + session-launch + oversight-rehydration paths that are only exercised in E2E. Under-counting translates into false "coverage regression" alerts when a new unit test lands and shifts the denominator. Closing this normalises the denominator.
- **`pick_port` TOCTOU**: one observed flake so far, `gh run rerun --failed` cleared it. Two-of-five port collisions happened because the bind-close-return pattern gives the OS a window to reassign. The probability grows with parallelism; when §13.7.7 Playwright runs in CI alongside `ci-console` integration suites, the flake rate will compound.
- **Mutation-pipeline hardening**: the 2026-04-20 first-run bugs (§15.1 + §15.2) cost one Monday cron's worth of lost signal. The next cargo-mutants major-version bump will do the same without a version pin. The parser fixture prevents the exact class of bug §15.2 was.
- **Three single-survivor targets**: wacp-transport/auth is at 92.3 % with 4 boundary-operator survivors, wacp-tools/execution at 91.7 % with 1 timeout-comparison survivor, console-core/session_launcher at 92.9 % with 1 `build_directive` equality survivor. Each survivor is a real coverage gap — they are boundary cases that current tests happen to pass around. Fixing them doesn't block anything, but each closes a documented gap cheaply (30–60 min apiece).

Nothing in here is urgent. But all of it is stale enough that a single batch closes cleanly before the context evaporates.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| A — Playwright `--coverage` merge | V8 coverage emitted from Playwright; merged with Vitest lcov; Codecov sees the combined file | ~1–2 h | — | Codecov frontend baseline shifts by > 0 pp; `pnpm test:e2e` emits `coverage/playwright.lcov`; `pnpm coverage:merge` produces unified `coverage/lcov.info`; CI green |
| B — `pick_port` TOCTOU fix | Race-free port acquisition in `RuntimeHarness`; 100× stress-test passes | ~2–3 h | — | New test in integration harness hammers `spawn_default()` in a tight loop; no "address already in use"; HEALTH-LOG §14.1 struck through |
| C — Mutation-pipeline hardening | `cargo-mutants` pinned in `ci-mutation.yml`; fixture-based parser regression test for score/summary scripts | ~1 h | — | `workflow_dispatch` run produces identical output to last week's run on the same SHA; `python3 scripts/mutation-score.py fixtures/mutants.sample/` passes in repo CI |
| D — Three single-survivor mutation triages | wacp-transport/auth, wacp-tools/execution, console-core/session_launcher each triaged to ≥ 95 % *or* documented-equivalent | ~1.5–3 h | — | Post-triage `workflow_dispatch` run shows all three targets either ≥ 95 % or with an in-file `#[mutants::skip]` + comment per survivor |
| E — Close | HEALTH-LOG strikethroughs, AUDIT §13.9 update, plan archive, ff'd phase branches | ~20–30 min | A+B+C+D | All plan §4 acceptance boxes ticked; plan moved to `impl/archive/` |

**Branching strategy.** Four independent topic branches ff to `dev` in any order (or in parallel if convenient). A single bundled branch would tie unrelated work together; per `git-strategy.md` §4 each phase gets its own branch:
- `testing/playwright-coverage-merge` (Phase A)
- `fix/pick-port-toctou` (Phase B)
- `ci/mutation-pipeline-pin` (Phase C)
- `testing/mutation-survivor-triage` (Phase D)

Phase E's docs-only commit can land directly on `dev` once A–D have all ff'd.

## 3. Deliverables — per phase

### 3.1 Phase A — Playwright `--coverage` merge

Per `wacp-console/frontend/e2e/README.md` + SEED §13.7.7 D3 deferral note, the Playwright test run currently emits no coverage data. The Vitest side emits `wacp-console/frontend/coverage/lcov.info` consumed by Codecov via the `ci-console.yml` coverage job. Goal: produce a merged lcov that includes both.

Options:
- **Playwright native V8 coverage** (`--coverage-dir`) + `c8 report` to convert to lcov. Deterministic but needs a build-time sourcemap configuration, since Vite's `build.sourcemap: false` (landed in closeout-plan P1) hides the source mapping. Re-enabling sourcemaps for the E2E preview server only is fine.
- **`@playwright/test` built-in `experimental-ct` coverage** — rejected; V8 path is standard.
- **`nyc` instrumentation** — rejected; Vite + V8 is the cleaner pipeline.

Execute path (V8):

1. **`wacp-console/frontend/vite.config.ts`** — add a `preview` mode flag that re-enables sourcemaps when `E2E_COVERAGE=1` is set. Production build still ships with sourcemap off.
2. **`wacp-console/frontend/playwright.config.ts`** — add a global `use.launchOptions.args: ['--js-flags=--coverage']` or use Playwright's built-in `coverage.startJSCoverage()` via a `test.beforeEach` + `test.afterEach` wrapper that writes to `coverage/playwright-raw/${testId}.json`.
3. **New script `wacp-console/frontend/scripts/merge-coverage.sh`** — shells out to `c8 report --reporter=lcovonly --temp-directory=coverage/playwright-raw/ --report-dir=coverage/playwright/` then concatenates or merges with Vitest's `coverage/lcov.info`. Lcov supports simple concatenation.
4. **`wacp-console/frontend/package.json`** — add `"coverage:merge": "bash scripts/merge-coverage.sh"` and wire `"test:all": "npm run test && npm run test:e2e && npm run coverage:merge"`.
5. **`.github/workflows/ci-console.yml`** — the `coverage` job currently runs Vitest then uploads to Codecov. Extend to also run `test:e2e` with `E2E_COVERAGE=1` → `coverage:merge` → upload the merged file.

Commit message: `feat(frontend): wire Playwright V8 coverage into merged lcov`. Body: one-paragraph outline of the merge path + reference to SEED §13.7.7 D3 deferral.

### 3.2 Phase B — `pick_port` TOCTOU fix

Per HEALTH-LOG §14.1, `RuntimeHarness::pick_port` binds a `TcpListener` to port 0, reads `.local_addr()`, then closes the listener and returns the port number. The runtime child binds to that port a moment later. Between the close and the child's bind, another process (or another test harness instance in parallel) can grab the port.

Two resolutions sketched in §14.1:
- **Holder-listener + --listen-fd.** Keep the listener open in the harness; pass the file descriptor to the runtime child via a new `--listen-fd N` CLI mode. The runtime calls `TcpListener::from_raw_fd(N)` and binds it directly — no re-bind, no race. Requires a small CLI change in `wacp-runtime`.
- **Deterministic port-range partitioning.** Env-variable-driven port range per test worker (`WACP_TEST_PORT_BASE=20000 + $RAYON_WORKER * 100`). Simple but fragile under test re-runs + nextest.

Lean: **holder-listener**. It's the correct fix; the partitioning workaround is brittle and doesn't scale past a single test binary. Let me size it:

Execute path (holder-listener):

1. **`wacp/crates/wacp-runtime/src/main.rs`** (or the CLI parser — check current structure) — add a `--listen-fd-agent N --listen-fd-highway M --listen-fd-coordinator P` trio of args. When present, skip the bind step and hand the listener directly to the tonic server builder.
2. **`wacp/crates/wacp-runtime/src/server.rs`** (or wherever the server builders live) — accept a `Option<TcpListener>` per service; when `Some`, use `Server::builder().serve_with_incoming(TcpListenerStream::new(l))` instead of `Server::builder().serve(addr)`.
3. **`wacp-console/integration/src/runtime_harness.rs`** — `spawn_default()` creates three `TcpListener`s bound to `:0`, reads their ports, passes both the ports (for config) and the FDs (for `--listen-fd-*`) to the child via env vars + `std::process::Command::pre_exec` to set the fds in the child.
4. **Stress test** — new `#[tokio::test] async fn spawn_default_survives_100x_tight_loop()` in the integration crate that spawns + drops 100 harnesses back-to-back. Asserts zero "address already in use" errors.
5. **Strike through HEALTH-LOG §14.1** with the commit SHA.

Commit message: `fix(integration): §14.1 — race-free port acquisition via holder-listener + --listen-fd`. Body: references §14.1 + notes the new CLI surface.

**Fallback.** If the holder-listener approach runs into an unanticipated snag (tonic API shape, unix-only `from_raw_fd`), fall back to the partitioning approach in a second attempt — estimate 1 h extra.

### 3.3 Phase C — Mutation-pipeline hardening

Per HEALTH-LOG §15.2 Prevention block: "Pin `cargo-mutants` to a specific major version via the install step or set up schema-snapshot tests against a checked-in `outcomes.json` sample."

Two parts, both cheap:

**C1. Version pin.** In `.github/workflows/ci-mutation.yml`, the `cargo install cargo-mutants` step currently installs the latest. Change to `cargo install cargo-mutants --version "^27"` (matches the v27.x we currently parse against). A future v28 breaking-change lands loudly via this pin failing.

**C2. Fixture-based parser regression test.** Create `scripts/fixtures/mutants-sample.outcomes.json` as a small hand-crafted sample covering `CaughtMutant`, `MissedMutant`, and `Timeout` scenario variants (the three parse paths in `mutation-score.py` + `mutation-summary.py`). Add `scripts/test_mutation_scripts.sh` that runs both Python scripts against the fixture and asserts exact stdout matches a committed `expected.txt`. Wire into `ci-lint.yml` (new `mutation-scripts` job, ~5 s).

Files touched:
- `.github/workflows/ci-mutation.yml`
- `.github/workflows/ci-lint.yml`
- `scripts/fixtures/mutants-sample.outcomes.json` (new)
- `scripts/fixtures/mutants-score-expected.txt` (new)
- `scripts/fixtures/mutants-summary-expected.txt` (new)
- `scripts/test_mutation_scripts.sh` (new)

Commit message: `ci(mutation): pin cargo-mutants to ^27 + add parser fixture regression test`. Body: refs HEALTH-LOG §15.2 Prevention.

**Scope note.** If the fixture assembly turns out to be > 1 h (because crafting a realistic `outcomes.json` needs subtle schema introspection), split: land C1 pin alone; punt C2 as its own follow-up.

### 3.4 Phase D — Three single-survivor mutation triages

Per AUDIT §13.7.9 post-triage close (SEED 20th pass), three of the four mutation targets clear 85 % but retain 1–4 non-blocking survivors:

| Target | Score | Survivor shape | Est. |
|---|---|---|---|
| wacp-transport/auth | 92.3 % (48/52) | 4 boundary-operator mutants in rate-limiting logic (e.g., `>` → `>=` on attempt-count thresholds) | 45–60 min |
| wacp-tools/execution | 91.7 % (11/12) | 1 timeout-comparison (`>` → `>=` on elapsed-ms check) | 15–30 min |
| console-core/session_launcher | 92.9 % (13/14) | 1 `build_directive` equality check (`==` → `!=` on a config-lookup arm) | 15–30 min |

Execute path per target:
1. Run `cargo mutants --package <P> --file <F> --json` locally to reproduce the survivor list.
2. For each survivor, write a test that exercises the boundary exactly — typically a pair of assertions straddling the threshold (e.g., "attempt N+1 triggers lockout, attempt N does not").
3. Re-run mutants to confirm kill. If the survivor is genuinely equivalent (rare for boundary operators, but common for dead-match-arm mutants), add `#[mutants::skip]` to the arm with a one-line explanatory comment — per HEALTH-LOG §15.2's side lesson, do NOT use the `// mutants:skip` comment form.

Each target gets its own commit on the shared `testing/mutation-survivor-triage` branch:
- `test(wacp-transport): kill auth rate-limit boundary mutants`
- `test(wacp-tools): kill execution timeout-comparison mutant`
- `test(console-core): kill session_launcher build_directive equality mutant`

After all three commits land, trigger a `workflow_dispatch` run of `ci-mutation.yml` and confirm all four targets ≥ 95 % (or documented-equivalent). Update the HEALTH-LOG §15.2 "Post-fix run" table with the new numbers.

### 3.5 Phase E — Verify + close

1. Confirm A, B, C, D all ff'd to `dev` (`git log --oneline main..dev` shows the four commit ranges).
2. `cargo test -p wacp-transport -p wacp-tools -p console-core` green — makes sure Phase D's new tests don't regress anything.
3. `cargo test -p console-integration --test lifecycle --test chaos` green — B's holder-listener change passes the main integration suites.
4. `pnpm test:all` green under `E2E_COVERAGE=1` — A's merge produces a valid lcov.
5. `workflow_dispatch` `ci-mutation.yml` — four targets all ≥ 85 % (ideally ≥ 95 %); confirm scores in HEALTH-LOG §15.2 "Post-triage" block.
6. HEALTH-LOG:
   - §14.1: strike through with "Resolved in {commit}: holder-listener pattern".
   - §15.2 Prevention: strike through with "Resolved in {commit}: pinned ^27 + fixture regression test".
   - §15.2 Post-triage survivor list: update with the three target closures.
7. AUDIT-2026-04-15.md §13.9:
   - §13.9.3 (§16 test-only clippy drifts) — unchanged, still open.
   - §13.9.4 (§14.1 pick_port TOCTOU) — strike through + commit SHA.
   - Add §13.9.5 (Playwright coverage merge — §13.7.7 D3 follow-up closed).
   - Add §13.9.6 (Mutation pipeline hardening — HEALTH-LOG §15.2 Prevention closed).
   - Add §13.9.7 (Three single-survivor targets — HEALTH-LOG §15.2 Post-triage closed).
8. Archive via `archive-plan` skill → `impl/archive/test-infra-followups-plan.md`.
9. SEED refresh via `seed-refresh` skill at batch boundary (once all four ff's complete).

## 4. Acceptance Criteria

- [x] Phase A ff'd to `dev` (`f6e1647`): merged lcov in Codecov via Playwright V8 + c8 + same-flag upload. CI `frontend-e2e` job green first run.
- [x] Phase B ff'd to `dev` (`4aef1df`): batch-pick eliminates intra-harness collision; 1000-iteration regression test; HEALTH-LOG §14.1 struck through.
- [x] Phase C ff'd to `dev` (`d9e9c60`): `ci-mutation.yml` pins `cargo-mutants` to `^27`; `scripts/test_mutation_scripts.sh` runs in `ci-lint`. CI `mutation-scripts` job green first run.
- [x] Phase D ff'd to `dev` (`7c11186..f5658cf`, 5 commits): all 4 mutation targets at 100 % locally — `wacp-tools` + `session_launcher` via boundary tests; `wacp-transport` via 4 `#[mutants::skip]` helpers documenting the time-unreachable residual.
- [x] `cargo test -p wacp-transport -p wacp-tools -p console-core` green (per-phase local runs — 209 + 136 + 188 = 533 tests).
- [x] `cargo test -p console-integration --test lifecycle --test chaos` green (Phase B local run — 6/6).
- [x] `pnpm test:all` green under `E2E_COVERAGE=1` — validated in CI `frontend-e2e` job on Phase A push (7/7 E2E passed with coverage, c8 report emitted, Codecov upload accepted).
- [x] HEALTH-LOG §14.1 and §15.2 updated with resolution pointers.
- [x] AUDIT-2026-04-15.md §13.9 table updated with entries 13.9.5–13.9.7 (and 13.9.4 closed).
- [ ] Plan archived via `archive-plan` skill.
- [ ] SEED refreshed via `seed-refresh` skill at the batch boundary.

## 5. Risks / Open Questions

- **Phase A sourcemap exposure.** Re-enabling sourcemaps for the E2E preview server increases the attack surface marginally — if the preview container is accidentally reachable from outside CI, sources leak. Mitigation: gate behind `E2E_COVERAGE=1` env flag and never set that flag in release builds. Document in the Vite config comment.
- **Phase B holder-listener on non-Unix.** `TcpListener::from_raw_fd` is Unix-specific. The repo's CI is Ubuntu-only, but developer machines include WSL + native Linux. Windows-native is a hypothetical; don't block on it. If a future Windows-native push exists, revisit.
- **Phase B fallback trigger.** If the tonic Server builder API doesn't cleanly accept a pre-bound listener (it should — `serve_with_incoming` exists), fallback to partitioning adds 1 h. Decision point at the end of Phase B step 2 — if `serve_with_incoming` requires restructuring the init path, stop and re-evaluate.
- **Phase C fixture brittleness.** A sample `outcomes.json` may drift as cargo-mutants releases patch versions that tweak unrelated fields. Counter: keep the fixture minimal — just the three fields the parser reads (`CaughtMutant`/`MissedMutant`/scenario kind). Don't mirror the full schema.
- **Phase D equivalent-mutant judgment.** Some of the remaining survivors may be genuinely equivalent (e.g., "change `==` to `!=` in an arm that is unreachable via normal flow"). Deciding whether a mutant is "equivalent" or "real gap" is a judgment call. Bias toward writing a test that explicitly exercises the boundary — if that test exists and fails on the mutant, it's a real gap; if it can't be written without contriving impossible state, the mutant is equivalent.
- **Parallel-execution risk for Phase D.** Running `cargo mutants` on three packages in parallel locally saturates CPU — serial execution is safer, and each target takes ~5–10 min wall. CI's `ci-mutation.yml` already runs them in parallel via matrix, so the cost-of-serial is only local.
- **Scope inflation.** The four phases are independent on paper. If Phase A's V8-coverage pipeline turns out to require Vite config surgery, the 1–2 h estimate could slip to 3–4 h. Hold the estimate as stated; if actual goes > 2x, stop and decide whether to split into its own plan.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| `wcon-test` | Console test strategy | authoritative; plan phases A/B/D operate under it |
| `wcon-mutation-testing` | Mutation testing spec | authoritative; plan phases C + D operate under it |
| `HEALTH-LOG.md` §14.1 | `RuntimeHarness::pick_port` TOCTOU | triggering finding for Phase B |
| `HEALTH-LOG.md` §15.2 Prevention | Pin `cargo-mutants` + fixture test | triggering finding for Phase C |
| `HEALTH-LOG.md` §15.2 Post-triage | Three single-survivor targets | triggering finding for Phase D |
| `AUDIT-2026-04-15.md` §13.7.7 D3 deferral | Playwright `--coverage` defer note | triggering finding for Phase A |
| `AUDIT-2026-04-15.md` §13.7.9 | Mutation testing work package | parent of Phases C + D |
| `AUDIT-2026-04-15.md` §13.9 | Post-audit follow-ups table | where E's new rows land |
| `impl/archive/console-db-schema-alignment-plan.md` | Sibling pattern | worked example of multi-phase test-infra plan closing via strike-through |

## 7. Execution log

| Phase | Commit(s) | Date | Note |
|---|---|---|---|
| A | `f6e1647` | 2026-04-21 | Playwright V8 coverage → c8 → Codecov `frontend` flag. Deviation from plan's merge-script: same-flag Codecov upload unions vitest + Playwright lcovs. |
| B | `4aef1df` | 2026-04-21 | Batch-pick eliminates intra-harness collision; 1000-iter regression test. Plan deviation: batch-pick (~45 min) vs. full holder-listener (~2–3 h+). Cross-harness TOCTOU residual accepted. |
| C | `d9e9c60` | 2026-04-21 | `cargo-mutants` pinned to `^27` + parser-fixture regression test wired into ci-lint. CI `mutation-scripts` job green first run. |
| D | `7c11186..f5658cf` | 2026-04-21 | 5 commits across 3 targets + fmt fixup + HEALTH-LOG update. wacp-tools + session_launcher killed via boundary tests; wacp-transport marked via 4 `#[mutants::skip]` helpers. Plan deviation: skip helpers instead of clock-injection refactor (plan-documented alternative). |
| E | _in flight_ | 2026-04-21 | Close + archive. AUDIT §13.9.4–§13.9.7 closed. |

---

*Plan doc — authored by AAkil98 + Claude Opus 4.7 (1M context). Move to `impl/archive/` once every §4 box is ticked.*
