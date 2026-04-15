//! `MockRuntime` — starts gRPC + REST servers on random ports for testing.
//!
//! Usage:
//! ```ignore
//! let rt = MockRuntime::start().await.unwrap();
//! // agent_addr, highway_addr, coordinator_addr, rest_addr available
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tonic::transport::Server;
use tracing::info;

use crate::fixtures;
use crate::mock_grpc::{
    AgentServiceServer, CoordinatorServiceServer, HighwayConfig, HighwayServiceServer,
    MockAgentService, MockCoordinatorService, MockHighwayService,
};
use crate::mock_rest::{RestState, rest_router};

/// A running mock WACP runtime with all four endpoints.
pub struct MockRuntime {
    pub agent_addr: SocketAddr,
    pub highway_addr: SocketAddr,
    pub coordinator_addr: SocketAddr,
    pub rest_addr: SocketAddr,
    /// When `start_with_highway_config` was used, holds the shared config so
    /// tests can program outcomes and inspect captured requests.
    pub highway_config: Option<Arc<HighwayConfig>>,
    _handles: Vec<JoinHandle<()>>,
}

impl MockRuntime {
    /// Start all four servers on random OS-assigned ports.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::start_with_highway_config(None).await
    }

    /// Same as `start` but installs a programmable `HighwayConfig` on the
    /// mock highway service. The config is also returned via the
    /// `highway_config` field for later mutation / inspection.
    pub async fn start_with_highway_config(
        highway_config: Option<Arc<HighwayConfig>>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut handles = Vec::with_capacity(4);

        // --- Agent gRPC ---
        let agent_listener = TcpListener::bind("[::1]:0").await?;
        let agent_addr = agent_listener.local_addr()?;
        let agent_stream = tokio_stream::wrappers::TcpListenerStream::new(agent_listener);
        handles.push(tokio::spawn(async move {
            Server::builder()
                .add_service(AgentServiceServer::new(MockAgentService))
                .serve_with_incoming(agent_stream)
                .await
                .ok();
        }));
        info!(addr = %agent_addr, "mock AgentService started");

        // --- Highway gRPC ---
        let highway_listener = TcpListener::bind("[::1]:0").await?;
        let highway_addr = highway_listener.local_addr()?;
        let highway_stream = tokio_stream::wrappers::TcpListenerStream::new(highway_listener);
        let highway_service = match highway_config.clone() {
            Some(cfg) => MockHighwayService::with_config(cfg),
            None => MockHighwayService::default(),
        };
        handles.push(tokio::spawn(async move {
            Server::builder()
                .add_service(HighwayServiceServer::new(highway_service))
                .serve_with_incoming(highway_stream)
                .await
                .ok();
        }));
        info!(addr = %highway_addr, "mock HighwayService started");

        // --- Coordinator gRPC ---
        let coord_listener = TcpListener::bind("[::1]:0").await?;
        let coordinator_addr = coord_listener.local_addr()?;
        let coord_stream = tokio_stream::wrappers::TcpListenerStream::new(coord_listener);
        handles.push(tokio::spawn(async move {
            Server::builder()
                .add_service(CoordinatorServiceServer::new(MockCoordinatorService))
                .serve_with_incoming(coord_stream)
                .await
                .ok();
        }));
        info!(addr = %coordinator_addr, "mock CoordinatorService started");

        // --- REST gateway ---
        let rest_listener = TcpListener::bind("[::1]:0").await?;
        let rest_addr = rest_listener.local_addr()?;
        let mut verticals = HashMap::new();
        let simple = fixtures::fixture_simple();
        let complex = fixtures::fixture_complex();
        verticals.insert(simple.id.clone(), simple);
        verticals.insert(complex.id.clone(), complex);
        let state = RestState {
            verticals: Arc::new(verticals),
        };
        let app = rest_router(state);
        handles.push(tokio::spawn(async move {
            axum::serve(rest_listener, app).await.ok();
        }));
        info!(addr = %rest_addr, "mock REST gateway started");

        Ok(MockRuntime {
            agent_addr,
            highway_addr,
            coordinator_addr,
            rest_addr,
            highway_config,
            _handles: handles,
        })
    }
}
