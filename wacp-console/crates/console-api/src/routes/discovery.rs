//! Discovery endpoints — global entity queries with pagination and filtering.
//!
//! Spec: `wcon-discovery` §4, `wcon-api` §6

use axum::extract::{Path, Query, State};
use axum::{Json, Router, routing::get};
use serde::Deserialize;
use std::sync::Arc;

use console_core::authorizer::{self, Action};
use console_core::taxonomy::{
    CheckpointTypeEntry, EnvelopeTypeEntry, RoleEntry, ToolEntry,
};

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::Auth;
use crate::pagination::{PaginatedResponse, PaginationParams, paginate};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/roles", get(list_roles))
        .route("/api/roles/{id}", get(get_role))
        .route("/api/tools", get(list_tools))
        .route("/api/tools/{name}", get(get_tool))
        .route("/api/verticals", get(list_verticals))
        .route("/api/verticals/{id}", get(get_vertical))
        .route("/api/envelope-types", get(list_envelope_types))
        .route("/api/envelope-types/{name}", get(get_envelope_type))
        .route("/api/checkpoint-types", get(list_checkpoint_types))
        .route("/api/checkpoint-types/{name}", get(get_checkpoint_type))
}

// --- Roles ---

#[derive(Deserialize)]
struct RoleFilters {
    base_role: Option<String>,
    vertical: Option<String>,
    #[serde(flatten)]
    pagination: PaginationParams,
}

async fn list_roles(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<RoleFilters>,
) -> Result<Json<PaginatedResponse<RoleEntry>>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let roles = index.list_roles(
        params.base_role.as_deref(),
        params.vertical.as_deref(),
    );
    let owned: Vec<RoleEntry> = roles.into_iter().cloned().collect();
    Ok(Json(paginate(&owned, &params.pagination, |r| &r.name)))
}

async fn get_role(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let role = index
        .get_role(&id)
        .ok_or_else(|| ApiError::not_found("role", &id))?;

    // Resolve tools to full entries.
    let tools: Vec<&ToolEntry> = role
        .tools
        .iter()
        .filter_map(|t| index.get_tool(t))
        .collect();

    // Envelope types this role can send/receive.
    let envelope_types: Vec<&EnvelopeTypeEntry> = index
        .envelope_types
        .values()
        .filter(|e| e.sender_roles.contains(&id) || e.receiver_roles.contains(&id))
        .collect();

    // Checkpoint types this role can create.
    let checkpoint_types: Vec<&CheckpointTypeEntry> = index
        .checkpoint_types
        .values()
        .filter(|c| c.allowed_roles.contains(&id) || c.allowed_roles.contains(&role.base_role))
        .collect();

    Ok(Json(serde_json::json!({
        "name": role.name,
        "base_role": role.base_role,
        "extends": role.extends,
        "capabilities_added": role.capabilities_added,
        "capabilities_removed": role.capabilities_removed,
        "vertical": role.vertical,
        "tools": tools,
        "envelope_types": envelope_types,
        "checkpoint_types": checkpoint_types,
    })))
}

// --- Tools ---

#[derive(Deserialize)]
struct ToolFilters {
    vertical: Option<String>,
    has_policy: Option<bool>,
    #[serde(flatten)]
    pagination: PaginationParams,
}

async fn list_tools(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<ToolFilters>,
) -> Result<Json<PaginatedResponse<ToolEntry>>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let tools = index.list_tools(params.vertical.as_deref(), params.has_policy);
    let owned: Vec<ToolEntry> = tools.into_iter().cloned().collect();
    Ok(Json(paginate(&owned, &params.pagination, |t| &t.name)))
}

async fn get_tool(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let tool = index
        .get_tool(&name)
        .ok_or_else(|| ApiError::not_found("tool", &name))?;

    let roles: Vec<&RoleEntry> = tool
        .roles
        .iter()
        .filter_map(|r| index.get_role(r))
        .collect();

    Ok(Json(serde_json::json!({
        "name": tool.name,
        "description": tool.description,
        "vertical": tool.vertical,
        "roles": roles,
        "policy": tool.policy,
    })))
}

// --- Verticals ---

async fn list_verticals(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<serde_json::Value>>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let verticals = index.list_verticals();
    let summaries: Vec<serde_json::Value> = verticals
        .into_iter()
        .map(|v| {
            serde_json::json!({
                "id": v.id,
                "name": v.name,
                "defining_constraint": v.defining_constraint,
                "load_error": v.load_error,
                "task_type_count": v.task_types.len(),
                "workflow_count": v.workflows.len(),
                "tool_count": v.tools.len(),
                "role_count": v.roles.len(),
            })
        })
        .collect();
    Ok(Json(paginate(&summaries, &params, |v| {
        v["id"].as_str().unwrap_or("")
    })))
}

async fn get_vertical(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let v = index
        .get_vertical(&id)
        .ok_or_else(|| ApiError::not_found("vertical", &id))?;

    Ok(Json(serde_json::json!({
        "id": v.id,
        "name": v.name,
        "load_error": v.load_error,
        "defining_constraint": v.defining_constraint,
        "roles": v.roles,
        "context_schema": v.context_schema,
        "tool_policies": v.tool_policies,
        "checkpoint_types": v.checkpoint_types,
        "quality_criteria": v.quality_criteria,
        "task_types": v.task_types,
        "workflows": v.workflows,
        "default_profiles": v.default_profiles,
        "tools": v.tools,
    })))
}

// --- Envelope types ---

async fn list_envelope_types(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<EnvelopeTypeEntry>>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let types = index.list_envelope_types();
    let owned: Vec<EnvelopeTypeEntry> = types.into_iter().cloned().collect();
    Ok(Json(paginate(&owned, &params, |e| &e.name)))
}

async fn get_envelope_type(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(name): Path<String>,
) -> Result<Json<EnvelopeTypeEntry>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let entry = index
        .envelope_types
        .get(&name)
        .ok_or_else(|| ApiError::not_found("envelope_type", &name))?;
    Ok(Json(entry.clone()))
}

// --- Checkpoint types (protocol-level) ---

async fn list_checkpoint_types(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<CheckpointTypeEntry>>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let types = index.list_checkpoint_types();
    let owned: Vec<CheckpointTypeEntry> = types.into_iter().cloned().collect();
    Ok(Json(paginate(&owned, &params, |c| &c.name)))
}

async fn get_checkpoint_type(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(name): Path<String>,
) -> Result<Json<CheckpointTypeEntry>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    let index = state.taxonomy.load();
    let entry = index
        .checkpoint_types
        .get(&name)
        .ok_or_else(|| ApiError::not_found("checkpoint_type", &name))?;
    Ok(Json(entry.clone()))
}
