---
id: wacp-integration-deferred-scenarios-plan
type: impl
status: draft
created: 2026-04-22T17:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, integration, testing, chaos, mock-highway]
depends_on: [wacp-test-cleanup-followups-plan]
---

# Integration Deferred Scenarios — Plan

> **Triggering finding:** HEALTH-LOG §13.2 (two deferred sub-scenarios in the §13.7.8 I2 recovery_matrix suite — Workspace-`Failed`-state probe needs a mock highway; DB-degraded boot needs a read-failure fault mode). Both were explicitly tracked as "deferred, see §13.2 follow-up" with in-file notes in `recovery_matrix.rs`.
> **Target branch:** `testing/integration-deferred-scenarios` (topic).
> **Rough effort:** ~3–5h — **medium** confidence. Phase P2 (`MockHighwayService` scriptability) is the largest unknown; the rest is straightforward composition once the mock + harness overrides exist.
> **Not in scope:** §13.3 runtime-auth matrix (blocked on runtime gaining real auth — spec work), §13.5 `context_schema` evolution (explicitly gated on "if that becomes a priority"), §13.4 gap-fill replay (explicitly recommended strike — closed via parallel plan `health-log-residual-plan.md` P3), §14.1 cross-harness `pick_port` TOCTOU residual (never observed).

## 1. Goal & Motivation

Close the two deferred sub-scenarios in `recovery_matrix.rs` that were filed as "need infrastructure that doesn't exist yet" during §13.7.8 I2. Each represents a load-bearing branch of `recovery::recover_one` that currently lacks end-to-end integration coverage:

**Scenario (a): Workspace `Failed` state → session FAILED.** HEALTH-LOG §13.2 notes: "The real runtime won't reach `WorkspaceState::Failed` without the workspace actor emitting a signal the coordinator interprets as fatal, which isn't a clean seeding path." The in-crate `#[cfg(test)]` tests in `recovery.rs:249+` cover the Rust-level mapping, but no integration test exercises the wire path. Required infra: a `MockHighwayService` that can script `GetWorkspace` responses per-workspace (stable proto-wire-correct `state = Failed`), plus a `ConsoleHarness` variant that can be pointed at the mock instead of the real runtime's highway.

**Scenario (b): DB-degraded boot → `list_active` error arm.** HEALTH-LOG §13.2: "`FaultyDb::hold_write_lock` holds a WRITE lock, but `recovery::run` only reads from sessions on boot — the write lock doesn't block the read path." A different fault-injection mode is needed. Good news: `console-db::testing::closed_pool` already exists and returns `PoolClosed` on every operation — sufficient to drive the read-failure arm. Scenario may collapse to a composition-only phase if `closed_pool` turns out to be wiring-compatible with `ConsoleHarness`.

**Cost of inaction.** The `_ => COMPLETED` fallback in `recover_one` (the non-`Failed`/non-`Closed` terminal branch for hypothetical future states) is covered by `terminal_workspace_aborted_marked_failed` (landed 2026-04-18) + `terminal_workspace_closed_marked_completed` (landed 2026-04-22, test-cleanup-followups B.1+B.2). The direct `Failed → FAILED` branch and the `list_active` error arm are the only uncovered recovery branches today. Both are latent until a real incident exercises them — standard shape of bugs this codebase has caught by writing the missing integration test.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| P1 | Add `ConsoleHarness::spawn_with_db_and_highway(rt, db, highway_url)` variant | ~30–45 min | — | `cargo check -p console-integration --tests` compiles; existing `spawn_with_db_and_rest` usage in `taxonomy_reload.rs` unaffected |
| P2 | Extend `MockHighwayService` in `console-test-support/src/mock_grpc.rs` with scriptable per-workspace `GetWorkspace` responses (ArcSwap pattern from `mock_rest::RestState`) | ~60–90 min | — | New `ScriptableHighway` wrapper or extended `MockHighwayService` compiles; small unit test proves `script_workspace(id, state)` returns the scripted state via a client roundtrip |
| P3 | Wire §13.2 (a) — add `workspace_failed_marked_session_failed` integration test using P1 + P2 | ~30–45 min | P1, P2 | `cargo test -p console-integration --test recovery_matrix` is 9/9 (up from 8/8) |
| P4 | Wire §13.2 (b) — add `db_degraded_list_active_error_path` (or similar) integration test using `console-db::testing::closed_pool` | ~30–45 min | P1 (may not need P2) | `cargo test -p console-integration --test recovery_matrix` is 10/10 |
| P5 | Closeout: HEALTH-LOG §13.2 strikes (both deferred scenarios), AUDIT §13.9.10 closure entry, plan archive, footer update | ~20–30 min | P1 + P2 + P3 + P4 | Plan archived; `grep -n "Not covered" recovery_matrix.rs` no longer flags the two sub-scenarios |

