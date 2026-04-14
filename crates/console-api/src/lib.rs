pub mod error;
pub mod middleware;
pub mod pagination;
pub mod routes;

use arc_swap::ArcSwap;
use axum::Router;
use console_core::taxonomy::TaxonomyIndex;
use console_db::DbPool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::Level;

/// Shared application state injected into all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    /// The taxonomy index, atomically swappable for reload.
    pub taxonomy: Arc<ArcSwap<TaxonomyIndex>>,
}

/// Builds the full API router with per-request tracing and all endpoints.
pub fn api_router(state: AppState) -> Router {
    let shared = Arc::new(state);

    Router::new()
        .merge(routes::health::router())
        .merge(routes::auth::router())
        .merge(routes::users::router())
        .merge(routes::tokens::router())
        .merge(routes::audit::router())
        .merge(routes::settings::router())
        .merge(routes::discovery::router())
        .merge(routes::verticals::router())
        .merge(routes::search::router())
        .merge(routes::taxonomy::router())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    tracing::info_span!(
                        "request",
                        method = %request.method(),
                        uri = %request.uri(),
                        status = tracing::field::Empty,
                    )
                })
                .on_response(
                    |response: &axum::http::Response<_>,
                     latency: std::time::Duration,
                     span: &tracing::Span| {
                        span.record("status", response.status().as_u16());
                        tracing::event!(Level::INFO, latency_ms = latency.as_millis(), "response");
                    },
                ),
        )
        .with_state(shared)
}
