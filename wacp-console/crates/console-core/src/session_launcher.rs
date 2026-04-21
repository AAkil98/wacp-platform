//! Session launch orchestration — W2 of the wiring plan.
//!
//! Replaces the pre-W2 SQLite-only `LAUNCHING → ACTIVE` transition with the
//! real gRPC sequence against `CoordinatorService`. See
//! `impl/archive/notes/w2-proto-shapes.md` for the proto-level contract the launcher
//! relies on; see `wacp-console/specs/coding/wcon-w2-launch-flow.md` for the
//! coding spec.
//!
//! Sequence:
//! ```text
//!   Step 1  SubmitGoal                  → root_workspace_id (= sessions.coordinator_workspace_id)
//!   Step 2  Decompose (N tasks)         → task_ids[N]
//!   Step 3  Dispatch × N                → workspace_ids[N]
//!   Step 4  (no-op; Dispatch carries the directive)
//!   Step 5  Finalize transaction        → sessions.state = 'active'
//! ```
//!
//! On failure at or after Step 3, every created workspace is `AbortWorkspace`d
//! (tolerating individual failures). Session transitions `LAUNCHING → FAILED`
//! with `reason = "launch_step_{step}: {original_reason}"`.

use std::sync::Arc;

use console_db::DbPool;
use console_db::queries::{session_assignments, sessions};
use console_runtime::grpc_pool::GrpcPool;
use console_runtime::proto::{self as proto, coordinator_service_client::CoordinatorServiceClient};
use serde::Serialize;
use tonic::transport::Channel;
use tracing::{info, warn};
use uuid::Uuid;

use crate::session_state;

/// Outcome of a successful launch attempt.
#[derive(Debug)]
pub enum LaunchOutcome {
    Active {
        coordinator_workspace_id: String,
        assignments: Vec<LaunchedAssignment>,
    },
    /// Already past LAUNCHING — idempotency signal; handler returns 409.
    AlreadyActive { state: String },
}

#[derive(Debug, Clone)]
pub struct LaunchedAssignment {
    pub assignment_id: String,
    pub workspace_id: String,
}

/// Identifiable step within the launch sequence. Kept as a typed enum so
/// callers can pattern-match without string inspection.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchStep {
    SubmitGoal,
    Decompose,
    Dispatch,
    Finalize,
}

impl LaunchStep {
    fn as_str(self) -> &'static str {
        match self {
            LaunchStep::SubmitGoal => "submit_goal",
            LaunchStep::Decompose => "decompose",
            LaunchStep::Dispatch => "dispatch",
            LaunchStep::Finalize => "finalize",
        }
    }
}

impl std::fmt::Display for LaunchStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("session {0} not found")]
    SessionNotFound(String),

    #[error("session in unexpected state: expected LAUNCHING, got {0}")]
    UnexpectedState(String),

    #[error("session has no assignments")]
    NoAssignments,

    #[error("coordinator service channel not available")]
    PoolUnavailable,

    #[error("step {step}: {reason}")]
    Step {
        step: LaunchStep,
        reason: String,
        source: Option<tonic::Status>,
        recoverable: bool,
    },

    #[error("database error: {0}")]
    Db(String),
}

impl LaunchError {
    /// Short machine-readable code describing this error — used for the
    /// session row's `failure_reason` column (stored on the session as part
    /// of audit / state context).
    pub fn reason_code(&self) -> String {
        match self {
            LaunchError::SessionNotFound(_) => "session_not_found".into(),
            LaunchError::UnexpectedState(s) => format!("unexpected_state_{s}"),
            LaunchError::NoAssignments => "no_assignments".into(),
            LaunchError::PoolUnavailable => "pool_unavailable".into(),
            LaunchError::Step { step, reason, .. } => {
                format!("launch_{}: {reason}", step.as_str())
            }
            LaunchError::Db(msg) => format!("db: {msg}"),
        }
    }
}

/// Payload embedded in the initial directive envelope for each dispatched
/// workspace. Worker agents parse this to pick up LLM config + tool scope.
/// The shape is console-internal and may evolve; workers that don't
/// recognize a field should ignore it.
#[derive(Debug, Serialize)]
pub struct DirectivePayload<'a> {
    pub role: &'a str,
    pub llm_provider: &'a str,
    pub llm_model: &'a str,
    pub llm_temperature: Option<f64>,
    pub llm_max_tokens: Option<i64>,
    pub autonomy: &'a str,
    pub tools: Vec<String>,
    pub session_context: Option<&'a str>,
    pub slot_position: i64,
}