## 3. Deliverables — per phase

### 3.1 Phase P1 — `ConsoleHarness::spawn_with_db_and_highway`

Mirror of the existing `spawn_with_db_and_rest` pattern (landed in §13.7.8 I5 `taxonomy_reload.rs`). Target file: `wacp-console/integration/src/console_harness.rs`.

Shape:
```rust
pub async fn spawn_with_db_and_highway(
    rt: &RuntimeHarness,
    db: DbPool,
    highway_url: String,
) -> Result<Self, ConsoleHarnessError> {
    // Same as spawn_with_db but override AppState.runtime_config.highway_address
    // with `highway_url` before constructing the grpc pool.
}
```

**Key question to resolve in-phase.** Does the grpc pool re-read `runtime_config.highway_address` on each RPC, or is the highway channel established at pool construction? If the latter, the override must happen BEFORE `GrpcPool::new` — same constraint as `spawn_with_db_and_rest`. Check `console-runtime/src/grpc_pool.rs` at plan pickup.

**Back-compat.** Existing `spawn_with_db`, `spawn`, `spawn_with_db_and_rest` signatures unchanged.

Verification: `cargo check -p console-integration --tests`. Existing I5 `taxonomy_reload` tests still pass (4/4).

### 3.2 Phase P2 — scriptable `MockHighwayService`

**Current state.** `wacp-console/crates/console-test-support/src/mock_grpc.rs` has a skeleton `MockHighwayService` with `Unimplemented` for most RPCs. For this plan, only `GetWorkspace` needs real behaviour.

**Target shape.** Mirror `mock_rest::RestState`'s `Arc<ArcSwap<HashMap<WorkspaceId, State>>>` pattern:

```rust
#[derive(Default)]
pub struct ScriptableHighway {
    workspaces: Arc<ArcSwap<HashMap<String, proto::WorkspaceState>>>,
    // optionally: other GetWorkspace fields (role, parent, ...) if tests need them
}

impl ScriptableHighway {
    pub fn script_workspace(&self, id: &str, state: proto::WorkspaceState) { ... }
    pub fn clear(&self) { ... }
}

#[tonic::async_trait]
impl HighwayService for ScriptableHighway {
    async fn get_workspace(
        &self,
        req: Request<proto::GetWorkspaceRequest>,
    ) -> Result<Response<proto::WorkspaceView>, Status> {
        let id = req.into_inner().workspace_id;
        let map = self.workspaces.load();
        match map.get(&id) {
            Some(state) => Ok(Response::new(proto::WorkspaceView {
                id,
                state: *state as i32,
                ..Default::default()
            })),
            None => Err(Status::not_found(format!("unknown workspace: {id}"))),
        }
    }
    // All other RPCs: Unimplemented.
}
```

**Spawn helper.** Add `pub async fn spawn(port: u16) -> (JoinHandle, ScriptableHighway)` mirroring `InjectableCoordinator::spawn` — so tests can: pick a port, start the server, get a handle, pass the URL to `ConsoleHarness::spawn_with_db_and_highway`.

**Enum-cast discipline.** `state: *state as i32` is the direct cast — proto enum variants are safe here because the test constructs proto-side values directly (not internal Rust enums). Unlike HEALTH-LOG §11.1 / §11.4 (internal → proto cast hazard), this is proto → i32 wire, which is always correct.

**Smoke test.** New `tests/mock_highway_smoke.rs`: script one workspace, call `get_workspace` via `HighwayServiceClient`, assert state matches. ~2 tests.

Verification: `cargo test -p console-integration --test mock_highway_smoke` passes. `cargo clippy --workspace --all-targets -- -D warnings` clean.

### 3.3 Phase P3 — `workspace_failed_marked_session_failed`

New test in `recovery_matrix.rs`:

