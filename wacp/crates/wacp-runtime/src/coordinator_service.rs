//! `CoordinatorService` gRPC/REST request handling — dispatched by the runtime event loop.
//!
//! Extracted from `init.rs` per `tech-debt-2026-04-18.md` §3.2 B.1 (closeout-plan P4). Lives
//! in an `impl Runtime` block that the Rust compiler merges with the block in `init.rs`.

use wacp_coordinator::DispatchRequest;
use wacp_types::*;

use crate::conversions::task_status_to_proto;
use crate::init::Runtime;

impl Runtime {
    /// Handle a coordinator gRPC/REST request.
    pub(crate) async fn handle_coordinator_request(
        &mut self,
        req: wacp_transport::CoordinatorRequest,
    ) {
        use wacp_fsm::TaskTrigger;
        use wacp_transport::CoordinatorRequest;
        use wacp_workspace::actor::CoordinatorCommand;

        match req {
            CoordinatorRequest::SubmitGoal { request, reply } => {
                let gid = self.next_goal_id;
                self.next_goal_id += 1;
                let goal_id = format!("goal-{gid}");

                // Create a root-level task for this goal.
                let task_id = TaskId::from(format!("task-{goal_id}").as_str());
                let task = Task {
                    id: task_id.clone(),
                    name: request.description.clone(),
                    description: request.description.clone(),
                    depends_on: vec![],
                    parent_task: None,
                    status: TaskStatus::Draft,
                    workspace_ref: None,
                    workspace_history: vec![],
                    checkpoint_ref: None,
                };
                if let Err(e) = self.coordinator.task_graph.add_task(task) {
                    let _ = reply.send(Err(tonic::Status::internal(format!(
                        "task graph error: {e}"
                    ))));
                    return;
                }
                // Approve the task so it becomes Pending (dispatchable).
                let _ = self
                    .coordinator
                    .task_graph
                    .transition(&task_id, TaskTrigger::Approve);

                // Create a workspace for the goal and dispatch.
                let wsid = self.next_workspace_id;
                self.next_workspace_id += 1;
                let ws_id = WorkspaceId::from(format!("ws-{wsid}").as_str());

                let ws_config = wacp_workspace::state::WorkspaceConfig {
                    id: ws_id.clone(),
                    role: "worker".into(),
                    base_role: BaseRole::Worker,
                    parent: self.coordinator.tree.root().clone(),
                    owner: UserId::from("system"),
                    originator: Originator::System,
                    directive: Envelope {
                        id: EnvelopeId::from(format!("env-goal-{gid}").as_str()),
                        from_workspace: self.coordinator.tree.root().clone(),
                        to_workspace: ws_id.clone(),
                        envelope_type: "directive".into(),
                        payload: request.context,
                        in_reply_to: None,
                        timestamp: 0,
                        priority: EnvelopePriority::Normal,
                        origin: EnvelopeOrigin::Human,
                        state: EnvelopeState::Created,
                    },
                    context: Vec::new(),
                    visibility: std::collections::HashSet::new(),
                    authority: std::collections::HashSet::new(),
                    delegate: false,
                    budget: None,
                };
                // Cache the config so AgentService::Bind can project its
                // fields into the BindResponse (WA1).
                self.workspace_configs
                    .insert(ws_id.to_string(), ws_config.clone());
                self.coordinator.dispatch(DispatchRequest {
                    task_id: task_id.clone(),
                    config: ws_config,
                });
                self.record_workspace_created(ws_id.as_ref());

                // Bind task to workspace and advance to Assigned.
                let _ = self.coordinator.task_graph.bind(&task_id, &ws_id);
                let _ = self
                    .coordinator
                    .task_graph
                    .transition(&task_id, TaskTrigger::Assign);

                let response = wacp_transport::wacp_v1::SubmitGoalResponse {
                    goal_id,
                    root_workspace_id: ws_id.to_string(),
                };
                let _ = reply.send(Ok(response));
            }
            CoordinatorRequest::GetReadyTasks { reply, .. } => {
                let ready: Vec<wacp_transport::wacp_v1::TaskView> = self
                    .coordinator
                    .task_graph
                    .ready_tasks()
                    .into_iter()
                    .filter_map(|tid| {
                        let t = self.coordinator.task_graph.get(tid)?;
                        Some(wacp_transport::wacp_v1::TaskView {
                            task_id: t.id.to_string(),
                            name: t.name.clone(),
                            description: t.description.clone(),
                            status: task_status_to_proto(t.status) as i32,
                            assigned_workspace: t
                                .workspace_ref
                                .as_ref()
                                .map(|w| w.to_string())
                                .unwrap_or_default(),
                            depends_on: t.depends_on.iter().map(|d| d.to_string()).collect(),
                        })
                    })
                    .collect();
                let response = wacp_transport::wacp_v1::GetReadyTasksResponse { tasks: ready };
                let _ = reply.send(Ok(response));
            }
            CoordinatorRequest::Dispatch { request, reply } => {
                let task_id = TaskId::from(request.task_id.as_str());

                // Validate the task exists in the graph.
                if self.coordinator.task_graph.get(&task_id).is_none() {
                    let _ = reply.send(Err(tonic::Status::not_found(format!(
                        "task '{}' not found",
                        request.task_id
                    ))));
                    return;
                }

                let wsid = self.next_workspace_id;
                self.next_workspace_id += 1;
                let ws_id = WorkspaceId::from(format!("ws-{wsid}").as_str());

                let ws_config = wacp_workspace::state::WorkspaceConfig {
                    id: ws_id.clone(),
                    role: request.role.clone(),
                    base_role: BaseRole::Worker,
                    parent: self.coordinator.tree.root().clone(),
                    owner: UserId::from("system"),
                    originator: Originator::System,
                    directive: Envelope {
                        id: EnvelopeId::from(format!("env-dispatch-{wsid}").as_str()),
                        from_workspace: self.coordinator.tree.root().clone(),
                        to_workspace: ws_id.clone(),
                        envelope_type: "directive".into(),
                        payload: request.directive_payload,
                        in_reply_to: None,
                        timestamp: 0,
                        priority: EnvelopePriority::Normal,
                        origin: EnvelopeOrigin::Human,
                        state: EnvelopeState::Created,
                    },
                    context: Vec::new(),
                    visibility: std::collections::HashSet::new(),
                    authority: std::collections::HashSet::new(),
                    delegate: false,
                    budget: None,
                };
                // Cache the config so AgentService::Bind can project its
                // fields into the BindResponse (WA1).
                self.workspace_configs
                    .insert(ws_id.to_string(), ws_config.clone());
                self.coordinator.dispatch(DispatchRequest {
                    task_id: task_id.clone(),
                    config: ws_config,
                });
                self.record_workspace_created(ws_id.as_ref());

                // Bind task to workspace and advance to Assigned.
                let _ = self.coordinator.task_graph.bind(&task_id, &ws_id);
                let _ = self
                    .coordinator
                    .task_graph
                    .transition(&task_id, TaskTrigger::Assign);

                let response = wacp_transport::wacp_v1::DispatchResponse {
                    workspace_id: ws_id.to_string(),
                    task_id: request.task_id,
                };
                let _ = reply.send(Ok(response));
            }
            CoordinatorRequest::AbortWorkspace { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                self.coordinator.abort_workspace(&ws_id).await;
                let _ = reply.send(Ok(
                    wacp_transport::wacp_v1::AbortWorkspaceResponse::default(),
                ));
            }
            CoordinatorRequest::SuspendWorkspace { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                let result = match self.coordinator.handle(&ws_id) {
                    Some(handle) => {
                        if handle
                            .coordinator_tx
                            .send(CoordinatorCommand::Suspend)
                            .await
                            .is_ok()
                        {
                            self.notify_command_subs(&request.workspace_id, "suspend", Vec::new());
                            Ok(wacp_transport::wacp_v1::SuspendWorkspaceResponse::default())
                        } else {
                            Err(tonic::Status::unavailable("workspace actor not running"))
                        }
                    }
                    None => Err(tonic::Status::not_found(format!(
                        "workspace '{}' not found",
                        request.workspace_id
                    ))),
                };
                let _ = reply.send(result);
            }
            CoordinatorRequest::ResumeWorkspace { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                let result = match self.coordinator.handle(&ws_id) {
                    Some(handle) => {
                        if handle
                            .coordinator_tx
                            .send(CoordinatorCommand::Resume)
                            .await
                            .is_ok()
                        {
                            self.notify_command_subs(&request.workspace_id, "resume", Vec::new());
                            Ok(wacp_transport::wacp_v1::ResumeWorkspaceResponse::default())
                        } else {
                            Err(tonic::Status::unavailable("workspace actor not running"))
                        }
                    }
                    None => Err(tonic::Status::not_found(format!(
                        "workspace '{}' not found",
                        request.workspace_id
                    ))),
                };
                let _ = reply.send(result);
            }
            CoordinatorRequest::Decompose { request, reply } => {
                let mut task_ids = Vec::new();
                for task_def in &request.tasks {
                    let gid = self.next_goal_id;
                    self.next_goal_id += 1;
                    let tid = TaskId::from(format!("task-decompose-{gid}").as_str());
                    let deps: Vec<TaskId> = task_def
                        .depends_on
                        .iter()
                        .map(|d| TaskId::from(d.as_str()))
                        .collect();
                    let task = Task {
                        id: tid.clone(),
                        name: task_def.name.clone(),
                        description: task_def.description.clone(),
                        depends_on: deps,
                        parent_task: None,
                        status: TaskStatus::Draft,
                        workspace_ref: None,
                        workspace_history: vec![],
                        checkpoint_ref: None,
                    };
                    match self.coordinator.task_graph.add_task(task) {
                        Ok(()) => {
                            // Approve so it becomes dispatchable.
                            let _ = self
                                .coordinator
                                .task_graph
                                .transition(&tid, TaskTrigger::Approve);
                            task_ids.push(tid.to_string());
                        }
                        Err(e) => {
                            tracing::warn!(task_id = %tid, error = %e, "decompose: task rejected");
                        }
                    }
                }
                let response = wacp_transport::wacp_v1::DecomposeResponse { task_ids };
                let _ = reply.send(Ok(response));
            }
            CoordinatorRequest::CancelTask { request, reply } => {
                let task_id = TaskId::from(request.task_id.as_str());
                match self
                    .coordinator
                    .task_graph
                    .transition(&task_id, TaskTrigger::Cancel)
                {
                    Ok(_) => {
                        // If the task is bound to a workspace, abort the workspace.
                        if let Some(task) = self.coordinator.task_graph.get(&task_id)
                            && let Some(ws_id) = &task.workspace_ref
                        {
                            self.coordinator.abort_workspace(ws_id).await;
                        }
                        let _ =
                            reply.send(Ok(wacp_transport::wacp_v1::CancelTaskResponse::default()));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(tonic::Status::failed_precondition(format!(
                            "cannot cancel task: {e}"
                        ))));
                    }
                }
            }
            CoordinatorRequest::SendDirective { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                let env_id = self.next_envelope_id;
                self.next_envelope_id += 1;
                let envelope = Envelope {
                    id: EnvelopeId::from(format!("env-{env_id}").as_str()),
                    from_workspace: self.coordinator.tree.root().clone(),
                    to_workspace: ws_id.clone(),
                    envelope_type: "directive".into(),
                    payload: request.payload,
                    in_reply_to: None,
                    timestamp: 0,
                    priority: EnvelopePriority::Normal,
                    origin: EnvelopeOrigin::Human,
                    state: EnvelopeState::Created,
                };
                let envelope_id = envelope.id.to_string();
                self.notify_envelope_subs(ws_id.as_ref(), &envelope);
                if self.coordinator.route_envelope(&ws_id, envelope).await {
                    let response = wacp_transport::wacp_v1::SendDirectiveResponse { envelope_id };
                    let _ = reply.send(Ok(response));
                } else {
                    let _ = reply.send(Err(tonic::Status::unavailable(format!(
                        "workspace '{}' is not running",
                        request.workspace_id
                    ))));
                }
            }
            CoordinatorRequest::SendFeedback { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                let env_id = self.next_envelope_id;
                self.next_envelope_id += 1;
                let envelope = Envelope {
                    id: EnvelopeId::from(format!("env-{env_id}").as_str()),
                    from_workspace: self.coordinator.tree.root().clone(),
                    to_workspace: ws_id.clone(),
                    envelope_type: "feedback".into(),
                    payload: request.payload,
                    in_reply_to: if request.in_reply_to.is_empty() {
                        None
                    } else {
                        Some(EnvelopeId::from(request.in_reply_to.as_str()))
                    },
                    timestamp: 0,
                    priority: EnvelopePriority::Normal,
                    origin: EnvelopeOrigin::Human,
                    state: EnvelopeState::Created,
                };
                let envelope_id = envelope.id.to_string();
                self.notify_envelope_subs(ws_id.as_ref(), &envelope);
                if self.coordinator.route_envelope(&ws_id, envelope).await {
                    let response = wacp_transport::wacp_v1::SendFeedbackResponse { envelope_id };
                    let _ = reply.send(Ok(response));
                } else {
                    let _ = reply.send(Err(tonic::Status::unavailable(format!(
                        "workspace '{}' is not running",
                        request.workspace_id
                    ))));
                }
            }
            CoordinatorRequest::TriggerIntegration { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                match self.coordinator.handle(&ws_id) {
                    Some(handle) => {
                        if handle
                            .coordinator_tx
                            .send(CoordinatorCommand::IntegrationSucceeded)
                            .await
                            .is_ok()
                        {
                            let _ = reply.send(Ok(
                                wacp_transport::wacp_v1::TriggerIntegrationResponse {
                                    result: "accepted".into(),
                                    detail: format!(
                                        "integration triggered for {}",
                                        request.workspace_id
                                    ),
                                },
                            ));
                        } else {
                            let _ = reply.send(Err(tonic::Status::unavailable(
                                "workspace actor not running",
                            )));
                        }
                    }
                    None => {
                        let _ = reply.send(Err(tonic::Status::not_found(format!(
                            "workspace '{}' not found",
                            request.workspace_id
                        ))));
                    }
                }
            }
            CoordinatorRequest::GetAllocatable { reply, .. } => {
                let budget = &self.config.resources.default_budget;
                let _ = reply.send(Ok(wacp_transport::wacp_v1::GetAllocatableResponse {
                    remaining: Some(wacp_transport::wacp_v1::ResourceBudget {
                        max_tokens: budget.max_tokens,
                        max_wall_time_ms: budget.max_wall_time_ms,
                        max_storage_bytes: budget.max_storage_bytes,
                        max_network_bytes: budget.max_network_bytes,
                        max_cost_micros: budget.max_cost_micros,
                        warning_threshold: self.config.resources.warning_threshold,
                    }),
                }));
            }
            CoordinatorRequest::StreamSignals { tx, .. } => {
                self.signal_subs.push(tx);
            }
        }
    }
}
