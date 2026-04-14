//! Health endpoint — unauthenticated.
//!
//! Spec: `wcon-api` §11

use axum::extract::State;
use axum::{Json, Router, routing::get};
use std::sync::Arc;

use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/health", get(health))
}

async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let db_status = match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => "ok",
        Err(_) => "error",
    };

    let overall = if db_status == "ok" {
        "healthy"
    } else {
        "degraded"
    };

    Json(serde_json::json!({
        "status": overall,
        "checks": {
            "database": db_status,
        },
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
