//! Health endpoint — unauthenticated, with per-service runtime checks.
//!
//! Spec: `wcon-api` §11
//!
//! The three gRPC health rows (`runtime_agent`, `runtime_highway`,
//! `runtime_coordinator`) read from the shared `GrpcPool` rather than issuing
//! TCP probes. This reflects real dial state (which only updates when the
//! pool successfully connects) and gives handlers a single source of truth —
//! if a handler sees `.agent().await == None`, the next health hit will report
//! `"error"` for that row until a reconnect succeeds.

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

    let pool_snapshot = state.grpc_pool.status_snapshot().await;
    let agent_status = pool_snapshot[0].1.as_health_string();
    let highway_status = pool_snapshot[1].1.as_health_string();
    let coordinator_status = pool_snapshot[2].1.as_health_string();
    let rest_status = check_rest(&state.runtime_config.rest_address).await;

    let all_ok = db_status == "ok"
        && agent_status == "ok"
        && highway_status == "ok"
        && coordinator_status == "ok"
        && rest_status == "ok";

    let overall = if all_ok {
        "healthy"
    } else if db_status == "ok" {
        "degraded"
    } else {
        "unhealthy"
    };

    Json(serde_json::json!({
        "status": overall,
        "checks": {
            "database": db_status,
            "runtime_agent": agent_status,
            "runtime_highway": highway_status,
            "runtime_coordinator": coordinator_status,
            "runtime_rest": rest_status,
        },
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Check REST gateway reachability via HTTP HEAD.
async fn check_rest(address: &str) -> &'static str {
    let url = format!("{}/v1/verticals", address.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build();

    let Ok(client) = client else {
        return "error";
    };

    match client.head(&url).send().await {
        Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 405 => "ok",
        _ => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use console_core::config::RuntimeConfig;
    use console_core::taxonomy_builder;
    use console_db::create_test_pool;
    use console_runtime::grpc_pool::GrpcPool;
    use console_test_support::mock_runtime::MockRuntime;
    use tower::ServiceExt;

    /// Build an `AppState` wired to the given pool + runtime addresses. Uses an in-memory
    /// SQLite pool and an empty taxonomy index — health only needs the db + pool.
    async fn build_state(
        pool: Arc<GrpcPool>,
        rest_address: String,
        agent_address: String,
        highway_address: String,
        coordinator_address: String,
    ) -> Arc<AppState> {
        let db = create_test_pool().await.expect("test pool");
        let taxonomy = Arc::new(ArcSwap::from_pointee(
            taxonomy_builder::build_index(None, &[], &[]).index,
        ));
        Arc::new(AppState {
            db,
            taxonomy,
            runtime_config: RuntimeConfig {
                agent_address,
                highway_address,
                coordinator_address,
                rest_address,
            },
            grpc_pool: pool,
            active_sessions: Arc::new(tokio::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
        })
    }

    async fn request_health(state: Arc<AppState>) -> (StatusCode, serde_json::Value) {
        let app = router().with_state(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    #[tokio::test]
    async fn health_reports_healthy_when_pool_connected_and_rest_ok() {
        let rt = MockRuntime::start().await.expect("mock runtime");
        let pool = GrpcPool::new(
            &rt.agent_addr.to_string(),
            &rt.highway_addr.to_string(),
            &rt.coordinator_addr.to_string(),
        );
        pool.connect().await;

        let state = build_state(
            pool,
            format!("http://{}", rt.rest_addr),
            rt.agent_addr.to_string(),
            rt.highway_addr.to_string(),
            rt.coordinator_addr.to_string(),
        )
        .await;
        let (status, body) = request_health(state).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "healthy");
        assert_eq!(body["checks"]["database"], "ok");
        assert_eq!(body["checks"]["runtime_agent"], "ok");
        assert_eq!(body["checks"]["runtime_highway"], "ok");
        assert_eq!(body["checks"]["runtime_coordinator"], "ok");
        assert_eq!(body["checks"]["runtime_rest"], "ok");
    }

    #[tokio::test]
    async fn health_reports_degraded_when_pool_disconnected() {
        // No mock runtime — pool will fail to dial all three channels.
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        pool.connect().await;

        let state = build_state(
            pool,
            "http://[::1]:1".to_string(),
            "[::1]:1".to_string(),
            "[::1]:1".to_string(),
            "[::1]:1".to_string(),
        )
        .await;
        let (status, body) = request_health(state).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "degraded");
        assert_eq!(body["checks"]["database"], "ok");
        assert_eq!(body["checks"]["runtime_agent"], "error");
        assert_eq!(body["checks"]["runtime_highway"], "error");
        assert_eq!(body["checks"]["runtime_coordinator"], "error");
        assert_eq!(body["checks"]["runtime_rest"], "error");
    }

    #[tokio::test]
    async fn health_reports_unknown_as_error_before_connect() {
        // Pool constructed but `connect()` never called — all statuses `Unknown`.
        let pool = GrpcPool::new("[::1]:9090", "[::1]:9091", "[::1]:9092");

        let state = build_state(
            pool,
            "http://[::1]:9093".to_string(),
            "[::1]:9090".to_string(),
            "[::1]:9091".to_string(),
            "[::1]:9092".to_string(),
        )
        .await;
        let (status, body) = request_health(state).await;

        assert_eq!(status, StatusCode::OK);
        // db ok + rest unreachable + 3 unknown → degraded (not unhealthy — db is ok)
        assert_eq!(body["status"], "degraded");
        assert_eq!(body["checks"]["runtime_agent"], "error");
    }
}
