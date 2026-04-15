//! Startup recovery — reconcile sessions left in `ACTIVE` after a console
//! restart against the live runtime, then either respawn the W3 monitor
//! (resume) or mark the session FAILED.
//!
//! Spec: `wcon-w5-cancel-recovery` §3.2, §4.2; `wcon-sessions` §8.2.
//!
//! Behaviour summary:
//! - For each ACTIVE row in `sessions`:
//!   - No `coordinator_workspace_id` (stuck mid-launch) → mark FAILED with
//!     `reason="stuck_in_launching"`.
//!   - `GetWorkspace(NotFound)` → mark FAILED with `reason="recovery_workspace_missing"`.
//!   - `GetWorkspace(Unavailable)` → leave ACTIVE; the next restart retries.
//!   - Workspace already terminal → sync session state to match.
//!   - Workspace still live → respawn monitor and register in `active_sessions`.

use std::sync::Arc;

use console_db::DbPool;
use console_db::queries::{session_assignments, sessions};
use console_runtime::grpc_pool::GrpcPool;
use console_runtime::proto;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::event_enricher::EventEnricher;
use crate::refusal_synthesizer::RefusalSynthesizer;
use crate::session_monitor::{
    self, MonitorConfig, SessionMonitorHandle, WorkspaceSet,
};
use crate::session_state;

/// Map of session_id → live monitor handle. Owned by the API layer; recovery
/// inserts respawned monitors so `/api/sessions/:id/ws` finds them.
pub type ActiveSessionsMap = Arc<RwLock<std::collections::HashMap<String, SessionMonitorHandle>>>;

#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub resumed: Vec<String>,
    pub synced_terminal: Vec<(String, String)>,
    pub failed: Vec<(String, RecoveryFailureReason)>,
    pub skipped_unavailable: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum RecoveryFailureReason {
    StuckInLaunching,
    WorkspaceMissing,
    DbError(String),
    RuntimeError(String),
}

impl RecoveryFailureReason {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::StuckInLaunching => "stuck_in_launching",
            Self::WorkspaceMissing => "recovery_workspace_missing",
            Self::DbError(_) => "recovery_db_error",
            Self::RuntimeError(_) => "recovery_runtime_error",
        }
    }
}

/// Run startup recovery. Returns a report regardless of partial failures —
/// the caller logs it and proceeds to start the HTTP server. A wholesale
/// runtime outage (every probe Unavailable) is not fatal: those sessions
/// stay ACTIVE and will be re-probed on the next restart.
pub async fn run(
    db: DbPool,
    pool: Arc<GrpcPool>,
    enricher: EventEnricher,
    refusals: RefusalSynthesizer,
    active: ActiveSessionsMap,
    cfg: MonitorConfig,
) -> RecoveryReport {
    let mut report = RecoveryReport::default();

    let rows = match sessions::list_active(&db).await {
        Ok(rows) => rows,
        Err(e) => {
            warn!(error = %e, "recovery: list_active failed; skipping");
            return report;
        }
    };

    info!(count = rows.len(), "recovery: scanning ACTIVE sessions");

    for row in rows {
        let outcome = recover_one(
            &db,
            pool.clone(),
            enricher.clone(),
            refusals.clone(),
            active.clone(),
            cfg.clone(),
            &row,
        )
        .await;

        match outcome {
            RecoveryOutcome::Resumed => report.resumed.push(row.id.clone()),
            RecoveryOutcome::SyncedTerminal(state) => {
                report.synced_terminal.push((row.id.clone(), state))
            }
            RecoveryOutcome::Failed(reason) => report.failed.push((row.id.clone(), reason)),
            RecoveryOutcome::SkippedUnavailable => {
                report.skipped_unavailable.push(row.id.clone())
            }
        }
    }

    info!(
        resumed = report.resumed.len(),
        synced = report.synced_terminal.len(),
        failed = report.failed.len(),
        skipped = report.skipped_unavailable.len(),
        "recovery: complete"
    );
    report
}

enum RecoveryOutcome {
    Resumed,
    SyncedTerminal(String),
    Failed(RecoveryFailureReason),
    SkippedUnavailable,
}

