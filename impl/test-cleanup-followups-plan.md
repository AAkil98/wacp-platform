---
id: wacp-test-cleanup-followups-plan
type: impl
status: draft
created: 2026-04-22T12:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, clippy, testing, coverage, ci]
depends_on: [wacp-test-infra-followups-plan]
---

# Test-Cleanup Follow-ups — Plan

> **Triggering findings:** HEALTH-LOG §16 (six test-only clippy drifts CI's per-crate-no-`--all-targets` invocation skips) + HEALTH-LOG §11.4 trailing follow-up (Closed-terminal → COMPLETED branch in `recovery::recover_one` lost integration coverage when `terminal_workspace_closed_marked_completed` was renamed + flipped after the enum-offset sweep exposed it as coincidentally-green).
> **Target branch:** `testing/cleanup-followups` (topic).
> **Rough effort:** ~2.5–4h — **medium** confidence. Phase A is mechanical; Phase B size depends on how invasive the test-only `mark_workspace_closed` helper turns out to be.
> **Not in scope:** cross-harness `pick_port` TOCTOU residual (§14.1 narrower case, never observed), AUDIT §13.7.10 Codecov ratchet (scheduled / awaiting baseline settle), full sweep of *every* `--all-targets` clippy warning across the workspace beyond what Phase A.2 exposes (we fix what the tightening surfaces in this plan; future drifts are ordinary follow-ups).

## 1. Goal & Motivation

Two small, orthogonal clean-ups bundled into one plan because either one alone is under the plan threshold and both close the last two "Primary track" follow-ups from the 21st-pass SEED Resumption Point that didn't land in `test-infra-followups`.

**Phase A (clippy-test-drift).** CI's Rust clippy steps in `ci-wacp.yml:54–62` and `ci-console.yml:71–76` invoke `cargo clippy -p <crate>` **without** `--all-targets`, which only compiles the default lib / bin targets and skips `#[cfg(test)]` modules + separate `tests.rs` siblings. Six errors have accumulated behind that gate — 5 in `wacp-runtime/src/tests.rs` (two `prometheus::proto::MetricFamily::get_name` deprecations + `clippy::single_match` + `clippy::collapsible_match` + `clippy::collapsible_if`) and 1 in `console-db/src/queries/tests.rs` (`clippy::module_inception`). Left untightened, this class of drift will keep re-accumulating; anyone running the workspace-wide invocation locally (e.g., to satisfy a future plan's acceptance criterion) will eat the same surprise.

**Phase B (closed-terminal-coverage).** The 2026-04-18 §11.4 P0 enum-offset sweep exposed `terminal_workspace_closed_marked_completed` (`recovery_matrix.rs:254`) as coincidentally-green — its supposed "terminal Closed" path was actually landing in `Failed` via the cascade from `abort_workspace`, and the off-by-one cast at `WorkspaceView.state` had been aliasing wire `Failed` to proto `Closed`. Post-fix the test was renamed to `terminal_workspace_aborted_marked_failed` and flipped to assert `FAILED`, which matches actual behaviour. The Closed-terminal → COMPLETED branch at `recovery.rs:173–175` is now uncovered at integration scope — covered only by the unit-level test `wa3_6_complete_signal_drives_workspace_to_closed` in-crate.

Cost of inaction: the branch is load-bearing for the recovery contract — a session that reached `WorkspaceState::Closed` before restart must mark COMPLETED. A future refactor could regress it silently; only unit-scope coverage stands in the way. This plan adds a test-only `mark_workspace_closed` helper to `RuntimeHarness` so the integration test can drive the path without needing a real agent + Complete signal + WA3.6 auto-integration cascade.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| A.1 | Fix 6 test-only clippy errors (§16.1 × 5, §16.2 × 1) | ~30 min | — | `cargo clippy --workspace --all-targets -- -D warnings` green locally |
| A.2 | Tighten both CI clippy invocations to `--all-targets` + fix anything else it surfaces | ~30–60 min | A.1 | `ci-wacp` + `ci-console` clippy jobs green on draft-PR push |
| B.1 | Add `RuntimeHarness::mark_workspace_closed(ws_id)` test-only helper | ~45–60 min | — | Helper compiles; existing `cargo test -p console-integration` still green |
| B.2 | Add `terminal_workspace_closed_marked_completed` integration test (re-taking the original name, correctly this time) | ~30 min | B.1 | `cargo test -p console-integration --test recovery_matrix` 8/8 (was 7) |
| C | Closeout: HEALTH-LOG §16 / §11.4 follow-up strikes, AUDIT append, plan archive, SEED refresh | ~30 min | A + B | Plan moved to `impl/archive/`; HEALTH-LOG entries struck; dev tip advanced |

