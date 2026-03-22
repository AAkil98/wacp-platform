use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub data_dir: PathBuf,
    pub taxonomy_path: Option<PathBuf>,
    pub protocol_version: String,
    pub max_segment_size: u64,
    pub agent_port: u16,
    pub highway_port: u16,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("data"),
            taxonomy_path: None,
            protocol_version: "0.1".into(),
            max_segment_size: 64 * 1024 * 1024,
            agent_port: 9400,
            highway_port: 9401,
        }
    }
}
