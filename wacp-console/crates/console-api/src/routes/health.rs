//! Health endpoint — unauthenticated, with per-service runtime checks.
//!
//! Spec: `wcon-api` §11

use axum::extract::State;
use axum::{Json, Router, routing::get};
use std::sync::Arc;
use tokio::net::TcpStream;

use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/health", get(health))
}

async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let db_status = match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => "ok",
        Err(_) => "error",
    };

    // Per-service runtime health checks (TCP connect for gRPC, HTTP HEAD for REST).
    let rt = &state.runtime_config;
    let agent_status = check_grpc(&rt.agent_address).await;
    let highway_status = check_grpc(&rt.highway_address).await;
    let coordinator_status = check_grpc(&rt.coordinator_address).await;
    let rest_status = check_rest(&rt.rest_address).await;

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

/// Check gRPC service reachability via TCP connect.
async fn check_grpc(address: &str) -> &'static str {
    let addr = address
        .trim_start_matches("http://")
        .trim_start_matches("https://");

    match tokio::time::timeout(std::time::Duration::from_secs(2), TcpStream::connect(addr)).await {
        Ok(Ok(_)) => "ok",
        _ => "error",
    }
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