## 3. Deliverables — per phase

### 3.1 Phase A.1 — fix 6 test-only clippy errors

**File-attribution correction from HEALTH-LOG §16.1.** The 2 `get_name` deprecations live in `metrics.rs` inside its own file-local `#[cfg(test)]` module (starting L193), **not** `tests.rs` as §16.1 stated. The error-count (5 in wacp-runtime) stands; only the location was mis-attributed. Fix the log in Phase C closeout.

Files + fixes:
- `wacp/crates/wacp-runtime/src/metrics.rs` (inside the `#[cfg(test)]` module at L193+):
  - L214 — `f.get_name()` → `f.name()` on `prometheus::proto::MetricFamily`.
  - L242 — same (second call site in a sibling test).
- `wacp/crates/wacp-runtime/src/tests.rs`:
  - L117 — collapse `match event { WorkspaceEvent::StateChanged { to, .. } => { ... }, _ => {} }` to `if let WorkspaceEvent::StateChanged { to, .. } = event { ... }` (kills both `clippy::single_match` and `clippy::collapsible_match` at the same site).
  - L146 — collapse `if let WorkspaceEvent::StateChanged { to, .. } = &event { if *to == WorkspaceState::Failed { ... } }` via `&&` let-chain. Clippy's auto-suggestion output already confirms the syntax is accepted on the current toolchain (rust-1.94.0) — Risk #3 resolved.
- `wacp-console/crates/console-db/src/queries/tests.rs:2` — the file is included from `queries/mod.rs:11` via `mod tests;` (no `#[cfg(test)]` on the `mod` line). Drop the outer `mod tests { ... }` wrapper; hoist contents to file scope; attach `#![cfg(test)]` at file top (inner attribute) so the gate still applies. Verify no callers reference `crate::queries::tests::tests::...` (double-`tests`) — should be zero.

Verification: `cargo clippy --workspace --all-targets -- -D warnings` returns clean; `cargo test -p wacp-runtime` + `cargo test -p console-db` still pass at pre-change counts.

### 3.2 Phase A.2 — tighten CI clippy + sweep whatever surfaces

Edits:
- `.github/workflows/ci-wacp.yml:54–62` — add `--all-targets` to the `cargo clippy` invocation.
- `.github/workflows/ci-console.yml:71–76` — add `--all-targets` to the `cargo clippy` invocation.

