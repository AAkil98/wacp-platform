//! User management endpoints — admin only.
//!
//! Spec: `wcon-api` §3.6, `wcon-auth` §2

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
use console_core::password::{hash_password, validate_password_strength};
use console_db::queries::users;

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::{Auth, RequestContext, is_bearer_auth, validate_csrf};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{id}", get(get_user).patch(update_user))
        .route("/api/users/{id}/reset-password", post(reset_password))
        .route("/api/users/{id}/unlock", post(unlock_user))
}

#[derive(Deserialize)]
struct ListParams {
    #[serde(default = "default_limit")]
    limit: i64,
    cursor: Option<String>,
}

fn default_limit() -> i64 {
    50
}

async fn list_users(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    authorizer::authorize(&auth, Action::ListUsers).map_err(ApiError::from)?;

    let rows = users::list_users(&state.db, true, params.limit, params.cursor.as_deref())
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    let result: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|u| {
            serde_json::json!({
                "id": u.id,
                "username": u.username,
                "display_name": u.display_name,
                "console_role": u.console_role,
                "disabled_at": u.disabled_at,
                "created_at": u.created_at,
                "updated_at": u.updated_at,
            })
        })
        .collect();

    Ok(Json(result))
}

#[derive(Deserialize)]
struct CreateUserRequest {
    username: String,
    display_name: String,
    password: String,
    console_role: String,
}

async fn create_user(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Json(body): Json<CreateUserRequest>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    authorizer::authorize(&auth, Action::CreateUser).map_err(ApiError::from)?;

    validate_password_strength(&body.password).map_err(ApiError::from)?;

    let password_hash = hash_password(&body.password).map_err(ApiError::from)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    users::insert_user(
        &state.db,
        &id,
        &body.username,
        &body.display_name,
        &password_hash,
        &body.console_role,
        false,
        &now,
    )
    .await
    .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::UserCreate,
            target_id: id.clone(),
            detail: Some(serde_json::json!({"username": body.username, "role": body.console_role})),
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
            "username": body.username,
            "console_role": body.console_role,
        })),
    ))
}

async fn get_user(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::ListUsers).map_err(ApiError::from)?;

    let user = users::get_by_id(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("user", &id))?;

    Ok(Json(serde_json::json!({
        "id": user.id,
        "username": user.username,
        "display_name": user.display_name,
        "console_role": user.console_role,
        "must_change_password": user.must_change_password,
        "disabled_at": user.disabled_at,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    })))
}

#[derive(Deserialize)]
struct UpdateUserRequest {
    console_role: Option<String>,
    disabled: Option<bool>,
}

async fn update_user(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateUserRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    let now = chrono::Utc::now().to_rfc3339();

    if let Some(role) = &body.console_role {
        authorizer::authorize(&auth, Action::ChangeUserRole).map_err(ApiError::from)?;

        // LAST_ADMIN guard: check if demoting the only admin
        let user = users::get_by_id(&state.db, &id)
            .await
            .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
            .ok_or_else(|| ApiError::not_found("user", &id))?;

        if user.console_role == "admin" && role != "admin" {
            let admin_count = users::count_active_admins(&state.db)
                .await
                .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;
            if admin_count <= 1 {
                return Err(ApiError::from(ConsoleError::LastAdmin));
            }
        }

        users::update_role(&state.db, &id, role, &now)
            .await
            .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

        log_audit(
            &state.db,
            AuditEntry {
                user_id: auth.user_id.clone(),
                action: AuditAction::UserChangeRole,
                target_id: id.clone(),
                detail: Some(serde_json::json!({"new_role": role})),
                ip: ctx.ip.clone(),
                user_agent: ctx.user_agent.clone(),
            },
        )
        .await
        .ok();
    }

    if let Some(disabled) = body.disabled {
        if disabled {
            authorizer::authorize(&auth, Action::DisableUser).map_err(ApiError::from)?;

            // LAST_ADMIN guard
            let user = users::get_by_id(&state.db, &id)
                .await
                .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
                .ok_or_else(|| ApiError::not_found("user", &id))?;

            if user.console_role == "admin" {
                let admin_count = users::count_active_admins(&state.db)
                    .await
                    .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;
                if admin_count <= 1 {
                    return Err(ApiError::from(ConsoleError::LastAdmin));
                }
            }

            users::disable_user(&state.db, &id, &now)
                .await
                .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

            log_audit(
                &state.db,
                AuditEntry {
                    user_id: auth.user_id.clone(),
                    action: AuditAction::UserDisable,
                    target_id: id.clone(),
                    detail: None,
                    ip: ctx.ip.clone(),
                    user_agent: ctx.user_agent.clone(),
                },
            )
            .await
            .ok();
        } else {
            authorizer::authorize(&auth, Action::DisableUser).map_err(ApiError::from)?;
            users::enable_user(&state.db, &id, &now)
                .await
                .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

            log_audit(
                &state.db,
                AuditEntry {
                    user_id: auth.user_id.clone(),
                    action: AuditAction::UserEnable,
                    target_id: id.clone(),
                    detail: None,
                    ip: ctx.ip.clone(),
                    user_agent: ctx.user_agent.clone(),
                },
            )
            .await
            .ok();
        }
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn reset_password(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    ctx: RequestContext,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    authorizer::authorize(&auth, Action::ResetUserPassword).map_err(ApiError::from)?;

    users::set_must_change_password(&state.db, &id, &chrono::Utc::now().to_rfc3339())
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;

    log_audit(
        &state.db,
        AuditEntry {
            user_id: auth.user_id.clone(),
            action: AuditAction::UserResetPassword,
            target_id: id,
            detail: None,
            ip: ctx.ip,
            user_agent: ctx.user_agent,
        },
    )
    .await
    .ok();

    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn unlock_user(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_csrf(&headers, is_bearer_auth(&headers)).map_err(ApiError::from)?;
    authorizer::authorize(&auth, Action::DisableUser).map_err(ApiError::from)?;

    // Unlocking clears failed login_attempts for the target user
    let user = users::get_by_id(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("user", &id))?;

    console_db::queries::login_attempts::clear_for_username(&state.db, &user.username)
        .await
        .ok();

    Ok(axum::http::StatusCode::NO_CONTENT)
}
