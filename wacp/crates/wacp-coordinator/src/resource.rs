use std::collections::HashMap;

use wacp_types::{ResourceBudget, ResourceUsage, WorkspaceId, WorkspaceState};

// ── Timeout tracking (runtime.md §12) ───────────────────────────────

/// Timer-active states: Active, Blocked, Conflicted.
fn is_timer_active(state: WorkspaceState) -> bool {
    matches!(
        state,
        WorkspaceState::Active | WorkspaceState::Blocked | WorkspaceState::Conflicted
    )
}

/// Per-workspace timeout entry.
#[derive(Debug, Clone)]
struct TimeoutEntry {
    elapsed_ms: u64,
    limit_ms: u64,
    running: bool,
    last_resume_ms: u64,
}

/// Tracks per-workspace elapsed time in timer-active states (runtime.md §12).
///
/// Pure state tracking — the coordinator advances time by calling `tick`.
/// No async, no tokio timers. The coordinator calls `check_expired` to
/// find workspaces that have exceeded their timeout.
pub struct TimeoutTracker {
    entries: HashMap<String, TimeoutEntry>,
}

impl TimeoutTracker {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Register a workspace with a timeout limit.
    /// A limit of 0 means no timeout.
    pub fn register(&mut self, id: &WorkspaceId, limit_ms: u64) {
        self.entries.insert(
            id.to_string(),
            TimeoutEntry {
                elapsed_ms: 0,
                limit_ms,
                running: false,
                last_resume_ms: 0,
            },
        );
    }

    /// Notify a workspace state change. Starts/stops the timer based on state.
    pub fn on_state_change(&mut self, id: &WorkspaceId, new_state: WorkspaceState, now_ms: u64) {
        let entry = match self.entries.get_mut(id.as_ref()) {
            Some(e) => e,
            None => return,
        };

        let should_run = is_timer_active(new_state);

        if should_run && !entry.running {
            // Resume timer.
            entry.running = true;
            entry.last_resume_ms = now_ms;
        } else if !should_run && entry.running {
            // Pause timer — accumulate elapsed.
            entry.elapsed_ms += now_ms.saturating_sub(entry.last_resume_ms);
            entry.running = false;
        }
    }

    /// Extend a workspace's timeout additively. Elapsed time is never zeroed.
    pub fn extend(&mut self, id: &WorkspaceId, additional_ms: u64) {
        if let Some(entry) = self.entries.get_mut(id.as_ref()) {
            entry.limit_ms += additional_ms;
        }
    }

    /// Compute the current elapsed time for a workspace at the given timestamp.
    pub fn elapsed_ms(&self, id: &WorkspaceId, now_ms: u64) -> u64 {
        match self.entries.get(id.as_ref()) {
            Some(entry) => {
                let base = entry.elapsed_ms;
                if entry.running {
                    base + now_ms.saturating_sub(entry.last_resume_ms)
                } else {
                    base
                }
            }
            None => 0,
        }
    }

