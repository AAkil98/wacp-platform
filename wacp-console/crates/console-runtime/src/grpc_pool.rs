//! gRPC client pool — three independent Tonic channels for WACP runtime services.
//!
//! Spec: `wcon-architecture` §4.1, §7

use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::transport::{Channel, Endpoint};
use tracing::{info, warn};

use crate::proto::{
    agent_service_client::AgentServiceClient, coordinator_service_client::CoordinatorServiceClient,
    highway_service_client::HighwayServiceClient,
};

/// Per-service health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Connected,
    Disconnected,
    Unknown,
}

/// gRPC client pool with per-service health tracking.
pub struct GrpcPool {
    agent_endpoint: String,
    highway_endpoint: String,
    coordinator_endpoint: String,
    agent: RwLock<Option<AgentServiceClient<Channel>>>,
    highway: RwLock<Option<HighwayServiceClient<Channel>>>,
    coordinator: RwLock<Option<CoordinatorServiceClient<Channel>>>,
    agent_status: RwLock<ServiceStatus>,
    highway_status: RwLock<ServiceStatus>,
    coordinator_status: RwLock<ServiceStatus>,
}

impl GrpcPool {
    pub fn new(agent_addr: &str, highway_addr: &str, coordinator_addr: &str) -> Arc<Self> {
        Arc::new(Self {
            agent_endpoint: normalize_endpoint(agent_addr),
            highway_endpoint: normalize_endpoint(highway_addr),
            coordinator_endpoint: normalize_endpoint(coordinator_addr),
            agent: RwLock::new(None),
            highway: RwLock::new(None),
            coordinator: RwLock::new(None),
            agent_status: RwLock::new(ServiceStatus::Unknown),
            highway_status: RwLock::new(ServiceStatus::Unknown),
            coordinator_status: RwLock::new(ServiceStatus::Unknown),
        })
    }

    /// Connect all three channels. Non-fatal — channels that fail are logged and left disconnected.
    pub async fn connect(&self) {
        self.connect_agent().await;
        self.connect_highway().await;
        self.connect_coordinator().await;
    }

    pub async fn agent(&self) -> Option<AgentServiceClient<Channel>> {
        self.agent.read().await.clone()
    }

    pub async fn highway(&self) -> Option<HighwayServiceClient<Channel>> {
        self.highway.read().await.clone()
    }

    pub async fn coordinator(&self) -> Option<CoordinatorServiceClient<Channel>> {
        self.coordinator.read().await.clone()
    }

    pub async fn agent_status(&self) -> ServiceStatus {
        *self.agent_status.read().await
    }

    pub async fn highway_status(&self) -> ServiceStatus {
        *self.highway_status.read().await
    }

    pub async fn coordinator_status(&self) -> ServiceStatus {
        *self.coordinator_status.read().await
    }

    async fn connect_agent(&self) {
        match connect_channel(&self.agent_endpoint).await {
            Ok(channel) => {
                *self.agent.write().await = Some(AgentServiceClient::new(channel));
                *self.agent_status.write().await = ServiceStatus::Connected;
                info!(endpoint = %self.agent_endpoint, "AgentService connected");
            }
            Err(e) => {
                *self.agent_status.write().await = ServiceStatus::Disconnected;
                warn!(endpoint = %self.agent_endpoint, error = %e, "AgentService connection failed");
            }
        }
    }

    async fn connect_highway(&self) {
        match connect_channel(&self.highway_endpoint).await {
            Ok(channel) => {
                *self.highway.write().await = Some(HighwayServiceClient::new(channel));
                *self.highway_status.write().await = ServiceStatus::Connected;
                info!(endpoint = %self.highway_endpoint, "HighwayService connected");
            }
            Err(e) => {
                *self.highway_status.write().await = ServiceStatus::Disconnected;
                warn!(endpoint = %self.highway_endpoint, error = %e, "HighwayService connection failed");
            }
        }
    }

