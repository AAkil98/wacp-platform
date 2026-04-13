use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wacp_fsm::{StateMachine, TaskFsm, TaskTrigger, TransitionError};
use wacp_types::{Task, TaskId, TaskStatus, WorkspaceId};

/// Error from task graph operations.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("dependency not found: {0}")]
    DependencyNotFound(String),
    #[error("task already exists: {0}")]
    DuplicateTask(String),
    #[error("task not found: {0}")]
    TaskNotFound(String),
    #[error("transition error: {0}")]
    Transition(#[from] TransitionError),
    #[error("workspace already bound: {0}")]
    WorkspaceAlreadyBound(WorkspaceId),
    #[error("task already bound: {0}")]
    TaskAlreadyBound(TaskId),
}

/// DAG of tasks with dependency tracking (PROTOCOL.md §4.6, topology.md §3).
///
/// Readiness is counter-based: each task tracks how many non-terminal
/// dependencies remain. When a dependency completes, its dependents'
/// counters are decremented. Counter reaching zero = ready.
#[derive(Serialize, Deserialize)]
pub struct TaskGraph {
    tasks: HashMap<String, Task>,
    /// Per-task readiness counter: how many deps are not yet terminal.
    remaining_deps: HashMap<String, u32>,
    /// Forward adjacency: task → tasks that depend on it (dependents).
    forward: HashMap<String, Vec<TaskId>>,
    /// Task → workspace binding.
    task_workspace: HashMap<String, WorkspaceId>,
    /// Workspace → task reverse binding.
    workspace_task: HashMap<String, TaskId>,
}