    /// Check which workspaces have exceeded their timeout at the given timestamp.
    /// Returns workspace ids that should be aborted. Ignores 0-limit (no timeout).
    pub fn check_expired(&self, now_ms: u64) -> Vec<WorkspaceId> {
        self.entries
            .iter()
            .filter_map(|(id, entry)| {
                if entry.limit_ms == 0 {
                    return None; // no timeout
                }
                let elapsed = if entry.running {
                    entry.elapsed_ms + now_ms.saturating_sub(entry.last_resume_ms)
                } else {
                    entry.elapsed_ms
                };
                if elapsed >= entry.limit_ms {
                    Some(WorkspaceId::from(id.as_str()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Remove a workspace (terminal state).
    pub fn remove(&mut self, id: &WorkspaceId) {
        self.entries.remove(id.as_ref());
    }
}

impl Default for TimeoutTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Budget enforcement (runtime.md §12) ─────────────────────────────

/// Result of a budget check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetCheckResult {
    /// Within limits.
    Ok,
    /// At least one dimension crossed the warning threshold.
    Warning(Vec<String>),
    /// At least one dimension exceeded the hard limit.
    Exceeded(Vec<String>),
}

/// Checks resource usage against budget limits (runtime.md §12).
pub struct BudgetEnforcer;

impl BudgetEnforcer {
    /// Check usage against budget. Returns the most severe result.
    pub fn check(usage: &ResourceUsage, budget: &ResourceBudget) -> BudgetCheckResult {
        let mut warnings = Vec::new();
        let mut exceeded = Vec::new();

        Self::check_dim(
            "tokens",
            usage.tokens,
            budget.max_tokens,
            budget.warning_threshold,
            &mut warnings,
            &mut exceeded,
        );
        Self::check_dim(
            "wall_time_ms",
            usage.wall_time_ms,
            budget.max_wall_time_ms,
            budget.warning_threshold,
            &mut warnings,
            &mut exceeded,
        );
        Self::check_dim(
            "storage_bytes",
            usage.storage_bytes,
            budget.max_storage_bytes,
            budget.warning_threshold,
            &mut warnings,
            &mut exceeded,
        );
        Self::check_dim(
            "network_bytes",
            usage.network_bytes,
            budget.max_network_bytes,
            budget.warning_threshold,
            &mut warnings,
            &mut exceeded,
        );
        Self::check_dim(
            "cost_micros",
            usage.cost_micros,
            budget.max_cost_micros,
            budget.warning_threshold,
            &mut warnings,
            &mut exceeded,
        );

        if !exceeded.is_empty() {
            BudgetCheckResult::Exceeded(exceeded)
        } else if !warnings.is_empty() {
            BudgetCheckResult::Warning(warnings)
        } else {
            BudgetCheckResult::Ok
        }
    }

    fn check_dim(
        name: &str,
        used: u64,
        limit: Option<u64>,
        threshold: f32,
        warnings: &mut Vec<String>,
        exceeded: &mut Vec<String>,
    ) {
        let limit = match limit {
            Some(l) if l > 0 => l,
            _ => return, // unlimited
        };

        if used >= limit {
            exceeded.push(name.to_string());
        } else if used as f64 >= limit as f64 * threshold as f64 {
            warnings.push(name.to_string());
        }
    }

    /// Increase a budget additively. Budgets are never decreased.
    pub fn increase_budget(budget: &mut ResourceBudget, additional: &ResourceBudget) {
        if let (Some(cur), Some(add)) = (budget.max_tokens, additional.max_tokens) {
            budget.max_tokens = Some(cur + add);
        }
        if let (Some(cur), Some(add)) = (budget.max_wall_time_ms, additional.max_wall_time_ms) {
            budget.max_wall_time_ms = Some(cur + add);
        }
        if let (Some(cur), Some(add)) = (budget.max_storage_bytes, additional.max_storage_bytes) {
            budget.max_storage_bytes = Some(cur + add);
        }
        if let (Some(cur), Some(add)) = (budget.max_network_bytes, additional.max_network_bytes) {
            budget.max_network_bytes = Some(cur + add);
        }
        if let (Some(cur), Some(add)) = (budget.max_cost_micros, additional.max_cost_micros) {
            budget.max_cost_micros = Some(cur + add);
        }
    }
}

// ── Liveness monitoring (runtime.md §12) ─────────────────────────────

/// Tracks last activity timestamp per workspace for liveness monitoring.
/// Advisory — the coordinator decides the response to inactivity.
pub struct LivenessMonitor {
    /// workspace → last activity timestamp (ms).
    last_activity: HashMap<String, u64>,
    /// Inactivity interval before warning (ms). 0 = disabled.
    interval_ms: u64,
}

impl LivenessMonitor {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            last_activity: HashMap::new(),
            interval_ms,
        }
    }

    /// Register a workspace.
    pub fn register(&mut self, id: &WorkspaceId, now_ms: u64) {
        self.last_activity.insert(id.to_string(), now_ms);
    }

    /// Record activity for a workspace.
    pub fn record_activity(&mut self, id: &WorkspaceId, now_ms: u64) {
        self.last_activity.insert(id.to_string(), now_ms);
    }

    /// Check which workspaces have been inactive beyond the interval.
    /// Returns empty if monitoring is disabled (interval_ms == 0).
    pub fn check_inactive(&self, now_ms: u64) -> Vec<WorkspaceId> {
        if self.interval_ms == 0 {
            return vec![];
        }
        self.last_activity
            .iter()
            .filter_map(|(id, &last)| {
                if now_ms.saturating_sub(last) >= self.interval_ms {
                    Some(WorkspaceId::from(id.as_str()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Remove a workspace (terminal state).
    pub fn remove(&mut self, id: &WorkspaceId) {
        self.last_activity.remove(id.as_ref());
    }

    /// Reset activity timestamp for a workspace.
    /// Called after migration completion to prevent false liveness
    /// warnings from the migration gap.
    pub fn reset_activity(&mut self, workspace_id: &WorkspaceId, now_ms: u64) {
        if let Some(ts) = self.last_activity.get_mut(workspace_id.as_ref()) {
            *ts = now_ms;
        }
    }

    /// Is monitoring enabled?
    pub fn is_enabled(&self) -> bool {
        self.interval_ms > 0
    }
}
