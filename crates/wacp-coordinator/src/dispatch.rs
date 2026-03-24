use wacp_types::{Originator, ResourceBudget, Task, TaskId, WorkspaceId, WorkspaceState};

use crate::task_graph::TaskGraph;
use crate::topology::{CreateWorkspaceParams, TopologySet};

/// Configuration for the dispatcher.
#[derive(Debug, Clone)]
pub struct DispatchConfig {
    pub max_concurrent_workspaces: Option<usize>,
    pub default_budget: ResourceBudget,
    /// Margin over estimate, e.g. 0.2 for 20%.
    pub budget_margin: f32,
}

impl Default for DispatchConfig {
    fn default() -> Self {
        Self {
            max_concurrent_workspaces: None,
            default_budget: ResourceBudget::default(),
            budget_margin: 0.2,
        }
    }
}

/// A completed dispatch action — returned to the orchestrator for
/// trail recording and workspace actor spawning.
#[derive(Debug, Clone)]
pub struct DispatchAction {
    pub task_id: TaskId,
    pub workspace_id: WorkspaceId,
    pub parent: WorkspaceId,
    pub budget: ResourceBudget,
}

/// Selects dispatchable tasks, allocates budgets, creates workspaces,
/// and binds tasks (task-scheduling.md §4–5).
pub struct Dispatcher {
    config: DispatchConfig,
    next_ws_id: u64,
}

impl Dispatcher {
    pub fn new(config: DispatchConfig) -> Self {
        Self {
            config,
            next_ws_id: 1,
        }
    }

    /// Main dispatch entry: collect dispatchable tasks, check capacity,
    /// create workspaces, bind. Returns actions taken.
    pub fn try_dispatch(
        &mut self,
        graph: &mut TaskGraph,
        topo: &mut TopologySet,
    ) -> Vec<DispatchAction> {
        let dispatchable: Vec<TaskId> = graph.dispatchable().into_iter().cloned().collect();
        if dispatchable.is_empty() {
            return vec![];
        }

        let mut actions = Vec::new();

        for task_id in dispatchable {
            if !Self::has_capacity(topo, self.config.max_concurrent_workspaces) {
                break;
            }

            let task = match graph.get(&task_id) {
                Some(t) => t.clone(),
                None => continue,
            };

            let parent = Self::select_parent(graph, &task, topo.tree.root());
            let budget = self.allocate_budget(&self.config.default_budget);
            let workspace_id = self.generate_workspace_id();

            let params = CreateWorkspaceParams {
                id: workspace_id.clone(),
                parent: parent.clone(),
                owner: topo
                    .tree
                    .get(topo.tree.root())
                    .map(|n| n.owner.clone())
                    .unwrap_or_else(|| wacp_types::UserId::from("system")),
                originator: Originator::System,
                status: WorkspaceState::Idle,
                task_id: Some(task_id.clone()),
            };

            if topo.create_workspace(params).is_err() {
                continue;
            }

            if graph.bind(&task_id, &workspace_id).is_err() {
                continue;
            }

            actions.push(DispatchAction {
                task_id,
                workspace_id,
                parent,
                budget,
            });
        }

        actions
    }

    /// Select parent workspace for a task.
    /// Subtask → workspace executing the parent task. Root task → coordinator root.
    pub fn select_parent(graph: &TaskGraph, task: &Task, root: &WorkspaceId) -> WorkspaceId {
        if let Some(ref parent_task_id) = task.parent_task
            && let Some(parent_task) = graph.get(parent_task_id)
            && let Some(ref ws) = parent_task.workspace_ref
        {
            return ws.clone();
        }
        root.clone()
    }

    /// Allocate a budget by applying the configured margin to the default.
    pub fn allocate_budget(&self, base: &ResourceBudget) -> ResourceBudget {
        let scale = 1.0_f64 + self.config.budget_margin as f64;
        ResourceBudget {
            max_tokens: base.max_tokens.map(|v| (v as f64 * scale) as u64),
            max_wall_time_ms: base.max_wall_time_ms.map(|v| (v as f64 * scale) as u64),
            max_storage_bytes: base.max_storage_bytes.map(|v| (v as f64 * scale) as u64),
            max_network_bytes: base.max_network_bytes.map(|v| (v as f64 * scale) as u64),
            max_cost_micros: base.max_cost_micros.map(|v| (v as f64 * scale) as u64),
            warning_threshold: base.warning_threshold,
        }
    }

    /// Check if there's capacity for another workspace.
    pub fn has_capacity(topo: &TopologySet, max: Option<usize>) -> bool {
        match max {
            None => true,
            Some(limit) => {
                let active = topo.tree.active_workspaces().len();
                // Subtract 1 for root which is always active.
                let worker_count = if active > 0 { active - 1 } else { 0 };
                worker_count < limit
            }
        }
    }

    /// Generate a monotonic workspace id.
    pub fn generate_workspace_id(&mut self) -> WorkspaceId {
        let id = WorkspaceId::from(format!("ws-{}", self.next_ws_id));
        self.next_ws_id += 1;
        id
    }
}
