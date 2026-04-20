---
id: wcon-session-monitor-mutation-triage-plan
type: impl
status: draft
created: 2026-04-20T14:10:03
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, mutation-testing, session-monitor, coverage]
depends_on: [wcon-w3-session-monitor, wcon-mutation-testing]
---

# Session-Monitor Mutation Triage — Plan

> **Triggering finding:** HEALTH-LOG §15.2 — session_monitor scored 72.7 % (40/55) on the first ci-mutation run after §15.1 + §15.2 fixes unblocked the pipeline. 15 real survivors need triage.
> **Target branch:** `testing/session-monitor-mutants` (topic, per `git-strategy.md` §4).
> **Rough effort:** 3–5 h total — medium confidence. Arithmetic mutants in `event_enricher_util::timestamp_rfc3339` may push higher if they need helper-exposure refactors.
> **Not in scope:** other mutation targets (wacp-transport, wacp-tools, session_launcher) — each has 1–4 survivors but is already ≥85 %. File separately if/when they regress.

## 1. Goal & Motivation

Get `session_monitor` above the 85 % mutation-score threshold so the Monday ci-mutation cron produces a green signal on all four targets. Today one target is legitimately red; the pipeline works but the red is stale until triage closes the 15 survivors.

The cost of not doing it: every scheduled mutation run sends a failure notification indistinguishable from a regression. After 2–3 cycles the team will learn to ignore the notification, which re-opens the exact gap §13.7.9 was set up to close. Mutation-testing only pays off when the team trusts the signal.

Scope-wise, `session_monitor.rs` is the most consequential file in the console for correctness during active sessions — W3 delivers trail frames, gates, escalations, and workspace changes via four concurrent Tokio drivers and fans them out over bounded broadcast channels. A mutant that no-ops any one driver would silently drop that channel in production; the fact that 4 of the 15 survivors are exactly that class of mutation (`trail_driver` / `gates_driver` / `escalations_driver` / `workspace_changes_driver` → `Ok(())`) means the existing unit tests are verifying the framing of enriched events without asserting the drivers themselves ran.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| A — Classify | Per-mutant table (gap / equivalent / observability-only) + killer-test sketch per row | ~45 min | — | Table complete; can skip to kill-work |
| B — Stream-driver killers | 4 tests asserting each of trail/gates/escalations/workspace-changes drivers actually ran (channel produced an event with driver-side enrichment) | ~60–90 min | A | L669/695/717/741 mutants caught; re-run shows +4 killed |
| C — Handle + init observability | Tests for shutdown / snapshot / seed_workspace_labels / spawn_stream_drivers post-conditions; `lag_refresh_hint` "trail" arm coverage | ~45–75 min | A | L70/74/347/358/373/581 mutants caught; re-run +6 killed |
| D — Arithmetic boundary | Counter (`+=`) + nanosecond math tests, OR verified-equivalent `// mutants:skip` annotations | ~30–60 min | A | L648/659/768×2/770 resolved (caught or skipped) |
| E — Verify + close | Final local `cargo mutants` run ≥85 %, CI cron next Monday green, AUDIT §13.7.9 table updated, plan archived | ~15 min | B+C+D | Score ≥85 %; plan graduates to `impl/archive/` |

## 3. Deliverables — per phase

### 3.1 Phase A — Classify

Output: expand the table below to one row per mutant. Format per row: "gap | equivalent | observability-only", test sketch (one line), estimated difficulty (easy / medium / hard). Land as a commit on the topic branch that touches only this plan doc.

