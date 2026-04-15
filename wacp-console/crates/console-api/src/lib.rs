pub mod error;
pub mod middleware;
pub mod openapi;
pub mod pagination;
pub mod routes;

use arc_swap::ArcSwap;
use axum::Router;
use console_core::session_monitor::SessionMonitorHandle;
use console_core::taxonomy::TaxonomyIndex;
use console_db::DbPool;
use console_runtime::grpc_pool::GrpcPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::Level;

/// Shared map of currently-running session monitors. W3 populates on launch
/// (and recovery); W3 removes on terminal state; W6 reads for cross-session
/// pending aggregation.
pub type ActiveSessionsMap = Arc<RwLock<HashMap<String, SessionMonitorHandle>>>;

/// Shared application state injected into all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    /// The taxonomy index, atomically swappable for reload.
    pub taxonomy: Arc<ArcSwap<TaxonomyIndex>>,
    /// Runtime connection addresses for health checks and client construction.
    pub runtime_config: console_core::config::RuntimeConfig,
    /// Shared gRPC client pool. Held across all Axum handlers; reconnect is
    /// triggered by callers on per-service `None` responses.
    pub grpc_pool: Arc<GrpcPool>,
    /// Live session monitors keyed by session_id. W3 inserts on launch;
    /// monitor removes itself on terminal state exit.
    pub active_sessions: ActiveSessionsMap,
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
        .merge(routes::profiles::router())
        .merge(routes::discovery::router())
        .merge(routes::verticals::router())
        .merge(routes::search::router())
        .merge(routes::taxonomy::router())
        .merge(routes::sessions::router())
        .merge(routes::highway::router())
        .merge(routes::ws::router())
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
