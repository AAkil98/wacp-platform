---
id: wacp-context-schema-evolution-plan
type: impl
status: draft
created: 2026-04-24T15:14:19
revised: 2026-04-24T15:30:00
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

**Framing check — confirmed in P0 (2026-04-24).** Validation fires ONLY at `POST /api/sessions/:id/launch` (handler at `wacp-console/crates/console-api/src/routes/sessions.rs:405–445`), not at `POST /api/sessions` (create, `:117–176`). The create endpoint stores `vertical, workflow, context` verbatim into the session row in `CONFIGURING` state without taxonomy lookup. `validate_session` at the launch handler reads `state.taxonomy.load()` fresh every call — no caching, no per-request lazy path. So the test shape is: create (always 201) → launch (422 with violations containing context codes). The "multi-step" language in §13.5 referring to `SubmitGoal` was approximately accurate — the launcher's first step internally is `SubmitGoal` — but the rejection we're testing happens *before* the launcher runs, so no mock-highway / mock-coordinator infrastructure is needed.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| P0 | Recon + scope-freeze gate — confirm validation-only path, final scenario list, fixture-pair shape | ~30 min | — | Plan §3.0 notes written with Q1–Q3 resolved; acceptance scenarios frozen; deviations from §13.5 framing (if any) documented |
| P1 | `fixtures::fixture_context_v1()` + `fixture_context_v2_breaking()` + `fixture_context_v2_additive()` trio (see §7 deviation) + `evolution_skeleton()` shared helper | ~45 min | P0 | `cargo check -p console-test-support` clean; 3 round-trip tests pass |
| P2 | `session_lifecycle_with_schema_change.rs` integration file — 4 scenario tests | ~2h | P1 | `cargo test -p console-integration --test session_lifecycle_with_schema_change` is 4/4 green; existing `--test taxonomy_reload` unaffected (4/4) |
| P3 | Closeout — HEALTH-LOG §13.5 strike, SEED open follow-up #3 strike, `taxonomy_reload.rs` module doc edit, AUDIT closure row, plan archive | ~20 min | P0+P1+P2 | Plan moved to `impl/archive/`; `grep -n "context_schema" HEALTH-LOG.md` shows only resolved text; SEED follow-up list down to 4 items |

## 3. Deliverables — per phase

### 3.0 Phase P0 — recon + scope freeze

**Resolved 2026-04-24.** All four questions answered; scope frozen.

