//! Taxonomy reload endpoint.
//!
//! Spec: `wcon-discovery` §7.2–7.4

use axum::extract::State;
use axum::{Json, Router, routing::post};
use std::sync::Arc;

use console_core::authorizer::{self, Action};
use console_core::taxonomy_builder;
use console_runtime::rest_client;

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::Auth;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/taxonomy/reload", post(reload))
}

async fn reload(
    State(state): State<Arc<AppState>>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorizer::authorize(&auth, Action::ReloadTaxonomy).map_err(ApiError::from)?;

    // Get the REST address from the current settings or config.
    // For now, use the runtime rest address from settings if available,
    // otherwise fall back to the default.
    let rest_address = get_rest_address(&state).await;

    let load_result = match rest_client::load_verticals(&rest_address).await {
        Ok(r) => r,
        Err(e) => {
            return Ok(Json(serde_json::json!({
                "status": "failed",
                "error": e,
                "counts": null,
                "warnings": [],
            })));
        }
    };

    let build = taxonomy_builder::build_index(
        None,
        &load_result.manifests,
        &load_result.failed,
    );

    let counts = serde_json::json!({
        "roles": build.index.roles.len(),
        "tools": build.index.tools.len(),
        "verticals": build.index.verticals.len(),
        "envelope_types": build.index.envelope_types.len(),
        "checkpoint_types": build.index.checkpoint_types.len(),
    });

    let status = if load_result.failed.is_empty() {
        "success"
    } else {
        "partial"
    };

    // Atomic swap.
    state.taxonomy.store(Arc::new(build.index));

    Ok(Json(serde_json::json!({
        "status": status,
        "counts": counts,
        "warnings": build.warnings,
    })))
}

async fn get_rest_address(state: &AppState) -> String {
    // Try to read from settings table.
    if let Ok(setting) = console_core::settings::get(&state.db, "runtime.rest_address").await
        && let Some(addr) = setting.value.as_str()
        && !addr.is_empty()
    {
        return addr.to_string();
    }
    // Fall back to default.
    "http://[::1]:9093".to_string()
}
