use std::time::Duration;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use wacp_transport::wacp_v1;
use wacp_transport::wacp_v1::coordinator_service_client::CoordinatorServiceClient;
use wacp_types::ResourceBudget;

use crate::error::Error;
use crate::types::*;

/// Client-facing coordinator API.
///
/// Drives the runtime's coordinator via gRPC. Provides typed methods
/// for task graph management, workspace lifecycle, signals, and integration.
pub struct CoordinatorContext {
    client: CoordinatorServiceClient<tonic::transport::Channel>,
    cancellation: CancellationToken,
}

impl CoordinatorContext {
    /// Connect to the runtime's CoordinatorService.
    pub async fn connect(url: &str) -> Result<Self, Error> {
        let channel = tonic::transport::Channel::from_shared(url.to_string())
            .map_err(|e| Error::ConnectionFailed(e.to_string()))?
            .connect()
            .await?;

        Ok(Self {
            client: CoordinatorServiceClient::new(channel),
            cancellation: CancellationToken::new(),
        })
    }

    /// Create with an explicit cancellation token.
    pub fn with_cancellation(
        client: CoordinatorServiceClient<tonic::transport::Channel>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            client,
            cancellation,
        }
    }

    // ── Task graph ──────────────────────────────────────────

    /// Submit a top-level goal. Returns the goal ID and root workspace ID.
    pub async fn submit_goal(
        &mut self,
        description: &str,
        context: &[u8],
    ) -> Result<(String, String), Error> {
        let response = self
            .client
            .submit_goal(wacp_v1::SubmitGoalRequest {
                description: description.into(),
                context: context.to_vec(),
                client_request_id: String::new(),
            })
            .await?
            .into_inner();
        Ok((response.goal_id, response.root_workspace_id))
    }

    /// Create a task graph from definitions. Returns assigned task IDs.
    pub async fn decompose(&mut self, tasks: Vec<TaskDefinition>) -> Result<Vec<String>, Error> {
        let proto_tasks: Vec<wacp_v1::TaskDefinition> = tasks
            .into_iter()
            .map(|t| wacp_v1::TaskDefinition {
                name: t.name,
                description: t.description,
                depends_on: t.depends_on,
                role: t.role,
                directive_payload: t.directive_payload,
                tools: t.tools,
            })
            .collect();

        let response = self
            .client
            .decompose(wacp_v1::DecomposeRequest {
                tasks: proto_tasks,
                client_request_id: String::new(),
            })
            .await?
            .into_inner();
        Ok(response.task_ids)
    }

    /// Get tasks ready for dispatch (all dependencies satisfied).
    pub async fn ready_tasks(&mut self) -> Result<Vec<TaskView>, Error> {
        let response = self
            .client
            .get_ready_tasks(wacp_v1::GetReadyTasksRequest {
                client_request_id: String::new(),
            })
            .await?
            .into_inner();

        Ok(response
            .tasks
            .into_iter()
            .map(|t| TaskView {
                task_id: t.task_id,
                name: t.name,
                description: t.description,
                status: format!(
                    "{:?}",
                    wacp_v1::TaskStatus::try_from(t.status)
                        .unwrap_or(wacp_v1::TaskStatus::Unspecified)
                ),
                assigned_workspace: if t.assigned_workspace.is_empty() {
                    None
                } else {
                    Some(t.assigned_workspace)
                },
                depends_on: t.depends_on,
            })
            .collect())
    }

    /// Cancel a task and cascade to dependents.
    pub async fn cancel_task(&mut self, task_id: &str, reason: &str) -> Result<(), Error> {
        self.client
            .cancel_task(wacp_v1::CancelTaskRequest {
                task_id: task_id.into(),
                reason: reason.into(),
                client_request_id: String::new(),
            })
            .await?;
        Ok(())
    }

    // ── Workspace lifecycle ─────────────────────────────────

    /// Create a workspace for a task.
    pub async fn dispatch(
        &mut self,
        task_id: &str,
        config: WorkspaceConfig,
    ) -> Result<WorkspaceHandle, Error> {
        let response = self
            .client
            .dispatch(wacp_v1::DispatchRequest {
                task_id: task_id.into(),
                role: config.role,
                directive_payload: config.directive_payload,
                tools: config.tools,
                budget: config.budget.map(|b| wacp_v1::ResourceBudget {
                    max_tokens: b.max_tokens.unwrap_or(0),
                    max_wall_time_ms: b.max_wall_time_ms.unwrap_or(0),
                    max_storage_bytes: b.max_storage_bytes.unwrap_or(0),
                    max_network_bytes: b.max_network_bytes.unwrap_or(0),
                    max_cost_micros: b.max_cost_micros.unwrap_or(0),
                    warning_threshold: b.warning_threshold,
                }),
                client_request_id: String::new(),
            })
            .await?
            .into_inner();

        Ok(WorkspaceHandle {
            workspace_id: response.workspace_id,
            task_id: response.task_id,
        })
    }

    /// Terminate a workspace.
    pub async fn abort(&mut self, workspace_id: &str, reason: &str) -> Result<(), Error> {
        self.client
            .abort_workspace(wacp_v1::AbortWorkspaceRequest {
                workspace_id: workspace_id.into(),
                reason: reason.into(),
                client_request_id: String::new(),
            })
            .await?;
        Ok(())
    }

    /// Pause a workspace.
    pub async fn suspend(&mut self, workspace_id: &str) -> Result<(), Error> {
        self.client
            .suspend_workspace(wacp_v1::SuspendWorkspaceRequest {
                workspace_id: workspace_id.into(),
                client_request_id: String::new(),
            })
            .await?;
        Ok(())
    }

    /// Resume a paused workspace.
    pub async fn resume(&mut self, workspace_id: &str) -> Result<(), Error> {
        self.client
            .resume_workspace(wacp_v1::ResumeWorkspaceRequest {
                workspace_id: workspace_id.into(),
                client_request_id: String::new(),
            })
            .await?;
        Ok(())
    }

    // ── Communication ───────────────────────────────────────

    /// Send a directive envelope to a workspace. Returns envelope ID.
    pub async fn send_directive(
        &mut self,
        workspace_id: &str,
        payload: &[u8],
    ) -> Result<String, Error> {
        let response = self
            .client
            .send_directive(wacp_v1::SendDirectiveRequest {
                workspace_id: workspace_id.into(),
                payload: payload.to_vec(),
                client_request_id: String::new(),
            })
            .await?
            .into_inner();
        Ok(response.envelope_id)
    }

    /// Send a feedback envelope to a workspace. Returns envelope ID.
    pub async fn feedback(
        &mut self,
        workspace_id: &str,
        payload: &[u8],
        in_reply_to: Option<&str>,
    ) -> Result<String, Error> {
        let response = self
            .client
            .send_feedback(wacp_v1::SendFeedbackRequest {
                workspace_id: workspace_id.into(),
                payload: payload.to_vec(),
                in_reply_to: in_reply_to.unwrap_or_default().into(),
                client_request_id: String::new(),
            })
            .await?
            .into_inner();
        Ok(response.envelope_id)
    }

    // ── Integration ─────────────────────────────────────────

    /// Trigger the integration pipeline for a completed workspace.
    pub async fn integrate(&mut self, workspace_id: &str) -> Result<IntegrationResult, Error> {
        let response = self
            .client
            .trigger_integration(wacp_v1::TriggerIntegrationRequest {
                workspace_id: workspace_id.into(),
                client_request_id: String::new(),
            })
            .await?
            .into_inner();

        Ok(IntegrationResult {
            result: response.result,
            detail: response.detail,
        })
    }

    // ── Observation ─────────────────────────────────────────

    /// Get remaining allocatable budget.
    pub async fn allocatable(&mut self) -> Result<ResourceBudget, Error> {
        let response = self
            .client
            .get_allocatable(wacp_v1::GetAllocatableRequest {
                client_request_id: String::new(),
            })
            .await?
            .into_inner();

        let budget = response.remaining.unwrap_or_default();
        Ok(ResourceBudget {
            max_tokens: Some(budget.max_tokens),
            max_wall_time_ms: Some(budget.max_wall_time_ms),
            max_storage_bytes: Some(budget.max_storage_bytes),
            max_network_bytes: Some(budget.max_network_bytes),
            max_cost_micros: Some(budget.max_cost_micros),
            warning_threshold: budget.warning_threshold,
        })
    }

    /// Stream signals from child workspaces.
    #[allow(clippy::result_large_err)]
    pub async fn signals(
        &mut self,
        filter: SignalFilter,
    ) -> Result<impl futures::Stream<Item = Result<SignalEvent, Error>>, Error> {
        let stream = self
            .client
            .stream_signals(wacp_v1::StreamSignalsRequest {
                workspace_filter: filter.workspace_id.unwrap_or_default(),
                signal_type_filter: filter.signal_type.unwrap_or_default(),
            })
            .await?
            .into_inner();

        Ok(stream.map(|item| {
            item.map(|ev| SignalEvent {
                workspace_id: ev.workspace_id,
                signal_type: format!(
                    "{:?}",
                    wacp_v1::SignalType::try_from(ev.signal_type)
                        .unwrap_or(wacp_v1::SignalType::Unspecified)
                ),
                reason: ev.reason,
                context: ev.context,
                timestamp: ev.timestamp,
            })
            .map_err(Error::from)
        }))
    }

    /// Wait for a specific signal, with timeout.
    pub async fn wait_for_signal(
        &mut self,
        filter: SignalFilter,
        timeout_ms: u64,
    ) -> Result<SignalEvent, Error> {
        let mut stream = self.signals(filter).await?;
        tokio::time::timeout(Duration::from_millis(timeout_ms), async {
            stream.next().await.ok_or(Error::StreamEnded)?
        })
        .await
        .map_err(|_| Error::SignalTimeout(timeout_ms))?
    }

    // ── Cancellation ────────────────────────────────────────

    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Context requires a real gRPC connection, so we test the type construction
    // and proto mapping logic directly.

    #[test]
    fn task_definition_to_proto() {
        let task = TaskDefinition {
            name: "test task".into(),
            description: "do testing".into(),
            depends_on: vec!["task-1".into()],
            role: "worker".into(),
            directive_payload: b"test this".to_vec(),
            tools: vec!["test_runner".into()],
        };
        let proto = wacp_v1::TaskDefinition {
            name: task.name.clone(),
            description: task.description.clone(),
            depends_on: task.depends_on.clone(),
            role: task.role.clone(),
            directive_payload: task.directive_payload.clone(),
            tools: task.tools.clone(),
        };
        assert_eq!(proto.name, "test task");
        assert_eq!(proto.depends_on.len(), 1);
        assert_eq!(proto.tools.len(), 1);
    }

    #[test]
    fn workspace_config_with_budget() {
        let config = WorkspaceConfig {
            role: "swe:implementer".into(),
            directive_payload: b"implement auth".to_vec(),
            tools: vec!["file_read".into(), "file_write".into()],
            budget: Some(ResourceBudget {
                max_tokens: Some(100_000),
                max_wall_time_ms: Some(300_000),
                max_storage_bytes: Some(10_000_000),
                max_network_bytes: None,
                max_cost_micros: Some(1_000_000),
                warning_threshold: 0.8,
            }),
        };
        assert!(config.budget.is_some());
        assert_eq!(config.budget.unwrap().max_tokens, Some(100_000));
    }

    #[test]
    fn signal_event_fields() {
        let event = SignalEvent {
            workspace_id: "ws-child-1".into(),
            signal_type: "Complete".into(),
            reason: String::new(),
            context: vec![],
            timestamp: 1234567890,
        };
        assert_eq!(event.workspace_id, "ws-child-1");
        assert_eq!(event.signal_type, "Complete");
    }

    #[test]
    fn integration_result_accepted() {
        let result = IntegrationResult {
            result: "accepted".into(),
            detail: "all checks passed".into(),
        };
        assert_eq!(result.result, "accepted");
    }
}
