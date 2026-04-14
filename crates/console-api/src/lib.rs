pub mod error;

use axum::Router;
use console_db::DbPool;
use std::sync::Arc;

/// Shared application state injected into all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
}

/// Builds the full API router. In Phase 0 this returns an empty router;
/// endpoint modules are added in subsequent phases.
pub fn api_router(state: AppState) -> Router {
    Router::new().with_state(Arc::new(state))
}