Push to draft PR against `main` (CI trigger scope — per user memory, scratch-branch pushes don't fire CI on this repo). If the tightened jobs surface additional test-only drift not already enumerated in §16, fix at source in-phase; do not allow-list. If a surfaced drift is structurally large (>30 min to fix) stop and reassess — it may warrant carving out to its own plan rather than bloating A.2.

Verification: `ci-wacp` clippy job + `ci-console` clippy job green on draft-PR push SHA.

### 3.3 Phase B.1 — `RuntimeHarness::mark_workspace_closed` helper

Target file: `wacp-console/integration/src/runtime_harness.rs`. Add a test-only method that lets an integration test force a workspace into internal `WorkspaceState::Closed` without routing through an agent Complete signal + WA3.6 auto-integration:

```rust
// Pseudocode — shape only; exact wiring depends on WorkspaceTree visibility.
impl RuntimeHarness {
    #[cfg(any(test, feature = "test-helpers"))]
    pub async fn mark_workspace_closed(&self, ws_id: &WorkspaceId) -> Result<()> {
        // Reach into the runtime child's WorkspaceTree — likely via a test-only
        // gRPC method or an admin RPC that already exists. If neither, add a
        // minimal test-only Coordinator surface.
    }
}
```

Open mechanism questions (resolve in-phase):
1. Does `WorkspaceTree::get_mut(...).status = Closed` need crate-level visibility widening? Check `wacp-coordinator/src/tree.rs`.
2. Can we reach the coordinator state from the harness without cross-process plumbing, given the runtime runs as a child binary? If not, a test-only gRPC admin method may be the cleanest path — scope-creepy, flag + reassess before committing.
3. Fallback option (b) from §11.4: if the helper turns out to require >1h of plumbing, stop and accept the existing unit-level coverage at `wa3_6_complete_signal_drives_workspace_to_closed` as sufficient. Document the decision in the HEALTH-LOG §11.4 strike line + close this phase with a "deferred, rationale recorded" note.

Verification: `cargo check -p wacp-integration-tests --tests` compiles; `cargo test -p wacp-integration-tests` pre-change test count unchanged.

### 3.4 Phase B.2 — `terminal_workspace_closed_marked_completed` integration test

Add to `wacp-console/integration/tests/recovery_matrix.rs` (reclaiming the name freed up by the 2026-04-18 rename — the new test correctly exercises the path the old name implied):

1. Seed session ACTIVE with a workspace ID.
2. Spawn runtime, obtain harness, drive to a point where the workspace exists in the tree.
3. Call `harness.mark_workspace_closed(ws_id)`.
4. Run `recovery::run(&db, &pool, config)`.
5. Assert: session state = `COMPLETED`; `RecoveryReport.completed` incremented; `active_sessions` map does not contain this session.

Verification: `cargo test -p wacp-integration-tests --test recovery_matrix` is 8/8 (was 7/7).

### 3.5 Phase C — closeout

- HEALTH-LOG §16.1, §16.2, §16.3, §16.4 — strike-through with resolution note + commit SHA.
- HEALTH-LOG §11.4 follow-up paragraph ("Follow-up (small): either add a `mark_workspace_closed` ... or accept the unit-level coverage ...") — resolve with chosen option + SHA.
- AUDIT-2026-04-15.md — append closure entries under §13.9 (consistent with §13.9.4–§13.9.7 test-infra-followups style); strike §13.9.3 if it exists for §16, otherwise add.
- Archive plan: `impl/test-cleanup-followups-plan.md` → `impl/archive/test-cleanup-followups-plan.md` via `archive-plan` skill.
- SEED refresh at batch boundary (22nd pass) per `seed-refresh` skill: only after ff'd to main (or explicit user decision to refresh at dev-only landing).

## 4. Acceptance Criteria

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` green locally on the final tip.
- [ ] `cargo test -p wacp-runtime` passes at pre-change count.
- [ ] `cargo test -p console-db` passes at pre-change count.
- [ ] `cargo test -p wacp-integration-tests --test recovery_matrix` is 8/8 (or 7/7 + documented B.1 fallback decision).
- [ ] `ci-wacp` clippy job green with `--all-targets` on a push SHA.
- [ ] `ci-console` clippy job green with `--all-targets` on a push SHA.
- [ ] HEALTH-LOG §16.1–§16.4 struck through with resolution note + SHA.
- [ ] HEALTH-LOG §11.4 follow-up paragraph resolved (either with new test landed or with documented fallback rationale).
- [ ] AUDIT-2026-04-15.md §13.9 has closure entries for this plan.
- [ ] Plan file moved to `impl/archive/test-cleanup-followups-plan.md`.

## 5. Risks / Open Questions

1. **A.2 may surface additional drift.** The `--all-targets` tightening could expose more than the 6 enumerated errors — e.g., benchmarks, integration test crates, other `tests.rs` siblings that have never been linted. Mitigation: fix at source in-phase if small; stop and reassess if large (>30 min).
2. **B.1 mechanism may require scope-creep.** If `WorkspaceTree` state can't be externally forced from the harness, adding a test-only admin RPC is scope-creep beyond a single plan phase. Explicit fallback: option (b) from §11.4 — accept unit-level coverage + close with documented rationale.
3. ~~**Let-chain stabilisation (A.1 L146 fix).**~~ **Resolved** — clippy's auto-suggestion on the current toolchain (rust-1.94.0 per error URL; `rust-toolchain.toml` pins `stable`) already produced the let-chain syntax, so no fallback needed.
4. **§13.9.3 slot.** AUDIT §13.9.3 was the original slot this work was hinted at in the 21st-pass SEED ("open follow-up 7"). Verify it exists / hasn't been re-used before appending closure rows.
5. **Single-maintainer branch protection.** `dev` has `required_linear_history=true` — the ff back to dev from the topic branch must be fast-forward only. No issue if phases land sequentially.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| `HEALTH-LOG` §16 | Clippy drift in test-only code, unflagged by CI | implements (closes §16.1–§16.4) |
| `HEALTH-LOG` §11.4 | The same enum-offset trap, second instance — follow-up paragraph | implements (closes trailing coverage follow-up) |
| `AUDIT-2026-04-15` §13.9 | Post-audit closure log | extends (appends closure rows) |
| `impl/archive/test-infra-followups-plan.md` | Prior four-phase bundle plan | informs (structural precedent + Phase-C pattern) |
| `impl/git-strategy.md` §4 | Topic-branch naming | constrains (`testing/cleanup-followups`) |
| `wcon-mutation-testing` | Mutation-testing spec | informs (A.2 tightening mirrors §13.7.9's "loud-fail rather than silent-drift" posture) |

## 7. Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| A.1 | `f04f404` | 2026-04-22 | **Risk #1 fired.** §16 enumerated 6 errors; the full `--all-targets` run surfaced 23 across 11 files (wacp-runtime, wacp-coordinator, wacp-workspace, wacp-trail, wacp-transport, wacp-integration-tests, console-db). All fixed in-phase, no allow-lists. Categories: 2 `prometheus::get_name` deprecations (file-attribution corrected from tests.rs → metrics.rs #[cfg(test)] mod), 2 `single_match` + 2 `collapsible_match`, 8 `collapsible_if` (mostly `&&`-let-chain folds), 2 `redundant_guards` (pattern-binding), 1 `needless_borrows_for_generic_args`, 1 `useless_conversion`, 2 `unused_must_use` (real missing `.await` on `coord.handle_event` — tests were coincidentally-green), 1 unused import, 3 `field_reassign_with_default`, 2 `len_zero`, 1 `module_inception` (dedent). Test counts unchanged: wacp-runtime 109/109, wacp-coordinator 387/387, console-db 96/96. |
| A.2 | `3125da1` | 2026-04-22 | `--all-targets` added to both `ci-wacp.yml` + `ci-console.yml` clippy steps. Draft PR #10 CI run verified: `Rust — build, clippy, test` (ci-wacp) green at 6m23s; `Rust — console crates` (ci-console) green at 2m23s. All other workflows unaffected (coverage, frontend, e2e, integration, python, ts verticals, fmt, deny, mutation-scripts, gitguardian, file-size — all pass). The tightening caught no surprise drift since A.1 cleaned every `--all-targets` error locally before push — A.2 is the gate, A.1 was the cleanup. |
| B.1 | `537a554` | 2026-04-22 | **Plan deviation.** Original §3.3 called for `RuntimeHarness::mark_workspace_closed(ws_id)` pokey-method shape. Reality: `RuntimeHarness` spawns wacp-runtime as a **child process** (see its module docstring) — in-process state-poking unreachable without a test-only admin gRPC (Risk #2's scope-creepy branch). Short-path helper found in `lifecycle.rs:311–321`: `wacp_sdk::Agent::connect(...).signal(Complete)` → runtime event loop fires WA3.6 auto-integration → workspace reaches `Closed` through the real code path. Kept helper local to `recovery_matrix.rs` since only recovery tests need it. |
| B.2 | `537a554` | 2026-04-22 | Combined with B.1 in a single commit — the helper and its sole consumer are tightly coupled; splitting would require temporary `#[allow(dead_code)]` on the helper (which `-D warnings` would block anyway). `cargo test -p console-integration --test recovery_matrix` now **8/8** (was 7/7). Plan-described crate `wacp-integration-tests` was wrong in §4 acceptance criterion — the right crate is `console-integration`. |
| C | — | — | — |