pub struct SessionLauncher {
    pool: Arc<GrpcPool>,
    db: DbPool,
}

impl SessionLauncher {
    pub fn new(pool: Arc<GrpcPool>, db: DbPool) -> Self {
        Self { pool, db }
    }

    /// Execute the full launch. Idempotent on session_id: a session whose
    /// state is already past LAUNCHING returns `AlreadyActive` without any
    /// gRPC traffic.
    pub async fn launch(&self, session_id: &str) -> Result<LaunchOutcome, LaunchError> {
        // ----- Load state -----
        let session = sessions::get_by_id(&self.db, session_id)
            .await
            .map_err(|e| LaunchError::Db(e.to_string()))?
            .ok_or_else(|| LaunchError::SessionNotFound(session_id.to_string()))?;

        if session.state != session_state::LAUNCHING {
            // Terminal or back-pre-launching states are idempotent no-ops.
            if matches!(
                session.state.as_str(),
                session_state::ACTIVE
                    | session_state::COMPLETED
                    | session_state::FAILED
                    | session_state::CANCELLED
            ) {
                return Ok(LaunchOutcome::AlreadyActive {
                    state: session.state,
                });
            }
            return Err(LaunchError::UnexpectedState(session.state));
        }

        let assignments = session_assignments::list_by_session(&self.db, session_id)
            .await
            .map_err(|e| LaunchError::Db(e.to_string()))?;
        if assignments.is_empty() {
            self.mark_failed(session_id, "no_assignments").await;
            return Err(LaunchError::NoAssignments);
        }

        // ----- Acquire coordinator client -----
        let mut coord = self
            .pool
            .coordinator()
            .await
            .ok_or(LaunchError::PoolUnavailable)?;

        // ----- Step 1: SubmitGoal -----
        let description = session
            .name
            .clone()
            .unwrap_or_else(|| session.workflow.clone());
        let context_bytes = session
            .context
            .as_deref()
            .map(|s| s.as_bytes().to_vec())
            .unwrap_or_default();

        let submit_req = proto::SubmitGoalRequest {
            description: description.clone(),
            context: context_bytes.clone(),
            client_request_id: Uuid::new_v4().to_string(),
        };
        let submit_started = std::time::Instant::now();
        let submit_resp = match coord.submit_goal(submit_req).await {
            Ok(r) => r.into_inner(),
            Err(status) => {
                let err = self
                    .step_error(LaunchStep::SubmitGoal, &status, vec![])
                    .await;
                self.mark_failed(session_id, &err.reason_code()).await;
                return Err(err);
            }
        };
        info!(
            session_id = %session_id,
            step = LaunchStep::SubmitGoal.as_str(),
            duration_ms = submit_started.elapsed().as_millis() as u64,
            root_workspace_id = %submit_resp.root_workspace_id,
            "launch step ok"
        );
        let root_ws = submit_resp.root_workspace_id;

        // ----- Step 2: Decompose -----
        let mut task_defs = Vec::with_capacity(assignments.len());
        let mut directive_payloads = Vec::with_capacity(assignments.len());
        for asgn in &assignments {
            let (payload_bytes, tools, directive) =
                build_directive(&self.db, &session, asgn).await?;
            task_defs.push(proto::TaskDefinition {
                name: format!("{}:{}", session.workflow, asgn.role_ref),
                description: format!("role {} slot {}", asgn.role_ref, asgn.slot_position),
                depends_on: vec![],
                role: asgn.role_ref.clone(),
                directive_payload: payload_bytes.clone(),
                tools: tools.clone(),
            });
            directive_payloads.push(directive);
        }
        let decompose_req = proto::DecomposeRequest {
            tasks: task_defs,
            client_request_id: Uuid::new_v4().to_string(),
        };
        let decompose_started = std::time::Instant::now();
        let decompose_resp = match coord.decompose(decompose_req).await {
            Ok(r) => r.into_inner(),
            Err(status) => {
                let err = self
                    .step_error(LaunchStep::Decompose, &status, vec![root_ws.clone()])
                    .await;
                self.mark_failed(session_id, &err.reason_code()).await;
                return Err(err);
            }
        };
        if decompose_resp.task_ids.len() != assignments.len() {
            // Partial decompose — rollback the root workspace.
            let err = LaunchError::Step {
                step: LaunchStep::Decompose,
                reason: format!(
                    "runtime decomposed {} of {} tasks",
                    decompose_resp.task_ids.len(),
                    assignments.len()
                ),
                source: None,
                recoverable: false,
            };
            self.rollback(vec![root_ws.clone()]).await;
            self.mark_failed(session_id, &err.reason_code()).await;
            return Err(err);
        }
        info!(
            session_id = %session_id,
            step = LaunchStep::Decompose.as_str(),
            duration_ms = decompose_started.elapsed().as_millis() as u64,
            task_count = decompose_resp.task_ids.len(),
            "launch step ok"
        );

        // ----- Step 3: Dispatch × N -----
        let mut launched: Vec<LaunchedAssignment> = Vec::with_capacity(assignments.len());
        let mut created_workspaces: Vec<String> = vec![root_ws.clone()];
        for (i, asgn) in assignments.iter().enumerate() {
            let directive = &directive_payloads[i];
            let dispatch_req = proto::DispatchRequest {
                task_id: decompose_resp.task_ids[i].clone(),
                role: asgn.role_ref.clone(),
                directive_payload: directive.payload_bytes.clone(),
                tools: directive.tools.clone(),
                budget: directive.budget,
                client_request_id: Uuid::new_v4().to_string(),
            };
            let dispatch_started = std::time::Instant::now();
            let ws_id = match coord.dispatch(dispatch_req).await {
                Ok(r) => r.into_inner().workspace_id,
                Err(status) => {
                    let err = self
                        .step_error(LaunchStep::Dispatch, &status, created_workspaces.clone())
                        .await;
                    self.mark_failed(session_id, &err.reason_code()).await;
                    return Err(err);
                }
            };
            info!(
                session_id = %session_id,
                step = LaunchStep::Dispatch.as_str(),
                duration_ms = dispatch_started.elapsed().as_millis() as u64,
                assignment = %asgn.id,
                workspace_id = %ws_id,
                "launch step ok"
            );
            created_workspaces.push(ws_id.clone());
            launched.push(LaunchedAssignment {
                assignment_id: asgn.id.clone(),
                workspace_id: ws_id,
            });
        }

        // ----- Step 5: Finalize -----
        let finalize_started = std::time::Instant::now();
        if let Err(e) = self.finalize(session_id, &root_ws, &launched).await {
            // Finalize failed after workspaces exist — full rollback.
            let err = LaunchError::Step {
                step: LaunchStep::Finalize,
                reason: format!("finalize_db_failed: {e}"),
                source: None,
                recoverable: false,
            };
            self.rollback(created_workspaces).await;
            self.mark_failed(session_id, &err.reason_code()).await;
            return Err(err);
        }
        info!(
            session_id = %session_id,
            step = LaunchStep::Finalize.as_str(),
            duration_ms = finalize_started.elapsed().as_millis() as u64,
            "launch step ok"
        );

        Ok(LaunchOutcome::Active {
            coordinator_workspace_id: root_ws,
            assignments: launched,
        })
    }