```rust
#[tokio::test]
async fn workspace_failed_marked_session_failed() {
    // P1: runtime still needed for console-harness' other gRPC channels
    // (coord, agent, rest). Only the highway channel gets mocked.
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");

    // P2: mock highway, scripted to report the target workspace as Failed
    let (_highway_task, mock_highway) = ScriptableHighway::spawn(/* port */).await;
    let failed_ws = format!("ws-{}", uuid::Uuid::new_v4());
    mock_highway.script_workspace(&failed_ws, proto::WorkspaceState::Failed);

    let db = console_db::create_test_pool().await.expect("db");
    seed_user(&db, "u-1").await;
    let sid = format!("s-{}", uuid::Uuid::new_v4());
    seed_active_session(&db, &sid, "u-1", Some(&failed_ws)).await;

    // P1: override highway URL; console now probes the mock instead of rt
    let highway_url = format!("http://[::1]:{}", /* mock highway port */);
    let console = ConsoleHarness::spawn_with_db_and_highway(&rt, db.clone(), highway_url)
        .await
        .expect("console");

    assert!(
        !console.state.active_sessions.read().await.contains_key(&sid),
        "terminal session must not have a monitor"
    );
    // Failed → FAILED per recovery.rs:173-175 (the `Failed` specific arm,
    // sibling to Closed → COMPLETED).
    assert_eq!(session_state(&db, &sid).await, session_state::FAILED);
}
```

Verification: `cargo test -p console-integration --test recovery_matrix` 9/9.

### 3.4 Phase P4 — `db_degraded_list_active_error_path`

**Strategy choice.** `console-db::testing::closed_pool` already exists and returns `PoolClosed` on every operation. Pass it as the db to `ConsoleHarness::spawn_with_db`; `recovery::run`'s first `sessions::list_active(&db)` call will fail, exercising the error arm.

**Open question at plan-pickup:** What does `recovery::run` currently do when `list_active` fails? Check at `wacp-console/crates/console-core/src/recovery.rs` — likely either (a) panics/returns error up to the caller, or (b) logs + returns empty, or (c) has some other recovery behavior. The test shape depends on the answer:

- **If (a) error-up:** Test asserts `ConsoleHarness::spawn_with_db(&rt, closed_db)` returns `ConsoleHarnessError::Recovery(...)` (or wraps the sqlx error).
- **If (b) logs + empty:** Test asserts `console.state.active_sessions.read().await.is_empty()` and no DB mutation happened (seeded session still shows its original state).

New test:
```rust
#[tokio::test]
async fn db_degraded_read_path_handled_gracefully() {
    let rt = RuntimeHarness::spawn_default().await.expect("runtime");
    let closed_db = console_db::testing::closed_pool().await;
    // ... seed nothing (pool is dead) ...
    let result = ConsoleHarness::spawn_with_db(&rt, closed_db).await;
    // Assertion depends on the answer above
    match result {
        Ok(console) => {
            // (b) — recovery must have tolerated the failure
            assert!(console.state.active_sessions.read().await.is_empty());
        }
        Err(e) => {
            // (a) — recovery error propagates
            assert!(format!("{e:?}").to_lowercase().contains("pool") || format!("{e:?}").to_lowercase().contains("closed"));
        }
    }
}
```

If neither (a) nor (b) matches reality — i.e., `recovery::run` doesn't currently handle `list_active` errors robustly — that's itself a finding to file in HEALTH-LOG §13.2 during execution. Could become a secondary fix in this plan or carve out to its own follow-up.

**`drop_reads` alternative.** If `closed_pool` doesn't exercise the right code path (e.g., recovery uses a specific query that doesn't flow through the shared `list_active` read), add a `FaultyDb::drop_reads` mode that returns `sqlx::Error::Io` on every `fetch_*` call. Scope-add of ~30 min if needed.

Verification: `cargo test -p console-integration --test recovery_matrix` 10/10.

### 3.5 Phase P5 — closeout

- **HEALTH-LOG §13.2** — strike both "Not covered (deferred, see §13.2):" bullets with resolution notes + commit SHAs.
- **HEALTH-LOG §13 header** — update "Not covered" summary if it exists.
- **AUDIT §13.7.8 closeout** (line ~715 / §12.5 reference at line 463) — update the "Four sub-scenarios deferred" prose to reflect that 2 of 4 are now closed (the remaining 2 are §13.3 runtime-auth + §13.5 context_schema, both gated on external triggers).
- **AUDIT §13.9** — add row `13.9.10` closure entry (or next available number).
- **AUDIT footer** — append `§13.9.10 closed 2026-04-?? by Claude Opus 4.7 (1M context) via impl/archive/integration-deferred-scenarios-plan.md (5 phases)`.
- **`recovery_matrix.rs` module doc** — remove the two bullet points from the "Not covered" block; keep the DB-degraded note IF the code path didn't land a fix (otherwise strike both).
- **Plan archive** — `archive-plan` skill.
- **SEED refresh** — wait for next batch boundary.

