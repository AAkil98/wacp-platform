---
id: wcon-console-db-schema-alignment-plan
type: impl
status: draft
created: 2026-04-20T16:43:30
revised: 2026-04-20T17:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, console-db, schema-drift, api-contract]
depends_on: [wcon-data-model, wcon-sessions, wcon-profiles]
---

# Console-DB Schema Alignment — Plan

> **Triggering finding:** HEALTH-LOG §9.1 + §9.2 — two distinct API lies in `wacp-console/crates/console-db/queries/` surfaced by the §13.7.5 coverage sweep. Open since ~2026-04-17.
> **Target branch:** `refactor/console-db-schema-alignment` per `git-strategy.md` §4.
> **Rough effort:** ~2–3 h — medium confidence. Bumped from 1.5–2 h after recon found that §9.1 resolution has a downstream caller (`sessions.rs:159`) whose current behaviour is silently broken (`.ok()` swallows a `NotNullViolation` on every session create), widening the §9.1 scope to "pick a resolution AND fix the swallowed-error call site".
> **Not in scope:** broader sqlx / migration hygiene (other tables' structs-vs-schema alignment, schema dump + diff tooling, migration ordering review) — file separately if needed. This plan is strictly §9.1 + §9.2, and only the call sites their resolutions touch.

## 1. Goal & Motivation

The `console-db` crate's public API claims two things that aren't true:

1. `SessionAssignmentRow::profile_id` and `profile_version` are typed `Option<T>`, but migration 007 declares both columns `NOT NULL`. A caller that sets either field to `None` gets a runtime `NotNullViolation` at `INSERT` time, not a compile error. Worse, a real caller exists at `wacp-console/crates/console-api/src/routes/sessions.rs:159` — the session-creation path's slot-auto-derivation — which inserts placeholder rows with `profile_id: None` and uses `.ok()` to silently swallow the resulting `NotNullViolation`. So the API lies AND a production call site is silently broken.

2. `profiles::max_version` returns `Result<Option<i64>, sqlx::Error>`, but the `Option` discriminant is always `Some` — SQLite's `MAX()` over an empty set returns NULL, which sqlx decodes to `i64 = 0`. The caller cannot distinguish "no profile exists" from "profile exists at version 0". Every current caller does `.unwrap_or(0) + 1`, so the sentinel is already load-bearing.

Cost of not fixing: the `console-db` crate is the narrowest waist in the backend — console-core, console-api, and every integration test route data through it. As more callers land, each one re-encounters the same API ambiguity. §9.1's silently-broken call site will keep doing nothing, which means session launches can't use auto-derived slots (they rely on the frontend sending fully-populated slot rows). §9.2's sentinel blocks any future caller who needs "does this profile exist" separately from "what version is it at".

Both resolutions are small and can ship together on one topic branch.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| A — Decision memo | §3.1 + §3.2 below, filled in with chosen resolution paths + caller audit for each | ~30–45 min | — | §3.1/§3.2 sections say "Chosen: path X because Y"; caller list complete |
| B — §9.1 resolution (Path A) | Tighten `SessionAssignmentRow::profile_id`/`profile_version` to non-Optional, drop `sessions.rs:155–175` dead block, ripple-remove `derive_slots` + its 2 tests, drop `count_assigned`'s dead `IS NOT NULL` clause, update 6 caller sites, retire one coverage test | ~45–60 min | A | `cargo test -p console-db` green; `sessions.rs:159` block gone; `derive_slots` removed; HEALTH-LOG §9.1 struck through with resolution pointer |
| C — §9.2 resolution (Path C) | Delete `profiles::max_version` + 1 happy-path test + 1 pool-closed assertion | ~15–25 min | A | `cargo test -p console-db` green; zero `max_version` references in the tree; HEALTH-LOG §9.2 struck through with resolution pointer |
| D — Verify + close | Full workspace test pass (cargo test -p console-db + console-core + console-api + console-integration subset), clippy, fmt. AUDIT pointer added. Archive plan. | ~20–30 min | B+C | 190+ console-core lib tests still green; workspace clippy -D warnings clean; AUDIT-2026-04-15.md has a follow-up table entry |

## 3. Deliverables — per phase

### 3.1 Phase A — Decision memo (§9.1 + caller audit)

**Options (restated from HEALTH-LOG §9.1):**
- **Path A: tighten the struct.** `SessionAssignmentRow::profile_id: String`, `profile_version: i64`. Schema is source of truth; caller can't represent an invalid state. `count_assigned`'s `WHERE profile_id IS NOT NULL` dead clause removed. `sessions.rs:159` auto-slot-derivation must change — either remove it entirely (slots are only created when the frontend sends fully-populated rows) or switch to a separate "placeholder slot" table / nullable-FK scheme.
- **Path B: loosen the schema.** Migration that drops `NOT NULL` on `profile_id` + `profile_version`. Struct stays Optional. `count_assigned`'s clause becomes load-bearing. `sessions.rs:159` auto-slot-derivation actually starts working (placeholder rows persist + appear in `list_by_session`).

**Decision record (locked 2026-04-20):**
> Chosen: **Path A — tighten the struct.**
>
> Reasoning: spec `wcon-data-model` §4.2 line 294 reads `profile_version INTEGER NOT NULL — pinned version at assignment time`; spec is the source of truth and Path A is the spec-compliant choice. Path B would require revising the spec, which is out of scope. The `sessions.rs:155–175` auto-derivation block is already a no-op today (every `insert_assignment` returns `NotNullViolation`, `.ok()` swallows, zero rows persist) — so removing it under Path A changes no observable behaviour. Path A makes the code truthful without changing semantics.
>
> Caller audit (verified via `grep 'SessionAssignmentRow\s*\{'`):
>
> | Site | Current | Change under Path A |
> |---|---|---|
> | `wacp-console/crates/console-api/src/routes/sessions.rs:159` — `create_session` auto-slot | `profile_id: None`, `.ok()`'d | Delete the whole block at lines 155–175 (derive_slots call + insert loop). Already a no-op; no behaviour change. |
> | `wacp-console/crates/console-api/src/routes/sessions.rs:370` — `set_assignments` | `profile_id: Some(input.profile_id.clone())`, `profile_version: Some(version)` | Drop the `Some(...)` wrappers. |
> | `wacp-console/crates/console-api/src/routes/sessions.rs:773` — `clone_session` | `profile_id: a.profile_id.clone()`, `profile_version: a.profile_version` | **Zero line changes.** Types flip `Option<String>`/`Option<i64>` → `String`/`i64`; `.clone()` still valid on `String`, `i64` is `Copy`. |
> | `wacp-console/crates/console-core/src/recovery_tests.rs:1029, 1042` | `profile_id: Some("…".into())` | Drop `Some(...)` wrappers. |
> | `wacp-console/crates/console-core/src/session_launcher_tests.rs:36` | helper param `profile_id: Option<&str>` → `profile_id.map(String::from)` | Change param to `profile_id: &str`, `.into()` directly. All callers already pass concrete values — confirmed via grep. |
> | `wacp-console/crates/console-db/src/queries/coverage_tests.rs:85` — `sample_assignment` helper | `profile_id: Some(profile_id.into())`, `profile_version: Some(profile_version)` | Drop `Some(...)` wrappers. |
> | `wacp-console/crates/console-db/src/queries/coverage_tests.rs:1500` — `not_null_violation_when_profile_id_is_none` | exercises `r.profile_id = None` | Delete the test. Field is no longer nullable → compile-time enforced. Replacement negative test not needed: `fk_session_violation` at `:1515` and `fk_owner_user_violation` still exercise adjacent invariants; region-coverage impact on T11's 98.3 % baseline is negligible (~12-line single-error-kind test). |
> | `wacp-console/integration/tests/launch_failure_matrix.rs:393`, `chaos.rs:316` | fixtures with `profile_id: Some(...)` | Drop `Some(...)` wrappers. |
>
> Ripple — also in Phase B: `wacp-console/crates/console-core/src/session_validation.rs:251-261` `derive_slots` loses its only production caller (grep confirms `sessions.rs:157` is the sole non-test site). It has 2 tests at `session_validation.rs:401, 410`. **Remove the function + its tests** in the same commit. An orphan `pub fn` with a suggestive name invites a future caller to re-wire it and re-introduce the exact bug we're fixing.
>
> Other struct fields audited against migration 007 — `stage_id`, `workspace_id`, and the three `budget_*` fields are legitimately nullable in the schema. No other drift; §9.1 scope is strictly `profile_id` + `profile_version`.
>
> Placeholder-row reader check — confirmed safe: `list_by_session` has no GET endpoint that depends on pre-populated empty slots. The wizard flow is `POST /api/sessions` → 0 assignments → frontend calls `PUT /api/sessions/:id/assignments` (`set_assignments`) with explicit rows. The pre-existing auto-derivation block was belt-and-suspenders that never did anything because the insert always failed.

### 3.2 Phase A — Decision memo (§9.2 + caller audit)

**Options (restated from HEALTH-LOG §9.2):**
- **Path A: wrap the tuple.** `sqlx::query_as::<_, (Option<i64>,)>(...)` → `Result<Option<(Option<i64>,)>, _>` then `Ok(row.and_then(|(v,)| v))` → `Result<Option<i64>, sqlx::Error>`. Inner `Option` is the NULL discriminant (missing row), outer `Option` is "row exists but aggregate is NULL" (same thing, hence `.and_then`). Callers learn to handle `None` as "no versions exist".
- **Path B: rename + commit to the sentinel.** Change signature to `next_version(pool, id) -> Result<i64, sqlx::Error>`, returning `max_version + 1` or `1` if no versions exist. Encodes the "+1" pattern every caller already does. `max_version` goes away.

**Decision record (locked 2026-04-20):**
> Chosen: **Path C — delete `max_version` entirely. (Plan deviation; not in the original A/B options.)**
>
> Reasoning: `grep 'max_version|::max_version'` across all `.rs` files surfaced **zero** production callers. The §5 open question — "grep found two hits in `coverage_tests.rs` but none in `profiles.rs` production code — that can't be right" — turns out to be exactly right. The function is orphan. Every profile-version-bump site in production (`routes/profiles.rs:359` edit handler, `:537` rollback handler) computes `current.version + 1` inline, where `current` came from `profiles::get_current(...)`. This is both correct and preferable:
> - One fewer DB roundtrip per save (the caller already has the version in scope).
> - Sidesteps the NULL-aggregate ambiguity entirely because the query isn't aggregate.
> - Matches the spec's concept of "current version" (`wcon-data-model` §3.2 defines `is_current` as the canonical discriminator, not a MAX() scan).
>
> Both original options (Path A wrap-tuple, Path B rename-to-`next_version`) would preserve dead code with a cleaner shape. Deleting eliminates the drift entirely and costs nothing. This is the fewer-moving-parts choice.
>
> Caller audit (verified via `grep 'max_version|::max_version' --type rust`):
>
> | Site | Current | Change under Path C |
> |---|---|---|
> | `wacp-console/crates/console-db/src/queries/profiles.rs:196-202` | `pub async fn max_version(...) -> Result<Option<i64>, sqlx::Error>` | Delete the function + its doc-comment. |
> | `wacp-console/crates/console-db/src/queries/coverage_tests.rs:875-892` | `max_version_covers_present_case` happy-path test | Delete the entire test (~18 lines). |
> | `wacp-console/crates/console-db/src/queries/coverage_tests.rs:970` | single-line `expect_pool_closed(&profiles::max_version(&pool, "p").await.unwrap_err())` inside `closed_pool_errors_on_read_and_write` | Delete that line. The surrounding test covers ~10 other functions — signal preserved. |
>
> Net: −7 lines production, −~19 lines test. Region-coverage impact is near-neutral (removed both numerator and denominator). No consumer surface to deprecate — `max_version` is `pub` inside `console-db` but that crate isn't published outside this workspace.

### 3.3 Phase B — §9.1 resolution (Path A)

Deliverables, in order of edit:

1. **`wacp-console/crates/console-db/src/queries/session_assignments.rs`:**
   - `SessionAssignmentRow::profile_id: String` (drop `Option`).
   - `SessionAssignmentRow::profile_version: i64` (drop `Option`).
   - `count_assigned`: drop `AND profile_id IS NOT NULL` from the SQL. The filter is dead code once the field can't be null. Keep the function name as-is — renaming to `count_for_session` is a cosmetic follow-up not worth conflating with this change.
2. **`wacp-console/crates/console-api/src/routes/sessions.rs`:**
   - Delete lines 155–175 (the `derive_slots` call + the for-loop that inserts placeholder assignments with `.ok()`-swallowed errors). The create_session handler returns with zero assignments; the frontend wizard populates them via `PUT /api/sessions/:id/assignments` (`set_assignments`, existing endpoint).
   - At `:370` — drop `Some(...)` wrappers on `profile_id` and `profile_version`.
   - At `:773` — zero changes (types flip but code already uses `.clone()` / direct `i64` access).
3. **`wacp-console/crates/console-core/src/session_validation.rs` (derive_slots ripple):**
   - Delete `pub fn derive_slots` (lines 251–261) — now orphan with no production caller.
   - Delete its two `#[test]` functions at `:401` (`slot_derivation_happy_path`) and `:410` (`slot_derivation_empty_vertical`).
4. **Test fixtures:**
   - `wacp-console/crates/console-core/src/recovery_tests.rs:1029, 1042` — drop `Some(...)` wrappers.
   - `wacp-console/crates/console-core/src/session_launcher_tests.rs:36` — change helper param `profile_id: Option<&str>` → `profile_id: &str`; body becomes `profile_id: profile_id.into()`.
   - `wacp-console/crates/console-db/src/queries/coverage_tests.rs:85` (`sample_assignment` helper) — drop `Some(...)` wrappers on two fields.
   - `wacp-console/crates/console-db/src/queries/coverage_tests.rs:1500` — delete the `not_null_violation_when_profile_id_is_none` test (invariant now compile-time enforced).
   - `wacp-console/integration/tests/launch_failure_matrix.rs:393` + `chaos.rs:316` — drop `Some(...)` wrappers.
5. **Migration:** none. Path A leaves the SQL schema untouched; the struct bends toward the schema, not the reverse.

Commit message: `fix(console-db): §9.1 — tighten SessionAssignmentRow to match NOT NULL schema`. Body: one-line summary of the deleted sessions.rs block + derive_slots ripple + retired coverage test.

### 3.4 Phase C — §9.2 resolution (Path C — delete)

Deliverables:

1. **`wacp-console/crates/console-db/src/queries/profiles.rs:195-202`** — delete the `/// Get the max version number for a profile.` doc comment and the entire `pub async fn max_version` function.
2. **`wacp-console/crates/console-db/src/queries/coverage_tests.rs:874-892`** — delete the entire `#[tokio::test] async fn max_version_covers_present_case` test (including its `// Note: for a missing profile...` doc comment, which referenced the drift that no longer exists).
3. **`wacp-console/crates/console-db/src/queries/coverage_tests.rs:970`** — delete the single line `expect_pool_closed(&profiles::max_version(&pool, "p").await.unwrap_err());`. The enclosing `closed_pool_errors_on_read_and_write` test remains and keeps its signal.
4. **Migration:** none.
5. **Spec update:** `wcon-data-model` spec doesn't mention `max_version` by name; no spec change needed. `wcon-profiles` §7 (versioning) describes the version bump conceptually but doesn't commit to a specific implementation strategy — no revision needed.

Commit message: `fix(console-db): §9.2 — delete orphan max_version (zero production callers)`. Body: one-line note pointing at the `current.version + 1` inline pattern already used at `routes/profiles.rs:359` + `:537`.

### 3.5 Phase D — Verify + close

1. `cargo test -p console-db` green (all coverage tests, including any newly added or renamed).
2. `cargo test -p console-core` green (≥190 lib tests still pass).
3. `cargo test -p console-api` green.
4. `cargo test -p console-integration` green on a subset of suites that touch session assignments (lifecycle, cross_session, launch_failure_matrix, chaos).
5. `cargo clippy -p console-db -p console-core -p console-api -- -D warnings` clean. **Scope note:** this matches CI's invocation (`ci-console.yml:73-76` — per-crate, no `--all-targets`). The wider `--all-targets` invocation surfaces six pre-existing test-only lint errors unrelated to this plan — see `HEALTH-LOG.md` §16 for triage + follow-up path.
6. `cargo fmt --check` clean.
7. Update `HEALTH-LOG.md`:
   - §9.1: strike through the "pick one" resolution block, replace with "Resolved in commit {SHA}: Path {X}. Call site at `sessions.rs:{line}` also fixed."
   - §9.2: same shape.
8. Update `AUDIT-2026-04-15.md`: add a one-row entry in the §13.5 / §13.8 post-audit follow-up area (or wherever schema-alignment work gets tracked — this isn't an AUDIT §13.7 slot but is post-audit).
9. **No spec update needed.** Path A leaves `wcon-data-model` §4.2 untouched (the spec already committed to `NOT NULL`; the struct bends to match it). Path C deletes a function the spec never named; `wcon-profiles` §7 versioning description is implementation-neutral.
10. Archive via `archive-plan` skill → `impl/archive/console-db-schema-alignment-plan.md`.
11. Ff `refactor/console-db-schema-alignment` → `dev`.

## 4. Acceptance Criteria

- [x] Phase A decision memo complete in §3.1 + §3.2 with chosen path + caller audit.
- [x] HEALTH-LOG §9.1 struck through with resolution pointer (Path A + `2921ecc`).
- [x] HEALTH-LOG §9.2 struck through with resolution pointer (Path C + `a32bd02`).
- [x] `cargo test -p console-db` green — 96/96 (was 98 pre-Phase-B; retired `not_null_violation_when_profile_id_is_none` + `max_version_covers_present_case`).
- [x] `cargo test -p console-core` green — 188/188 (was 190; retired the 2 `derive_slots` tests as ripple of the §9.1 fix).
- [x] `cargo test -p console-api` green — 143/143.
- [x] `cargo test -p console-integration` green — 20/20 across lifecycle (3) + chaos (3) + launch_failure_matrix (10) + cross_session (4).
- [x] `cargo clippy -p console-db -p console-core -p console-api -- -D warnings` clean (matches CI; §16-deferred pre-existing lints not in scope).
- [x] `cargo fmt --check` clean.
- [x] AUDIT-2026-04-15.md has a follow-up entry noting the schema alignment — §13.9.1 + §13.9.2.
- [ ] Plan archived via `archive-plan` skill.
- [ ] Topic branch ff'd to `dev`.

## 5. Risks / Open Questions

Items marked **RESOLVED** below were open questions in the draft; Phase A's caller audit answered them. Residual risks listed afterward.

- ~~**§9.1 Path A breaks silent-swallow behaviour at `sessions.rs:172`.**~~ **RESOLVED.** The `.ok()` block has been a no-op since it landed — every `insert_assignment` call returns `NotNullViolation`, zero rows ever persist. `list_by_session` has no GET-endpoint reader that depends on placeholder rows; the wizard flow populates via `PUT /api/sessions/:id/assignments` with explicit rows. Removing the block changes no observable behaviour.
- ~~**SQLite `ALTER COLUMN` limitations (Path B only).**~~ **N/A** under Path A — no migration needed.
- ~~**Spec vs impl direction.**~~ **RESOLVED.** Spec `wcon-data-model` §4.2 line 294 commits explicitly to `profile_version INTEGER NOT NULL — pinned version at assignment time`. Path A is spec-compliant; no spec revision needed.
- ~~**§9.2 caller count unknown.**~~ **RESOLVED.** Zero production callers. The function was dead; Path C (delete) follows.
- **Coverage test retirement under Path A.** Deleting `not_null_violation_when_profile_id_is_none` (~12 lines, single error-kind) is small against T11's 98.3 % region-coverage baseline. Adjacent FK + autonomy + visibility + duplicate-version tests still exercise the other error kinds. Mitigation: if Phase D measures the numeric drop and it's material (e.g., > 0.2 pp), add a complementary negative test — but expect this not to be needed.

Residual risks for Phase B + C:

- **`derive_slots` deletion widens Phase B blast radius.** Going from "delete 21 lines in sessions.rs" to "delete 21 lines in sessions.rs + 11 lines in session_validation.rs + 2 tests" crosses a crate boundary (console-api → console-core). Acceptable because the orphan is a foot-gun: a suggestively-named `pub fn` whose only caller was the buggy block. Leaving it invites a future caller to re-wire it and re-introduce the exact bug. The alternative (add `#[allow(dead_code)]`) violates the repo's no-technical-debt principle.
- **Commit ordering under Path A.** Phase B must ship the struct change and every caller-site update in the same commit — a split commit would leave the tree uncompilable. Coverage test fixtures + integration test fixtures live in separate crates but are compiled by the same `cargo test` that Phase D runs, so a single commit is needed.
- **Integration-suite subset for Phase D.** `cargo test -p console-integration` spawns a real runtime child and takes 30–60 s wall. Phase D runs `lifecycle` + `cross_session` + `launch_failure_matrix` + `chaos` specifically (the suites that exercise `session_assignments`), not the full 50-test set. The rest can regress on CI if there's something we missed.
- **Schema migration hygiene broader than §9.1/§9.2.** `SessionAssignmentRow` has 5 other `Option<T>` fields (`stage_id`, `workspace_id`, three `budget_*`). All of them match nullable columns in migration 007 — audited during Phase A; no drift. But other tables' structs haven't been systematically audited. File as a follow-up if the principle catches on.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| `wcon-data-model` §3, §4.2 | Data model — profiles + session_assignments | authoritative; plan must not diverge |
| `wcon-sessions` | Session lifecycle | the slot-auto-derivation code being removed/fixed lives in this flow |
| `wcon-profiles` | Profile system | §7 versioning — existing `current.version + 1` inline pattern supersedes `max_version` |
| `HEALTH-LOG.md` §9.1 + §9.2 | Schema-vs-struct drift + NULL-aggregate ambiguity | triggering finding |
| `AUDIT-2026-04-15.md` §13.7.5 | Console-db T11 coverage sweep | surfaced these drifts |

## 7. Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| A | `5ff1946` | 2026-04-20 | Decision memo locked: §9.1 Path A (tighten struct + ripple-remove `derive_slots`); §9.2 Path C (delete `max_version`; plan deviation, zero production callers). |
| B | `2921ecc` | 2026-04-21 | §9.1 Path A. 11 files changed across 4 crates + docs (+64/−123). HEALTH-LOG §16 also filed (6 pre-existing test-only clippy drifts surfaced during verification). Plan Phase D gate narrowed to match CI. |
| C | `a32bd02` | 2026-04-21 | §9.2 Path C (delete). 2 files changed (−30 lines). Zero `max_version` references remain. |
| D | _in flight_ | 2026-04-21 | Verify + close. HEALTH-LOG §9.1 + §9.2 struck through; AUDIT §13.9 follow-up entries added; acceptance boxes ticked. Archive + ff next. |

---

*Plan doc — authored by AAkil98 + Claude Opus 4.7 (1M context). Move to `impl/archive/` once every §4 box is ticked.*