impl TaskGraph {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            remaining_deps: HashMap::new(),
            forward: HashMap::new(),
            task_workspace: HashMap::new(),
            workspace_task: HashMap::new(),
        }
    }

    /// Add a task. All dependencies must already exist in the graph.
    /// Computes initial readiness counter and builds forward edges.
    pub fn add_task(&mut self, task: Task) -> Result<(), GraphError> {
        let id_str = task.id.to_string();
        if self.tasks.contains_key(&id_str) {
            return Err(GraphError::DuplicateTask(id_str));
        }

        // Validate deps exist, count non-terminal ones.
        let mut remaining = 0u32;
        for dep in &task.depends_on {
            let dep_task = self
                .tasks
                .get(dep.as_ref())
                .ok_or_else(|| GraphError::DependencyNotFound(dep.to_string()))?;

            // Build forward edge: dep → this task.
            self.forward
                .entry(dep.to_string())
                .or_default()
                .push(task.id.clone());

            // Count non-terminal deps for readiness counter.
            if !matches!(
                dep_task.status,
                TaskStatus::Integrated | TaskStatus::Cancelled
            ) {
                remaining += 1;
            }
        }

        self.remaining_deps.insert(id_str.clone(), remaining);
        self.tasks.insert(id_str, task);
        Ok(())
    }

    pub fn get(&self, id: &TaskId) -> Option<&Task> {
        self.tasks.get(id.as_ref())
    }

    pub fn get_mut(&mut self, id: &TaskId) -> Option<&mut Task> {
        self.tasks.get_mut(id.as_ref())
    }

    /// Transition a task's status via the FSM.
    pub fn transition(
        &mut self,
        id: &TaskId,
        trigger: TaskTrigger,
    ) -> Result<TaskStatus, GraphError> {
        let task = self
            .tasks
            .get_mut(id.as_ref())
            .ok_or_else(|| GraphError::TaskNotFound(id.to_string()))?;

        let new_status = TaskFsm::transition(task.status, &trigger)?;
        task.status = new_status;
        Ok(new_status)
    }

    /// Notify the graph that a task has reached a terminal state.
    /// Decrements remaining_deps for all dependents.
    /// Returns task ids whose counter reached zero (newly ready).
    pub fn mark_completed(&mut self, id: &TaskId) -> Vec<TaskId> {
        let dependents = self.forward.get(id.as_ref()).cloned().unwrap_or_default();

        let mut newly_ready = Vec::new();
        for dep_id in dependents {
            let key = dep_id.to_string();
            if let Some(count) = self.remaining_deps.get_mut(&key) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    newly_ready.push(dep_id);
                }
            }
        }

        newly_ready
    }

    /// Return dependents of a failed task (they are now blocked).
    /// Advisory — caller decides whether to cancel, retry, or escalate.
    pub fn mark_failed(&mut self, id: &TaskId) -> Vec<TaskId> {
        self.forward.get(id.as_ref()).cloned().unwrap_or_default()
    }

    /// Bind a task to a workspace. Sets workspace_ref and records in both maps.
    pub fn bind(&mut self, task_id: &TaskId, workspace_id: &WorkspaceId) -> Result<(), GraphError> {
        // Check task exists and is not already bound.
        let task = self
            .tasks
            .get(task_id.as_ref())
            .ok_or_else(|| GraphError::TaskNotFound(task_id.to_string()))?;
        if task.workspace_ref.is_some() {
            return Err(GraphError::TaskAlreadyBound(task_id.clone()));
        }

        // Check workspace not already bound.
        if self.workspace_task.contains_key(workspace_id.as_ref()) {
            return Err(GraphError::WorkspaceAlreadyBound(workspace_id.clone()));
        }

        // Set binding.
        let task = self.tasks.get_mut(task_id.as_ref()).unwrap();
        task.workspace_ref = Some(workspace_id.clone());
        task.workspace_history.push(workspace_id.clone());

        self.task_workspace
            .insert(task_id.to_string(), workspace_id.clone());
        self.workspace_task
            .insert(workspace_id.to_string(), task_id.clone());

        Ok(())
    }

    /// Unbind a task from its workspace. Clears binding maps.
    pub fn unbind(&mut self, task_id: &TaskId) {
        if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
            if let Some(ws) = task.workspace_ref.take() {
                self.workspace_task.remove(ws.as_ref());
            }
            self.task_workspace.remove(task_id.as_ref());
        }
    }

    /// Reverse lookup: workspace → task.
    pub fn task_for_workspace(&self, workspace_id: &WorkspaceId) -> Option<&TaskId> {
        self.workspace_task.get(workspace_id.as_ref())
    }

    /// Tasks in Pending status with remaining_deps == 0.
    pub fn dispatchable(&self) -> Vec<&TaskId> {
        self.tasks
            .values()
            .filter(|t| {
                t.status == TaskStatus::Pending
                    && self.remaining_deps.get(t.id.as_ref()).copied().unwrap_or(0) == 0
            })
            .map(|t| &t.id)
            .collect()
    }

    /// Tasks in Pending state whose dependencies are all satisfied.
    /// Delegates to dispatchable (counter-based).
    pub fn ready_tasks(&self) -> Vec<&TaskId> {
        self.dispatchable()
    }

    /// True if all tasks are in a terminal state (Integrated or Cancelled).
    pub fn is_complete(&self) -> bool {
        self.tasks
            .values()
            .all(|t| matches!(t.status, TaskStatus::Integrated | TaskStatus::Cancelled))
    }

    /// Tasks that depend on the given task (forward edges).
    pub fn dependents(&self, id: &TaskId) -> Vec<&TaskId> {
        self.forward
            .get(id.as_ref())
            .map(|ids| ids.iter().collect())
            .unwrap_or_default()
    }

    /// Root tasks (no parent_task).
    pub fn roots(&self) -> Vec<&TaskId> {
        self.tasks
            .values()
            .filter(|t| t.parent_task.is_none())
            .map(|t| &t.id)
            .collect()
    }

    /// Remaining dependency count for a task.
    pub fn remaining_deps(&self, id: &TaskId) -> Option<u32> {
        self.remaining_deps.get(id.as_ref()).copied()
    }
}

impl Default for TaskGraph {
    fn default() -> Self {
        Self::new()
    }
}
