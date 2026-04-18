//! Mock REST gateway for vertical manifest endpoints.
//!
//! Implements `GET /v1/verticals` and `GET /v1/verticals/{id}` using the
//! same `wacp_taxonomy::VerticalManifest` type as the real runtime.
//!
//! The underlying manifest map is wrapped in an `ArcSwap` so tests can
//! hot-swap the served set between requests — used by §13.7.8 I5
//! `taxonomy_reload.rs` to drive the console through
//! `/api/taxonomy/reload` with a changing upstream.

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use serde::Serialize;
use wacp_taxonomy::VerticalManifest;

/// Shared state holding the fixture manifests keyed by vertical ID.
/// Cheap to `Clone` (one `Arc` deep); tests hot-swap via
/// [`RestState::set_verticals`].
#[derive(Clone)]
pub struct RestState {
    pub verticals: Arc<ArcSwap<HashMap<String, VerticalManifest>>>,
}

impl RestState {
    pub fn new(verticals: HashMap<String, VerticalManifest>) -> Self {
        Self {
            verticals: Arc::new(ArcSwap::from_pointee(verticals)),
        }
    }

    /// Atomically replace the served vertical set. Existing in-flight
    /// requests see the old snapshot; new requests see the new one.
    pub fn set_verticals(&self, verticals: HashMap<String, VerticalManifest>) {
        self.verticals.store(Arc::new(verticals));
    }
}

/// Summary returned in the list endpoint. Shape matches
/// `console_runtime::rest_client::VerticalSummary` so the console's real
/// REST client can deserialize the mock's response without drift. See
/// perf-opt §12.2 for the mismatch this replaced.
#[derive(Serialize)]
struct VerticalListItem {
    id: String,
    name: String,
    defining_constraint: String,
    task_type_count: u32,
    workflow_count: u32,
    tool_count: u32,
}

/// Build the Axum router for the mock REST gateway.
pub fn rest_router(state: RestState) -> Router {
    Router::new()
        .route("/v1/verticals", get(list_verticals))
        .route("/v1/verticals/{id}", get(get_vertical))
        .with_state(state)
}

async fn list_verticals(State(state): State<RestState>) -> impl IntoResponse {
    let snapshot = state.verticals.load();
    let items: Vec<VerticalListItem> = snapshot
        .values()
        .map(|m| VerticalListItem {
            id: m.id.clone(),
            name: m.name.clone(),
            defining_constraint: m.defining_constraint.clone(),
            task_type_count: m.task_types.len() as u32,
            workflow_count: m.workflows.len() as u32,
            tool_count: m.tool_policies.len() as u32,
        })
        .collect();
    Json(items)
}

async fn get_vertical(State(state): State<RestState>, Path(id): Path<String>) -> impl IntoResponse {
    let snapshot = state.verticals.load();
    match snapshot.get(&id) {
        Some(manifest) => Ok(Json(manifest.clone())),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "NOT_FOUND",
                "message": format!("vertical '{id}' not found"),
            })),
        )),
    }
}
