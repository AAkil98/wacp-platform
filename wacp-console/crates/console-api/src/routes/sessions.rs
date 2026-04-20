//! Session endpoints — create, list, get, patch, assignments, launch, cancel, clone.
//!
//! Spec: `wcon-sessions` §2, `wcon-api` §8

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::{
    Json, Router,
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;

use console_core::audit::{AuditAction, AuditEntry, log_audit};
use console_core::authorizer::{self, Action};
use console_core::error::ConsoleError;
use console_core::event_enricher::EventEnricher;
use console_core::refusal_synthesizer::RefusalSynthesizer;
use console_core::session_launcher::{LaunchError, LaunchOutcome, LaunchStep, SessionLauncher};
use console_core::session_monitor::{self, MonitorConfig, WorkspaceSet};
use console_core::session_state;
use console_core::session_validation::{self, SessionValidationInput};
use console_db::queries::{session_assignments, sessions};

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::{Auth, RequestContext, is_bearer_auth, validate_csrf};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/{id}", get(get_session).patch(update_session))
        .route(
            "/api/sessions/{id}/assignments",
            axum::routing::put(set_assignments),
        )
        .route("/api/sessions/{id}/launch", post(launch_session))
        .route("/api/sessions/{id}/cancel", post(cancel_session))
        .route("/api/sessions/{id}/clone", post(clone_session))
}

// --- List ---

#[derive(Deserialize)]
struct ListParams {
    state: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    cursor: Option<String>,
}

fn default_limit() -> i64 {
    50
}

async fn list_sessions(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    authorizer::authorize(&auth, Action::ListOwnSessions).map_err(ApiError::from)?;

    let is_admin = auth.console_role == console_core::ConsoleRole::Admin;
    let rows = if is_admin {
        sessions::list_all(
            &state.db,
            params.state.as_deref(),
            params.limit,
            params.cursor.as_deref(),
        )
        .await
    } else {
        sessions::list_by_owner(
            &state.db,
            &auth.user_id,
            params.state.as_deref(),
            params.limit,
            params.cursor.as_deref(),
        )
        .await
    }
    .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    let result: Vec<serde_json::Value> = rows
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "name": s.name,
                "owner_user_id": s.owner_user_id,
                "vertical": s.vertical,
                "workflow": s.workflow,
                "state": s.state,
                "created_at": s.created_at,
                "launched_at": s.launched_at,
                "closed_at": s.closed_at,
            })
        })
        .collect();

    Ok(Json(result))
}

// --- Create ---

#[derive(Deserialize)]
struct CreateSessionRequest {
    name: Option<String>,
    vertical: String,
    workflow: String,
    context: Option<serde_json::Value>,
    budget_max_cost_micros: Option<i64>,
    budget_max_tokens: Option<i64>,
    budget_max_wall_time_ms: Option<i64>,
}

async fn create_session(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Json(body): Json<CreateSessionRequest>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    authorizer::authorize(&auth, Action::CreateSession).map_err(ApiError::from)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let context_str = body
        .context
        .as_ref()
        .map(|c| serde_json::to_string(c).unwrap_or_else(|_| "{}".into()));

    let row = sessions::SessionRow {
        id: id.clone(),
        name: body.name,
        owner_user_id: auth.user_id.clone(),
        vertical: body.vertical,
        workflow: body.workflow,
        context: context_str,
        coordinator_workspace_id: None,
        state: session_state::CONFIGURING.into(),
        created_at: now,
        launched_at: None,
        closed_at: None,
        budget_max_cost_micros: body.budget_max_cost_micros,
        budget_max_tokens: body.budget_max_tokens,
        budget_max_wall_time_ms: body.budget_max_wall_time_ms,
    };

    sessions::insert_session(&state.db, &row)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    // Auto-derive slots from vertical roles
    let index = state.taxonomy.load();
    let slots = session_validation::derive_slots(&index, &row.vertical);
    for (role_ref, position) in &slots {
        let assignment = session_assignments::SessionAssignmentRow {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: id.clone(),
            role_ref: role_ref.clone(),
            stage_id: None,
            slot_position: *position,
            profile_id: None,
            profile_version: None,
            workspace_id: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
        };
        session_assignments::insert_assignment(&state.db, &assignment)
            .await
            .ok();
    }

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::SessionCreate,
            target_id: id.clone(),
            detail: Some(serde_json::json!({"vertical": row.vertical, "workflow": row.workflow})),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "id": id,
            "state": session_state::CONFIGURING,
            "slots": slots.len(),
        })),
    ))
}

