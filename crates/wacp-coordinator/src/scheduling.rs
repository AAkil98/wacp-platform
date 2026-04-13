use wacp_fsm::TaskTrigger;
use wacp_types::{CheckpointId, Task, TaskId, TaskStatus};

use crate::task_graph::{GraphError, TaskGraph};

/// Output of a completed dependency, for workspace context assembly.
#[derive(Debug, Clone, PartialEq)]
pub struct DependencyOutput {
    pub task_id: TaskId,
    pub task_name: String,
    pub checkpoint_ref: CheckpointId,
}

/// Read-only context assembled from dependency outputs.
#[derive(Debug, Clone, Default)]
pub struct TaskContext {
    pub dependency_outputs: Vec<DependencyOutput>,
}

/// Configurable retry policy (task-scheduling.md §7).
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum total attempts (0 = no retry).
    pub max_attempts: u32,
    pub retry_on_timeout: bool,
    pub retry_on_budget: bool,
    pub retry_on_agent_failure: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            retry_on_timeout: false,
            retry_on_budget: false,
            retry_on_agent_failure: true,
        }
    }
}

/// Stateless scheduling operations on the task graph (task-scheduling.md §6–8).
pub struct SchedulingOps;

impl SchedulingOps {
    /// Assemble context from completed dependency outputs.
    /// For each dependency with a checkpoint_ref, include it as a DependencyOutput.
    pub fn assemble_context(graph: &TaskGraph, task_id: &TaskId) -> TaskContext {
        let task = match graph.get(task_id) {
            Some(t) => t,
            None => return TaskContext::default(),
        };

        let mut outputs = Vec::new();
        for dep_id in &task.depends_on {
            let dep = match graph.get(dep_id) {
                Some(t) => t,
                None => continue,
            };
            if !matches!(dep.status, TaskStatus::Completed | TaskStatus::Integrated) {
                continue;
            }
            if let Some(ref cp) = dep.checkpoint_ref {
                outputs.push(DependencyOutput {
                    task_id: dep_id.clone(),
                    task_name: dep.name.clone(),
                    checkpoint_ref: cp.clone(),
                });
            }
        }

        TaskContext {
            dependency_outputs: outputs,
        }
    }

    /// Check if a failed task should be retried.
    pub fn should_retry(policy: &RetryPolicy, task: &Task, reason: &str) -> bool {
        if policy.max_attempts == 0 {
            return false;
        }
        // Attempts = workspace_history length (each entry is a prior attempt).
        let attempts = task.workspace_history.len() as u32;
        if attempts >= policy.max_attempts {
            return false;
        }
        match reason {
            "timeout" => policy.retry_on_timeout,
            "budget_exceeded" => policy.retry_on_budget,
            _ => policy.retry_on_agent_failure,
        }
    }

    /// Prepare a failed task for retry: unbind from workspace.
    /// The task remains in Failed state — the dispatcher uses
    /// `(Failed, Assign) → Assigned` when re-dispatching.
    pub fn prepare_retry(graph: &mut TaskGraph, task_id: &TaskId) {
        graph.unbind(task_id);
    }

    /// Cancel a task and cascade to unreachable dependents.
    /// Returns the list of all cancelled task ids (including the original).
    pub fn cancel_task(graph: &mut TaskGraph, task_id: &TaskId) -> Vec<TaskId> {
        let mut cancelled = Vec::new();

        // Try to cancel the task itself.
        if graph.transition(task_id, TaskTrigger::Cancel).is_ok() {
            cancelled.push(task_id.clone());
        } else {
            return cancelled; // already terminal
        }

        // Cascade: cancel Draft/Pending dependents.
        let dependents = graph.mark_failed(task_id); // reuse: returns forward edges
        for dep_id in dependents {
            if let Some(dep) = graph.get(&dep_id)
                && matches!(dep.status, TaskStatus::Draft | TaskStatus::Pending)
            {
                let sub = Self::cancel_task(graph, &dep_id);
                cancelled.extend(sub);
            }
        }

        cancelled
    }

    /// Insert subtasks into the graph with parent_task set.
    /// Validates that the parent task exists.
    pub fn add_subtasks(
        graph: &mut TaskGraph,
        parent_task_id: &TaskId,
        subtasks: Vec<Task>,
    ) -> Result<Vec<TaskId>, GraphError> {
        // Validate parent exists.
        if graph.get(parent_task_id).is_none() {
            return Err(GraphError::TaskNotFound(parent_task_id.to_string()));
        }

        let mut ids = Vec::new();
        for mut task in subtasks {
            task.parent_task = Some(parent_task_id.clone());
            let id = task.id.clone();
            graph.add_task(task)?;
            ids.push(id);
        }

        Ok(ids)
    }
}
