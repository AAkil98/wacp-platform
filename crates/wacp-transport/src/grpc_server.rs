use std::net::SocketAddr;

use tokio::sync::mpsc;
use tonic::transport::server::ServerTlsConfig;
use tonic::transport::Server;

use crate::grpc_agent::{AgentRequest, AgentServiceImpl};
use crate::grpc_highway::{HighwayRequest, HighwayServiceImpl};
use crate::proto::wacp_v1::agent_service_server::AgentServiceServer;
use crate::proto::wacp_v1::highway_service_server::HighwayServiceServer;
use crate::TransportError;

/// Configuration for the gRPC server.
pub struct GrpcServerConfig {
    pub agent_addr: SocketAddr,
    pub highway_addr: SocketAddr,
    /// When set, both endpoints serve TLS. When None, plaintext.
    pub tls: Option<ServerTlsConfig>,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            agent_addr: ([0, 0, 0, 0], 9400).into(),
            highway_addr: ([0, 0, 0, 0], 9401).into(),
            tls: None,
        }
    }
}

/// Handles returned from starting the gRPC server.
pub struct GrpcServerHandles {
    pub agent_request_rx: mpsc::Receiver<AgentRequest>,
    pub highway_request_rx: mpsc::Receiver<HighwayRequest>,
}

/// Start the gRPC server with both agent and highway services.
/// Returns channels for the coordinator to receive requests.
pub async fn start_grpc_server(
    config: GrpcServerConfig,
) -> Result<GrpcServerHandles, TransportError> {
    let (agent_tx, agent_rx) = mpsc::channel(256);
    let (highway_tx, highway_rx) = mpsc::channel(256);

    let agent_service = AgentServiceImpl::new(agent_tx);
    let highway_service = HighwayServiceImpl::new(highway_tx);

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
    let highway_tls = config.tls;
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

    Ok(GrpcServerHandles {
        agent_request_rx: agent_rx,
        highway_request_rx: highway_rx,
    })
}
