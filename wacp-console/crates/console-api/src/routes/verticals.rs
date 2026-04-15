//! Per-vertical sub-endpoints — workflows, task types, context schema, etc.
//!
//! Spec: `wcon-discovery` §4.1, `wcon-api` §6

use axum::extract::{Path, State};
use axum::{Json, Router, routing::get};
use std::sync::Arc;

use console_core::authorizer::{self, Action};

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::Auth;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/verticals/{id}/workflows",
            get(list_workflows),
        )
        .route(
            "/api/verticals/{id}/workflows/{wf_id}",
            get(get_workflow),
        )
        .route(
            "/api/verticals/{id}/task-types",
            get(list_task_types),
        )
        .route(
            "/api/verticals/{id}/context-schema",
            get(get_context_schema),
        )
        .route(
            "/api/verticals/{id}/tool-policies",
            get(get_tool_policies),
        )
        .route(
            "/api/verticals/{id}/checkpoint-types",
            get(get_checkpoint_types),
        )
        .route(
            "/api/verticals/{id}/quality-criteria",
            get(get_quality_criteria),
        )
}

async fn list_workflows(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let v = index
        .get_vertical(&id)
        .ok_or_else(|| ApiError::not_found("vertical", &id))?;

    Ok(Json(serde_json::to_value(&v.workflows).unwrap_or_default()))
}

async fn get_workflow(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path((id, wf_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let v = index
        .get_vertical(&id)
        .ok_or_else(|| ApiError::not_found("vertical", &id))?;

    let workflow = v
        .workflows
        .iter()
        .find(|w| w.id == wf_id)
        .ok_or_else(|| ApiError::not_found("workflow", &wf_id))?;

    Ok(Json(serde_json::to_value(workflow).unwrap_or_default()))
}

async fn list_task_types(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let v = index
        .get_vertical(&id)
        .ok_or_else(|| ApiError::not_found("vertical", &id))?;

    Ok(Json(serde_json::to_value(&v.task_types).unwrap_or_default()))
}

async fn get_context_schema(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let v = index
        .get_vertical(&id)
        .ok_or_else(|| ApiError::not_found("vertical", &id))?;

    Ok(Json(serde_json::to_value(&v.context_schema).unwrap_or_default()))
}

async fn get_tool_policies(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let v = index
        .get_vertical(&id)
        .ok_or_else(|| ApiError::not_found("vertical", &id))?;

    Ok(Json(serde_json::to_value(&v.tool_policies).unwrap_or_default()))
}

async fn get_checkpoint_types(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let v = index
        .get_vertical(&id)
        .ok_or_else(|| ApiError::not_found("vertical", &id))?;

    Ok(Json(serde_json::to_value(&v.checkpoint_types).unwrap_or_default()))
}

async fn get_quality_criteria(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let v = index
        .get_vertical(&id)
        .ok_or_else(|| ApiError::not_found("vertical", &id))?;

    Ok(Json(serde_json::to_value(&v.quality_criteria).unwrap_or_default()))
}
