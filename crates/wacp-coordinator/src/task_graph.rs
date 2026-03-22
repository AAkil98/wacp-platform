use std::collections::HashMap;

use wacp_fsm::{StateMachine, TaskFsm, TaskTrigger, TransitionError};
use wacp_types::{Task, TaskId, TaskStatus};

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
}

/// DAG of tasks with dependency tracking (PROTOCOL.md §4.6).
pub struct TaskGraph {
    tasks: HashMap<String, Task>,
}

impl TaskGraph {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    /// Add a task. All dependencies must already exist in the graph.
    pub fn add_task(&mut self, task: Task) -> Result<(), GraphError> {
        let id_str = task.id.to_string();
        if self.tasks.contains_key(&id_str) {
            return Err(GraphError::DuplicateTask(id_str));
        }

        for dep in &task.depends_on {
            if !self.tasks.contains_key(dep.as_ref()) {
                return Err(GraphError::DependencyNotFound(dep.to_string()));
            }
        }

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

    /// Tasks in Pending state whose dependencies are all Integrated.
    pub fn ready_tasks(&self) -> Vec<&TaskId> {
        self.tasks
            .values()
            .filter(|t| {
                t.status == TaskStatus::Pending
                    && t.depends_on.iter().all(|dep| {
                        self.tasks
                            .get(dep.as_ref())
                            .is_some_and(|d| d.status == TaskStatus::Integrated)
                    })
            })
            .map(|t| &t.id)
            .collect()
    }

    /// True if all tasks are in a terminal state (Integrated or Cancelled).
    pub fn is_complete(&self) -> bool {
        self.tasks.values().all(|t| {
            matches!(t.status, TaskStatus::Integrated | TaskStatus::Cancelled)
        })
    }

    /// Tasks that depend on the given task.
    pub fn dependents(&self, id: &TaskId) -> Vec<&TaskId> {
        self.tasks
            .values()
            .filter(|t| t.depends_on.contains(id))
            .map(|t| &t.id)
            .collect()
    }

    /// Root tasks (no parent_task).
    pub fn roots(&self) -> Vec<&TaskId> {
        self.tasks
            .values()
            .filter(|t| t.parent_task.is_none())
            .map(|t| &t.id)
            .collect()
    }
}

impl Default for TaskGraph {
    fn default() -> Self {
        Self::new()
    }
}
