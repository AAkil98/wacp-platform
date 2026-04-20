//! `AgentService` gRPC request handling — dispatched by the runtime event loop.
//!
//! Extracted from `init.rs` per `tech-debt-2026-04-18.md` §3.2 B.1 (closeout-plan P4). The
//! method lives in an `impl Runtime` block that the Rust compiler merges with the block in
//! `init.rs`; all state access goes through `self` against the fields promoted to `pub(crate)`.

use sha2::{Digest, Sha256};
use wacp_types::*;

use crate::conversions::{
    budget_to_proto, envelope_to_proto, gate_type_to_proto, proto_to_checkpoint_status,
    proto_to_confidence, proto_to_signal_type, workspace_state_to_proto,
};
use crate::init::{CheckpointRecord, Runtime};

impl Runtime {
    /// Handle an agent gRPC request by forwarding to the coordinator.
    pub(crate) async fn handle_agent_request(&mut self, req: wacp_transport::AgentRequest) {
        use wacp_transport::AgentRequest;

        match req {
            AgentRequest::Bind { request, reply } => {
                let ws_id = WorkspaceId::from(request.workspace_id.as_str());
                if let Some(node) = self.coordinator.tree.get(&ws_id) {
                    // WA1: project the cached WorkspaceConfig's bind-relevant
                    // fields into the response. If the config isn't cached
                    // (e.g., a workspace created via a future code path that
                    // skipped the cache), fall back to empty — matches the
                    // pre-WA1 behaviour rather than erroring.
                    let cfg = self.workspace_configs.get(&ws_id.to_string());
                    let response = wacp_transport::wacp_v1::BindResponse {
                        workspace_id: ws_id.to_string(),
                        state: workspace_state_to_proto(node.status) as i32,
                        role: cfg.map(|c| c.role.clone()).unwrap_or_default(),
                        directive: cfg.map(|c| envelope_to_proto(&c.directive)),
                        context: cfg.map(|c| c.context.clone()).unwrap_or_default(),
                        visibility: cfg
                            .map(|c| c.visibility.iter().cloned().collect())
                            .unwrap_or_default(),
                        authority: cfg
                            .map(|c| c.authority.iter().cloned().collect())
                            .unwrap_or_default(),
                        budget: cfg.and_then(|c| c.budget.as_ref().map(budget_to_proto)),
                    };
                    let _ = reply.send(Ok(response));
                } else {
                    let _ = reply.send(Err(tonic::Status::not_found("workspace not found")));
                }
            }
            AgentRequest::EmitSignal {
                workspace_id,
                request,
                reply,
            } => {
                // WA2: forward the signal to the workspace actor so
                // `handle_agent_msg::EmitSignal` can map it to a FSM trigger
                // and transition the workspace state. The actor emits a
                // `WorkspaceEvent::Signal` for the trail + a
                // `WorkspaceEvent::StateChanged` if the trigger advances
                // the FSM; both are fanned out by the event loop's
                // `fan_out_event` path. See `wacp/impl/wa2-emit-signal-fsm.md`.
                let ws_id = WorkspaceId::from(workspace_id.as_str());
                let signal_type =
                    match wacp_transport::wacp_v1::SignalType::try_from(request.r#type) {
                        Ok(t) => proto_to_signal_type(t),
                        Err(_) => {
                            let _ = reply.send(Err(tonic::Status::invalid_argument(format!(
                                "unknown SignalType discriminant: {}",
                                request.r#type
                            ))));
                            return;
                        }
                    };

                let handle = match self.coordinator.handle(&ws_id) {
                    Some(h) => h,
                    None => {
                        let _ = reply.send(Err(tonic::Status::not_found(format!(
                            "workspace '{workspace_id}' not running"
                        ))));
                        return;
                    }
                };

                let reason = if request.reason.is_empty() {
                    None
                } else {
                    Some(request.reason.clone())
                };
                let context = if request.context.is_empty() {
                    None
                } else {
                    Some(request.context.clone())
                };
                let send_result = handle
                    .agent_tx
                    .send(wacp_workspace::AgentMessage::EmitSignal {
                        signal_type,
                        reason,
                        context,
                    })
                    .await;
                if send_result.is_err() {
                    let _ = reply.send(Err(tonic::Status::unavailable(format!(
                        "workspace '{workspace_id}' actor channel closed"
                    ))));
                    return;
                }

                let response = wacp_transport::wacp_v1::EmitSignalResponse {
                    timestamp: None,
                    client_request_id: request.client_request_id,
                };
                let _ = reply.send(Ok(response));
            }
            AgentRequest::SendEnvelope {
                workspace_id,
                request,
                reply,
            } => {
                let from_ws = WorkspaceId::from(workspace_id.as_str());
                let to_ws = WorkspaceId::from(request.to_workspace.as_str());

                // Validate target exists in the tree.
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
                    from_workspace: from_ws,
                    to_workspace: to_ws.clone(),
                    envelope_type: request.r#type,
                    payload: request.payload,
                    in_reply_to: if request.in_reply_to.is_empty() {
                        None
                    } else {
                        Some(EnvelopeId::from(request.in_reply_to.as_str()))
                    },
                    timestamp: 0,
                    priority,
                    origin: EnvelopeOrigin::Agent,
                    state: EnvelopeState::Created,
                };

                // Notify any agents subscribed to the target workspace.
                self.notify_envelope_subs(to_ws.as_ref(), &envelope);

                // Route through the coordinator to the target workspace actor.
                if self.coordinator.route_envelope(&to_ws, envelope).await {
                    let response = wacp_transport::wacp_v1::SendEnvelopeResponse {
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
            AgentRequest::CreateCheckpoint {
                workspace_id,
                request,
                reply,
            } => {
                // SHA-256 content hash.
                let mut hasher = Sha256::new();
                hasher.update(&request.payload);
                let hash_bytes: [u8; 32] = hasher.finalize().into();
                let hash_hex: String = hash_bytes.iter().map(|b| format!("{b:02x}")).collect();

                // Persist to content-addressable store.
                if let Err(e) = self.checkpoint_storage.store(&hash_bytes, &request.payload) {
                    tracing::warn!(error = %e, "checkpoint persistence failed");
                }

                let cp_id = self.next_checkpoint_id;
                self.next_checkpoint_id += 1;
                let checkpoint_id = format!("cp-{cp_id}");

                // Index the checkpoint for later retrieval via GetCheckpoint.
                // ResourceUsage impls Copy, so no `.clone()` on that field.
                self.checkpoint_index.insert(
                    checkpoint_id.clone(),
                    CheckpointRecord {
                        content_hash: hash_bytes,
                        workspace_id: workspace_id.clone(),
                        checkpoint_type: request.r#type.clone(),
                        intent: request.intent.clone(),
                        status: request.status,
                        confidence: request.confidence,
                        resource_usage: request.resource_usage,
                    },
                );

                // WA3: forward to the workspace actor so the checkpoint is
                // pushed onto state.checkpoint_register, resource_meter is
                // updated, and WorkspaceEvent::CheckpointCreated +
                // auto-Signal(Checkpoint) events fire. If the actor has
                // already terminated the response still succeeds — the
                // payload is archived via the index above; this matches a
                // "recorded but not observed" semantics.
                let ws_id = WorkspaceId::from(workspace_id.as_str());
                if let Some(handle) = self.coordinator.handle(&ws_id) {
                    let status =
                        wacp_transport::wacp_v1::CheckpointStatus::try_from(request.status)
                            .map(proto_to_checkpoint_status)
                            .unwrap_or(CheckpointStatus::Provisional);
                    let confidence =
                        wacp_transport::wacp_v1::Confidence::try_from(request.confidence)
                            .map(proto_to_confidence)
                            .unwrap_or(Confidence::High);
                    let resource_usage = request.resource_usage.as_ref().map(|ru| ResourceUsage {
                        tokens: ru.tokens,
                        wall_time_ms: ru.wall_time_ms,
                        storage_bytes: ru.storage_bytes,
                        network_bytes: ru.network_bytes,
                        cost_micros: ru.cost_micros,
                    });
                    if handle
                        .agent_tx
                        .send(wacp_workspace::AgentMessage::CreateCheckpoint {
                            checkpoint_type: request.r#type.clone(),
                            payload: request.payload.clone(),
                            content_hash: hash_hex.clone(),
                            intent: request.intent.clone(),
                            status,
                            confidence,
                            resource_usage,
                        })
                        .await
                        .is_err()
                    {
                        tracing::warn!(
                            workspace_id = %workspace_id,
                            "checkpoint actor-forward failed: channel closed"
                        );
                    }
                }

                // WA3.5: provisional checkpoints open a highway gate so an
                // operator can approve or reject before the workspace
                // continues. The actor-side Active→Blocked transition
                // already fires from within handle_create_checkpoint above;
                // here we emit the GateEvent that the Console's
                // StreamGates driver consumes, and enqueue a
                // PendingCheckpointGate so RespondToGate can route the
                // eventual decision back to the actor. See
                // `wacp/impl/wa3-5-checkpoint-gates.md`.
                if request.status == wacp_transport::wacp_v1::CheckpointStatus::Provisional as i32
                    && self.coordinator.tree.get(&ws_id).is_some()
                {
                    let gate_event = self.gate_controller.open_checkpoint_gate(
                        ws_id.clone(),
                        CheckpointId::from(checkpoint_id.as_str()),
                        request.r#type.clone(),
                        None,
                        None,
                    );
                    let proto_ev = Ok(wacp_transport::wacp_v1::GateEvent {
                        gate_id: gate_event.gate_id.to_string(),
                        r#type: gate_type_to_proto(gate_event.gate_type) as i32,
                        subject: gate_event.subject.clone(),
                        workspace_id: gate_event.workspace_id.to_string(),
                        task_id: gate_event
                            .task_id
                            .as_ref()
                            .map(|t| t.to_string())
                            .unwrap_or_default(),
                        timeout_ms: gate_event.timeout_ms,
                        fallback_action: gate_event.fallback_action.clone(),
                        created_at: None,
                    });
                    self.gate_subs
                        .retain(|tx| tx.try_send(proto_ev.clone()).is_ok());
                }

                let response = wacp_transport::wacp_v1::CreateCheckpointResponse {
                    checkpoint_id,
                    content_hash: hash_hex,
                    timestamp: None,
                    client_request_id: request.client_request_id,
                };
                let _ = reply.send(Ok(response));
            }
            AgentRequest::QueryTrail { request, reply, .. } => {
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
                    None,
                    request.limit,
                    &request.client_request_id,
                );
                let _ = reply.send(Ok(response));
            }
            AgentRequest::SubscribeEnvelopes { workspace_id, tx } => {
                self.envelope_subs.entry(workspace_id).or_default().push(tx);
            }
            AgentRequest::SubscribeCommands { workspace_id, tx } => {
                self.command_subs.entry(workspace_id).or_default().push(tx);
            }
            AgentRequest::ReadResource { request, reply, .. } => {
                // Resolve resource_id against checkpoint storage.
                if let Some(record) = self.checkpoint_index.get(&request.resource_id) {
                    match self.checkpoint_storage.read(&record.content_hash) {
                        Ok(Some(data)) => {
                            let _ = reply.send(Ok(wacp_transport::wacp_v1::ReadResourceResponse {
                                content: data,
                                client_request_id: request.client_request_id,
                            }));
                        }
                        Ok(None) => {
                            let _ = reply.send(Err(tonic::Status::not_found(
                                "resource content not found in storage",
                            )));
                        }
                        Err(e) => {
                            let _ = reply
                                .send(Err(tonic::Status::internal(format!("storage error: {e}"))));
                        }
                    }
                } else {
                    let _ = reply.send(Err(tonic::Status::not_found(format!(
                        "resource '{}' not found",
                        request.resource_id
                    ))));
                }
            }
        }
    }
}