| Line | Mutation | Classification | Test sketch | Difficulty |
|---|---|---|---|---|
| 70 | `SessionMonitorHandle::shutdown with ()` | TBD | assert shutdown channel receives `()` after call | easy |
| 74 | `SessionMonitorHandle::snapshot -> Option<MonitorSnapshot> with None` | TBD | after drivers produce events, snapshot returns `Some(s)` with non-empty state | easy |
| 347 | `Monitor::seed_workspace_labels with ()` | TBD | after construct, assert `workspace_labels` map populated | easy |
| 358 | `delete ! in Monitor::seed_workspace_labels` | TBD | test with a specific input that the `!` inverts (likely a short-circuit / guard) | medium |
| 373 | `Monitor::spawn_stream_drivers -> Vec<JoinHandle<()>> with vec![]` | TBD | assert returned vec has 4 handles (one per driver) | easy |
| 581 | `delete match arm "trail" in lag_refresh_hint` | TBD | feed a lag signal on trail channel, assert refresh hint distinct from gates/escalations/workspace | medium |
| 648 | `replace += with *= in run_stream_driver` | TBD | counter arithmetic — probably a lag / retry counter; test with a scenario where 2×2 ≠ 2+2 (i.e. ≥2 increments) | easy |
| 659 | `replace * with / in run_stream_driver` | TBD | likely delay / backoff multiplier; assert next-iteration delay equals expected product | medium |
| 669 | `trail_driver -> Result<(), String> with Ok(())` | gap (likely) | spawn monitor, feed a `TrailFrame`, subscribe to `trail` channel, assert received event has driver-side enrichment | medium |
| 695 | `gates_driver -> Result<(), String> with Ok(())` | gap (likely) | same shape — gates channel | medium |
| 717 | `escalations_driver -> Result<(), String> with Ok(())` | gap (likely) | same — escalations | medium |
| 741 | `workspace_changes_driver -> Result<(), String> with Ok(())` | gap (likely) | same — workspace_changes | medium |
| 768 | `replace * with + in event_enricher_util::timestamp_rfc3339` | TBD | timestamp math — may need `(secs=2, nanos=3)` where `2*1e9+3 ≠ 2+3` | medium |
| 768 | `replace * with / in event_enricher_util::timestamp_rfc3339` | TBD | same — assert timestamp with secs≥1 and nanos<secs | medium |
| 770 | `replace match guard ts.logical == 0 with false in event_enricher_util::timestamp_rfc3339` | TBD | test with `ts.logical == 0` and `ts.logical != 0` separately, assert they produce distinct output | medium |

### 3.2 Phase B — Stream-driver killer tests

Add to `wacp-console/crates/console-core/src/session_monitor_tests.rs`. Four tests of similar shape; each:
1. Spins up a `Monitor` with a mock upstream stream pushing one frame.
2. Subscribes to the matching broadcast channel (`trail` / `gates` / `escalations` / `workspace_changes`).
3. Awaits one message with timeout.
4. Asserts the message carries a field that is populated *by the driver*, not passed through untouched — e.g., the `session_id` field, the enriched timestamp, or the queue-position annotation.

Check whether `session_monitor_tests.rs` already has a driver-level test fixture. If not, bring in `tokio::sync::broadcast::channel` + a `tokio::time::timeout` wrapper.

Land as one commit: `test(console-core): §13.7.9 B — stream-driver killer tests (L669/695/717/741)`.

### 3.3 Phase C — Handle + init observability

Tests for `SessionMonitorHandle::shutdown` / `snapshot`, `Monitor::seed_workspace_labels`, `Monitor::spawn_stream_drivers`, and `lag_refresh_hint`:

- **shutdown:** construct handle with `tokio::sync::watch::channel`; call `shutdown()`; assert receiver sees `true` / sent.
- **snapshot:** feed drivers with at least one frame per channel; call `snapshot()`; assert `Some(s)` with `s.trail_len > 0` etc.
- **seed_workspace_labels:** construct monitor with a seed input containing two workspace IDs; assert `workspace_labels` has both keys with expected values.
- **seed_workspace_labels negation (L358):** identify what the `!` guards — likely a presence check. Add a test that would pass with `!x` but fail with `x`.
- **spawn_stream_drivers:** call the method, assert returned `Vec<JoinHandle<()>>.len() == 4`.
- **lag_refresh_hint "trail" arm:** match on `("trail", n)` vs `("gates", n)` etc. — assert the four arms produce distinguishable hints (e.g., different prefix, different ratio).

Land as one commit: `test(console-core): §13.7.9 C — handle + init observability (L70/74/347/358/373/581)`.

### 3.4 Phase D — Arithmetic boundary

Two sub-problems:

**run_stream_driver counter + arithmetic (L648, L659).** Read lines 640–670 to identify what the counter represents (lag count, retry count, chunk size?). Write a test that exercises ≥2 iterations with inputs where `+=` and `*=` diverge (trivially: any counter where the second iteration adds 1, so `n=1 → 2` (correct) vs `n=1 → 1` (`*=1` mutant) or `n=2 → 4` (`*=`). Similarly for `*` vs `/` at L659.

**event_enricher_util::timestamp_rfc3339 nanosecond math (L768 ×2, L770).** Read the helper. Three mutants here:
- L768 `*` → `+`: probably `secs * 1_000_000_000 + nanos`. Test with `secs=1, nanos=0` where `1_000_000_000 ≠ 1`.
- L768 `*` → `/`: same, `1_000_000_000 ≠ 0` (since 1/1_000_000_000 == 0 in integer arithmetic).
- L770 match guard `ts.logical == 0`: a fast-path for the zero-logical-clock case. Test two vectors: `(secs=1, logical=0)` and `(secs=1, logical=5)` — assert different output.

If the helper isn't `pub(crate)`, either widen visibility for test-only access or add a doctest-style invocation path via a public caller. Prefer widening — doctest paths fragilize faster.

Land as one commit: `test(console-core): §13.7.9 D — arithmetic + timestamp killers (L648/659/768×2/770)`.

### 3.5 Phase E — Verify + close

1. Run `cargo mutants -p console-core --file wacp-console/crates/console-core/src/session_monitor.rs --no-shuffle` locally — confirm score ≥85 %.
2. For any mutants still Missed after Phases B–D, decide equivalent / accept and annotate with `// mutants:skip` + one-line reason. Re-run to confirm.
3. Push topic branch, open workflow_dispatch run on `ci-mutation.yml` against that branch, confirm green.
4. ff `testing/session-monitor-mutants` → `dev`.
5. Update AUDIT-2026-04-15.md §13.7.9's per-target table: session_monitor `72.7 %` → new score, with link to the killer-tests commit range.
6. Mark HEALTH-LOG §15.2's "Follow-up" block resolved (strike-through the triage line, keep the baseline numbers for posterity).
7. Invoke `archive-plan` to move this doc to `impl/archive/`.

## 4. Acceptance Criteria

- [ ] `cargo mutants -p console-core --file wacp-console/crates/console-core/src/session_monitor.rs --no-shuffle` returns ≥ 85 % score.
- [ ] Any remaining `Missed` mutants carry a `// mutants:skip` annotation with one-line justification.
- [ ] `cargo test -p console-core` green (no regression from new test additions).
- [ ] `cargo clippy -p console-core --all-targets -- -D warnings` clean.
- [ ] CI `ci-mutation` `Mutants — console-core (session_monitor)` job returns `success` on scheduled Monday cron or manual `workflow_dispatch`.
- [ ] AUDIT-2026-04-15.md §13.7.9 table shows session_monitor row updated with new score + commit link.
- [ ] HEALTH-LOG.md §15.2 follow-up line struck through (not the finding itself — just the triage follow-up).
- [ ] Plan doc moved to `impl/archive/session-monitor-mutation-triage-plan.md` via `archive-plan` skill.

## 5. Risks / Open Questions

- **Mock upstream for driver tests (B).** `session_monitor_tests.rs` may currently use the real integration harness `MockCoordinator`, which is overweight for unit-level mutant killing. If so, Phase B should introduce a lighter per-channel mock (just `tokio::sync::broadcast` or `mpsc` senders the test controls directly) rather than bringing up the whole harness. Cost delta: ~30 min.
- **Visibility of `event_enricher_util::timestamp_rfc3339` (D).** If it's private-to-module, broadening to `pub(crate)` for tests is fine; if it's currently `pub(super)` or deeper, consider an `#[cfg(test)] pub fn` shim. Avoid publishing a production API just for tests.
- **Equivalent mutants in arithmetic.** Some timestamp-math mutants may be genuinely equivalent under the test inputs available (e.g., if all tested nanosecond values are 0). In that case `// mutants:skip` with a "only changes output for nanos ≠ 0; no such call site exists" justification is acceptable — but verify the claim by grepping callers, don't trust the intuition.
- **Cron timing.** The Monday 04:00 UTC cron won't re-evaluate until next Monday (2026-04-27). Use `workflow_dispatch` against the topic branch for interim verification — otherwise the feedback loop is a week.
- **Unviable count of 15.** Today's run shows 15 unviable mutants in session_monitor (compile errors after mutation). These are not scored, but if Phase B/C changes cause the unviable count to shift — e.g., a formerly-unviable mutant is now viable and Missed — the score math changes. Re-check survivor list after each phase's commit; don't assume the 15 pre-identified survivors are the full remaining set.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| `wcon-w3-session-monitor` | W3 Session Monitor | implemented by the module under test |
| `wcon-mutation-testing` | Mutation Testing Pipeline | triage loop lives in §7; threshold formula in §5 |
| `AUDIT-2026-04-15.md` §13.7.9 | Mutation-testing work package | post-work: update per-target table here |
| `HEALTH-LOG.md` §15.2 | Script-parser bug + first-run numbers | triggering finding; new score replaces the 72.7 % baseline |

## 7. Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| A | TBD | — | Classify each of 15 mutants; update §3.1 table in place |
| B | TBD | — | Stream-driver killers |
| C | TBD | — | Handle + init observability |
| D | TBD | — | Arithmetic + timestamp |
| E | TBD | — | Verify + archive |

---

*Plan doc — authored by AAkil98 + Claude Opus 4.7 (1M context). Move to `impl/archive/` once every §4 box is ticked.*
