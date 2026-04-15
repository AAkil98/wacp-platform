//! Profile endpoints — CRUD, versioning, export, import, clone.
//!
//! Spec: `wcon-profiles` §2, `wcon-api` §7

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;

use console_core::audit::{AuditAction, AuditEntry, log_audit};
use console_core::authorizer::{self, Action};
use console_core::error::ConsoleError;
use console_core::profile_validation::{self, ProfileInput};
use console_core::profile_yaml;
use console_db::queries::profiles;

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::{Auth, RequestContext, is_bearer_auth, validate_csrf};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/profiles", get(list_profiles).post(create_profile))
        .route(
            "/api/profiles/{id}",
            get(get_profile).put(update_profile).delete(delete_profile),
        )
        .route("/api/profiles/{id}/versions", get(list_versions))
        .route("/api/profiles/{id}/rollback", post(rollback))
        .route("/api/profiles/{id}/clone", post(clone_profile))
        .route("/api/profiles/{id}/export", get(export_profile))
        .route("/api/profiles/import", post(import_profile))
}

// --- List ---

#[derive(Deserialize)]
struct ListParams {
    role_ref: Option<String>,
    search: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    cursor: Option<String>,
}

fn default_limit() -> i64 {
    50
}

async fn list_profiles(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    authorizer::authorize(&auth, Action::ListOwnProfiles).map_err(ApiError::from)?;

    let is_admin = auth.console_role == console_core::ConsoleRole::Admin;
    let rows = profiles::list_visible(
        &state.db,
        &auth.user_id,
        is_admin,
        params.role_ref.as_deref(),
        params.search.as_deref(),
        params.limit,
        params.cursor.as_deref(),
    )
    .await
    .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    let index = state.taxonomy.load();
    let mut result: Vec<serde_json::Value> = Vec::with_capacity(rows.len());
    for p in &rows {
        let display_name = derive_display_name(p, &auth.user_id, &state.db).await;
        let role_name = index
            .get_role(&p.role_ref)
            .map(|r| r.name.as_str())
            .unwrap_or(&p.role_ref);
        let vertical = index.get_role(&p.role_ref).and_then(|r| r.vertical.clone());

        result.push(serde_json::json!({
            "id": p.id,
            "name": p.name,
            "display_name": display_name,
            "description": p.description,
            "role_ref": p.role_ref,
            "role_name": role_name,
            "vertical": vertical,
            "autonomy": p.autonomy,
            "visibility": p.visibility,
            "owner_user_id": p.owner_user_id,
            "version": p.version,
            "created_at": p.created_at,
        }));
    }

    Ok(Json(result))
}

// --- Create ---

#[derive(Deserialize)]
struct CreateProfileRequest {
    name: String,
    description: Option<String>,
    tags: Option<Vec<String>>,
    role_ref: String,
    llm_provider: String,
    llm_model: String,
    llm_temperature: Option<f64>,
    llm_max_tokens: Option<i64>,
    #[serde(default = "default_autonomy")]
    autonomy: String,
    tool_allowlist: Option<Vec<String>>,
    tool_denylist: Option<Vec<String>>,
    budget_max_cost_micros: Option<i64>,
    budget_max_tokens: Option<i64>,
    budget_max_wall_time_ms: Option<i64>,
    budget_warning_threshold: Option<f64>,
    #[serde(default = "default_visibility")]
    visibility: String,
}

fn default_autonomy() -> String {
    "assisted".into()
}
fn default_visibility() -> String {
    "private".into()
}

