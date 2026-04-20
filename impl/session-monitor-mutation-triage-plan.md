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

Classification complete 2026-04-20. Two mutants are **equivalent** and get `// mutants:skip`; 13 are real gaps. Noteworthy shift: the four stream-driver mutants can be killed with direct `driver(pool, tx, name)` calls against a disconnected `GrpcPool`, which is much simpler than the envisioned mock-upstream / broadcast-subscribe pattern in the original Phase B sketch — Phase B revised below accordingly. L358's `!` deletion needs a small production refactor (extract `process_workspace_view` helper) so the branch is unit-reachable.

| Line | Mutation | Classification | Test sketch | Difficulty |
|---|---|---|---|---|
| 70 | `SessionMonitorHandle::shutdown with ()` | gap | construct handle with explicit `(cmd_tx, cmd_rx)`; call `handle.shutdown().await`; `cmd_rx.try_recv()` returns `MonitorCmd::Shutdown` | easy |
| 74 | `snapshot -> Option<MonitorSnapshot> with None` | gap | spawn a tiny tokio task that drains `cmd_rx` and answers `MonitorCmd::Snapshot(reply)` with a default `MonitorSnapshot`; `handle.snapshot().await` returns `Some(..)` | medium |
| 347 | `Monitor::seed_workspace_labels with ()` | **equivalent at unit level** | fn early-returns under the disconnected pool used by all unit tests (`pool.highway()` → None); body-to-`()` is semantically identical. Real path covered by `wacp-console/integration/` suites via `MockHighwayService`. | skip w/ justification |
| 358 | `delete ! in seed_workspace_labels` | gap (requires refactor) | extract pure helper `process_workspace_view(view, labels, states)`; unit-test both empty-role + non-empty-role views; assert label insertion only when role non-empty | medium (refactor + test) |
| 373 | `spawn_stream_drivers -> Vec<JoinHandle<()>> with vec![]` | gap | construct `Monitor` via `make_monitor`; call `spawn_stream_drivers(tx)`; assert returned `Vec.len() == 4`; abort handles | easy |
| 581 | `delete match arm "trail" in lag_refresh_hint` | **genuinely equivalent** | the wildcard arm `_ => vec![]` produces identical output to `"trail" => vec![]`; no observable difference regardless of inputs | skip w/ justification |
| 648 | `replace += with *= in run_stream_driver` | gap | call `run_stream_driver` directly with an always-erroring mock driver + `reconnect_failure_cap=2`; assert `StreamEvent::Fatal` arrives within bounded time. With `*=`, `failures` stays at `0`, Fatal never fires, test would time out. | medium |
| 659 | `replace * with / in run_stream_driver` | gap | same harness as L648, larger initial backoff (~100ms); assert observed wall time ≥ `initial + 2·initial`. With `/`, backoff shrinks each iteration, wall time drops below threshold. | medium |
| 669 | `trail_driver -> Result<(), String> with Ok(())` | gap | call `trail_driver(disconnected_pool, tx, "trail").await`; assert `Err("highway unavailable")`. Mutant returns `Ok(())` → different | easy |
| 695 | `gates_driver -> Result<(), String> with Ok(())` | gap | same for `gates_driver` | easy |
| 717 | `escalations_driver -> Result<(), String> with Ok(())` | gap | same for `escalations_driver` | easy |
| 741 | `workspace_changes_driver -> Result<(), String> with Ok(())` | gap | same for `workspace_changes_driver` | easy |
| 768 | `* → + in timestamp_rfc3339` | gap | the existing `timestamp_rfc3339_handles_none_and_logical` test uses `physical_us = 1_700_000_000_000_000` where `physical_us % 1_000_000 == 0` — so `0 * 1000 == 0 + 1000 == 0 / 1000` all normalize to the same chrono nanos, coincidentally-green. Add a test with `physical_us = 1_700_000_000_123_456` and assert the rfc3339 string contains `.123456` (or the chrono-emitted fractional-seconds suffix). | medium |
| 768 | `* → / in timestamp_rfc3339` | gap | same fix catches both operator mutants once the fractional-nanos path is tested | medium |
| 770 | match guard `ts.logical == 0 with false` | gap | existing test with `logical = 0` only asserts `contains("2023")` — both branches contain `"2023"`. Add `assert!(!timestamp_rfc3339(&ts).contains("#"))` for the `logical = 0` case to assert the suffix-less branch | easy |

### 3.2 Phase B — Stream-driver killer tests (revised after classification)

Simplified from the original sketch: since each driver function independently calls `pool.highway().await.ok_or_else(|| "highway unavailable")?` as its first real statement, calling the driver directly against a disconnected pool produces an observable `Err` that the `Ok(())` mutant can't produce.

