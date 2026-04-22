---
id: wacp-health-log-residual-plan
type: impl
status: draft
created: 2026-04-22T17:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, cleanup, bench, docs, audit]
depends_on: [wacp-test-cleanup-followups-plan]
---

# HEALTH-LOG Residual Cleanup — Plan

> **Triggering findings:** HEALTH-LOG §8 (`session_launcher_bench` placeholder — blocked on `InjectableCoordinator` relocation) + §11.2 (exhaustive-match propagation comment recommendation) + §13.4 (gap-fill replay scenario — explicit "strike from audit" recommendation per `wcon-highway` §4.3).
> **Target branch:** `cleanup/health-log-residual` (topic).
> **Rough effort:** ~1–2h — **high** confidence. All three items are small, well-scoped, and orthogonal.
> **Not in scope:** `MockHighwayService` + `FaultyDb::drop_reads` + §13.2 deferred scenarios (that's the parallel plan `integration-deferred-scenarios-plan.md`), §14.1 cross-harness TOCTOU residual (never observed), §13.5 context_schema evolution (explicitly gated on "if that becomes a priority"), §13.7.10 Codecov ratchet (scheduled).

## 1. Goal & Motivation

Close the three smallest residual HEALTH-LOG follow-ups that survived the test-cleanup-followups closeout. Each is a single-commit task on its own and below the new-plan skill's "~3 commits → skip plan" threshold individually, but bundling them into one plan keeps the HEALTH-LOG cleanup momentum and hits a clean "every small thing closed" state before any larger infrastructure work starts. The payoff is mainly narrative — future sessions walk into a HEALTH-LOG where every non-strategic item is either resolved or explicitly deferred with an owner.

**Phase P1 (bench-unblock).** `docs/perf-baseline-2026-04-20.md` already calls out the blocker: "`session_launcher_bench` — placeholder only. A real benchmark of SubmitGoal → Decompose(N) → Dispatch(N) needs the `InjectableCoordinator` mock from `wacp-console/integration` (currently not a dev-dep chain available from `console-core`). Either (a) move the mock to `console-test-support` or (b) duplicate a minimal stub inline. Either is ~30–60 min; deferred." Option (a) is the right call — `InjectableCoordinator` is 296 lines of reusable infrastructure (per `wacp-console/integration/src/mock_coordinator.rs`) and a second copy would rot on the first proto change.

**Phase P2 (propagation-comment).** §11.2 documented a structural expectation: "`event_enricher::gate_type_string` is intentionally exhaustive; if a new `GateType` variant lands in `wacp-types`, this match must extend in the same PR." That expectation currently lives only in HEALTH-LOG — the `match` sites themselves carry no hint. A short `// exhaustive on proto::GateType — extend when wacp-types enum grows` comment above each site surfaces the invariant at the point of code. Cheap insurance against the next `_ =>` silently-wrong stringification.

**Phase P3 (gap-fill strike).** §13.4 surfaced that AUDIT §13.7.8 I4's "gap-fill replay" deliverable targets a `GET /api/sessions/:id/trail?since=<seq>` endpoint that does not exist. §13.4's own recommendation: "wcon-highway.md §4.3 already describes the `control/lag` frame as 'authoritative signal to client that it must refresh state via its own strategy' — there's no spec commitment to a server-driven replay." AUDIT row 336 + §13.7.8 closeout notes still list "gap-fill replay" as a scenario; update them to reflect the accepted spec-level answer.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| P1 | Relocate `InjectableCoordinator` → `console-test-support`; re-export from `console-integration` for caller back-compat; wire `session_launcher_bench`'s SubmitGoal→Decompose→Dispatch sweep | ~45–60 min | — | `cargo bench -p console-core --bench session_launcher_bench` emits real Criterion numbers (not the placeholder); `cargo test -p console-integration --test mock_coordinator_smoke` still 2/2 |
| P2 | Add propagation-comment above every exhaustive `match` on a `proto::*` enum variant in console-core + console-api | ~20–30 min | — | `grep -rn "^// exhaustive on proto::" wacp-console/crates/console-*` lists every relevant site; clippy still clean |
| P3 | Strike "gap-fill replay" from AUDIT §13.7.8 I4 row + §13.7.8 closeout list; optional one-line clarification in `wcon-highway` §4.3 noting server-side replay is out of scope | ~15–30 min | — | AUDIT §13.7.8 I4 row no longer lists gap-fill replay; HEALTH-LOG §13.4 strikes its open-option-1 prose with a pointer to this plan |
| P4 | Closeout: HEALTH-LOG §8 / §11.2 / §13.4 strikes, AUDIT §13.9.9 (new) closure entry, plan archive | ~20 min | P1 + P2 + P3 | Plan archived to `impl/archive/`; all 3 strikes in place; `cargo clippy --workspace --all-targets -- -D warnings` still clean |

## 3. Deliverables — per phase

### 3.1 Phase P1 — InjectableCoordinator relocation

**Move.** `wacp-console/integration/src/mock_coordinator.rs` (296 lines) → `wacp-console/crates/console-test-support/src/mock_coordinator.rs`. Use `git mv` to preserve blame. Add `pub mod mock_coordinator;` to `console-test-support/src/lib.rs` (alongside existing `mock_grpc`, `mock_rest`, `mock_runtime`, `programmable_coordinator` modules).

**Back-compat re-export.** `wacp-console/integration/src/lib.rs:27` currently has `pub use mock_coordinator::InjectableCoordinator;` — change the module declaration from `pub mod mock_coordinator;` to `pub use console_test_support::mock_coordinator::{self, InjectableCoordinator};` so existing `use console_integration::{InjectableCoordinator, RuntimeHarness};` call sites don't break. Verify via `grep -rn "InjectableCoordinator" wacp-console/integration/tests/` — every hit should still resolve.

**Deps.** `console-test-support` already pulls `tonic`, `tokio-stream`, `wacp-transport` transitively (check `wacp-console/crates/console-test-support/Cargo.toml`); if not, port the dep manifest entries from `wacp-console/integration/Cargo.toml`. Note: the integration crate had to move those from `[dev-dependencies]` to `[dependencies]` when the mock was added — per HEALTH-LOG §13.6: "Cargo.toml edit — moved `tonic`, `tokio-stream`, `wacp-transport` from `[dev-dependencies]` to `[dependencies]` (the new mock lives in `src/`)." Same shape applies for console-test-support.

**Bench wiring.** `scripts/bench-baseline.sh:23` runs `cargo bench -p console-core --bench session_launcher_bench`. The bench file (check `wacp-console/crates/console-core/benches/session_launcher_bench.rs` — may be a placeholder) needs to import `console_test_support::InjectableCoordinator`, drive a SubmitGoal → Decompose(N) → Dispatch(N) sequence, and record `launch_session` walltime across N={1,3,10,30}. Regression tripwire: write into `docs/perf-baseline-2026-04-20.md` as a new row.

Verification: `cargo bench -p console-core --bench session_launcher_bench` runs clean with real numbers (not the placeholder). `cargo test -p console-integration --test mock_coordinator_smoke` still 2/2. `cargo clippy --workspace --all-targets -- -D warnings` clean.

### 3.2 Phase P2 — propagation-comment above exhaustive proto::* matches

**Identify sites.** The HEALTH-LOG §11.2 example is `console-core::event_enricher::gate_type_string`. Verify with:
```
rg -n "match.*proto::(GateType|WorkspaceState|TaskStatus|SignalType|EnvelopeState|EnvelopePriority|EnvelopeOrigin|CheckpointStatus|Confidence|BaseRole|MergeStrategy|IntegrationMode|ConflictType|ResolutionStrategy|PortRightType|GateDecision|TrailScope|StorageTier|ErrorCategory)" wacp-console/crates
```
Any such match without a `_ =>` wildcard is exhaustive and in scope. The initial grep (this plan's scaffolding session) returned 0 hits with the above pattern — either the matches bind via `let proto: proto::X = ...` before the `match`, or they live in paths the pattern missed. A fresh session should re-grep with a looser pattern (`rg "match.*GateType|match.*WorkspaceState|match.*TaskStatus" wacp-console/crates/console-core/src`) before concluding.

**Comment shape.** Above each exhaustive match, add:
```rust
// Exhaustive on `proto::<Enum>` — extend when `wacp-types::<Enum>` gains a
// variant. Build-break is intentional; see HEALTH-LOG §11.2.
```

**Do not change** the match bodies. Just the comment above the `match` expression.

Verification: `grep -rn "Exhaustive on \`proto::" wacp-console/crates/console-*/src` lists every relevant site. `cargo clippy --workspace --all-targets -- -D warnings` clean (comments don't affect clippy, but re-verify).

### 3.3 Phase P3 — strike gap-fill replay from AUDIT §13.7.8

**AUDIT §13.7.8 I4 row** (`AUDIT-2026-04-15.md:336` per the scaffolding grep):
```
| I4 | `ws_chaos.rs` | Already partially there; add: broadcast backpressure (256-cap exhaustion), abrupt disconnect mid-event, malformed frame injection, gap-fill replay |
```
Strike "gap-fill replay" with `~~...~~` and append the rationale:
```
| I4 | `ws_chaos.rs` | Already partially there; add: broadcast backpressure (256-cap exhaustion), abrupt disconnect mid-event, malformed frame injection, ~~gap-fill replay~~ (endpoint doesn't exist; `wcon-highway` §4.3 treats `control/lag` as authoritative signal for client-side catch-up; no spec commitment — struck 2026-04-?? via `impl/archive/health-log-residual-plan.md` P3) |
```

**AUDIT §13.7.8 closeout list** (line 714): the existing prose already documents the recommendation ("gap-fill replay — `GET /api/sessions/:id/trail?since=<seq>` endpoint **does not exist**. Recommended: strike from the audit..."). Update to indicate the recommendation has been accepted and dated.

**AUDIT §13.9 table.** Add row `13.9.9` for this plan's closure (consistent with §13.9.1..§13.9.8 pattern; effectively the "parent" row for all three phases).

**Optional `wcon-highway` §4.3 clarification.** If the spec is terse on client-side catch-up, add one line: "Server does not provide a replay endpoint; clients must reconcile state from the next full snapshot via their own reconnect strategy." Only if it adds value — skip if §4.3 already says this.

Verification: `grep -n "gap-fill replay" AUDIT-2026-04-15.md` shows struck text + rationale note.

### 3.4 Phase P4 — closeout

- **HEALTH-LOG §8** — strike the "Placeholder: `session_launcher_bench` needs ..." paragraph with resolution note + P1 SHA.
- **HEALTH-LOG §11.2** — strike the "Recommendation" paragraph with a pointer to P2's commit (or add a sentence noting the propagation comments landed).
- **HEALTH-LOG §13.4** — strike option 1 ("Build it.") with a note that option 2 was accepted; the deferral wrapper around the scenario remains historical record.
- **AUDIT §13.9** — add row `13.9.9` closure entry.
- **AUDIT footer** — append `§13.9.9 closed 2026-04-?? by Claude Opus 4.7 (1M context) via impl/archive/health-log-residual-plan.md (3 phases)`.
- **Plan archive** — `archive-plan` skill.
- **SEED refresh** — wait for next batch boundary (ff-main cycle that carries this plan + any parallel plan). Not in this plan's scope.

## 4. Acceptance Criteria

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo bench -p console-core --bench session_launcher_bench` emits real Criterion output (not placeholder).
- [ ] `cargo test -p console-integration --test mock_coordinator_smoke` 2/2.
- [ ] Every exhaustive `match` on a `proto::*` enum in `console-core` + `console-api` carries the `// Exhaustive on proto::...` propagation comment.
- [ ] AUDIT §13.7.8 I4 row + closeout list have "gap-fill replay" struck with rationale.
- [ ] HEALTH-LOG §8 + §11.2 + §13.4 struck with resolution notes + commit SHAs.
- [ ] AUDIT §13.9.9 closure row + footer updated.
- [ ] Plan moved to `impl/archive/health-log-residual-plan.md`.

## 5. Risks / Open Questions

1. **Dep cycle risk (P1).** `console-test-support` is a dev-only crate. Moving `InjectableCoordinator` there may require elevating some deps from `[dev-dependencies]` to `[dependencies]`, which could pull `tonic`/`tokio-stream` into the production path unnecessarily. Mitigation: check if `console-test-support` is already marked as a test-only crate via `publish = false` or similar; if so, the distinction doesn't matter at runtime. If a full `tonic` pull-in is introduced, reassess — alternative is option (b) from the baseline doc (duplicate a minimal stub inline for the bench only).
2. **Grep coverage gap (P2).** The scaffolding grep for exhaustive proto-enum matches returned zero hits, suggesting the matches may use bind-then-match syntax (`let proto = ...; match proto { ... }`) or live in paths the initial pattern missed. A fresh session must re-grep with broader patterns before concluding "zero sites to comment." If truly zero sites exist, skip P2 with a rationale note rather than fabricating commentary.
3. **wcon-highway spec edit boundary (P3).** Editing protocol specs (under `wacp-protocol` sibling repo) is a different ceremony — they're CC BY-SA 4.0 and have their own review. If §4.3 already implies no-server-replay, the optional clarification is genuinely optional. Avoid opening a sibling-repo PR just for this plan.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| `HEALTH-LOG` §8 | Backend baseline — session_launcher_bench placeholder | implements (closes placeholder paragraph) |
| `HEALTH-LOG` §11.2 | Cross-crate exhaustive matches on shared enums | implements (lands the Recommendation comment) |
| `HEALTH-LOG` §13.4 | I4 ws_chaos — gap-fill replay deferral | implements (accepts option 2 from the two resolutions) |
| `AUDIT-2026-04-15` §13.7.8 | Integration I1–I5 closeout | extends (adjusts I4 row + closeout) |
| `AUDIT-2026-04-15` §13.9 | Post-audit follow-ups | extends (appends §13.9.9 row) |
| `docs/perf-baseline-2026-04-20.md` | Backend bench baseline | extends (adds session_launcher numbers) |
| `impl/archive/test-cleanup-followups-plan.md` | Prior test-infra cleanup plan | informs (structural precedent for multi-phase closure) |
| `impl/git-strategy.md` §4 | Topic-branch naming | constrains (`cleanup/health-log-residual`) |

## 7. Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| P1 | — | — | — |
| P2 | — | — | — |
| P3 | — | — | — |
| P4 | — | — | — |