async fn create_profile(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Json(body): Json<CreateProfileRequest>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    authorizer::authorize(&auth, Action::CreateProfile).map_err(ApiError::from)?;

    let tags_json = body
        .tags
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".into()));
    let allow_json = body
        .tool_allowlist
        .as_ref()
        .map(|a| serde_json::to_string(a).unwrap_or_else(|_| "[]".into()));
    let deny_json = body
        .tool_denylist
        .as_ref()
        .map(|d| serde_json::to_string(d).unwrap_or_else(|_| "[]".into()));

    let index = state.taxonomy.load();
    let input = ProfileInput {
        name: &body.name,
        description: body.description.as_deref(),
        tags: tags_json.as_deref(),
        role_ref: &body.role_ref,
        llm_provider: &body.llm_provider,
        llm_model: &body.llm_model,
        llm_temperature: body.llm_temperature,
        llm_max_tokens: body.llm_max_tokens,
        autonomy: &body.autonomy,
        tool_allowlist: allow_json.as_deref(),
        tool_denylist: deny_json.as_deref(),
        budget_max_cost_micros: body.budget_max_cost_micros,
        budget_max_tokens: body.budget_max_tokens,
        budget_max_wall_time_ms: body.budget_max_wall_time_ms,
        budget_warning_threshold: body.budget_warning_threshold,
        visibility: &body.visibility,
        owner_user_id: &auth.user_id,
        exclude_id: None,
    };

    let validation = profile_validation::validate_profile(&input, &index, &state.db).await;
    if !validation.is_valid() {
        return Err(ApiError::from(to_validation_error(&validation)));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let row = profiles::ProfileRow {
        id: id.clone(),
        version: 1,
        name: body.name.clone(),
        description: body.description,
        tags: tags_json,
        role_ref: body.role_ref,
        llm_provider: body.llm_provider,
        llm_model: body.llm_model,
        llm_temperature: body.llm_temperature,
        llm_max_tokens: body.llm_max_tokens,
        autonomy: body.autonomy,
        tool_allowlist: allow_json,
        tool_denylist: deny_json,
        budget_max_cost_micros: body.budget_max_cost_micros,
        budget_max_tokens: body.budget_max_tokens,
        budget_max_wall_time_ms: body.budget_max_wall_time_ms,
        budget_warning_threshold: body.budget_warning_threshold,
        owner_user_id: auth.user_id.clone(),
        visibility: body.visibility,
        is_current: true,
        created_at: now,
        deleted_at: None,
    };

    profiles::insert_profile(&state.db, &row)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::ProfileCreate,
            target_id: id.clone(),
            detail: Some(serde_json::json!({"name": row.name})),
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
            "version": 1,
            "warnings": validation.warnings,
        })),
    ))
}

// --- Get ---

async fn get_profile(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let profile = profiles::get_current(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("profile", &id))?;

    // Visibility check
    check_profile_read_access(&auth, &profile)?;

    let index = state.taxonomy.load();
    let display_name = derive_display_name(&profile, &auth.user_id, &state.db).await;
    let role = index.get_role(&profile.role_ref);
    let vertical = role.and_then(|r| r.vertical.clone());

    // Derive available tools and policy-gated tools
    let available_tools: Vec<&str> = role
        .map(|r| r.tools.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();
    let policy_gated: Vec<&str> = available_tools
        .iter()
        .filter(|t| index.get_tool(t).is_some_and(|tool| tool.policy.is_some()))
        .copied()
        .collect();

    Ok(Json(serde_json::json!({
        "id": profile.id,
        "version": profile.version,
        "name": profile.name,
        "display_name": display_name,
        "description": profile.description,
        "tags": profile.tags.as_ref().and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok()),
        "role_ref": profile.role_ref,
        "role_name": role.map(|r| r.name.as_str()).unwrap_or(&profile.role_ref),
        "vertical": vertical,
        "llm_provider": profile.llm_provider,
        "llm_model": profile.llm_model,
        "llm_temperature": profile.llm_temperature,
        "llm_max_tokens": profile.llm_max_tokens,
        "autonomy": profile.autonomy,
        "tool_allowlist": profile.tool_allowlist.as_ref().and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok()),
        "tool_denylist": profile.tool_denylist.as_ref().and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok()),
        "available_tools": available_tools,
        "policy_gated_tools": policy_gated,
        "budget_max_cost_micros": profile.budget_max_cost_micros,
        "budget_max_tokens": profile.budget_max_tokens,
        "budget_max_wall_time_ms": profile.budget_max_wall_time_ms,
        "budget_warning_threshold": profile.budget_warning_threshold,
        "owner_user_id": profile.owner_user_id,
        "visibility": profile.visibility,
        "created_at": profile.created_at,
    })))
}

// --- Update (new version) ---