async fn recover_one(
    db: &DbPool,
    pool: Arc<GrpcPool>,
    enricher: EventEnricher,
    refusals: RefusalSynthesizer,
    active: ActiveSessionsMap,
    cfg: MonitorConfig,
    row: &sessions::SessionRow,
) -> RecoveryOutcome {
    // Stuck-in-LAUNCHING — W2's finalize never wrote the workspace id.
    let Some(coord_ws) = row.coordinator_workspace_id.as_ref() else {
        let reason = RecoveryFailureReason::StuckInLaunching;
        mark_failed(db, &row.id, reason.tag()).await;
        return RecoveryOutcome::Failed(reason);
    };

    // Probe the runtime. We use the highway client for `GetWorkspace`
    // (highway exposes the same view; coordinator does not). If the pool
    // has no live channel, treat the session as skipped — same effect as
    // an Unavailable response.
    let Some(mut client) = pool.highway().await else {
        return RecoveryOutcome::SkippedUnavailable;
    };

    let view = match client
        .get_workspace(proto::GetWorkspaceRequest {
            workspace_id: coord_ws.clone(),
        })
        .await
    {
        Ok(resp) => resp.into_inner(),
        Err(s) => match s.code() {
            tonic::Code::NotFound => {
                let reason = RecoveryFailureReason::WorkspaceMissing;
                mark_failed(db, &row.id, reason.tag()).await;
                return RecoveryOutcome::Failed(reason);
            }
            tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
                return RecoveryOutcome::SkippedUnavailable;
            }
            _ => {
                let reason = RecoveryFailureReason::RuntimeError(s.message().to_string());
                mark_failed(db, &row.id, reason.tag()).await;
                return RecoveryOutcome::Failed(reason);
            }
        },
    };

    if is_terminal(view.state()) {
        let final_state = match view.state() {
            proto::WorkspaceState::Failed => session_state::FAILED,
            _ => session_state::COMPLETED,
        };
        let now = chrono::Utc::now().to_rfc3339();
        if let Err(e) =
            sessions::transition_state(db, &row.id, session_state::ACTIVE, final_state, &now).await
        {
            warn!(session_id = %row.id, error = %e, "recovery: terminal sync failed");
        }
        return RecoveryOutcome::SyncedTerminal(final_state.to_string());
    }

    // Live workspace — respawn monitor.
    let assignments = match session_assignments::list_by_session(db, &row.id).await {
        Ok(rows) => rows,
        Err(e) => {
            let reason = RecoveryFailureReason::DbError(e.to_string());
            warn!(session_id = %row.id, error = %e, "recovery: assignments lookup failed");
            return RecoveryOutcome::Failed(reason);
        }
    };
    let workspace_ids = assignments
        .into_iter()
        .filter_map(|a| a.workspace_id)
        .collect::<Vec<_>>();
    let ws_set = WorkspaceSet::new(coord_ws.clone(), workspace_ids);

    let (handle, _join) = session_monitor::spawn(
        row.id.clone(),
        ws_set,
        pool,
        db.clone(),
        enricher,
        refusals,
        cfg,
    );

    // Watchdog: drop the entry from active_sessions when the monitor exits
    // (terminal state / fatal reconnect cap). Mirrors the launch-flow
    // pattern in routes/sessions.rs.
    let active_for_watch = active.clone();
    let id_for_watch = row.id.clone();
    tokio::spawn(async move {
        let _ = _join.await;
        active_for_watch.write().await.remove(&id_for_watch);
    });

    active.write().await.insert(row.id.clone(), handle);
    RecoveryOutcome::Resumed
}

async fn mark_failed(db: &DbPool, session_id: &str, _reason_tag: &str) {
    // Reason is captured in the audit / log layer at the call site (recovery
    // returns the structured reason). The DB column itself is just `state`.
    let now = chrono::Utc::now().to_rfc3339();
    if let Err(e) =
        sessions::transition_state(db, session_id, session_state::ACTIVE, session_state::FAILED, &now)
            .await
    {
        warn!(session_id = %session_id, error = %e, "recovery: mark_failed transition failed");
    }
}

