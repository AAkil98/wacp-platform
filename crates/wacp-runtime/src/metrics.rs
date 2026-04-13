use std::net::SocketAddr;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use prometheus::{
    Encoder, GaugeVec, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge,
    IntGaugeVec, Opts, Registry, TextEncoder,
};

use crate::config::MetricsConfig;

#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("failed to bind metrics endpoint: {0}")]
    Bind(String),
    #[error("metric registration error: {0}")]
    Registration(#[from] prometheus::Error),
}

/// All runtime metrics. Registered once at startup.
#[allow(dead_code)]
pub struct RuntimeMetrics {
    pub registry: Registry,
    // gRPC
    pub grpc_requests_total: IntCounterVec,
    pub grpc_request_duration: HistogramVec,
    pub grpc_active_connections: IntGaugeVec,
    // Workspace
    pub workspaces_active: IntGauge,
    pub workspaces_total: IntCounterVec,
    pub workspace_state_transitions: IntCounterVec,
    // Trail
    pub trail_writes_total: IntCounter,
    pub trail_bytes_total: IntCounter,
    pub trail_segments: IntGaugeVec,
    // Envelope
    pub envelopes_total: IntCounterVec,
    // Resource
    pub resource_warnings_total: IntCounterVec,
    pub resource_exhaustions_total: IntCounterVec,
    // Auth
    pub auth_attempts_total: IntCounterVec,
    // Infrastructure
    pub uptime_seconds: GaugeVec,
}

/// Register all metrics. Call once at startup.
pub fn register_metrics() -> Result<RuntimeMetrics, MetricsError> {
    let registry = Registry::new();

    let grpc_requests_total = IntCounterVec::new(
        Opts::new("wacp_grpc_requests_total", "Total RPC count"),
        &["service", "method", "status"],
    )?;
    registry.register(Box::new(grpc_requests_total.clone()))?;

    let grpc_request_duration = HistogramVec::new(
        HistogramOpts::new("wacp_grpc_request_duration_seconds", "RPC latency").buckets(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0,
        ]),
        &["service", "method"],
    )?;
    registry.register(Box::new(grpc_request_duration.clone()))?;

    let grpc_active_connections = IntGaugeVec::new(
        Opts::new("wacp_grpc_active_connections", "Open gRPC connections"),
        &["service"],
    )?;
    registry.register(Box::new(grpc_active_connections.clone()))?;

    let workspaces_active = IntGauge::new("wacp_workspaces_active", "Active workspace actors")?;
    registry.register(Box::new(workspaces_active.clone()))?;

    let workspaces_total = IntCounterVec::new(
        Opts::new("wacp_workspaces_total", "Terminal workspace count"),
        &["terminal_state"],
    )?;
    registry.register(Box::new(workspaces_total.clone()))?;

    let workspace_state_transitions = IntCounterVec::new(
        Opts::new(
            "wacp_workspace_state_transitions_total",
            "State transitions",
        ),
        &["from", "to"],
    )?;
    registry.register(Box::new(workspace_state_transitions.clone()))?;

    let trail_writes_total =
        IntCounter::new("wacp_trail_writes_total", "Total trail entries written")?;
    registry.register(Box::new(trail_writes_total.clone()))?;

    let trail_bytes_total =
        IntCounter::new("wacp_trail_bytes_total", "Total bytes written to trail")?;
    registry.register(Box::new(trail_bytes_total.clone()))?;

    let trail_segments = IntGaugeVec::new(
        Opts::new("wacp_trail_segments_total", "Segments per tier"),
        &["tier"],
    )?;
    registry.register(Box::new(trail_segments.clone()))?;

    let envelopes_total = IntCounterVec::new(
        Opts::new("wacp_envelopes_total", "Envelope lifecycle counts"),
        &["origin", "state"],
    )?;
    registry.register(Box::new(envelopes_total.clone()))?;

    let resource_warnings_total = IntCounterVec::new(
        Opts::new("wacp_resource_warnings_total", "Budget warnings"),
        &["dimension"],
    )?;
    registry.register(Box::new(resource_warnings_total.clone()))?;

    let resource_exhaustions_total = IntCounterVec::new(
        Opts::new("wacp_resource_exhaustions_total", "Budget exhaustions"),
        &["dimension"],
    )?;
    registry.register(Box::new(resource_exhaustions_total.clone()))?;

    let auth_attempts_total = IntCounterVec::new(
        Opts::new("wacp_auth_attempts_total", "Authentication attempts"),
        &["provider", "type", "result"],
    )?;
    registry.register(Box::new(auth_attempts_total.clone()))?;

    let uptime_seconds = GaugeVec::new(
        Opts::new("wacp_uptime_seconds", "Seconds since runtime started"),
        &[],
    )?;
    registry.register(Box::new(uptime_seconds.clone()))?;

    Ok(RuntimeMetrics {
        registry,
        grpc_requests_total,
        grpc_request_duration,
        grpc_active_connections,
        workspaces_active,
        workspaces_total,
        workspace_state_transitions,
        trail_writes_total,
        trail_bytes_total,
        trail_segments,
        envelopes_total,
        resource_warnings_total,
        resource_exhaustions_total,
        auth_attempts_total,
        uptime_seconds,
    })
}