async fn update_profile(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<CreateProfileRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let current = profiles::get_current(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("profile", &id))?;

    check_profile_write_access(&auth, &current)?;

    let tags_json = body
        .tags
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".into()));
    let allow_json = body
        .tool_allowlist
        .as_ref()
        .map(|a| serde_json::to_string(a).unwrap_or_else(|_| "[]".into()));
    let deny_json = body
        .tool_denylist
        .as_ref()
        .map(|d| serde_json::to_string(d).unwrap_or_else(|_| "[]".into()));

    let index = state.taxonomy.load();
    let input = ProfileInput {
        name: &body.name,
        description: body.description.as_deref(),
        tags: tags_json.as_deref(),
        role_ref: &body.role_ref,
        llm_provider: &body.llm_provider,
        llm_model: &body.llm_model,
        llm_temperature: body.llm_temperature,
        llm_max_tokens: body.llm_max_tokens,
        autonomy: &body.autonomy,
        tool_allowlist: allow_json.as_deref(),
        tool_denylist: deny_json.as_deref(),
        budget_max_cost_micros: body.budget_max_cost_micros,
        budget_max_tokens: body.budget_max_tokens,
        budget_max_wall_time_ms: body.budget_max_wall_time_ms,
        budget_warning_threshold: body.budget_warning_threshold,
        visibility: &body.visibility,
        owner_user_id: &current.owner_user_id,
        exclude_id: Some(&id),
    };

    let validation = profile_validation::validate_profile(&input, &index, &state.db).await;
    if !validation.is_valid() {
        return Err(ApiError::from(to_validation_error(&validation)));
    }

    let new_version = current.version + 1;
    let now = chrono::Utc::now().to_rfc3339();

    let new_row = profiles::ProfileRow {
        id: id.clone(),
        version: new_version,
        name: body.name,
        description: body.description,
        tags: tags_json,
        role_ref: body.role_ref,
        llm_provider: body.llm_provider,
        llm_model: body.llm_model,
        llm_temperature: body.llm_temperature,
        llm_max_tokens: body.llm_max_tokens,
        autonomy: body.autonomy,
        tool_allowlist: allow_json,
        tool_denylist: deny_json,
        budget_max_cost_micros: body.budget_max_cost_micros,
        budget_max_tokens: body.budget_max_tokens,
        budget_max_wall_time_ms: body.budget_max_wall_time_ms,
        budget_warning_threshold: body.budget_warning_threshold,
        owner_user_id: current.owner_user_id,
        visibility: body.visibility,
        is_current: true,
        created_at: now,
        deleted_at: None,
    };

    profiles::create_new_version(&state.db, &id, &new_row)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::ProfileUpdate,
            target_id: id,
            detail: Some(serde_json::json!({"version": new_version})),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    Ok(Json(serde_json::json!({
        "version": new_version,
        "warnings": validation.warnings,
    })))
}

// --- Delete (soft) ---

async fn delete_profile(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let profile = profiles::get_current(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("profile", &id))?;

    check_profile_write_access(&auth, &profile)?;

    let now = chrono::Utc::now().to_rfc3339();
    profiles::soft_delete(&state.db, &id, &now)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::ProfileDelete,
            target_id: id.clone(),
            detail: None,
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    // Check for non-terminal session assignments and return warnings.
    let affected =
        console_db::queries::session_assignments::find_active_sessions_for_profile(&state.db, &id)
            .await
            .unwrap_or_default();

    let warnings: Vec<serde_json::Value> = affected
        .iter()
        .map(|sid| {
            serde_json::json!({
                "session_id": sid,
                "message": format!("Session '{sid}' uses this profile in a non-terminal state"),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "warnings": warnings })))
}

// --- Versions ---