- **Q1 ✓ Single validator.** `session_validation::validate_session` is the sole reader of `context_schema` on the launch path. Grep confirms two call sites in `routes/sessions.rs`: `:22` (import) and `:418` (invocation inside `launch_session`). The create endpoint `create_session:117–176` writes the session row without touching the taxonomy index. No second validator (no profile-level context pre-screen, no runtime-side context validation — wacp-runtime's `Bind` does not inspect `context_schema`).
- **Q2 ✓ Eager swap, no caching.** Index lives in `AppState.taxonomy: Arc<ArcSwap<TaxonomyIndex>>`. `sessions.rs:407` does `state.taxonomy.load()` fresh on every launch — no memoization, no per-request lazy path. `/api/taxonomy/reload` calls `build_index(...)` + `state.taxonomy.store(new)` atomically; subsequent launches see new schema with no settle delay. Risk #6 (reload atomicity) effectively resolved — it's an `ArcSwap::store`, synchronous from the caller's perspective.
- **Q3 ✓ No snapshot.** Session rows store `vertical, workflow, context: Option<String>` verbatim. No schema snapshot, no required-field copy. `build_index` + reload only writes to `state.taxonomy`; zero DB-table touches. `create_session:134–153` stores context as a JSON string with no validation. "Existing session unaffected" = session row fields unchanged + DB state column unchanged.
- **Q4 ✓ Four tests frozen:**
  1. **`launch_rejected_after_field_added_as_required`** — v1 schema with 2 fields; session created + context populated for v1. Reload to v2 (adds `region: string, required=true`). Launch the already-created session → 422 with violations array containing `{code: "MISSING_CONTEXT"}`. (We assert *containment* not *equality* on the violations list because absent assignments add `MISSING_ASSIGNMENT` noise — by design; see §3.2 note.)
  2. **`launch_rejected_after_field_type_narrowed`** — v1: `priority: string`. v2: narrows to `priority: number`. Session created with `"priority": "high"` under v1. Reload to v2. Launch → 422 with violations containing `{code: "INVALID_CONTEXT"}`.
  3. **`active_session_preserved_across_schema_evolution`** — seed a session row directly via `seed_active_session`-style helper (pattern lifted from `recovery_matrix.rs:174`) in state=`active` with a context that v1 allowed. Reload to a breaking v2. Assert `sessions::get_by_id(&db, &sid)` returns the same state + same context string (no row mutation). Reload path cannot touch the sessions table.
  4. **`additive_evolution_accepts_session_without_new_optional_field`** — v1 2 fields; v2 adds 3rd optional field. Session created with v1-shaped context. Reload to v2. Launch the session → 422 (MISSING_ASSIGNMENT only, since no profiles seeded) *but* violations list does NOT contain `MISSING_CONTEXT` or `INVALID_CONTEXT`. Absence-assertion documents the "safe evolution" contract.

Scenario (5) `schema_removed_vertical` dropped — indistinguishable from the existing `taxonomy_reload::reload_with_removed_vertical_updates_list` at the validation-path level.

**Risk resolutions in P0:** Risk #1 (pre-validation profile deps) dissolved — `validate_session` accumulates all violations without short-circuiting, so tests assert *containment* on the violations list rather than launch success; no profile seeding required. Risk #5 (validation-vs-launcher order) resolved — launch handler at `:395–456` does CONFIGURING→VALIDATING→[validate]→[on-fail back to CONFIGURING; on-pass LAUNCHING→launcher]. Validation strictly before any runtime RPC.

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

**Test skeletons (P0-corrected shape — validation fires on `/launch`, not `/sessions`):**

```rust
#[tokio::test]
async fn launch_rejected_after_field_added_as_required() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let mock_rest = spawn_mock_rest_with([fixtures::fixture_context_v1()]).await;
    let db = console_db::create_test_pool().await.expect("db");
    let console = ConsoleHarness::spawn_with_db(&rt, db.clone()).await.expect("console");
    point_settings_at_mock(&console, &mock_rest).await;
    let client = admin_client(&console).await;
    reload(&client).await; // pick up v1

    // Step 1 — create session under v1 (no validation at create time; always 201).
    let sid = create_session(
        &client,
        "evolution", // vertical id
        "debug",     // workflow id
        serde_json::json!({"project_id": "p-1", "priority": "high"}),
    ).await;

    // Step 2 — evolve schema to v2 (adds required `region`).
    mock_rest.state.set_verticals(map([fixtures::fixture_context_v2()]));
    reload(&client).await;

    // Step 3 — attempt launch; validator reads NEW index, rejects on missing `region`.
    let resp = launch_session(&client, &sid).await;
    assert_eq!(resp.status(), 422);
    let body: serde_json::Value = resp.json().await.unwrap();
    let codes: Vec<_> = body["violations"]
        .as_array().unwrap()
        .iter()
        .filter_map(|v| v["code"].as_str())
        .collect();
    assert!(
        codes.contains(&"MISSING_CONTEXT"),
        "expected MISSING_CONTEXT in violations (containment assertion — \
         other codes like MISSING_ASSIGNMENT are expected noise since no \
         profiles were seeded); got: {body:?}",
    );
}
```

**Why containment rather than equality on violations.** `validate_session` accumulates all 12 checks without short-circuiting (confirmed P0 Q4). A session launched without seeded profiles emits `MISSING_ASSIGNMENT` per vertical role plus whatever context violations apply. Tests assert the *context* codes are present — ignoring assignment noise — which matches the "am I testing the schema-evolution contract" intent. Pre-seeding profiles to clear the assignment violations would quadruple the setup cost for zero additional signal (profiles are exercised in `recovery_matrix` + `launch_failure_matrix`).

**Test 3 uses DB-direct seed** (no create/launch roundtrip) — `seed_active_session(&db, &sid, "u-1", ctx)` pattern lifted from `recovery_matrix.rs:174`. After reload, assert `sessions::get_by_id(&db, &sid)` returns row with unchanged `state == "active"` and unchanged `context`. No `active_sessions` map assertion (the seeded session has no monitor spawned; that would require recovery to run).

**Test 4 absence-assertion** — assert `codes.contains(&"MISSING_CONTEXT") == false && codes.contains(&"INVALID_CONTEXT") == false`. The 422 response still fires because `MISSING_ASSIGNMENT` accumulates; the test's point is that the optional-field evolution did not add a *context* violation.

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

1. ~~**Session creation pre-validation dependencies.**~~ **Resolved P0.** Create endpoint (`/api/sessions`) does not validate context_schema — only launch does. `validate_session` accumulates violations without short-circuiting, so tests use containment assertions on `MISSING_CONTEXT` / `INVALID_CONTEXT` and tolerate coexisting `MISSING_ASSIGNMENT` noise from absent profiles. Zero profile seeding needed.
2. **Reload atomicity under concurrent session creation.** If test (3)'s "existing session preserved" runs against a reload that fires mid-validation, a race could surface. In practice the test is strictly sequential (seed → reload → inspect), so this is a theoretical concern. Worth noting in the file's module doc so future contributors don't add parallelism without thinking about it. P0 Q2 confirmed the swap is `ArcSwap::store` (atomic, synchronous from caller's view) — even a concurrent launch would either see v1 fully or v2 fully, never partial.
3. **Fixture coupling to `wacp-taxonomy::VerticalManifest` shape.** The fixture helpers construct `VerticalManifest` directly, so any field addition to that struct breaks the fixtures the same way it breaks all other fixtures (prior art: `fixture_simple`, `fixture_complex`). Standard mechanical update; not a new risk class.
4. **Helper duplication between `taxonomy_reload.rs` and `session_lifecycle_with_schema_change.rs`.** `MockRest`, `admin_client`, `reload` would be duplicated across the two files. Two paths: (a) duplicate and accept; (b) factor into `console-test-support/src/mock_rest_spawn.rs` as a first cut. Recommend (a) unless the duplication becomes painful — two integration files is below the "rule of three" threshold and the shapes may legitimately diverge as more scenarios land. If a third file lands, factor then.
5. ~~**"Multi-step" may or may not need a real launcher roundtrip.**~~ **Resolved P0.** Validation fires strictly before the launcher (handler `:405–456` does CONFIGURING→VALIDATING→validate→(on-pass) LAUNCHING→launcher). No `InjectableCoordinator` / mock-highway needed; the 422 returns with the runtime never touched.
6. ~~**Taxonomy reload settling time.**~~ **Resolved P0.** `ArcSwap::store` is synchronous and atomic. `/api/taxonomy/reload` returning 200 means the next `.load()` sees the new index. No settle delay.