    async fn connect_coordinator(&self) {
        match connect_channel(&self.coordinator_endpoint).await {
            Ok(channel) => {
                *self.coordinator.write().await = Some(CoordinatorServiceClient::new(channel));
                *self.coordinator_status.write().await = ServiceStatus::Connected;
                info!(endpoint = %self.coordinator_endpoint, "CoordinatorService connected");
            }
            Err(e) => {
                *self.coordinator_status.write().await = ServiceStatus::Disconnected;
                warn!(endpoint = %self.coordinator_endpoint, error = %e, "CoordinatorService connection failed");
            }
        }
    }

    /// Attempt to reconnect a specific service.
    pub async fn reconnect_agent(&self) {
        self.connect_agent().await;
    }

    pub async fn reconnect_highway(&self) {
        self.connect_highway().await;
    }

    pub async fn reconnect_coordinator(&self) {
        self.connect_coordinator().await;
    }

    /// Snapshot of all three service statuses. Intended for the health endpoint —
    /// acquires and releases three read locks, so the cost is a constant handful of
    /// atomic ops. Field order is stable (agent, highway, coordinator).
    pub async fn status_snapshot(&self) -> [(&'static str, ServiceStatus); 3] {
        [
            ("runtime_agent", *self.agent_status.read().await),
            ("runtime_highway", *self.highway_status.read().await),
            ("runtime_coordinator", *self.coordinator_status.read().await),
        ]
    }
}

impl ServiceStatus {
    /// Map the internal status into the string shape used by the `/api/health` response.
    /// - `Connected` → `"ok"`
    /// - `Disconnected` → `"error"` (after a failed dial; caller should trigger reconnect)
    /// - `Unknown` → `"error"` (the pool has never successfully dialed this channel)
    pub fn as_health_string(self) -> &'static str {
        match self {
            ServiceStatus::Connected => "ok",
            ServiceStatus::Disconnected => "error",
            ServiceStatus::Unknown => "error",
        }
    }
}

async fn connect_channel(endpoint: &str) -> Result<Channel, tonic::transport::Error> {
    Endpoint::from_shared(endpoint.to_string())?
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(30))
        .connect()
        .await
}

fn normalize_endpoint(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{addr}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_scheme() {
        assert_eq!(normalize_endpoint("[::1]:9090"), "http://[::1]:9090");
        assert_eq!(normalize_endpoint("http://[::1]:9090"), "http://[::1]:9090");
    }

    #[tokio::test]
    async fn pool_starts_unknown() {
        let pool = GrpcPool::new("[::1]:9090", "[::1]:9091", "[::1]:9092");
        assert_eq!(pool.agent_status().await, ServiceStatus::Unknown);
        assert_eq!(pool.highway_status().await, ServiceStatus::Unknown);
        assert_eq!(pool.coordinator_status().await, ServiceStatus::Unknown);
    }

    #[test]
    fn service_status_health_strings() {
        assert_eq!(ServiceStatus::Connected.as_health_string(), "ok");
        assert_eq!(ServiceStatus::Disconnected.as_health_string(), "error");
        assert_eq!(ServiceStatus::Unknown.as_health_string(), "error");
    }

    #[tokio::test]
    async fn status_snapshot_returns_three_keys_in_order() {
        let pool = GrpcPool::new("[::1]:9090", "[::1]:9091", "[::1]:9092");
        let snap = pool.status_snapshot().await;
        assert_eq!(snap[0].0, "runtime_agent");
        assert_eq!(snap[1].0, "runtime_highway");
        assert_eq!(snap[2].0, "runtime_coordinator");
        assert!(snap.iter().all(|(_, s)| *s == ServiceStatus::Unknown));
    }

    #[tokio::test]
    async fn reconnect_highway_exists_and_is_callable() {
        // Bind an unused port so the connect fails fast but symmetrically with the
        // agent + coordinator reconnect helpers.
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        pool.reconnect_highway().await;
        assert_eq!(pool.highway_status().await, ServiceStatus::Disconnected);
    }
}