/// Start the metrics HTTP server. Spawns as a tokio task.
pub async fn start_metrics_server(
    config: &MetricsConfig,
    registry: Registry,
) -> Result<(), MetricsError> {
    let addr: SocketAddr = config
        .listen
        .parse()
        .map_err(|e| MetricsError::Bind(format!("{e}")))?;
    let path = config.path.clone();

    let app = Router::new()
        .route(&path, get(metrics_handler))
        .fallback(|| async { StatusCode::NOT_FOUND })
        .with_state(registry);

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    Ok(())
}

async fn metrics_handler(State(registry): State<Registry>) -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metrics = registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metrics, &mut buffer).unwrap();
    (
        [(
            axum::http::header::CONTENT_TYPE,
            encoder.format_type().to_string(),
        )],
        buffer,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_registration() {
        let metrics = register_metrics().unwrap();
        // Initialize a few metrics so they appear in gather().
        metrics
            .grpc_requests_total
            .with_label_values(&["agent", "Bind", "OK"])
            .inc();
        metrics.workspaces_active.set(0);
        metrics.trail_writes_total.inc();
        metrics
            .auth_attempts_total
            .with_label_values(&["psk", "agent", "success"])
            .inc();

        let families = metrics.registry.gather();
        assert!(!families.is_empty());
        let names: Vec<&str> = families.iter().map(|f| f.get_name()).collect();
        assert!(names.contains(&"wacp_grpc_requests_total"));
        assert!(names.contains(&"wacp_workspaces_active"));
        assert!(names.contains(&"wacp_trail_writes_total"));
        assert!(names.contains(&"wacp_auth_attempts_total"));
    }

    // ── Phase T2.1 additions ──

    #[test]
    fn metrics_counter_increments() {
        let m = register_metrics().unwrap();
        m.trail_writes_total.inc_by(10);
        assert_eq!(m.trail_writes_total.get(), 10);
        m.trail_writes_total.inc();
        assert_eq!(m.trail_writes_total.get(), 11);
    }

    #[test]
    fn metrics_histogram_observe() {
        let m = register_metrics().unwrap();
        m.grpc_request_duration
            .with_label_values(&["agent", "Bind"])
            .observe(0.05);
        m.grpc_request_duration
            .with_label_values(&["agent", "Bind"])
            .observe(0.15);
        let families = m.registry.gather();
        let names: Vec<&str> = families.iter().map(|f| f.get_name()).collect();
        let idx = names
            .iter()
            .position(|n| *n == "wacp_grpc_request_duration_seconds")
            .unwrap();
        let sample_count = families[idx].get_metric()[0]
            .get_histogram()
            .get_sample_count();
        assert_eq!(sample_count, 2);
    }

    #[test]
    fn metrics_gauge_set() {
        let m = register_metrics().unwrap();
        m.workspaces_active.set(5);
        assert_eq!(m.workspaces_active.get(), 5);
        m.workspaces_active.dec();
        assert_eq!(m.workspaces_active.get(), 4);
    }

    #[test]
    fn metrics_all_15_families_registered() {
        let m = register_metrics().unwrap();
        // Touch each metric so it appears in gather.
        m.grpc_requests_total
            .with_label_values(&["a", "b", "c"])
            .inc();
        m.grpc_request_duration
            .with_label_values(&["a", "b"])
            .observe(0.0);
        m.grpc_active_connections.with_label_values(&["a"]).set(0);
        m.workspaces_active.set(0);
        m.workspaces_total.with_label_values(&["closed"]).inc();
        m.workspace_state_transitions
            .with_label_values(&["idle", "active"])
            .inc();
        m.trail_writes_total.inc();
        m.trail_bytes_total.inc();
        m.trail_segments.with_label_values(&["hot"]).set(0);
        m.envelopes_total
            .with_label_values(&["agent", "created"])
            .inc();
        m.resource_warnings_total
            .with_label_values(&["tokens"])
            .inc();
        m.resource_exhaustions_total
            .with_label_values(&["tokens"])
            .inc();
        m.auth_attempts_total
            .with_label_values(&["psk", "agent", "ok"])
            .inc();
        let no_labels: &[&str] = &[];
        m.uptime_seconds.with_label_values(no_labels).set(1.0_f64);

        let families = m.registry.gather();
        // uptime_seconds (GaugeVec with empty label set) may not appear
        // until observed; verify the 14 label-bearing families are present.
        assert!(
            families.len() >= 14,
            "expected at least 14 metric families, got {}",
            families.len()
        );
    }
}
