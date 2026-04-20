//! `HighwayService` gRPC request handling — dispatched by the runtime event loop.
//!
//! Extracted from `init.rs` per `tech-debt-2026-04-18.md` §3.2 B.1 (closeout-plan P4). Lives
//! in an `impl Runtime` block that the Rust compiler merges with the block in `init.rs`.

use sha2::{Digest, Sha256};
use wacp_types::*;

use crate::conversions::{task_status_to_proto, workspace_state_to_proto};
use crate::init::Runtime;

impl Runtime {
    /// Handle a highway gRPC request.
    pub(crate) async fn handle_highway_request(&mut self, req: wacp_transport::HighwayRequest) {
        use wacp_transport::HighwayRequest;

        match req {
            HighwayRequest::Authenticate { request, reply } => {
                let token = request.auth_token.trim();
                if token.is_empty() {
                    let _ = reply.send(Err(tonic::Status::unauthenticated(
                        "auth_token is required",
                    )));
                    return;
                }
                if token.len() < 8 {
                    let _ = reply.send(Err(tonic::Status::unauthenticated(
                        "auth_token too short (minimum 8 characters)",
                    )));
                    return;
                }

                // Derive a stable user_id from the token via truncated SHA-256.
                let mut hasher = Sha256::new();
                hasher.update(token.as_bytes());
                let hash = hasher.finalize();
                let user_id = format!(
                    "user-{}",
                    hash[..8]
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                );

                let response = wacp_transport::wacp_v1::AuthenticateResponse {
                    user_id,
                    capabilities: vec!["observe".into(), "inject".into(), "gate".into()],
                };
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::GetTaskGraph { reply } => {
                let mut tasks = Vec::new();
                let roots = self.coordinator.task_graph.roots();
                let mut visited = std::collections::HashSet::new();
                let mut queue = std::collections::VecDeque::from_iter(roots.into_iter().cloned());

                while let Some(tid) = queue.pop_front() {
                    if !visited.insert(tid.to_string()) {
                        continue;
                    }
                    if let Some(task) = self.coordinator.task_graph.get(&tid) {
                        let status = task_status_to_proto(task.status) as i32;
                        tasks.push(wacp_transport::wacp_v1::Task {
                            id: task.id.to_string(),
                            name: task.name.clone(),
                            description: task.description.clone(),
                            depends_on: task.depends_on.iter().map(|d| d.to_string()).collect(),
                            parent_task: task
                                .parent_task
                                .as_ref()
                                .map(|p| p.to_string())
                                .unwrap_or_default(),
                            status,
                            workspace_ref: task
                                .workspace_ref
                                .as_ref()
                                .map(|w| w.to_string())
                                .unwrap_or_default(),
                            workspace_history: task
                                .workspace_history
                                .iter()
                                .map(|w| w.to_string())
                                .collect(),
                            checkpoint_ref: task
                                .checkpoint_ref
                                .as_ref()
                                .map(|c| c.to_string())
                                .unwrap_or_default(),
                        });
                        for dep in self.coordinator.task_graph.dependents(&tid) {
                            queue.push_back(dep.clone());
                        }
                    }
                }

                let response = wacp_transport::wacp_v1::TaskGraphView { tasks };
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::InjectEnvelope { request, reply } => {
                let to_ws = WorkspaceId::from(request.to_workspace.as_str());

                if self.coordinator.tree.get(&to_ws).is_none() {
                    let _ = reply.send(Err(tonic::Status::not_found(format!(
                        "target workspace '{}' not found",
                        request.to_workspace
                    ))));
                    return;
                }

                let env_id = self.next_envelope_id;
                self.next_envelope_id += 1;
                let envelope_id = EnvelopeId::from(format!("env-{env_id}"));

                let priority = match request.priority {
                    2 => EnvelopePriority::Urgent,
                    3 => EnvelopePriority::Blocking,
                    _ => EnvelopePriority::Normal,
                };

                let envelope = Envelope {
                    id: envelope_id.clone(),
                    from_workspace: WorkspaceId::from("highway"),
                    to_workspace: to_ws.clone(),
                    envelope_type: request.r#type,
                    payload: request.payload,
                    in_reply_to: None,
                    timestamp: 0,
                    priority,
                    origin: EnvelopeOrigin::Human,
                    state: EnvelopeState::Created,
                };

                self.notify_envelope_subs(to_ws.as_ref(), &envelope);

                if self.coordinator.route_envelope(&to_ws, envelope).await {
                    let response = wacp_transport::wacp_v1::InjectEnvelopeResponse {
                        envelope_id: envelope_id.to_string(),
                        timestamp: None,
                        client_request_id: request.client_request_id,
                    };
                    let _ = reply.send(Ok(response));
                } else {
                    let _ = reply.send(Err(tonic::Status::unavailable(format!(
                        "workspace '{}' is not running",
                        to_ws
                    ))));
                }
            }
            HighwayRequest::RespondToGate { request, reply } => {
                use wacp_workspace::actor::CoordinatorCommand;

                let gate_id = GateId::from(request.gate_id.as_str());
                let decision = match request.decision {
                    1 => GateDecision::Approve,
                    2 => GateDecision::Reject,
                    3 => GateDecision::Modify,
                    _ => GateDecision::Approve,
                };

                // WA3.5: checkpoint gates first. If the gate_id matches a
                // pending checkpoint gate, detach it and route the decision
                // to the bound workspace actor via CheckpointApproved /
                // CheckpointRejected commands. The actor's FSM drives
                // Blocked→Active on approve and Blocked→Failed on reject.
                if let Some(cp_gate) = self.gate_controller.resolve_checkpoint(&gate_id) {
                    let cmd = match decision {
                        GateDecision::Approve | GateDecision::Modify => {
                            CoordinatorCommand::CheckpointApproved {
                                checkpoint_id: cp_gate.checkpoint_id.to_string(),
                            }
                        }
                        GateDecision::Reject => CoordinatorCommand::CheckpointRejected {
                            checkpoint_id: cp_gate.checkpoint_id.to_string(),
                        },
                    };
                    let applied =
                        if let Some(handle) = self.coordinator.handle(&cp_gate.workspace_id) {
                            handle.coordinator_tx.send(cmd).await.is_ok()
                        } else {
                            false
                        };
                    let response = wacp_transport::wacp_v1::GateResponseAck {
                        gate_id: request.gate_id,
                        applied,
                        client_request_id: request.client_request_id,
                    };
                    let _ = reply.send(Ok(response));
                    return;
                }

                let applied = self.gate_controller.resolve(&gate_id, decision).is_some();

                let response = wacp_transport::wacp_v1::GateResponseAck {
                    gate_id: request.gate_id,
                    applied,
                    client_request_id: request.client_request_id,
                };
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::RespondToEscalation { request, reply } => {
                let ws_id_str = self.escalation_index.remove(&request.escalation_id);

                let applied = if let Some(ws_id_str) = ws_id_str {
                    let ws_id = WorkspaceId::from(ws_id_str.as_str());
                    match request.action {
                        Some(wacp_transport::wacp_v1::escalation_response::Action::Feedback(
                            env_proto,
                        )) => {
                            let env_id = self.next_envelope_id;
                            self.next_envelope_id += 1;
                            let envelope = Envelope {
                                id: EnvelopeId::from(format!("env-{env_id}")),
                                from_workspace: WorkspaceId::from("highway"),
                                to_workspace: ws_id.clone(),
                                envelope_type: env_proto.r#type,
                                payload: env_proto.payload,
                                in_reply_to: None,
                                timestamp: 0,
                                priority: EnvelopePriority::Normal,
                                origin: EnvelopeOrigin::Human,
                                state: EnvelopeState::Created,
                            };
                            self.notify_envelope_subs(ws_id.as_ref(), &envelope);
                            self.coordinator.route_envelope(&ws_id, envelope).await
                        }
                        Some(wacp_transport::wacp_v1::escalation_response::Action::Abort(true)) => {
                            self.coordinator.abort_workspace(&ws_id).await
                        }
                        Some(
                            wacp_transport::wacp_v1::escalation_response::Action::DelegateToCoordinator(
                                true,
                            ),
                        ) => true,
                        _ => false,
                    }
                } else {
                    false
                };

                let response = wacp_transport::wacp_v1::EscalationResponseAck {
                    escalation_id: request.escalation_id,
                    applied,
                    client_request_id: request.client_request_id,
                };
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::QueryTrail { request, reply } => {
                let response = self.query_trail_index(
                    if request.workspace_id.is_empty() {
                        None
                    } else {
                        Some(&request.workspace_id)
                    },
                    if request.event_type.is_empty() {
                        None
                    } else {
                        Some(&request.event_type)
                    },
                    if request.actor.is_empty() {
                        None
                    } else {
                        Some(&request.actor)
                    },
                    request.limit,
                    &request.client_request_id,
                );
                let _ = reply.send(Ok(response));
            }
            HighwayRequest::GetWorkspace { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                if let Some(node) = self.coordinator.tree.get(&ws_id) {
                    let response = wacp_transport::wacp_v1::WorkspaceView {
                        id: node.id.to_string(),
                        state: workspace_state_to_proto(node.status) as i32,
                        role: node.owner.to_string(),
                        parent: node
                            .parent
                            .as_ref()
                            .map(|p| p.to_string())
                            .unwrap_or_default(),
                        owner: node.owner.to_string(),
                        originator: match &node.originator {
                            Originator::System => "system".into(),
                            Originator::User(uid) => uid.to_string(),
                        },
                        task_id: node
                            .task_id
                            .as_ref()
                            .map(|t| t.to_string())
                            .unwrap_or_default(),
                        current_usage: Some(wacp_transport::wacp_v1::ResourceUsage {
                            tokens: 0,
                            wall_time_ms: 0,
                            storage_bytes: 0,
                            network_bytes: 0,
                            cost_micros: 0,
                        }),
                        budget: Some(wacp_transport::wacp_v1::ResourceBudget {
                            max_tokens: self.config.resources.default_budget.max_tokens,
                            max_wall_time_ms: self.config.resources.default_budget.max_wall_time_ms,
                            max_storage_bytes: self
                                .config
                                .resources
                                .default_budget
                                .max_storage_bytes,
                            max_network_bytes: self
                                .config
                                .resources
                                .default_budget
                                .max_network_bytes,
                            max_cost_micros: self.config.resources.default_budget.max_cost_micros,
                            warning_threshold: self.config.resources.warning_threshold,
                        }),
                        checkpoint_count: self
                            .checkpoint_index
                            .values()
                            .filter(|r| r.workspace_id == ws_id.as_ref())
                            .count() as u32,
                        created_at: self.workspace_timestamps.get(ws_id.as_ref()).map(
                            |(created, _)| wacp_transport::wacp_v1::Timestamp {
                                physical_us: *created,
                                logical: 0,
                            },
                        ),
                        last_activity: self.workspace_timestamps.get(ws_id.as_ref()).map(
                            |(_, last)| wacp_transport::wacp_v1::Timestamp {
                                physical_us: *last,
                                logical: 0,
                            },
                        ),
                    };
                    let _ = reply.send(Ok(response));
                } else {
                    let _ = reply.send(Err(tonic::Status::not_found("workspace not found")));
                }
            }
            HighwayRequest::GetCheckpoint { request, reply } => {
                let response = match self.checkpoint_index.get(&request.checkpoint_id) {
                    Some(record) => {
                        let hash_hex: String = record
                            .content_hash
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect();
                        match self.checkpoint_storage.read(&record.content_hash) {
                            Ok(Some(payload)) => Ok(wacp_transport::wacp_v1::CheckpointView {
                                metadata: Some(wacp_transport::wacp_v1::Checkpoint {
                                    id: request.checkpoint_id,
                                    workspace_id: record.workspace_id.clone(),
                                    r#type: record.checkpoint_type.clone(),
                                    payload: Vec::new(), // payload is in the top-level field
                                    content_hash: hash_hex,
                                    intent: record.intent.clone(),
                                    parent_checkpoint: String::new(),
                                    status: record.status,
                                    confidence: record.confidence,
                                    timestamp: None,
                                    resource_usage: record.resource_usage,
                                }),
                                payload,
                            }),
                            Ok(None) => Err(tonic::Status::not_found(
                                "checkpoint blob missing from store",
                            )),
                            Err(e) => Err(tonic::Status::internal(format!(
                                "checkpoint read error: {e}"
                            ))),
                        }
                    }
                    None => Err(tonic::Status::not_found(format!(
                        "checkpoint '{}' not found",
                        request.checkpoint_id
                    ))),
                };
                let _ = reply.send(response);
            }
            HighwayRequest::ListWorkspaces { parent_id, reply } => {
                let items: Vec<wacp_transport::WorkspaceSummaryItem> = self
                    .coordinator
                    .tree
                    .active_workspaces()
                    .iter()
                    .filter_map(|ws_id| {
                        let node = self.coordinator.tree.get(ws_id)?;
                        let node_parent = node.parent.as_ref().map(|p| p.to_string());
                        if let Some(ref filter) = parent_id
                            && node_parent.as_deref() != Some(filter)
                        {
                            return None;
                        }
                        Some(wacp_transport::WorkspaceSummaryItem {
                            id: ws_id.to_string(),
                            parent_id: node_parent.unwrap_or_default(),
                            state: workspace_state_to_proto(node.status) as i32,
                            owner: node.owner.to_string(),
                            task_id: node
                                .task_id
                                .as_ref()
                                .map(|t| t.to_string())
                                .unwrap_or_default(),
                        })
                    })
                    .collect();
                let _ = reply.send(Ok(items));
            }
            HighwayRequest::SubscribeTrail { tx } => {
                self.trail_subs.push(tx);
            }
            HighwayRequest::SubscribeGates { tx } => {
                self.gate_subs.push(tx);
            }
            HighwayRequest::SubscribeEscalations { tx } => {
                self.escalation_subs.push(tx);
            }
            HighwayRequest::SubscribeWorkspaceChanges { tx } => {
                self.ws_change_subs.push(tx);
            }
        }
    }
}