async fn list_versions(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    // Must be able to view the profile to see its versions
    let current = profiles::get_current(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    if let Some(ref p) = current {
        check_profile_read_access(&auth, p)?;
    }

    let versions = profiles::list_versions(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    let result: Vec<serde_json::Value> = versions
        .iter()
        .map(|v| {
            serde_json::json!({
                "version": v.version,
                "name": v.name,
                "role_ref": v.role_ref,
                "autonomy": v.autonomy,
                "is_current": v.is_current,
                "created_at": v.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}

// --- Rollback ---

#[derive(Deserialize)]
struct RollbackRequest {
    version: i64,
}

async fn rollback(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<RollbackRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;

    let current = profiles::get_current(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("profile", &id))?;

    check_profile_write_access(&auth, &current)?;

    // Fetch the target version
    let target = profiles::get_version(&state.db, &id, body.version)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| {
            ApiError::not_found("profile_version", &format!("{id}@v{}", body.version))
        })?;

    // Create a new version with the old content
    let new_version = current.version + 1;
    let now = chrono::Utc::now().to_rfc3339();

    let new_row = profiles::ProfileRow {
        id: id.clone(),
        version: new_version,
        name: target.name,
        description: target.description,
        tags: target.tags,
        role_ref: target.role_ref,
        llm_provider: target.llm_provider,
        llm_model: target.llm_model,
        llm_temperature: target.llm_temperature,
        llm_max_tokens: target.llm_max_tokens,
        autonomy: target.autonomy,
        tool_allowlist: target.tool_allowlist,
        tool_denylist: target.tool_denylist,
        budget_max_cost_micros: target.budget_max_cost_micros,
        budget_max_tokens: target.budget_max_tokens,
        budget_max_wall_time_ms: target.budget_max_wall_time_ms,
        budget_warning_threshold: target.budget_warning_threshold,
        owner_user_id: current.owner_user_id,
        visibility: current.visibility,
        is_current: true,
        created_at: now,
        deleted_at: None,
    };

    profiles::create_new_version(&state.db, &id, &new_row)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    log_audit(&state.db, AuditEntry {
        user_id: auth.user_id.clone(),
        action: AuditAction::ProfileUpdate,
        target_id: id,
        detail: Some(serde_json::json!({"rollback_from_version": body.version, "new_version": new_version})),
        ip: ctx.ip,
        user_agent: ctx.user_agent,
    }).await.ok();

    Ok(Json(serde_json::json!({ "version": new_version })))
}

// --- Clone ---

async fn clone_profile(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    authorizer::authorize(&auth, Action::CreateProfile).map_err(ApiError::from)?;

    let source = profiles::get_current(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("profile", &id))?;

    check_profile_read_access(&auth, &source)?;

    let new_id = uuid::Uuid::new_v4().to_string();
    let new_name = format!("{} (copy)", source.name);
    let now = chrono::Utc::now().to_rfc3339();

    // Validate the cloned profile against current taxonomy
    let index = state.taxonomy.load();
    let input = ProfileInput {
        name: &new_name,
        description: source.description.as_deref(),
        tags: source.tags.as_deref(),
        role_ref: &source.role_ref,
        llm_provider: &source.llm_provider,
        llm_model: &source.llm_model,
        llm_temperature: source.llm_temperature,
        llm_max_tokens: source.llm_max_tokens,
        autonomy: &source.autonomy,
        tool_allowlist: source.tool_allowlist.as_deref(),
        tool_denylist: source.tool_denylist.as_deref(),
        budget_max_cost_micros: source.budget_max_cost_micros,
        budget_max_tokens: source.budget_max_tokens,
        budget_max_wall_time_ms: source.budget_max_wall_time_ms,
        budget_warning_threshold: source.budget_warning_threshold,
        visibility: "private",
        owner_user_id: &auth.user_id,
        exclude_id: None,
    };

    let validation = profile_validation::validate_profile(&input, &index, &state.db).await;
    if !validation.is_valid() {
        return Err(ApiError::from(to_validation_error(&validation)));
    }

    let row = profiles::ProfileRow {
        id: new_id.clone(),
        version: 1,
        name: new_name,
        description: source.description,
        tags: source.tags,
        role_ref: source.role_ref,
        llm_provider: source.llm_provider,
        llm_model: source.llm_model,
        llm_temperature: source.llm_temperature,
        llm_max_tokens: source.llm_max_tokens,
        autonomy: source.autonomy,
        tool_allowlist: source.tool_allowlist,
        tool_denylist: source.tool_denylist,
        budget_max_cost_micros: source.budget_max_cost_micros,
        budget_max_tokens: source.budget_max_tokens,
        budget_max_wall_time_ms: source.budget_max_wall_time_ms,
        budget_warning_threshold: source.budget_warning_threshold,
        owner_user_id: auth.user_id.clone(),
        visibility: "private".into(),
        is_current: true,
        created_at: now,
        deleted_at: None,
    };

    profiles::insert_profile(&state.db, &row)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::ProfileClone,
            target_id: new_id.clone(),
            detail: Some(serde_json::json!({"source_id": id})),
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
            "version": 1,
            "warnings": validation.warnings,
        })),
    ))
}

// --- Export ---

async fn export_profile(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorizer::authorize(&auth, Action::ExportProfile).map_err(ApiError::from)?;

    let profile = profiles::get_current(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("profile", &id))?;

    check_profile_read_access(&auth, &profile)?;

    let yaml = profile_yaml::export_yaml(&profile).map_err(ApiError::from)?;

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/x-yaml")],
        yaml,
    ))
}

