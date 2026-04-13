use std::net::SocketAddr;

use tokio::sync::mpsc;
use tonic::transport::Server;
use tonic::transport::server::ServerTlsConfig;

use crate::TransportError;
use crate::grpc_agent::{AgentRequest, AgentServiceImpl};
use crate::grpc_coordinator::{CoordinatorRequest, CoordinatorServiceImpl};
use crate::grpc_highway::{HighwayRequest, HighwayServiceImpl};
use crate::proto::wacp_v1::agent_service_server::AgentServiceServer;
use crate::proto::wacp_v1::coordinator_service_server::CoordinatorServiceServer;
use crate::proto::wacp_v1::highway_service_server::HighwayServiceServer;

/// Configuration for the gRPC server.
pub struct GrpcServerConfig {
    pub agent_addr: SocketAddr,
    pub highway_addr: SocketAddr,
    pub coordinator_addr: SocketAddr,
    /// When set, all endpoints serve TLS. When None, plaintext.
    pub tls: Option<ServerTlsConfig>,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            agent_addr: ([0, 0, 0, 0], 9090).into(),
            highway_addr: ([0, 0, 0, 0], 9091).into(),
            coordinator_addr: ([0, 0, 0, 0], 9092).into(),
            tls: None,
        }
    }
}

/// Handles returned from starting the gRPC server.
pub struct GrpcServerHandles {
    pub agent_request_rx: mpsc::Receiver<AgentRequest>,
    pub highway_request_rx: mpsc::Receiver<HighwayRequest>,
    pub coordinator_request_rx: mpsc::Receiver<CoordinatorRequest>,
    /// Cloned senders — used by the REST gateway's ChannelBackend to send
    /// requests through the same channels the gRPC handlers use.
    pub highway_request_tx: mpsc::Sender<HighwayRequest>,
    pub coordinator_request_tx: mpsc::Sender<CoordinatorRequest>,
}

/// Start the gRPC server with agent, highway, and coordinator services.
/// Returns channels for the coordinator actor to receive requests.
pub async fn start_grpc_server(
    config: GrpcServerConfig,
) -> Result<GrpcServerHandles, TransportError> {
    let (agent_tx, agent_rx) = mpsc::channel(256);
    let (highway_tx, highway_rx) = mpsc::channel(256);
    let (coordinator_tx, coordinator_rx) = mpsc::channel(256);

    let highway_tx_clone = highway_tx.clone();
    let coordinator_tx_clone = coordinator_tx.clone();

    let agent_service = AgentServiceImpl::new(agent_tx);
    let highway_service = HighwayServiceImpl::new(highway_tx);
    let coordinator_service = CoordinatorServiceImpl::new(coordinator_tx);

    // Spawn agent service on its own port.
    let agent_addr = config.agent_addr;
    let agent_tls = config.tls.clone();
    tokio::spawn(async move {
        let mut builder = Server::builder();
        if let Some(tls) = agent_tls {
            builder = builder
                .tls_config(tls)
                .expect("agent TLS configuration failed");
        }
        builder
            .add_service(AgentServiceServer::new(agent_service))
            .serve(agent_addr)
            .await
            .expect("agent gRPC server failed");
    });

    // Spawn highway service on its own port.
    let highway_addr = config.highway_addr;
    let highway_tls = config.tls.clone();
    tokio::spawn(async move {
        let mut builder = Server::builder();
        if let Some(tls) = highway_tls {
            builder = builder
                .tls_config(tls)
                .expect("highway TLS configuration failed");
        }
        builder
            .add_service(HighwayServiceServer::new(highway_service))
            .serve(highway_addr)
            .await
            .expect("highway gRPC server failed");
    });

    // Spawn coordinator service on its own port.
    let coordinator_addr = config.coordinator_addr;
    let coordinator_tls = config.tls;
    tokio::spawn(async move {
        let mut builder = Server::builder();
        if let Some(tls) = coordinator_tls {
            builder = builder
                .tls_config(tls)
                .expect("coordinator TLS configuration failed");
        }
        builder
            .add_service(CoordinatorServiceServer::new(coordinator_service))
            .serve(coordinator_addr)
            .await
            .expect("coordinator gRPC server failed");
    });

    Ok(GrpcServerHandles {
        agent_request_rx: agent_rx,
        highway_request_rx: highway_rx,
        coordinator_request_rx: coordinator_rx,
        highway_request_tx: highway_tx_clone,
        coordinator_request_tx: coordinator_tx_clone,
    })
}
