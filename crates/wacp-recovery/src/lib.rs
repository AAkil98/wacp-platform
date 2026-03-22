//! WACP — Recovery engine.
//!
//! Reconstructs runtime state from the trail after a crash.
//! Five steps: integrity check, state reconstruction, in-flight detection,
//! timer reconstruction (placeholder), clock recovery.

use std::collections::{HashMap, HashSet};

use wacp_clock::Timestamp;
use wacp_trail::{verify_chain, ChainHash, SegmentId, StorageError, TrailStorage};
use wacp_types::*;

/// Error during recovery.
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("trail corruption at index {index}: {details}")]
    TrailCorruption { index: usize, details: String },

    #[error("storage error: {0}")]
    StorageError(#[from] StorageError),
}

/// Recovered state from trail replay.
#[derive(Debug)]
pub struct RecoveredState {
    /// Workspace ID → last known state.
    pub workspace_states: HashMap<String, WorkspaceState>,
    /// Task ID → last known status.
    pub task_statuses: HashMap<String, TaskStatus>,
    /// Last trail sequence number processed.
    pub last_sequence: u64,
    /// Clock initial timestamp (last entry + 1 logical tick).
    pub clock_initial: Timestamp,
    /// Envelope IDs that were created but not delivered.
    pub in_flight_envelopes: Vec<String>,
}

/// The recovery engine (runtime spec §13).
pub struct RecoveryEngine;

impl RecoveryEngine {
    /// Full recovery procedure.
    pub fn recover(trail: &dyn TrailStorage) -> Result<RecoveredState, RecoveryError> {
        let metadata = trail.metadata();

        // Empty trail → empty state.
        if metadata.total_entries == 0 {
            return Ok(RecoveredState {
                workspace_states: HashMap::new(),
                task_statuses: HashMap::new(),
                last_sequence: 0,
                clock_initial: Timestamp::ZERO,
                in_flight_envelopes: vec![],
            });
        }

        // Step 1: Trail integrity check — verify hash chain.
        let all_entries = Self::collect_entries(trail, &metadata)?;
        Self::verify_integrity(&all_entries)?;

        // Step 2: State reconstruction from trail replay.
        let (workspace_states, task_statuses) = Self::reconstruct_state(&all_entries);

        // Step 3: In-flight detection.
        let in_flight_envelopes = Self::detect_in_flight(&all_entries);

        // Step 5: Clock recovery — last entry timestamp + 1 logical tick.
        let last_timestamp = Self::extract_last_timestamp(&all_entries);
        let clock_initial = last_timestamp.succ();

        Ok(RecoveredState {
            workspace_states,
            task_statuses,
            last_sequence: metadata.total_entries,
            clock_initial,
            in_flight_envelopes,
        })
    }

    /// Collect all entries from all segments.
    fn collect_entries(
        trail: &dyn TrailStorage,
        metadata: &wacp_trail::StorageMetadata,
    ) -> Result<Vec<(Vec<u8>, ChainHash)>, RecoveryError> {
        let mut all = Vec::new();
        for seg_id in 0..=metadata.current_segment.0 {
            let entries = trail.scan(SegmentId(seg_id), 0)?;
            for (_offset, entry_bytes, hash) in entries {
                all.push((entry_bytes, ChainHash(hash)));
            }
        }
        Ok(all)
    }

    /// Step 1: Verify the hash chain.
    fn verify_integrity(entries: &[(Vec<u8>, ChainHash)]) -> Result<(), RecoveryError> {
        verify_chain(entries).map_err(|e| RecoveryError::TrailCorruption {
            index: e.index,
            details: format!(
                "expected {}, computed {}",
                e.expected, e.computed
            ),
        })
    }