    /// Build a `LaunchError::Step` and issue rollback for workspaces in scope.
    async fn step_error(
        &self,
        step: LaunchStep,
        status: &tonic::Status,
        rollback_workspaces: Vec<String>,
    ) -> LaunchError {
        let recoverable = matches!(
            status.code(),
            tonic::Code::Unavailable
                | tonic::Code::DeadlineExceeded
                | tonic::Code::ResourceExhausted
        );
        let reason = format!("{}: {}", status.code(), status.message());
        if !rollback_workspaces.is_empty() {
            self.rollback(rollback_workspaces).await;
        }
        LaunchError::Step {
            step,
            reason,
            source: Some(status.clone()),
            recoverable,
        }
    }

    /// Abort every workspace in `workspaces` (tolerate individual failures).
    async fn rollback(&self, workspaces: Vec<String>) {
        let Some(mut coord) = self.pool.coordinator().await else {
            warn!(
                count = workspaces.len(),
                "launch rollback skipped: coordinator channel unavailable"
            );
            return;
        };
        for ws in workspaces {
            let req = proto::AbortWorkspaceRequest {
                workspace_id: ws.clone(),
                reason: "launch_rollback".into(),
                client_request_id: Uuid::new_v4().to_string(),
            };
            match coord.abort_workspace(req).await {
                Ok(_) => info!(workspace_id = %ws, "rollback aborted workspace"),
                Err(e) => {
                    warn!(workspace_id = %ws, error = %e, "rollback abort failed (tolerated)")
                }
            }
        }
    }

    /// Transition session to FAILED with `reason`. Best-effort — we only log
    /// on db error because the caller has already decided the launch failed.
    async fn mark_failed(&self, session_id: &str, reason: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        if let Err(e) = sessions::transition_state(
            &self.db,
            session_id,
            session_state::LAUNCHING,
            session_state::FAILED,
            &now,
        )
        .await
        {
            warn!(session_id, error = %e, reason, "mark_failed: transition error");
        } else {
            info!(session_id, reason, "session marked FAILED");
        }
    }