// --- Get ---

async fn get_session(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = sessions::get_by_id(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &id))?;

    check_session_read_access(&auth, &session)?;

    let assignments = session_assignments::list_by_session(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    let assignments_json: Vec<serde_json::Value> = assignments
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "role_ref": a.role_ref,
                "slot_position": a.slot_position,
                "profile_id": a.profile_id,
                "profile_version": a.profile_version,
                "workspace_id": a.workspace_id,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "id": session.id,
        "name": session.name,
        "owner_user_id": session.owner_user_id,
        "vertical": session.vertical,
        "workflow": session.workflow,
        "context": session.context.as_ref().and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok()),
        "state": session.state,
        "created_at": session.created_at,
        "launched_at": session.launched_at,
        "closed_at": session.closed_at,
        "coordinator_workspace_id": session.coordinator_workspace_id,
        "budget_max_cost_micros": session.budget_max_cost_micros,
        "budget_max_tokens": session.budget_max_tokens,
        "budget_max_wall_time_ms": session.budget_max_wall_time_ms,
        "assignments": assignments_json,
    })))
}

// --- Patch (update config, configuring state only) ---

#[derive(Deserialize)]
struct UpdateSessionRequest {
    name: Option<String>,
    context: Option<serde_json::Value>,
    budget_max_cost_micros: Option<i64>,
    budget_max_tokens: Option<i64>,
    budget_max_wall_time_ms: Option<i64>,
}

