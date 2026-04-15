use serde::{Deserialize, Serialize};

/// Resource consumption tracking (runtime spec §12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResourceUsage {
    pub tokens: u64,
    pub wall_time_ms: u64,
    pub storage_bytes: u64,
    pub network_bytes: u64,
    /// Cost in microdollars.
    pub cost_micros: u64,
}

/// Per-workspace resource limits (PROTOCOL.md §6.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceBudget {
    pub max_tokens: Option<u64>,
    pub max_wall_time_ms: Option<u64>,
    pub max_storage_bytes: Option<u64>,
    pub max_network_bytes: Option<u64>,
    pub max_cost_micros: Option<u64>,
    /// 0.0–1.0, default 0.8.
    pub warning_threshold: f32,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            max_tokens: None,
            max_wall_time_ms: None,
            max_storage_bytes: None,
            max_network_bytes: None,
            max_cost_micros: None,
            warning_threshold: 0.8,
        }
    }
}
