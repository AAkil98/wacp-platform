---
id: wacp-audit-13-7-8-plan
type: impl
status: draft
created: 2026-04-18T04:15:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [integration, chaos, i1-i5, 13.7.8, no-tech-debt]
depends_on: [wacp-audit-2026-04-15, wacp-ci-cleanup-2.7-plan]
---

# §13.7.8 — Rust Integration + Chaos (I1–I5) Plan

> Closing the last §13.7 Rust-side work package. I6 (`llm_stub_e2e.rs`, 222 lines) already landed via §13.7.6 — this plan covers the remaining five suites under `wacp-console/integration/tests/`. The W7 harness (`RuntimeHarness`, `ConsoleHarness`, `TestClient`) is mature and reusable; every new suite slots in as a `tests/*.rs` file that imports from `console_integration::`.
>
> Full-fix principle carried forward from §2.7: no `#[ignore]` without an inline tracking reference; no `unwrap_or(Default)` to paper over a runtime error that should fail the test; each assertion must name what it's proving, not what it's passing through.

## Table of Contents

- 1. Goal + acceptance
- 2. Shared infrastructure (pre-work)
- 3. Per-suite plan
  - 3.1 I1 `launch_failure_matrix.rs`
  - 3.2 I2 `recovery_matrix.rs`
  - 3.3 I3 `auth_matrix.rs`
  - 3.4 I4 `ws_chaos.rs` (expand `chaos.rs`)
  - 3.5 I5 `taxonomy_reload.rs`
- 4. `performance-optimization.md` watch list
- 5. Verification matrix + CI
- 6. Phasing + commit strategy
- 7. Non-goals / deferred
- 8. References

---

## 1. Goal + acceptance

**Acceptance (from AUDIT §13.7.8):** `cargo test -p console-integration` green on `main` for all five new suites plus the existing four (`lifecycle`, `chaos`, `cross_session`, `llm_stub_e2e`). No `#[ignore]` beyond an explicit justification + tracking reference.

**Stretch targets not in the acceptance criterion but worth holding:**

- Each new suite's full walltime < 5 s on the dev box (I6 baseline: 0.28 s for 2 scenarios; expect more variance here because I1/I2 spin multiple runtime/console processes).
- No suite's per-test RSS peak > 500 MB (consistent with frontend §6 watch list; Rust tests are cheaper anyway).
- Every new suite produces at least one *failure-path* assertion that would have caught a historical bug (perf-opt §9/§11 are the prompt list).

**Anchors:** AUDIT §13.7.8 deliverables; `performance-optimization.md` §11.5 (T7.7/T7.8 lessons already baked in); `wacp-console/integration/src/*.rs` for the harness API.

## 2. Shared infrastructure (pre-work)

Done once, reused by I1–I5. Expect ~30–60 min.

### 2.1 Failure-injection mock `CoordinatorService`

**Why.** I1 needs to return specific `tonic::Status` codes from `SubmitGoal`, `Decompose`, `Dispatch` without hacking the real runtime. WA5 (`chaos.rs` T7.5) already ships a `ForwardingMockCoordinator` that wraps the real runtime's `CoordinatorServiceClient` and returns `Unavailable` on the Nth `Dispatch`. Generalize it.

**Scope.** New file `wacp-console/integration/src/mock_coordinator.rs`:

```rust
pub struct InjectableCoordinator {
    upstream: CoordinatorServiceClient<Channel>,
    inject: Arc<Mutex<InjectionTable>>,
}

pub struct InjectionTable {
    pub submit_goal: Vec<Option<Status>>,   // pop front per call
    pub decompose:  Vec<Option<Status>>,
    pub dispatch:   Vec<Option<Status>>,
    pub abort:      Vec<Option<Status>>,    // for rollback-failure permutations
}
```

Each RPC checks the head of its vec: `Some(status)` → return the failure without forwarding; `None` → forward to upstream. `serve_on(addr)` boots a local `tonic::transport::Server` that exposes the service.

**Acceptance.** One unit test asserting forward + inject both work. Keep it below 80 lines.