async fn update_session(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateSessionRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let session = sessions::get_by_id(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &id))?;

    check_session_write_access(&auth, &session)?;

    if session.state != session_state::CONFIGURING {
        return Err(ApiError::from(ConsoleError::Conflict(
            "Session can only be modified in configuring state".into(),
        )));
    }

    if let Some(ctx) = body.context {
        let ctx_str = serde_json::to_string(&ctx).unwrap_or_else(|_| "{}".into());
        sessions::update_context(&state.db, &id, &ctx_str)
            .await
            .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;
    }

    if let Some(name) = &body.name {
        sqlx::query("UPDATE sessions SET name = ? WHERE id = ? AND state = 'configuring'")
            .bind(name)
            .bind(&id)
            .execute(&state.db)
            .await
            .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;
    }

    if body.budget_max_cost_micros.is_some()
        || body.budget_max_tokens.is_some()
        || body.budget_max_wall_time_ms.is_some()
    {
        sqlx::query(
            "UPDATE sessions SET budget_max_cost_micros = COALESCE(?, budget_max_cost_micros),
             budget_max_tokens = COALESCE(?, budget_max_tokens),
             budget_max_wall_time_ms = COALESCE(?, budget_max_wall_time_ms)
             WHERE id = ? AND state = 'configuring'",
        )
        .bind(body.budget_max_cost_micros)
        .bind(body.budget_max_tokens)
        .bind(body.budget_max_wall_time_ms)
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --- Set Assignments ---

#[derive(Deserialize)]
struct AssignmentInput {
    role_ref: String,
    profile_id: String,
    profile_version: Option<i64>,
    budget_max_cost_micros: Option<i64>,
    budget_max_tokens: Option<i64>,
    budget_max_wall_time_ms: Option<i64>,
}

async fn set_assignments(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Vec<AssignmentInput>>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let session = sessions::get_by_id(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &id))?;

    check_session_write_access(&auth, &session)?;

    if session.state != session_state::CONFIGURING {
        return Err(ApiError::from(ConsoleError::Conflict(
            "Assignments can only be set in configuring state".into(),
        )));
    }

    // Pin profile versions: if not specified, use current version
    let mut rows = Vec::new();
    for (i, input) in body.iter().enumerate() {
        let version = if let Some(v) = input.profile_version {
            v
        } else {
            let profile = console_db::queries::profiles::get_current(&state.db, &input.profile_id)
                .await
                .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
                .ok_or_else(|| ApiError::not_found("profile", &input.profile_id))?;
            profile.version
        };

        rows.push(session_assignments::SessionAssignmentRow {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: id.clone(),
            role_ref: input.role_ref.clone(),
            stage_id: None,
            slot_position: i as i64,
            profile_id: Some(input.profile_id.clone()),
            profile_version: Some(version),
            workspace_id: None,
            budget_max_cost_micros: input.budget_max_cost_micros,
            budget_max_tokens: input.budget_max_tokens,
            budget_max_wall_time_ms: input.budget_max_wall_time_ms,
        });
    }

    session_assignments::replace_assignments(&state.db, &id, &rows)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --- Launch ---

async fn launch_session(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let session = sessions::get_by_id(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &id))?;

    check_session_write_access(&auth, &session)?;

    if session.state != session_state::CONFIGURING {
        return Err(ApiError::from(ConsoleError::Conflict(
            "Session can only be launched from configuring state".into(),
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();

    // Transition to validating
    sessions::transition_state(
        &state.db,
        &id,
        session_state::CONFIGURING,
        session_state::VALIDATING,
        &now,
    )
    .await
    .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    // Validate
    let index = state.taxonomy.load();
    let validation_input = SessionValidationInput {
        session_id: &id,
        vertical: &session.vertical,
        workflow: &session.workflow,
        context: session.context.as_deref(),
        budget_max_cost_micros: session.budget_max_cost_micros,
        budget_max_tokens: session.budget_max_tokens,
        budget_max_wall_time_ms: session.budget_max_wall_time_ms,
    };

    let result = session_validation::validate_session(&validation_input, &index, &state.db).await;

    if !result.is_valid() {
        // Back to configuring
        sessions::transition_state(
            &state.db,
            &id,
            session_state::VALIDATING,
            session_state::CONFIGURING,
            &now,
        )
        .await
        .ok();
        return Err(ApiError::from(ConsoleError::Validation {
            message: "Session validation failed".into(),
            violations: result
                .violations
                .into_iter()
                .map(|v| console_core::error::Violation {
                    field: v.slot.map(|s| format!("slot_{s}")),
                    code: v.code.to_string(),
                    message: v.message,
                    value: None,
                })
                .collect(),
            warnings: vec![],
        }));
    }

    // Transition to launching
    sessions::transition_state(
        &state.db,
        &id,
        session_state::VALIDATING,
        session_state::LAUNCHING,
        &now,
    )
    .await
    .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    // Execute the 5-step gRPC launch sequence. On failure the launcher has
    // already transitioned the session to FAILED and rolled back any created
    // workspaces — the handler only maps the error to an HTTP status.
    let launcher = SessionLauncher::new(state.grpc_pool.clone(), state.db.clone());
    match launcher.launch(&id).await {
        Ok(LaunchOutcome::Active {
            coordinator_workspace_id,
            assignments,
        }) => {
            // W3 — spawn the session monitor so subsequent WS connects see
            // live frames. Insertion into active_sessions races with any
            // concurrent attempt to insert (there isn't one in practice —
            // launch is state-guarded), but the RwLock keeps the observable
            // state coherent.
            let ws_set = WorkspaceSet::new(
                coordinator_workspace_id.clone(),
                assignments.iter().map(|a| a.workspace_id.clone()),
            );
            let enricher = EventEnricher::new(state.taxonomy.clone());
            let refusals = RefusalSynthesizer::new();
            let (handle, _join) = session_monitor::spawn(
                id.clone(),
                ws_set,
                state.grpc_pool.clone(),
                state.db.clone(),
                enricher,
                refusals,
                MonitorConfig::default(),
            );
            let active = state.active_sessions.clone();
            active.write().await.insert(id.clone(), handle);
            // Watchdog task that removes the handle when the monitor exits
            // on its own (terminal state / fatal reconnect cap).
            let session_id_watchdog = id.clone();
            tokio::spawn(async move {
                let _ = _join.await;
                active.write().await.remove(&session_id_watchdog);
            });

            log_audit(
                &state.db,
                AuditEntry {
                    user_id: auth.user_id.clone(),
                    action: AuditAction::SessionLaunch,
                    target_id: id.clone(),
                    detail: Some(serde_json::json!({
                        "coordinator_workspace_id": coordinator_workspace_id,
                    })),
                    ip: ctx.ip,
                    user_agent: ctx.user_agent,
                },
            )
            .await
            .ok();
            Ok(Json(serde_json::json!({
                "id": id,
                "state": session_state::ACTIVE,
                "coordinator_workspace_id": coordinator_workspace_id,
            })))
        }
        Ok(LaunchOutcome::AlreadyActive { state }) => Err(ApiError::from(ConsoleError::Conflict(
            format!("session already in state '{state}'"),
        ))),
        Err(err) => Err(map_launch_error(err)),
    }
}

fn map_launch_error(err: LaunchError) -> ApiError {
    match err {
        LaunchError::SessionNotFound(id) => ApiError::not_found("session", &id),
        LaunchError::UnexpectedState(s) => ApiError::from(ConsoleError::Conflict(format!(
            "session in state '{s}' cannot be launched"
        ))),
        LaunchError::NoAssignments => ApiError::from(ConsoleError::Validation {
            message: "session has no assignments".into(),
            violations: vec![],
            warnings: vec![],
        }),
        LaunchError::PoolUnavailable => ApiError::from(ConsoleError::Runtime {
            message: "coordinator channel unavailable".into(),
            grpc_status: None,
            service: Some("CoordinatorService".into()),
            method: None,
        }),
        LaunchError::Step {
            step,
            reason,
            source,
            recoverable,
        } => {
            let grpc_status = source.as_ref().map(|s| format!("{:?}", s.code()));
            let method = match step {
                LaunchStep::SubmitGoal => Some("SubmitGoal"),
                LaunchStep::Decompose => Some("Decompose"),
                LaunchStep::Dispatch => Some("Dispatch"),
                LaunchStep::Finalize => None,
            }
            .map(String::from);
            if recoverable {
                ApiError {
                    status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    body: crate::error::ApiErrorBody {
                        error: "runtime_unavailable".into(),
                        message: format!("launch {step}: {reason}"),
                        details: Some(serde_json::json!({
                            "step": step,
                            "grpc_status": grpc_status,
                            "service": "CoordinatorService",
                            "method": method,
                            "recoverable": true,
                        })),
                    },
                }
            } else {
                ApiError::from(ConsoleError::Runtime {
                    message: format!("launch {step}: {reason}"),
                    grpc_status,
                    service: Some("CoordinatorService".into()),
                    method,
                })
            }
        }
        LaunchError::Db(msg) => ApiError::from(ConsoleError::Database(msg)),
    }
}

// --- Cancel ---

#[derive(Deserialize, Default)]
struct CancelRequest {
    #[serde(default)]
    reason: Option<String>,
}

async fn cancel_session(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Option<Json<CancelRequest>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    let req = body.map(|Json(b)| b).unwrap_or_default();

    let session = sessions::get_by_id(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &id))?;

    check_session_write_access(&auth, &session)?;

    let cancel_action =
        session_state::cancel_action_for_state(&session.state).ok_or_else(|| {
            ApiError::from(ConsoleError::Conflict(format!(
                "Session in '{}' state cannot be cancelled",
                session.state
            )))
        })?;

    // Required-abort path: call the runtime FIRST. A runtime rejection bubbles
    // up as a 5xx and the session stays ACTIVE — otherwise the operator is
    // left with a session marked CANCELLED locally but still alive on the
    // runtime. BestEffortAbort runs after the DB transition (tolerated).
    if matches!(cancel_action, session_state::CancelAction::AbortWorkspace) {
        let workspace_id = session.coordinator_workspace_id.as_deref().ok_or_else(|| {
            ApiError::from(ConsoleError::Conflict(
                "Session has no coordinator workspace; cannot abort".into(),
            ))
        })?;
        let mut coord =
            state.grpc_pool.coordinator().await.ok_or_else(|| {
                ApiError::runtime_unavailable("CoordinatorService", "AbortWorkspace")
            })?;
        coord
            .abort_workspace(console_runtime::proto::AbortWorkspaceRequest {
                workspace_id: workspace_id.to_string(),
                reason: req.reason.clone().unwrap_or_default(),
                client_request_id: String::new(),
            })
            .await
            .map_err(|s| ApiError::from_tonic(s, "CoordinatorService", "AbortWorkspace"))?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let previous_state = sessions::cancel(&state.db, &id, &now)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    if previous_state.is_none() {
        return Err(ApiError::from(ConsoleError::Conflict(
            "Session is already in a terminal state".into(),
        )));
    }

    // Best-effort cleanup AFTER the local cancel succeeded. If the runtime
    // happens to reject (or be unreachable), the session is still CANCELLED
    // and the operator's only loss is a possibly-leaked partial launch on
    // the coordinator side — recovery on next restart will reconcile.
    if matches!(cancel_action, session_state::CancelAction::BestEffortAbort)
        && let Some(workspace_id) = session.coordinator_workspace_id.as_deref()
        && let Some(mut coord) = state.grpc_pool.coordinator().await
    {
        let _ = coord
            .abort_workspace(console_runtime::proto::AbortWorkspaceRequest {
                workspace_id: workspace_id.to_string(),
                reason: req.reason.clone().unwrap_or_default(),
                client_request_id: String::new(),
            })
            .await;
    }

    // Drop the active monitor (if any) — the WorkspaceChange event we just
    // triggered would also tear it down, but explicit shutdown avoids
    // racing on the next handle.subscribe() that a reconnecting WS client
    // might attempt before the stream catches up.
    if let Some(handle) = state.active_sessions.write().await.remove(&id) {
        handle.shutdown().await;
    }

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::SessionCancel,
            target_id: id.clone(),
            detail: Some(serde_json::json!({
                "from_state": previous_state,
                "reason": req.reason,
            })),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    Ok(Json(serde_json::json!({
        "id": id,
        "state": session_state::CANCELLED,
    })))
}

// --- Clone ---

async fn clone_session(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    authorizer::authorize(&auth, Action::CreateSession).map_err(ApiError::from)?;

    let source = sessions::get_by_id(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &id))?;

    check_session_read_access(&auth, &source)?;

    let new_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let new_row = sessions::SessionRow {
        id: new_id.clone(),
        name: source.name.map(|n| format!("{n} (copy)")),
        owner_user_id: auth.user_id.clone(),
        vertical: source.vertical,
        workflow: source.workflow,
        context: source.context,
        coordinator_workspace_id: None,
        state: session_state::CONFIGURING.into(),
        created_at: now,
        launched_at: None,
        closed_at: None,
        budget_max_cost_micros: source.budget_max_cost_micros,
        budget_max_tokens: source.budget_max_tokens,
        budget_max_wall_time_ms: source.budget_max_wall_time_ms,
    };

    sessions::insert_session(&state.db, &new_row)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    // Copy assignments (reset workspace_id)
    let source_assignments = session_assignments::list_by_session(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    for a in &source_assignments {
        let new_assignment = session_assignments::SessionAssignmentRow {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: new_id.clone(),
            role_ref: a.role_ref.clone(),
            stage_id: a.stage_id.clone(),
            slot_position: a.slot_position,
            profile_id: a.profile_id.clone(),
            profile_version: a.profile_version,
            workspace_id: None,
            budget_max_cost_micros: a.budget_max_cost_micros,
            budget_max_tokens: a.budget_max_tokens,
            budget_max_wall_time_ms: a.budget_max_wall_time_ms,
        };
        session_assignments::insert_assignment(&state.db, &new_assignment)
            .await
            .ok();
    }

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::SessionCreate,
            target_id: new_id.clone(),
            detail: Some(serde_json::json!({"cloned_from": id})),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "id": new_id,
            "state": session_state::CONFIGURING,
        })),
    ))
}

// --- Helpers ---

fn check_session_read_access(auth: &Auth, session: &sessions::SessionRow) -> Result<(), ApiError> {
    if auth.console_role == console_core::ConsoleRole::Admin {
        return Ok(());
    }
    if session.owner_user_id == auth.user_id {
        return Ok(());
    }
    Err(ApiError::from(ConsoleError::Forbidden(
        "You do not have access to this session".into(),
    )))
}

fn check_session_write_access(auth: &Auth, session: &sessions::SessionRow) -> Result<(), ApiError> {
    if auth.console_role == console_core::ConsoleRole::Admin {
        return Ok(());
    }
    if session.owner_user_id == auth.user_id {
        return Ok(());
    }
    Err(ApiError::from(ConsoleError::Forbidden(
        "Only the owner or an admin can modify this session".into(),
    )))
}

// ---------------------------------------------------------------------------
// Tests — W5 cancel handler
//
// Cover the four cancel arms (NoOp / BestEffortAbort / AbortWorkspace) plus
// the post-success monitor shutdown. Configurable mock CoordinatorService
// drives the AbortWorkspace outcomes; happy and Unavailable + missing-coord
// + already-cancelled paths are exercised end-to-end via tower::ServiceExt.
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod cancel_tests;