Add four tests to `session_monitor_tests.rs`, one per driver:

```rust
#[tokio::test]
async fn trail_driver_errors_when_highway_unavailable() {
    let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
    let (tx, _rx) = mpsc::channel(16);
    let r = trail_driver(pool, tx, "trail").await;
    assert!(matches!(r, Err(ref s) if s == "highway unavailable"));
}
```

Repeat for `gates_driver`, `escalations_driver`, `workspace_changes_driver`.

Land as one commit: `test(console-core): §13.7.9 B — driver `highway unavailable` coverage (L669/695/717/741)`.

### 3.3 Phase C — Handle + init observability (revised)

Five things to land here:

- **shutdown (L70)** — `let (cmd_tx, mut cmd_rx) = mpsc::channel::<MonitorCmd>(1); let (bcast_tx, _) = broadcast::channel(8); let handle = SessionMonitorHandle { ... }; handle.shutdown().await; assert!(matches!(cmd_rx.try_recv(), Ok(MonitorCmd::Shutdown)));`.
- **snapshot (L74)** — same handle plus a `tokio::spawn` that loops `cmd_rx.recv()` and answers `MonitorCmd::Snapshot(reply) → reply.send(MonitorSnapshot { default })`; then `assert!(handle.snapshot().await.is_some());`.
- **spawn_stream_drivers (L373)** — construct a Monitor via `make_monitor`, call `mon.spawn_stream_drivers(tx)`, assert `out.len() == 4`, abort handles.
- **process_workspace_view helper (L358)** — refactor: extract the `match client.get_workspace(req).await` Ok-arm body into `fn process_workspace_view(view, labels_map, states_map)`. Tests: `empty role → not inserted in labels`; `non-empty role → inserted`. State map always updated.
- **L347 + L581 skips** — add `// mutants:skip` annotations with one-line justification directly above the respective lines.

Land as one commit: `test(console-core): §13.7.9 C — handle ops + spawn_drivers + process_view helper (L70/74/373/358) + mutants:skip L347/L581`.

### 3.4 Phase D — Arithmetic boundary (revised after reading code)

Code locations confirmed: `run_stream_driver` at L615–661. L648 is `failures += 1`; L659 is `backoff = (backoff * 2).min(cfg.reconnect_max)`. `event_enricher_util::timestamp_rfc3339` at L763–774.

**`run_stream_driver` counter + backoff (L648, L659).** Write one driver-level test that calls `run_stream_driver` directly with an always-erroring mock driver:

```rust
#[tokio::test]
async fn run_stream_driver_emits_fatal_after_failure_cap() {
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(16);
    let cfg = MonitorConfig {
        broadcast_capacity: 8,
        reconnect_initial: Duration::from_millis(10),
        reconnect_max: Duration::from_millis(20),
        reconnect_failure_cap: 3,
    };
    let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
    let start = std::time::Instant::now();
    run_stream_driver("trail", pool, cfg, tx, |_pool, _tx, _name| async { Err("boom".to_string()) }).await;
    let elapsed = start.elapsed();

    // Expect 3 Lag events + 1 Fatal event
    let mut lags = 0;
    let mut fatals = 0;
    while let Ok(ev) = rx.try_recv() {
        match ev {
            StreamEvent::Lag { .. } => lags += 1,
            StreamEvent::Fatal { .. } => fatals += 1,
            _ => {}
        }
    }
    assert_eq!(lags, 3, "3 Lag events for 3 failures");
    assert_eq!(fatals, 1, "1 Fatal after cap");
    // Backoff doubles: 10ms + 20ms = 30ms (capped). With `/`, backoff would be 10 + 5 = 15ms.
    assert!(elapsed.as_millis() >= 25, "backoff math must have doubled: {elapsed:?}");
}
```

This one test kills both L648 (without `+=`, Fatal never fires, test fails) and L659 (without `*2`, elapsed time drops below threshold, assertion fails).

**`timestamp_rfc3339` nanos + guard (L768×2, L770).** The existing test uses `physical_us = 1_700_000_000_000_000` where `physical_us % 1_000_000 == 0` — all three mutants (`* → +`, `* → /`, guard → false) produce visibly-identical output for `logical=0` input. Fix: add two assertions to `timestamp_rfc3339_handles_none_and_logical` (or a new sibling test):

```rust
let ts = Some(proto::Timestamp { physical_us: 1_700_000_000_000_000, logical: 0 });
let out = timestamp_rfc3339(&ts);
assert!(out.contains("2023"));
assert!(!out.contains("#"), "logical=0 branch should not append #<n>");  // kills L770

let ts_frac = Some(proto::Timestamp { physical_us: 1_700_000_000_123_456, logical: 0 });
let out_frac = timestamp_rfc3339(&ts_frac);
// Real: (123_456) * 1000 = 123_456_000 nanos = 0.123456 s
// `+`: 123_456 + 1000 = 124_456 nanos = microseconds
// `/`: 123_456 / 1000 = 123 nanos
assert!(out_frac.contains(".123456"), "fractional seconds must reflect real nanos: {out_frac}");  // kills both L768 mutants
```