    /// Step 2: Replay entries to reconstruct state.
    /// Uses a simple JSON-based event body parsing approach.
    fn reconstruct_state(
        entries: &[(Vec<u8>, ChainHash)],
    ) -> (HashMap<String, WorkspaceState>, HashMap<String, TaskStatus>) {
        let mut ws_states: HashMap<String, WorkspaceState> = HashMap::new();
        let mut task_statuses: HashMap<String, TaskStatus> = HashMap::new();

        for (entry_bytes, _) in entries {
            // Attempt to parse entry as JSON for event processing.
            if let Ok(event) = serde_json::from_slice::<serde_json::Value>(entry_bytes)
                && let Some(event_type) = event.get("event_type").and_then(|v| v.as_str()) {
                    match event_type {
                        "workspace_created" | "workspace_state_changed" => {
                            if let (Some(ws_id), Some(state_str)) = (
                                event.get("workspace_id").and_then(|v| v.as_str()),
                                event.get("new_state").and_then(|v| v.as_str()),
                            )
                                && let Some(state) = parse_workspace_state(state_str) {
                                    ws_states.insert(ws_id.to_string(), state);
                                }
                        }
                        "task_status_changed" => {
                            if let (Some(task_id), Some(status_str)) = (
                                event.get("task_id").and_then(|v| v.as_str()),
                                event.get("new_status").and_then(|v| v.as_str()),
                            )
                                && let Some(status) = parse_task_status(status_str) {
                                    task_statuses.insert(task_id.to_string(), status);
                                }
                        }
                        _ => {}
                    }
                }
        }

        (ws_states, task_statuses)
    }

    /// Step 3: Detect in-flight envelopes (created but not delivered).
    fn detect_in_flight(entries: &[(Vec<u8>, ChainHash)]) -> Vec<String> {
        let mut created: HashSet<String> = HashSet::new();
        let mut delivered: HashSet<String> = HashSet::new();

        for (entry_bytes, _) in entries {
            if let Ok(event) = serde_json::from_slice::<serde_json::Value>(entry_bytes)
                && let Some(event_type) = event.get("event_type").and_then(|v| v.as_str())
                    && let Some(env_id) = event.get("envelope_id").and_then(|v| v.as_str()) {
                        match event_type {
                            "envelope_created" => {
                                created.insert(env_id.to_string());
                            }
                            "envelope_delivered" => {
                                delivered.insert(env_id.to_string());
                            }
                            _ => {}
                        }
                    }
        }

        created.difference(&delivered).cloned().collect()
    }

    /// Extract timestamp from the last entry (by parsing or using a default).
    fn extract_last_timestamp(entries: &[(Vec<u8>, ChainHash)]) -> Timestamp {
        if let Some((entry_bytes, _)) = entries.last()
            && let Ok(event) = serde_json::from_slice::<serde_json::Value>(entry_bytes)
                && let Some(ts) = event.get("timestamp").and_then(|v| v.as_u64()) {
                    return Timestamp::new(ts, 0);
                }
        Timestamp::ZERO
    }
}

fn parse_workspace_state(s: &str) -> Option<WorkspaceState> {
    match s {
        "Idle" => Some(WorkspaceState::Idle),
        "Active" => Some(WorkspaceState::Active),
        "Blocked" => Some(WorkspaceState::Blocked),
        "Suspended" => Some(WorkspaceState::Suspended),
        "Migrating" => Some(WorkspaceState::Migrating),
        "Integrating" => Some(WorkspaceState::Integrating),
        "Conflicted" => Some(WorkspaceState::Conflicted),
        "Closed" => Some(WorkspaceState::Closed),
        "Failed" => Some(WorkspaceState::Failed),
        _ => None,
    }
}

fn parse_task_status(s: &str) -> Option<TaskStatus> {
    match s {
        "Draft" => Some(TaskStatus::Draft),
        "Pending" => Some(TaskStatus::Pending),
        "Assigned" => Some(TaskStatus::Assigned),
        "InProgress" => Some(TaskStatus::InProgress),
        "Completed" => Some(TaskStatus::Completed),
        "Failed" => Some(TaskStatus::Failed),
        "Integrated" => Some(TaskStatus::Integrated),
        "Cancelled" => Some(TaskStatus::Cancelled),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
