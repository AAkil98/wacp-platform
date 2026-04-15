use std::net::SocketAddr;
use std::path::PathBuf;

/// Application configuration, resolved from defaults + settings table + environment.
#[derive(Debug, Clone)]
pub struct ConsoleConfig {
    /// HTTP listen address for the Axum server.
    pub listen_addr: SocketAddr,

    /// Path to the SQLite database file.
    pub database_path: PathBuf,

    /// Path to the data directory (XDG-aware).
    pub data_dir: PathBuf,

    /// Runtime connection addresses.
    pub runtime: RuntimeConfig,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// AgentService gRPC address.
    pub agent_address: String,
    /// HighwayService gRPC address.
    pub highway_address: String,
    /// CoordinatorService gRPC address.
    pub coordinator_address: String,
    /// REST gateway address.
    pub rest_address: String,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            agent_address: "[::1]:9090".to_string(),
            highway_address: "[::1]:9091".to_string(),
            coordinator_address: "[::1]:9092".to_string(),
            rest_address: "http://[::1]:9093".to_string(),
        }
    }
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            listen_addr: "[::1]:8080".parse().expect("valid default listen addr"),
            database_path: PathBuf::from("console.db"),
            data_dir: PathBuf::from("."),
            runtime: RuntimeConfig::default(),
        }
    }
}
