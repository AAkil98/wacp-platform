use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Json;
use axum::Router;

use crate::config::HealthConfig;

/// Health states: Starting(0), Ready(1), Draining(2).
pub const HEALTH_STARTING: u8 = 0;
pub const HEALTH_READY: u8 = 1;
pub const HEALTH_DRAINING: u8 = 2;

#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    #[error("failed to bind health endpoint: {0}")]
    Bind(String),
}

#[derive(Clone)]
struct HealthState {
    state: Arc<AtomicU8>,
    start_time: Instant,
}

/// Start the health HTTP server. Spawns as a tokio task.
pub async fn start_health_server(
    config: &HealthConfig,
    state: Arc<AtomicU8>,
) -> Result<(), HealthError> {
    let addr: SocketAddr = config
        .listen
        .parse()
        .map_err(|e| HealthError::Bind(format!("{e}")))?;
    let path = config.path.clone();

    let health_state = HealthState {
        state,
        start_time: Instant::now(),
    };

    let app = Router::new()
        .route(&path, get(health_handler))
        .fallback(|| async { StatusCode::NOT_FOUND })
        .with_state(health_state);

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    Ok(())
}

async fn health_handler(State(health): State<HealthState>) -> impl IntoResponse {
    let state_val = health.state.load(Ordering::Relaxed);
    let uptime = health.start_time.elapsed().as_secs();

    let (status_str, http_status) = match state_val {
        HEALTH_STARTING => ("starting", StatusCode::SERVICE_UNAVAILABLE),
        HEALTH_READY => ("ready", StatusCode::OK),
        HEALTH_DRAINING => ("draining", StatusCode::SERVICE_UNAVAILABLE),
        _ => ("unknown", StatusCode::INTERNAL_SERVER_ERROR),
    };

    let body = serde_json::json!({
        "status": status_str,
        "uptime_seconds": uptime,
    });

    (http_status, Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(val: u8) -> HealthState {
        let state = Arc::new(AtomicU8::new(val));
        HealthState {
            state,
            start_time: Instant::now(),
        }
    }

    #[tokio::test]
    async fn health_starting_returns_503() {
        let state = make_state(HEALTH_STARTING);
        let response = health_handler(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn health_ready_returns_200() {
        let state = make_state(HEALTH_READY);
        let response = health_handler(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_draining_returns_503() {
        let state = make_state(HEALTH_DRAINING);
        let response = health_handler(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // ── Phase T2.1 additions ──

    #[tokio::test]
    async fn health_ready_body_has_status() {
        let state = make_state(HEALTH_READY);
        let response = health_handler(State(state)).await.into_response();
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ready");
    }

    #[tokio::test]
    async fn health_starting_body_has_status() {
        let state = make_state(HEALTH_STARTING);
        let response = health_handler(State(state)).await.into_response();
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "starting");
    }

    #[tokio::test]
    async fn health_draining_body_has_status() {
        let state = make_state(HEALTH_DRAINING);
        let response = health_handler(State(state)).await.into_response();
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "draining");
    }

    #[tokio::test]
    async fn health_body_has_uptime() {
        let state = make_state(HEALTH_READY);
        let response = health_handler(State(state)).await.into_response();
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["uptime_seconds"].is_number());
    }

    #[tokio::test]
    async fn health_unknown_state_returns_500() {
        let state = make_state(99);
        let response = health_handler(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