## 4. Acceptance Criteria

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo test -p console-integration --test recovery_matrix` 10/10 (up from 8/8).
- [ ] `cargo test -p console-integration --test mock_highway_smoke` passes.
- [ ] `cargo test -p console-integration --test taxonomy_reload` unaffected (4/4).
- [ ] `ConsoleHarness::spawn_with_db_and_highway` API exists + documented in module doc.
- [ ] `ScriptableHighway` (or extended `MockHighwayService`) exposes `script_workspace` + `spawn` helpers.
- [ ] HEALTH-LOG §13.2 "Not covered" bullets struck with resolution notes.
- [ ] `recovery_matrix.rs` module doc updated — at most §13.5/§13.3 deferrals remain.
- [ ] AUDIT §13.9 has closure row; §13.7.8 closeout prose updated.
- [ ] Plan moved to `impl/archive/integration-deferred-scenarios-plan.md`.

## 5. Risks / Open Questions

1. **Highway URL override may not be a clean single-setting override.** If the grpc pool's highway channel is established at pool construction (most likely), overriding `runtime_config.highway_address` after spawn won't reach the already-built client. Mitigation: add the override as a construction-time parameter to the pool — same shape as `rest_address` override. If the grpc pool accepts a pre-built channel set, that's even cleaner.
2. **`closed_pool` may not cover the recovery read path.** If `recovery::run` uses a query or connection pattern that bypasses the PoolClosed error (unlikely but possible), Phase P4 will need the `drop_reads` fault mode. Plan documents both paths; pickup session decides which applies.
3. **Test flakiness risk from highway mock + real runtime mix.** The test spawns both a real `RuntimeHarness` (for agent/coord/rest channels) and a mock highway. The console ends up with a mixed view. If `recover::run` has code paths that cross-reference highway + coordinator state (e.g., reads a workspace ID from coord, then probes highway for state, then touches DB), the cross-reference could go wrong in ways that don't happen in the real world. Mitigation: pickup session must audit `recovery::run` for cross-service calls before committing the test shape.
4. **Mock highway port allocation.** The test picks a port for the mock highway; that port must not collide with runtime ports (`pick_ports_batch(6)` in RuntimeHarness). Use `pick_ports_batch(7)` or reserve via a separate pick. Prior art in HEALTH-LOG §14.1 covered the intra-harness TOCTOU; same shape applies.
5. **Scope creep into §13.5 territory.** If while writing P3 the test needs a richer `WorkspaceView` (checkpoint_count, budget, etc.), the `ScriptableHighway` surface grows. Keep strict: only script `state`; if a test wants more fields, it's a different scenario.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| `HEALTH-LOG` §13.2 | I2 recovery_matrix — two deferred sub-scenarios | implements (closes both "Not covered" bullets) |
| `HEALTH-LOG` §14.1 | RuntimeHarness pick_port TOCTOU | informs (port-allocation pattern to follow) |
| `AUDIT-2026-04-15` §13.7.8 | Integration I1–I5 closeout | extends (removes 2 of 4 deferrals) |
| `AUDIT-2026-04-15` §13.9 | Post-audit follow-ups | extends (appends §13.9.10 row) |
| `impl/archive/audit-13-7-8-plan.md` | Prior integration + chaos plan | informs (deferral rationale; mock-rest precedent) |
| `impl/archive/test-cleanup-followups-plan.md` | Prior plan closing §11.4 follow-up | informs (SDK-based agent-driven test pattern as alternative if mock approach proves too invasive) |
| `wacp-console/crates/console-test-support/src/mock_rest.rs` | Existing ArcSwap-based scriptable REST mock | implements (same pattern for highway) |
| `wacp-console/crates/console-test-support/src/mock_grpc.rs` | Existing skeleton `MockHighwayService` | implements (extend to scriptable) |
| `wacp-console/crates/console-db/src/testing.rs` | FaultyDb + closed_pool helpers | implements (Phase P4 dep) |
| `wacp-console/crates/console-core/src/recovery.rs` | Recovery logic (`recover_one`, probe via highway::get_workspace at :149) | implements (the system under test) |
| `impl/git-strategy.md` §4 | Topic-branch naming | constrains (`testing/integration-deferred-scenarios`) |

## 7. Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| P1 | — | — | — |
| P2 | — | — | — |
| P3 | — | — | — |
| P4 | — | — | — |
| P5 | — | — | — |