    /// Single transaction: write coordinator_workspace_id + per-assignment
    /// workspace_ids + state transition LAUNCHING → ACTIVE.
    async fn finalize(
        &self,
        session_id: &str,
        root_workspace_id: &str,
        launched: &[LaunchedAssignment],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.db.begin().await?;
        let now = chrono::Utc::now().to_rfc3339();

        let rows_affected = sqlx::query(
            "UPDATE sessions
               SET state = ?, coordinator_workspace_id = ?, launched_at = ?
             WHERE id = ? AND state = ?",
        )
        .bind(session_state::ACTIVE)
        .bind(root_workspace_id)
        .bind(&now)
        .bind(session_id)
        .bind(session_state::LAUNCHING)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            // Someone else moved the session out from under us (e.g. cancel).
            tx.rollback().await.ok();
            return Err(sqlx::Error::RowNotFound);
        }

        for la in launched {
            sqlx::query("UPDATE session_assignments SET workspace_id = ? WHERE id = ?")
                .bind(&la.workspace_id)
                .bind(&la.assignment_id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

/// Intermediate tuple: (payload_bytes, tool_list, directive_holder).
struct PreparedDirective {
    payload_bytes: Vec<u8>,
    tools: Vec<String>,
    budget: Option<proto::ResourceBudget>,
}

async fn build_directive(
    db: &DbPool,
    session: &console_db::queries::sessions::SessionRow,
    asgn: &console_db::queries::session_assignments::SessionAssignmentRow,
) -> Result<(Vec<u8>, Vec<String>, PreparedDirective), LaunchError> {
    let profile = console_db::queries::profiles::get_current(db, &asgn.profile_id)
        .await
        .map_err(|e| LaunchError::Db(e.to_string()))?;

    let (llm_provider, llm_model, llm_temperature, llm_max_tokens, autonomy, tools) = match &profile
    {
        Some(p) => {
            let allow: Vec<String> = p
                .tool_allowlist
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let deny: Vec<String> = p
                .tool_denylist
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let effective: Vec<String> = allow
                .into_iter()
                .filter(|t| !deny.iter().any(|d| d == t))
                .collect();
            (
                p.llm_provider.clone(),
                p.llm_model.clone(),
                p.llm_temperature,
                p.llm_max_tokens,
                p.autonomy.clone(),
                effective,
            )
        }
        None => (
            String::new(),
            String::new(),
            None,
            None,
            "supervised".into(),
            vec![],
        ),
    };

    let directive = DirectivePayload {
        role: &asgn.role_ref,
        llm_provider: &llm_provider,
        llm_model: &llm_model,
        llm_temperature,
        llm_max_tokens,
        autonomy: &autonomy,
        tools: tools.clone(),
        session_context: session.context.as_deref(),
        slot_position: asgn.slot_position,
    };
    let payload_bytes = serde_json::to_vec(&directive)
        .map_err(|e| LaunchError::Db(format!("directive serialization failed: {e}")))?;

    // Budget resolution: per-assignment override → else session → else zero.
    let budget = proto::ResourceBudget {
        max_tokens: asgn
            .budget_max_tokens
            .or(session.budget_max_tokens)
            .map(|v| v.max(0) as u64)
            .unwrap_or(0),
        max_wall_time_ms: asgn
            .budget_max_wall_time_ms
            .or(session.budget_max_wall_time_ms)
            .map(|v| v.max(0) as u64)
            .unwrap_or(0),
        max_storage_bytes: 0,
        max_network_bytes: 0,
        max_cost_micros: asgn
            .budget_max_cost_micros
            .or(session.budget_max_cost_micros)
            .map(|v| v.max(0) as u64)
            .unwrap_or(0),
        warning_threshold: profile
            .as_ref()
            .and_then(|p| p.budget_warning_threshold)
            .map(|v| v as f32)
            .unwrap_or(0.8),
    };

    let prepared = PreparedDirective {
        payload_bytes: payload_bytes.clone(),
        tools: tools.clone(),
        budget: Some(budget),
    };
    Ok((payload_bytes, tools, prepared))
}

// Silence unused-import warnings on the client type until call sites appear.
#[allow(dead_code)]
fn _ensure_client_type_used(_c: CoordinatorServiceClient<Channel>) {}

#[cfg(test)]
#[path = "session_launcher_tests.rs"]
mod tests;