**New risk surfaced in P0:**

7. **Vertical id collision in reload.** The evolved fixture v1/v2 share `id: "evolution"` (by design — they hot-swap in place). But any test that also seeds additional verticals via `fixture_simple` / `fixture_complex` must ensure the mock's `RestState::set_verticals` call passes a HashMap that includes or excludes the target id deliberately. Cost if wrong: schema evolution silently doesn't happen because v2's id changed. Mitigation: use a dedicated `id: "evolution"` in both v1 and v2; other scenarios' verticals either use different ids or are absent from the schema-evolution tests.

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
| P0 | `7c42659` | 2026-04-24 | Recon complete. Q1–Q4 resolved inline in §3.0. Framing correction in §1: validation fires on `/launch` not `/sessions` create (`routes/sessions.rs:418`, single call site). Q2 confirmed atomic `ArcSwap::store` with no settle delay. Q3 confirmed no schema snapshot — context stored verbatim. Q4 froze 4 scenarios; all use containment-on-violations assertions so no profile seeding is needed. Risks #1, #5, #6 resolved to ~; risk #7 (vertical-id collision in reload) newly surfaced. |
| P1 | (this commit) | 2026-04-24 | **Plan deviation — three fixtures, not two.** P0 Q4's scenario freeze needs one v1 + two distinct v2s: `v2_breaking` (narrows priority to Number + adds required `region`, exercises tests 1+2+3) and `v2_additive` (adds optional `notes`, exercises test 4). Consolidating both evolutions into a single v2 would conflate the additive-vs-breaking signal. Added `evolution_skeleton()` helper so all three fixtures share identical non-schema surface (task_types, workflows, profiles, tools) — only `context_schema` differs. Three fixtures + three round-trip tests; `cargo test -p console-test-support --lib fixtures` 6/6 (3 pre-existing + 3 new). Clippy + fmt clean. |
| P2 | — | — | Integration file + 4 scenarios (2 rejection via /launch, 1 DB-seeded preservation, 1 additive absence-assertion). |
| P3 | — | — | Closeout commits (HEALTH-LOG strike + AUDIT row + taxonomy_reload.rs doc edit + archive move). |