**Why it's pre-work, not I1 work.** I2 reuses the same mock with `abort` failures to simulate "rollback succeeds for N of M" during recovery; separating the mock from I1 keeps it a library function.

### 2.2 Helper: launch a session directly via the Console API

**Why.** Every I1–I5 test that wants a session row + launch attempt today has to POST `/api/sessions` with a full body, poll for the launch state, etc. Wrap that.

**Scope.** Extend `test_client.rs` with:

```rust
impl TestClient {
    pub async fn create_session(&self, body: &serde_json::Value) -> SessionCreated;
    pub async fn launch_session(&self, session_id: &str) -> LaunchResult; // parses LaunchError shape
    pub async fn session_state(&self, id: &str) -> console_db::queries::sessions::SessionRow;
}
```

**Acceptance.** One smoke call each from an existing test (re-use lifecycle.rs's pattern but via helpers). Don't change any existing test; the helpers are additive.

---

## 3. Per-suite plan

### 3.1 I1 `launch_failure_matrix.rs`

**Deliverables (one test each, unless noted):**

| # | Scenario | Assertion |
|---|---|---|
| 1.1 | `SubmitGoal` returns `Unavailable` | `LaunchError::Step { step: SubmitGoal, .. }`; session row state = `failed`; no workspaces created upstream |
| 1.2 | `SubmitGoal` returns `InvalidArgument` | As above; reason-code matches tonic status |
| 1.3 | `SubmitGoal` returns `Unauthenticated` | As above |
| 1.4 | `Decompose` returns `Unavailable` after `SubmitGoal` OK | `LaunchError::Step { step: Decompose, rollback: [root_ws] }`; root_ws aborted; session row state = `failed` |
| 1.5 | `Decompose` returns `Internal` | Same shape; rollback attempted |
| 1.6 | `Dispatch` on task 1 of 3 fails | `LaunchError::Step { step: Dispatch, rollback: [root_ws] }`; zero task workspaces created; session `failed` |
| 1.7 | `Dispatch` on task 2 of 3 fails (mid-sequence) | Rollback covers root_ws + first task's workspace; session `failed`; both aborted |
| 1.8 | `Dispatch` on task 3 of 3 fails (last in sequence) | Rollback covers root_ws + first + second |
| 1.9 | Rollback partial failure — `AbortWorkspace` fails for 1 of 3 targets | Launch still returns `LaunchError`; session `failed`; log records the abort-failure (observable via `tracing_test` capture); rollback does NOT panic or deadlock |
| 1.10 | Rollback total failure — every `AbortWorkspace` returns `Unavailable` | Launch returns `LaunchError`; session `failed`; launcher doesn't hang (finite retries); all abort attempts logged |

**Structure.** Each test:
1. Spawn `RuntimeHarness` + `InjectableCoordinator` wrapping it.
2. Spawn `ConsoleHarness` configured with the mock's addr as coordinator endpoint.
3. Seed a user + session row via `TestClient`.
4. Push the injection table.
5. `client.launch_session(sid)` and destructure the error.
6. Assert on the session row state + the side-effects observable via the runtime's state (e.g., `GetWorkspaceState` against root_ws should return Aborted for a rolled-back launch).

**Expected findings (perf-opt drift targets).** The `LaunchError::reason_code()` string set is a public contract; test 1.9 is the most likely site to surface a drift between the enum and the contract. Also watch for the `abort_unavailable_count` perf-opt tail — if the rollback loop retries forever, that's §11.3's async-cascade anti-pattern and should be documented.

**Effort.** 75–90 min. Mock coordinator + 10 tests + 1 helper assertion function.

**Commit.** `test(integration): §13.7.8 I1 — launch_failure_matrix with rollback permutations`.

---

### 3.2 I2 `recovery_matrix.rs`

**Deliverables (one test per scenario):**

| # | Scenario | Pre-state | Assertion |
|---|---|---|---|
| 2.1 | Happy path — runtime up, active session | `sessions.state='active'` row, 1 workspace, runtime returning `Active` | After `ConsoleHarness::spawn_with_db`: `active_sessions` map contains the sid; monitor task running (JoinHandle not yet resolved); session row state still `active` |
| 2.2 | Runtime down at boot | Same DB state, runtime killed before console spawn | Recovery logs failure for the session; session row state remains `active` (console doesn't mark it failed — runtime may come back); no monitor respawned; `RecoveryReport.failures` contains the sid with `RecoveryFailureReason::RuntimeUnreachable` |
| 2.3 | DB degraded at boot — `SQLITE_BUSY` on sessions read | Use `console-db::testing::FaultyDb::hold_write_lock` to keep a write lock → `recovery::run`'s `sessions::list_non_terminal` retry path | Recovery returns a report with `RecoveryFailureReason::DbError`; `active_sessions` empty; no panics |
| 2.4 | Orphaned `launching` session | `sessions.state='launching'` row, no coordinator workspace, no assignments committed | Recovery transitions to `failed` with reason `recovery:orphaned_launching`; logged at warn level; no monitor spawned |
| 2.5 | Terminal workspace — runtime says `completed` | `sessions.state='active'`, runtime's `GetWorkspaceState` returns `Closed` | Recovery transitions session to `completed`; no monitor spawned (terminal state doesn't need observation); trail entries ≥ 0 preserved |
| 2.6 | Terminal workspace — runtime says `aborted` | Same but runtime returns `Aborted` | Session transitions to `cancelled` (the canonical terminal mapping for Aborted); no monitor |
| 2.7 | Multi-session — mix of 2.1 / 2.4 / 2.5 in one DB | 3 sessions across the three states | All three reach correct end state in one `recovery::run` pass; `RecoveryReport.respawned=1, terminal_synced=1, orphan_failed=1` |

**Structure.** Uses the existing `ConsoleHarness::spawn_with_db(&rt, pre_seeded_db)` path. For 2.3, wire `FaultyDb` from `console-db::testing`; for 2.4, insert rows directly via `console_db::queries::sessions::insert_launching` fixtures.

**Expected findings.** Recovery's state-mapping table is narrow surface but high-consequence. Test 2.5/2.6 are the ones that would have caught the `WorkspaceState` enum-offset bug (perf-opt §11.4) if they had existed — the fix landed via §13.7.6b but this suite permanently guards it.

**Effort.** 60–75 min. Seven tests, each ~30–40 lines.

**Commit.** `test(integration): §13.7.8 I2 — recovery_matrix with seven boot scenarios`.

---

### 3.3 I3 `auth_matrix.rs`

**Deliverables.** One table-driven `rstest` fixture enumerating the matrix, plus a small number of non-table tests for auth paths that don't fit the grid.

**Matrix dimensions** (currently realized in the code):

- **Console auth mechanism:** `cookie` (browser session), `bearer` (API token), `anonymous` (unauthenticated — expected 401).
- **Runtime auth mechanism:** today the runtime's `Bind` accepts any token ≥ 8 chars (see note in `llm_stub_e2e.rs:95`); there is no api-key vs. session vs. oauth *per-request* distinction on the runtime wire. The audit's "every runtime auth path" phrasing was forward-looking. **Plan: document this in the suite header and assert the current contract (token length + presence) rather than imagined oauth/api-key distinctions.**
- **Console role:** `admin`, `operator`, `viewer`.
- **Action surface:** 5 representative actions — `/api/health` (anyone), `/api/profiles` (operator+), `/api/taxonomy/reload` (admin+), `/api/users` (admin+), `/api/sessions/{id}/ws` (operator+ for the session owner).

**Matrix cell count.** 3 console-auth × 3 roles × 5 actions = 45 cells; `rstest` generates them from one test body.

**Assertions per cell.**
- 200/2xx when expected; the right 403 vs 401 otherwise (401 = no auth, 403 = auth but insufficient role).
- Audit log row written for every mutation attempt regardless of outcome (read from `console_db::queries::audit::list` after each).

**Extra scenarios outside the matrix:**
- `3.i` Bearer token revoked between calls — second call returns 401.
- `3.ii` Cookie after `must_change_password=true` — only `/api/auth/change-password` accepts (tests the D2 fix from perf-opt §12.4).
- `3.iii` Rate-limited login — 5 bad attempts within 1 minute locks the account; 6th returns 423.

**Runtime-auth drift note.** Add a `// DRIFT:` comment at the top of the file citing perf-opt §11.4's recommendation — if/when the runtime starts enforcing a richer auth policy on `Bind`, this suite is the natural place to extend. Matches the drift-filing pattern from §13.7.7.

**Effort.** 90–120 min. Matrix + 3 extras + audit-log assertion helper.

**Commit.** `test(integration): §13.7.8 I3 — auth_matrix (3×3×5 rstest + edge cases)`.

---

### 3.4 I4 `ws_chaos.rs` (expands `chaos.rs`)

**Policy.** Do NOT rename `chaos.rs` — T7.4/T7.5/T7.6 live there with git history. Add `ws_chaos.rs` as a new file focused on the WebSocket transport layer.

**Deliverables:**

| # | Scenario | Assertion |
|---|---|---|
| 4.1 | 256-cap exhaustion forces `Lagged`; client recovers via REST replay | Direct push into `SessionMonitorHandle::broadcast_tx` with N=512 frames (per perf-opt §11.5 T7.8: loopback TCP absorbs small bursts, push direct). Slow consumer receives a `control/lag` frame; subsequent `/api/sessions/{sid}/trail?since=<seq>` returns the dropped frames |
| 4.2 | Abrupt disconnect mid-event | Client closes socket during a known-in-progress frame; server's WS handler cleans up without holding a broadcast receiver slot open (check `broadcast_tx.receiver_count()` drops by 1 within 500 ms) |
| 4.3 | Malformed frame injection — client sends text instead of JSON | Server closes the connection with a 1008 policy-violation close code; server logs a warn but keeps serving other clients |
| 4.4 | Gap-fill replay correctness | Drop 5 frames (seq 10–14), replay via REST; client reassembles 0–20 in order; no duplicates |
| 4.5 | Slow consumer without disconnect | Client reads at 1 frame/s while server emits 100 frames/s; eventually receives `Lagged`; assertion: client never gets partial frames — every emitted frame is either fully received or skipped |

**Perf-opt lesson baked in (§11.5):** RSS is not a useful per-task leak metric in this suite. Instead, assert `JoinHandle::is_finished()` on the monitor task at teardown (should be `false` if no terminal event, `true` if yes). Any leak would typically leak the join handle too.

**Effort.** 75–90 min. Five tests. Reuses `client.open_ws` from `TestClient`.

**Commit.** `test(integration): §13.7.8 I4 — ws_chaos (Lagged recovery, malformed, gap-fill)`.

---

### 3.5 I5 `taxonomy_reload.rs`

**Deliverables:**

| # | Scenario | Setup | Assertion |
|---|---|---|---|
| 5.1 | Reload while sessions active — pinned taxonomy held | Start session, snapshot its taxonomy pointer; POST `/api/taxonomy/reload` with a new vertical added server-side | Session's monitor still reads old taxonomy via its captured `Arc<ArcSwap<TaxonomyIndex>>` snapshot (per `event_enricher::EventEnricher` — it uses `taxonomy.load()` per-event, so it should reflect the new index for *future* events but not retroactively alter past enrichment). Assert: the new vertical shows up on `/api/verticals` immediately; the running session's workspace metadata is unchanged |
| 5.2 | Reload with new vertical added | Mock REST returns manifest set {v1, v2} → reload → mock returns {v1, v2, v3} → reload again | `/api/verticals` returns 3 entries; `index.verticals` count = 3 post-swap |
| 5.3 | Reload with vertical removed | Mock returns {v1, v2} → reload returns {v1} | `/api/verticals` returns 1; existing v2-bound session continues running (no immediate failure); a warning is recorded (observable via `tracing_test` or the response body's `warnings` array) |
| 5.4 | Reload with changed `context_schema` for an existing vertical | Mock returns v1 with schema A → reload with v1 schema B (added required field) | Post-reload: `/api/verticals/v1` returns schema B; a new session creation that omits the new field returns a validation error; a running session (created under schema A) is unaffected |
| 5.5 | Reload with mock REST returning 500 | Mock REST /v1/verticals returns 500 | Response: `{status: "failed", error: "...", counts: null, warnings: []}` per `taxonomy.rs:38`; the pre-reload index is preserved (`ArcSwap` not swapped); subsequent `/api/verticals` returns the same set as before the failed reload |

**Requires.** `console-test-support::mock_rest` needs a way to dynamically swap the vertical set between reloads. Currently `MockRest` takes fixtures at construction. Plan: add a `set_verticals(Vec<VerticalManifest>)` method that atomic-swaps the held list (perf-opt-style `ArcSwap` pattern).

**Effort.** 75–90 min. Five tests + small `MockRest` upgrade.

**Commit.** `test(integration): §13.7.8 I5 — taxonomy_reload + mock-rest hot-swap`.

---

## 4. `performance-optimization.md` watch list

Every suite below should produce either a PASS-path assertion or a WATCH-for addition to perf-opt §6 depending on what shows up. Keeping this explicit so drift-filing happens as part of each commit, not at the end.

**Existing patterns to guard against regression (use as authoring prompts):**

| Pattern | Perf-opt ref | Which I-suite is most likely to trip it |
|---|---|---|
| Rust-enum-as-i32 offset | §11.1, §11.4 | I2 (recovery reads `WorkspaceState`), I5 (vertical manifest decode) |
| Schema-vs-type drift (NOT NULL vs `Option`) | §9.1 | I2 (seeds sessions rows directly — will hit schema if column types diverge) |
| Cleanup-looks-right-but-leaks (broadcast receiver, join handle) | §11.5, §3.3 | I4 (the whole suite's concern) |
| Spec-vs-impl drift (feature described, not implemented) | §2.5, §12.1/§12.4 | I3 (runtime auth path — already flagged above) |
| Async cascade from signature change | §11.3 | None directly; but keep an eye while editing shared helpers |

**New findings expected to land in perf-opt during this workstream:**

Add a new `§13` section header pre-commit with empty subsections per suite so the file's shape is established up front. Each I-commit fills in its own subsection if it finds something. Examples:
- §13.1 — I1 findings (empty until something surfaces).
- §13.2 — I2 findings.
- …
- §13.5 — I5 findings.

**Pre-work (free, ~5 min) at the start:** append the `§13` shell to `performance-optimization.md` as part of I1's commit so future I-commits have a home.

**Specific items to look for — based on the audit's recommendations:**

1. Perf-opt §11.4 P0 enum-as-i32 audit across the runtime. If I2's recovery test surfaces the third instance of this bug on a different enum, file it as §13.2 there and fix it in a sibling commit (do not fold into the test commit). This is the "full-fix" principle: test that proves the bug stays in the test commit, actual fix goes in a separate commit.

2. Perf-opt §11.3 async cascade. If `Coordinator::handle_event` gets a new `.await` during this work (unlikely but possible), enumerate call sites in the commit message.

3. Perf-opt §12.5 `ProfilesPage` Create-New click — independent, not exercised by this workstream. Leave for §12.5 follow-up.

## 5. Verification matrix + CI

**Local verification (per suite):**

```bash
cargo test -p console-integration --test launch_failure_matrix
cargo test -p console-integration --test recovery_matrix
cargo test -p console-integration --test auth_matrix
cargo test -p console-integration --test ws_chaos
cargo test -p console-integration --test taxonomy_reload
```

Then:

```bash
cargo test -p console-integration            # full suite must be green
cargo clippy -p console-integration --all-targets -- -D warnings
cargo fmt -p console-integration --check
```

**CI:** no workflow changes needed. `ci-console.yml` already runs `cargo test -p console-integration`; the new files are picked up automatically. Integration-test walltime will grow — call out if any single suite breaks 5 s walltime in the commit message.

**Acceptance matrix:**

| Suite | Existing tests | New tests | Expected walltime |
|---|---|---|---|
| `lifecycle.rs` | 3 | 0 | ~2 s (unchanged) |
| `chaos.rs` | 3 | 0 | ~3 s (unchanged — T7.7/T7.8 direct-push patterns) |
| `cross_session.rs` | 3 | 0 | ~1 s (unchanged) |
| `llm_stub_e2e.rs` | 2 | 0 | 0.28 s (unchanged) |
| `launch_failure_matrix.rs` (I1) | 0 | 10 | ~3 s |
| `recovery_matrix.rs` (I2) | 0 | 7 | ~2 s |
| `auth_matrix.rs` (I3) | 0 | ~48 (45 matrix + 3 extra) | ~2 s |
| `ws_chaos.rs` (I4) | 0 | 5 | ~2 s |
| `taxonomy_reload.rs` (I5) | 0 | 5 | ~1 s |
| **Total** | 11 | ~75 | < 15 s |

## 6. Phasing + commit strategy

Ordered for fast-unblocked flow: pre-work first (shared), then the two suites that need it (I1 → I2), then the three independent suites in any order.

| Phase | Items | Commits | Est. effort |
|---|---|---|---|
| **P0** (pre-work) | `mock_coordinator.rs` + `TestClient` helpers + perf-opt §13 shell | 2 (infra, perf-opt-shell) | 30–45 min |
| **P1** (launch failures) | I1 | 1 | 75–90 min |
| **P2** (recovery) | I2 | 1 | 60–75 min |
| **P3** (auth) | I3 | 1 | 90–120 min |
| **P4** (ws chaos) | I4 | 1 | 75–90 min |
| **P5** (taxonomy reload) | I5 | 1 + MockRest upgrade | 75–90 min |
| **P6** (close) | AUDIT §13.5 line removal + SEED refresh + `/tmp` cleanup | 1 | 15 min |

**Total: 7–9 h** (slightly above the audit's 4–6 h estimate because P0 pre-work + perf-opt drift-filing wasn't in the original scope, and the runtime auth path drift in I3 needs explicit handling).

**Commit discipline:**
- Pre-work commit lands alone — no test changes mixed in. Makes bisect-through-this-workstream clean if a later test regresses.
- Each I-suite is its own commit (`test(integration): §13.7.8 IN — <scope>`).
- Any drift-fixing commits (e.g., if I2 surfaces an enum-offset bug): separate commit, `fix(<crate>): …`, landed BEFORE the test commit that proves it. Test commit then un-skips / re-asserts the fixed behavior.
- AUDIT/SEED closure is the last commit; no code changes.

**No draft PR needed.** Push straight to `dev` per the §13.7.7 precedent — `push: branches: [main, dev]` fires all four workflows on every push.

## 7. Non-goals / deferred

- **Runtime-side `Bind` auth hardening.** Flagged in I3's header comment; perf-opt §11 P0 audit recommendation stands. Not a §13.7.8 deliverable.
- **Playwright `--coverage` merge.** Tracked under §13.7.7 follow-ups.
- **`stryker` for TS mutation testing.** Out of scope; deferred per `wcon-mutation-testing.md` §2.2.
- **Integration-suite parallelism.** Current tests are `#[tokio::test]` serialized via cargo test's default single-binary run. Unless walltime blows past 30 s, no need to touch.
- **§13.7.10 Codecov monthly ratchet.** Still deferred — needs 2–3 `main` merges to settle the baseline; this plan doesn't change that.

## 8. References

| Doc | Relationship |
|---|---|
| `AUDIT-2026-04-15.md` §13.7.8 | Source deliverables (I1–I5). |
| `wacp-console/performance-optimization.md` §9, §11, §12 | Drift patterns to guard against + the pre-existing §11 P0 enum-audit recommendation. |
| `wacp-console/integration/src/{runtime_harness,console_harness,test_client}.rs` | Harness API reused by every new suite. |
| `wacp-console/integration/tests/chaos.rs` (T7.5 `ForwardingMockCoordinator`) | Prior-art mock for I1's failure injection. |
| `wacp-console/integration/tests/llm_stub_e2e.rs` | Reference shape for a clean I-style file (222 lines, 2 tests, strict assertions). |
| `impl/ci-cleanup-2.7-plan.md` | Commit-discipline template (per-phase commits, no bypasses, drift captured). |

*wacp-platform — §13.7.8 plan, drafted 2026-04-18 after §13.7.7 closed. Plan status: draft; execution pending.*