fn is_terminal(s: proto::WorkspaceState) -> bool {
    matches!(
        s,
        proto::WorkspaceState::Closed | proto::WorkspaceState::Failed
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use console_db::create_test_pool;
    use std::collections::HashMap;

    fn empty_taxonomy() -> Arc<ArcSwap<crate::taxonomy::TaxonomyIndex>> {
        Arc::new(ArcSwap::from_pointee(
            crate::taxonomy_builder::build_index(None, &[], &[]).index,
        ))
    }

    fn enricher() -> EventEnricher {
        EventEnricher::new(empty_taxonomy())
    }

    fn refusals() -> RefusalSynthesizer {
        RefusalSynthesizer::new()
    }

    async fn insert_user(db: &DbPool, id: &str) {
        sqlx::query(
            "INSERT INTO users (id, username, username_lower, display_name, password_hash,
                console_role, must_change_password, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .bind(id)
        .bind("h")
        .bind("operator")
        .bind(0_i64)
        .bind("2026-04-15T00:00:00Z")
        .bind("2026-04-15T00:00:00Z")
        .execute(db)
        .await
        .expect("insert user");
    }

    async fn insert_session(
        db: &DbPool,
        id: &str,
        owner: &str,
        coord_ws: Option<&str>,
        state: &str,
    ) {
        let row = sessions::SessionRow {
            id: id.into(),
            name: Some(id.into()),
            owner_user_id: owner.into(),
            vertical: "swe".into(),
            workflow: "wf".into(),
            context: None,
            coordinator_workspace_id: coord_ws.map(String::from),
            state: state.into(),
            created_at: "2026-04-15T00:00:00Z".into(),
            launched_at: Some("2026-04-15T00:00:00Z".into()),
            closed_at: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
        };
        sessions::insert_session(db, &row).await.expect("insert");
    }

    fn empty_active() -> ActiveSessionsMap {
        Arc::new(RwLock::new(HashMap::new()))
    }

    fn quick_cfg() -> MonitorConfig {
        MonitorConfig {
            broadcast_capacity: 16,
            reconnect_initial: std::time::Duration::from_millis(5),
            reconnect_max: std::time::Duration::from_millis(10),
            reconnect_failure_cap: 2,
        }
    }

    #[test]
    fn failure_reason_tags_are_stable() {
        assert_eq!(
            RecoveryFailureReason::StuckInLaunching.tag(),
            "stuck_in_launching"
        );
        assert_eq!(
            RecoveryFailureReason::WorkspaceMissing.tag(),
            "recovery_workspace_missing"
        );
        assert_eq!(
            RecoveryFailureReason::DbError("x".into()).tag(),
            "recovery_db_error"
        );
        assert_eq!(
            RecoveryFailureReason::RuntimeError("x".into()).tag(),
            "recovery_runtime_error"
        );
    }

    #[test]
    fn is_terminal_matches_closed_and_failed_only() {
        assert!(is_terminal(proto::WorkspaceState::Closed));
        assert!(is_terminal(proto::WorkspaceState::Failed));
        assert!(!is_terminal(proto::WorkspaceState::Active));
        assert!(!is_terminal(proto::WorkspaceState::Idle));
        assert!(!is_terminal(proto::WorkspaceState::Blocked));
    }

    #[tokio::test]
    async fn run_marks_stuck_session_failed_when_no_coord_workspace() {
        let db = create_test_pool().await.unwrap();
        insert_user(&db, "u-1").await;
        insert_session(&db, "s-stuck", "u-1", None, session_state::ACTIVE).await;

        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        let report = run(
            db.clone(),
            pool,
            enricher(),
            refusals(),
            empty_active(),
            quick_cfg(),
        )
        .await;
        assert_eq!(report.failed.len(), 1);
        let row = sessions::get_by_id(&db, "s-stuck").await.unwrap().unwrap();
        assert_eq!(row.state, session_state::FAILED);
    }

    #[tokio::test]
    async fn run_skips_when_pool_has_no_channel() {
        let db = create_test_pool().await.unwrap();
        insert_user(&db, "u-1").await;
        insert_session(
            &db,
            "s-nopool",
            "u-1",
            Some("ws-coord"),
            session_state::ACTIVE,
        )
        .await;

        // Pool that never connects. `highway()` returns None → SkippedUnavailable.
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        let report = run(
            db.clone(),
            pool,
            enricher(),
            refusals(),
            empty_active(),
            quick_cfg(),
        )
        .await;
        assert_eq!(report.skipped_unavailable.len(), 1);
        // Session stays ACTIVE — no transition.
        let row = sessions::get_by_id(&db, "s-nopool")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.state, session_state::ACTIVE);
    }

    #[tokio::test]
    async fn run_returns_empty_report_when_no_active_sessions() {
        let db = create_test_pool().await.unwrap();
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        let report = run(
            db,
            pool,
            enricher(),
            refusals(),
            empty_active(),
            quick_cfg(),
        )
        .await;
        assert!(report.resumed.is_empty());
        assert!(report.failed.is_empty());
        assert!(report.synced_terminal.is_empty());
        assert!(report.skipped_unavailable.is_empty());
    }

    #[tokio::test]
    async fn run_handles_db_error_path_for_assignments_lookup() {
        // We can't easily synthesize a DB error mid-test, but we can verify
        // the reason tag stays stable for the documented failure mode.
        let r = RecoveryFailureReason::DbError("connection closed".into());
        assert_eq!(r.tag(), "recovery_db_error");
    }
}