// --- Import ---

#[derive(Deserialize)]
struct ImportRequest {
    yaml: String,
}

async fn import_profile(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Json(body): Json<ImportRequest>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    authorizer::authorize(&auth, Action::ImportProfile).map_err(ApiError::from)?;

    let data = profile_yaml::parse_import_yaml(&body.yaml).map_err(ApiError::from)?;
    let mut row = profile_yaml::import_data_to_row(&data);

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    row.id = id.clone();
    row.owner_user_id = auth.user_id.clone();
    row.created_at = now;

    // Validate against taxonomy
    let index = state.taxonomy.load();
    let input = ProfileInput {
        name: &row.name,
        description: row.description.as_deref(),
        tags: row.tags.as_deref(),
        role_ref: &row.role_ref,
        llm_provider: &row.llm_provider,
        llm_model: &row.llm_model,
        llm_temperature: row.llm_temperature,
        llm_max_tokens: row.llm_max_tokens,
        autonomy: &row.autonomy,
        tool_allowlist: row.tool_allowlist.as_deref(),
        tool_denylist: row.tool_denylist.as_deref(),
        budget_max_cost_micros: row.budget_max_cost_micros,
        budget_max_tokens: row.budget_max_tokens,
        budget_max_wall_time_ms: row.budget_max_wall_time_ms,
        budget_warning_threshold: row.budget_warning_threshold,
        visibility: "private",
        owner_user_id: &auth.user_id,
        exclude_id: None,
    };

    let validation = profile_validation::validate_profile(&input, &index, &state.db).await;
    if !validation.is_valid() {
        return Err(ApiError::from(to_validation_error(&validation)));
    }

    profiles::insert_profile(&state.db, &row)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::ProfileImport,
            target_id: id.clone(),
            detail: Some(serde_json::json!({"name": row.name})),
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
            "version": 1,
            "warnings": validation.warnings,
        })),
    ))
}

// --- Helpers ---

async fn derive_display_name(
    profile: &profiles::ProfileRow,
    viewer_user_id: &str,
    db: &console_db::DbPool,
) -> String {
    if profile.visibility == "shared" && profile.owner_user_id != viewer_user_id {
        let owner_name = console_db::queries::users::get_by_id(db, &profile.owner_user_id)
            .await
            .ok()
            .flatten()
            .map(|u| u.display_name)
            .unwrap_or_else(|| profile.owner_user_id.clone());
        format!("{owner_name}'s {}", profile.name)
    } else {
        profile.name.clone()
    }
}

fn check_profile_read_access(auth: &Auth, profile: &profiles::ProfileRow) -> Result<(), ApiError> {
    if auth.console_role == console_core::ConsoleRole::Admin {
        return Ok(());
    }
    if profile.owner_user_id == auth.user_id {
        return Ok(());
    }
    if profile.visibility == "shared" {
        return Ok(());
    }
    Err(ApiError::from(ConsoleError::Forbidden(
        "You do not have access to this profile".into(),
    )))
}

fn to_validation_error(result: &profile_validation::ValidationResult) -> ConsoleError {
    ConsoleError::Validation {
        message: "Profile validation failed".into(),
        violations: result
            .violations
            .iter()
            .map(|v| console_core::error::Violation {
                field: v.field.clone(),
                code: v.code.to_string(),
                message: v.message.clone(),
                value: None,
            })
            .collect(),
        warnings: result
            .warnings
            .iter()
            .map(|w| console_core::error::Violation {
                field: None,
                code: w.code.to_string(),
                message: w.message.clone(),
                value: None,
            })
            .collect(),
    }
}

fn check_profile_write_access(auth: &Auth, profile: &profiles::ProfileRow) -> Result<(), ApiError> {
    if auth.console_role == console_core::ConsoleRole::Admin {
        authorizer::authorize(auth, Action::EditAnyProfile).map_err(ApiError::from)?;
        return Ok(());
    }
    if profile.owner_user_id == auth.user_id {
        authorizer::authorize(auth, Action::EditOwnProfile).map_err(ApiError::from)?;
        return Ok(());
    }
    Err(ApiError::from(ConsoleError::Forbidden(
        "Only the owner or an admin can edit this profile".into(),
    )))
}