Land as one commit: `test(console-core): §13.7.9 D — run_stream_driver Fatal + fractional-second timestamp (L648/659/768×2/770)`.

### 3.5 Phase E — Verify + close

1. Run `cargo mutants -p console-core --file wacp-console/crates/console-core/src/session_monitor.rs --no-shuffle` locally — confirm score ≥85 %.
2. For any mutants still Missed after Phases B–D, decide equivalent / accept and annotate with `// mutants:skip` + one-line reason. Re-run to confirm.
3. Push topic branch, open workflow_dispatch run on `ci-mutation.yml` against that branch, confirm green.
4. ff `testing/session-monitor-mutants` → `dev`.
5. Update AUDIT-2026-04-15.md §13.7.9's per-target table: session_monitor `72.7 %` → new score, with link to the killer-tests commit range.
6. Mark HEALTH-LOG §15.2's "Follow-up" block resolved (strike-through the triage line, keep the baseline numbers for posterity).
7. Invoke `archive-plan` to move this doc to `impl/archive/`.

## 4. Acceptance Criteria

- [x] `cargo mutants -p console-core --file wacp-console/crates/console-core/src/session_monitor.rs --no-shuffle` returns ≥ 85 % score. **→ 98.2 %** (54/55 killed; 1 documented-equivalent at L356).
- [x] Any remaining `Missed` mutants carry an explanatory comment. **→ L356 `seed_workspace_labels → ()`** has an in-file comment block explaining why it is equivalent at the unit level (disconnected pool → early return) and pointing at the integration-test coverage. The `// mutants:skip` comment form is not honoured by cargo-mutants (see cargo-mutants/book/src/attrs.md — only `#[mutants::skip]` works); since adding the `mutants` crate just to skip one mutant on a threshold-passing target isn't worth it, we let L356 surface on each run with the explanatory comment.
- [x] `cargo test -p console-core` green. **→ 66 session_monitor tests pass; full console-core suite unchanged.**
- [x] `cargo clippy -p console-core --all-targets -- -D warnings` clean.
- [x] CI `ci-mutation` `Mutants — console-core (session_monitor)` job returns `success`. **→ run `24674588762` all 5 jobs `success`.**
- [x] AUDIT-2026-04-15.md §13.7.9 table shows session_monitor row updated. **→ done in this commit.**
- [x] HEALTH-LOG.md §15.2 follow-up line struck through. **→ done in this commit.**
- [ ] Plan doc moved to `impl/archive/session-monitor-mutation-triage-plan.md` via `archive-plan` skill. **→ next commit, after this closeout lands.**

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
| A | `c960741` | 2026-04-20 | Classification complete; 2 equivalent (L347 unit-level, L581 genuinely), 13 real gaps. §3.2/§3.3/§3.4 simplified after reading code — driver tests use direct calls against disconnected pool; `process_workspace_view` refactor for L358; one run_stream_driver test targets L648 + L659. |
| B | `4ecb946` | 2026-04-20 | Stream-driver killers (L669/695/717/741) — 4 tests passing; all drivers verified to emit `Err("highway unavailable")` against a disconnected pool. |
| C | `aa3c279` | 2026-04-20 | Handle ops (shutdown+snapshot), spawn_drivers Vec::len assertion, `process_workspace_view` extracted with branch tests. `// mutants:skip` comments added (later discovered these don't work; replaced with explanatory prose in fixup commit). |
| D | `5e280d3` | 2026-04-20 | `run_stream_driver_emits_fatal_after_failure_cap` test + fractional-second timestamp assertions. First CI run showed 94.6 % (3 missed): 1 real gap (attempts counter) + 2 equivalent (trail arm + seed_workspace_labels body). |
| D+ | `deefc6c` | 2026-04-20 | Fixup: extended Phase D test to assert `lag_attempts == vec![0, 1, 2]` (kills L662 attempts counter); deleted redundant `"trail" => vec![]` match arm (kills L584 equivalent mutant by simplification); replaced non-functional `// mutants:skip` comment on seed_workspace_labels with explanatory note. Second CI run scored 98.2 % (54/55). |
| E | this commit | 2026-04-20 | AUDIT + HEALTH-LOG closeout + acceptance boxes ticked; ready for archive. |

---

*Plan doc — authored by AAkil98 + Claude Opus 4.7 (1M context). Move to `impl/archive/` once every §4 box is ticked.*
