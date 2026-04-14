pub mod error;

use axum::Router;
use console_db::DbPool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::Level;

/// Shared application state injected into all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
}

/// Builds the full API router with per-request tracing.
pub fn api_router(state: AppState) -> Router {
    Router::new()
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
        .with_state(Arc::new(state))
}
