---
id: wacp-context-schema-evolution-plan
type: impl
status: draft
created: 2026-04-24T15:14:19
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, integration, testing, taxonomy, session-validation]
depends_on: [wacp-integration-deferred-scenarios-plan]
---

# Context-Schema Evolution — Plan

> **Triggering finding:** HEALTH-LOG §13.5 deferred sub-scenario (I5 `taxonomy_reload` — "`context_schema` change affects new sessions but not running ones") + SEED open follow-up #3 — the last remaining §13.7.8 deferral.
> **Target branch:** `testing/context-schema-evolution` (topic).
> **Rough effort:** ~3–4h — **medium** confidence. P0 recon resolves whether session_validation re-fires on reload or is strictly creation-gated; rest is composition on already-landed infrastructure (ArcSwap mock REST + `ConsoleHarness::spawn_with_db_and_rest` + `/api/taxonomy/reload`).
> **Not in scope:** §13.3 runtime-auth uniformity (blocked on runtime gaining real auth — spec work upstream); cross-harness `pick_port` TOCTOU residual (never observed); profile-schema evolution (profiles store snapshots at version-save time; the evolution question only applies to the live taxonomy-vs-creation boundary); runtime-side `SubmitGoal` behaviour under schema drift (runtime doesn't re-validate at submit time — validation is console-side via `session_validation::validate_session` before the launcher fires).

## 1. Goal & Motivation

Close the last deferred sub-scenario from the §13.7.8 integration + chaos workstream. HEALTH-LOG §13.5 explicitly carried it forward as "outside the reload endpoint's surface; would fit better under a future `session_lifecycle_with_schema_change` scenario if that becomes a priority." The priority trigger is now: with v0.1.0 gate enforcement landed (2026-04-24), the §13.7.8 integration matrix is the only pre-v0.1.0 workstream with a tracked deferral still open. Closing it gives a clean pre-release state.

**What this proves.** The console's `session_validation::validate_session` at `wacp-console/crates/console-core/src/session_validation.rs:169–215` enforces `MISSING_CONTEXT` + `INVALID_CONTEXT` on every `POST /api/sessions` against the currently-indexed `VerticalManifest.context_schema`. There is no integration test that exercises the schema-evolution half of the contract — i.e., that *after* a `/api/taxonomy/reload` swaps the schema, subsequent creation attempts are validated against the new schema, while sessions already in the `active_sessions` map are not retroactively invalidated (validation is a creation-time gate, not an at-rest invariant).

**Cost of inaction.** The validation branch itself is covered by `session_validation`'s in-crate `#[cfg(test)]` unit tests (line ~241+ in the same file). What's uncovered: the *composition* of mock-REST fixture swap + `/api/taxonomy/reload` + new session creation + existing-session inspection across the full harness. A future change that accidentally coupled validation to a stale cached schema (e.g., a `lazy_static!` or an `OnceCell` that captures the first-loaded schema and never refreshes) would not be caught by any existing test — the in-crate tests build a fresh `VerticalManifest` per call; the reload suite only checks that the served vertical *list* updates. This plan adds the integration-level glue.

**Framing check (to confirm in P0).** The deferral wording in §13.5 sketches a flow starting with `SubmitGoal`. In practice, session creation via `POST /api/sessions` goes: validate → launch (`session_launcher::launch` which internally drives `SubmitGoal → Decompose → Dispatch`). Validation fires before the launcher, so the scenario does *not* require a full runtime coordination path — the mock-highway + mock-coordinator infrastructure added by the prior plan is not needed here. The "multi-step" aspect is the fixture evolution, not the launcher depth. P0 confirms this.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| P0 | Recon + scope-freeze gate — confirm validation-only path, final scenario list, fixture-pair shape | ~30 min | — | Plan §3.0 notes written with Q1–Q3 resolved; acceptance scenarios frozen; deviations from §13.5 framing (if any) documented |
| P1 | `fixtures::fixture_context_v1()` + `fixture_context_v2()` pair (evolved schema with one additive + one breaking change) | ~45 min | P0 | `cargo check -p console-test-support` clean; helpers compile; fixture round-trip test updated |
| P2 | `session_lifecycle_with_schema_change.rs` integration file — 4 scenario tests | ~2h | P1 | `cargo test -p console-integration --test session_lifecycle_with_schema_change` is 4/4 green; existing `--test taxonomy_reload` unaffected (4/4) |
| P3 | Closeout — HEALTH-LOG §13.5 strike, SEED open follow-up #3 strike, `taxonomy_reload.rs` module doc edit, AUDIT closure row, plan archive | ~20 min | P0+P1+P2 | Plan moved to `impl/archive/`; `grep -n "context_schema" HEALTH-LOG.md` shows only resolved text; SEED follow-up list down to 4 items |

## 3. Deliverables — per phase

### 3.0 Phase P0 — recon + scope freeze

Open questions to resolve, each with a concrete answer recorded in the plan or in the P0 commit body:

- **Q1:** Is `session_validation::validate_session` the only validator that reads `context_schema` on `POST /api/sessions`? Grep trace: `routes/sessions.rs::create_session` → `session_validation::validate_session` → `taxonomy::VerticalManifest`. Confirm no second validator (e.g., a profile-level schema validator) pre-screens the context.
- **Q2:** When does the indexed taxonomy refresh? Two candidates: (a) `taxonomy_builder::build_index` at boot, re-built only on `/api/taxonomy/reload`; (b) something lazier on a per-request basis. Confirmed (a) is expected; P0 grep verifies. If (b) were true, the test would need different pacing.
- **Q3:** Does the console store any schema snapshot alongside a session at creation time (e.g., copy required-field set into `session_context` or a sibling table)? If yes, "existing session unaffected" needs to check that the session's stored context survives the evolution; if no, "unaffected" just means the session row is untouched by the reload code path. Expected: no snapshot — validation is a boolean gate, the accepted context is stored verbatim.
- **Q4:** Final scenario list (scope freeze). Proposed four tests:
  1. `new_session_rejected_after_field_added_as_required` — v1 has no `region` field; v2 adds `region: string, required=true`. A session created under v1 with no `region` succeeds. After reload to v2, a new POST without `region` returns 422 with `MISSING_CONTEXT`.
  2. `new_session_rejected_after_field_type_narrowed` — v1 has `amount: string`; v2 narrows to `amount: number`. A POST under v2 with `amount: "100"` (string) returns 422 with `INVALID_CONTEXT`.
  3. `existing_active_session_preserved_across_schema_evolution` — create a session under v1, then reload to v2 (which would have rejected it), then assert the session is still `active` in DB and still in `active_sessions` map, and its `session_context` is unchanged. Complements (1)+(2): proves evolution isn't retroactive.
  4. `additive_evolution_accepts_new_optional_field` — v1 has two fields; v2 adds a third optional field. Sessions that omit the new field under v2 succeed. Proves the common safe-evolution path explicitly.

Drop (4) if scope creep appears — (1)+(2)+(3) are the load-bearing trio. Add (5) `schema_removed_vertical_rejects_new_sessions_with_unknown_vertical_id` only if the verticals-removal path is genuinely different from §13.5's `reload_with_removed_vertical_updates_list`; likely redundant.

### 3.1 Phase P1 — fixture pair

Target file: `wacp-console/crates/console-test-support/src/fixtures.rs`. Add two new paired helpers after `fixture_complex()`:

```rust
/// Base schema for schema-evolution tests. Three fields, all typed.
pub fn fixture_context_v1() -> VerticalManifest { ... }

/// Evolved schema: one field added as required, one field type-narrowed,
/// one unchanged. Pairs with `fixture_context_v1()` for evolution scenarios.
pub fn fixture_context_v2() -> VerticalManifest { ... }
```

Both share `id: "evolution"` so they hot-swap in place in the REST mock. Concrete shapes (subject to P0 Q4 freeze):

- **v1:** `{project_id: string required, priority: enum<low|high> required, notes: string optional}`.
- **v2:** `{project_id: string required, priority: number required /* narrowed */, notes: string optional, region: string required /* added */}`.

The pair exercises: narrowed-type rejection (scenario 2), newly-required rejection (scenario 1), unchanged field (regression guard).

Add a fixture round-trip smoke test in the existing `fixtures::tests` module:
```rust
#[test]
fn context_evolution_fixtures_differ_on_required_set() { ... }
```
Asserts v2 has strictly more required fields than v1 and at least one type difference. Keeps the fixture intent documented in code.

**Back-compat.** All existing fixtures (`fixture_simple`, `fixture_complex`, etc.) untouched. New helpers are additive.

Verification: `cargo check -p console-test-support --tests`; `cargo test -p console-test-support fixtures::tests` passes (pre-existing count + 1 new).

### 3.2 Phase P2 — `session_lifecycle_with_schema_change.rs`

New file: `wacp-console/integration/tests/session_lifecycle_with_schema_change.rs`.

**Shape.** Module-level helpers mirroring `taxonomy_reload.rs`:

- `MockRest` wrapper over `RestState` with `url()`, spawned via `axum::serve`. Lift directly from `taxonomy_reload.rs` — same shape, no `force_500` needed. If the duplication feels wrong, factor into a shared helper in `console-test-support`; see §5 risk #4.
- `admin_client(&console)` seeder — copy from `taxonomy_reload.rs`.
- `reload(&client)` + `create_session(&client, context: Value)` helper — the latter is new; POSTs to `/api/sessions` and returns the response for the caller to assert on.

**Test skeletons (P0 freezes shapes):**

```rust
#[tokio::test]
async fn new_session_rejected_after_field_added_as_required() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let mock_rest = spawn_mock_rest_with(fixtures::fixture_context_v1()).await;
    let db = console_db::create_test_pool().await.expect("db");
    let console = ConsoleHarness::spawn_with_db(&rt, db.clone()).await.expect("console");
    point_settings_at_mock(&console, &mock_rest).await;
    let client = admin_client(&console).await;
    reload(&client).await; // pick up v1

    // Create under v1 — succeeds.
    let resp_v1 = create_session(
        &client,
        serde_json::json!({"project_id": "p-1", "priority": "high"}),
    ).await;
    assert!(resp_v1.status().is_success(), "v1 creation: {:?}", resp_v1.status());

    // Evolve: swap to v2 (adds required `region` field).
    mock_rest.state.set_verticals(map(fixtures::fixture_context_v2()));
    reload(&client).await;

    // Create under v2 without `region` — 422 with MISSING_CONTEXT.
    let resp_v2 = create_session(
        &client,
        serde_json::json!({"project_id": "p-2", "priority": 5}),
    ).await;
    assert_eq!(resp_v2.status(), 422);
    let body: serde_json::Value = resp_v2.json().await.unwrap();
    assert!(
        body["violations"].as_array().unwrap().iter().any(|v| v["code"] == "MISSING_CONTEXT"),
        "expected MISSING_CONTEXT in: {body:?}",
    );
}
```

Tests 2 + 3 + 4 follow the same harness shape with different before/after schemas and different assertions. Test 3 inspects DB row state via `console_db::sessions::get(&db, &sid)` + the `active_sessions` map on `console.state`.

**Performance target.** 4 tests × ~(runtime-spawn 200 ms + mock-rest-spawn 10 ms + 2 reloads + 2–3 POSTs) ≈ ~1 s total. Well within the 5 s per-file stretch target.

Verification: `cargo test -p console-integration --test session_lifecycle_with_schema_change` 4/4; `cargo test -p console-integration --test taxonomy_reload` still 4/4 (unaffected); `cargo clippy --workspace --all-targets -- -D warnings` clean.

### 3.3 Phase P3 — closeout

- **HEALTH-LOG §13.5** — strike the "Deferred (confirmed from the audit scope): `context_schema` evolution…" paragraph; replace with `**Status: RESOLVED 2026-04-?? via impl/archive/context-schema-evolution-plan.md.** New integration file `session_lifecycle_with_schema_change.rs` exercises the full swap cycle (fixture v1 → create → reload to v2 → attempt new → assert rejection + existing-session unaffected).`
- **`taxonomy_reload.rs` module doc** (`:19–22`) — strike the "Not covered (deferred, see `HEALTH-LOG.md` §13.5)" block with a comment noting closure; leave a pointer to the new file: `See session_lifecycle_with_schema_change.rs for the schema-evolution scenario.`
- **SEED open follow-ups** — drop #3 from the list (down from 6 to 5 items in the §Primary-tracks block). Revised by next SEED refresh per the `seed-refresh` skill's batch-boundary rule; not this commit.
- **AUDIT §13.9** — append closure row (`§13.9.11` or next available number). Body: "Context-schema evolution integration scenario landed per `impl/archive/context-schema-evolution-plan.md`. Closes §13.7.8 I5 final deferral."
- **AUDIT §13.7.8 closeout prose** — update the "Four sub-scenarios deferred" phrasing (if still present) to "All four sub-scenarios now closed" with pointer to this plan.
- **AUDIT footer** — append `§13.9.11 closed 2026-04-?? by Claude Opus 4.7 (1M context) via impl/archive/context-schema-evolution-plan.md (4 phases)`.
- **Plan archive** — `git mv impl/context-schema-evolution-plan.md impl/archive/` via the `archive-plan` skill.
- **SEED refresh** — next batch boundary (this would be the 27th pass).

## 4. Acceptance Criteria

- [ ] P0 recon committed: Q1–Q4 answered in plan §3.0 edit or P0 commit body; scope frozen; any §13.5 framing-stale findings documented.
- [ ] `fixtures::fixture_context_v1()` + `fixture_context_v2()` present in `wacp-console/crates/console-test-support/src/fixtures.rs` with docstrings explaining the evolution intent.
- [ ] `fixtures::tests::context_evolution_fixtures_differ_on_required_set` (or equivalent) passes as part of `cargo test -p console-test-support`.
- [ ] `wacp-console/integration/tests/session_lifecycle_with_schema_change.rs` exists and `cargo test -p console-integration --test session_lifecycle_with_schema_change` is 4/4 (or the frozen P0 count) green.
- [ ] `cargo test -p console-integration --test taxonomy_reload` unaffected (still 4/4).
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean across the touched crates.
- [ ] HEALTH-LOG §13.5 "Deferred" block struck and replaced with resolution pointer.
- [ ] `taxonomy_reload.rs` module doc `:19–22` edited — §13.5 deferred-pointer removed, see-also added.
- [ ] AUDIT §13.9.N closure row added + footer appended.
- [ ] Plan moved to `impl/archive/context-schema-evolution-plan.md`; status: final; §7 execution log completed with per-phase SHAs.

## 5. Risks / Open Questions

1. **Session creation pre-validation dependencies.** `POST /api/sessions` may require a seeded profile + verticals + task_type that references `evolution` as a vertical. If the creation flow short-circuits on missing-profile before reaching `session_validation`, the test's assertions about `MISSING_CONTEXT` won't fire — the response would be a different 422 first. Mitigation: P0 traces `routes/sessions.rs::create_session` to confirm validation order; if profile-lookup fires first, P1 fixtures must include a paired profile setup in the mock-rest state or via DB seed.
2. **Reload atomicity under concurrent session creation.** If test (3)'s "existing session preserved" runs against a reload that fires mid-validation, a race could surface. In practice the test is strictly sequential (create → reload → inspect), so this is a theoretical concern. Worth noting in the file's module doc so future contributors don't add parallelism without thinking about it.
3. **Fixture coupling to `wacp-taxonomy::VerticalManifest` shape.** The fixture helpers construct `VerticalManifest` directly, so any field addition to that struct breaks the fixtures the same way it breaks all other fixtures (prior art: `fixture_simple`, `fixture_complex`). Standard mechanical update; not a new risk class.
4. **Helper duplication between `taxonomy_reload.rs` and `session_lifecycle_with_schema_change.rs`.** `MockRest`, `admin_client`, `reload` would be duplicated across the two files. Two paths: (a) duplicate and accept; (b) factor into `console-test-support/src/mock_rest_spawn.rs` as a first cut. Recommend (a) unless the duplication becomes painful — two integration files is below the "rule of three" threshold and the shapes may legitimately diverge as more scenarios land. If a third file lands, factor then.
5. **"Multi-step" may or may not need a real launcher roundtrip.** §13.5's framing sentence includes `SubmitGoal` in the step list. If P0 confirms validation fires before the launcher (expected per `routes/sessions.rs:418`), the test's `create_session` can assert on the 422 from validation without ever hitting a coordinator — simpler shape. If validation fires *after* launch begins (unexpected but possible), the test needs `InjectableCoordinator` setup. P0 resolves.
6. **Taxonomy reload settling time.** After `POST /api/taxonomy/reload` returns 200, is the new index immediately visible to the next `POST /api/sessions`, or is there an async swap boundary? `taxonomy_reload.rs` tests implicitly assume synchronous visibility (the following `list_verticals` call returns updated content). Should hold here; P0 cross-checks.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| `HEALTH-LOG` §13.5 | I5 taxonomy_reload — `context_schema` evolution deferral | implements (closes the last "Deferred" bullet) |
| `HEALTH-LOG` §13.2 | I2 recovery_matrix deferrals | informs (prior-art for "deferral closure" batch; pattern borrowed) |
| `AUDIT-2026-04-15` §13.7.8 | Integration I1–I5 closeout — four sub-scenarios deferred | extends (removes the final of four deferrals) |
| `AUDIT-2026-04-15` §13.9 | Post-audit follow-ups | extends (appends §13.9.11 closure row) |
| `impl/archive/integration-deferred-scenarios-plan.md` | Prior plan closing §13.2 deferrals | informs (structure, ff-style closeout; two-tests-per-deferral pattern) |
| `impl/archive/audit-13-7-8-plan.md` | Prior integration + chaos plan | informs (deferral rationale; mock-rest precedent) |
| `wacp-console/crates/console-core/src/session_validation.rs` | `validate_session` — the system under test (context loop at `:169–215`) | implements (validation branch coverage target) |
| `wacp-console/crates/console-core/src/taxonomy_builder.rs` | `build_index` — consumes `VerticalManifest.context_schema` on reload | informs (reload path semantics) |
| `wacp-console/crates/console-test-support/src/fixtures.rs` | Vertical manifest fixtures | implements (add v1/v2 pair) |
| `wacp-console/crates/console-test-support/src/mock_rest.rs` | ArcSwap-based scriptable REST mock | implements (hot-swap mechanism for evolution) |
| `wacp-console/integration/tests/taxonomy_reload.rs` | Sibling I5 suite (module-doc pointer to this plan on landing) | informs (MockRest shape; edit target in P3) |
| `impl/git-strategy.md` §4 | Topic-branch naming | constrains (`testing/context-schema-evolution`) |
| `SEED.md` §"Resumption Point" | Open follow-up #3 | extends (closes this item) |

## 7. Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| P0 | — | — | Recon + Q1–Q4 answers; scope freeze. |
| P1 | — | — | Fixture pair `fixture_context_v1/v2` + round-trip test. |
| P2 | — | — | Integration file + 4 (or frozen-N) scenarios. |
| P3 | — | — | Closeout commits (HEALTH-LOG strike + AUDIT row + taxonomy_reload.rs doc edit + archive move). |
