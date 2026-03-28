use tokio::sync::mpsc;
use wacp_types::*;
use wacp_fsm::{StateMachine, WorkspaceFsm, WorkspaceTrigger};

use crate::state::{ArchivedWorkspace, MigrationSnapshot, WorkspaceConfig, WorkspaceState};

/// Commands from the coordinator (high-priority channel).
#[derive(Debug)]
pub enum CoordinatorCommand {
    Abort,
    Suspend,
    Resume,
    MigrateBegin,
    MigrationComplete { restore_blocked: bool },
    MigrationFailed,
    IntegrationSucceeded,
    IntegrationFailed,
    ConflictDetected,
    ConflictResolved,
    ConflictUnresolvable,
    GrantVisibility(Vec<String>),
    UpdateBudget(ResourceBudget),
    DeliverEnvelope(Envelope),
    GracefulTermination { grace_period_ms: u64 },
}

/// Messages from the agent (normal channel).
#[derive(Debug)]
pub enum AgentMessage {
    EmitSignal {
        signal_type: SignalType,
        reason: Option<String>,
        context: Option<Vec<u8>>,
    },
    CreateCheckpoint {
        checkpoint_type: String,
        payload: Vec<u8>,
        content_hash: String,
        intent: String,
        status: CheckpointStatus,
        confidence: Confidence,
        resource_usage: Option<ResourceUsage>,
    },
}

/// Events emitted by the workspace actor.
#[derive(Debug)]
pub enum WorkspaceEvent {
    Signal(Signal),
    StateChanged {
        workspace_id: WorkspaceId,
        from: wacp_types::WorkspaceState,
        to: wacp_types::WorkspaceState,
    },
    Terminated(Box<ArchivedWorkspace>),
    CheckpointCreated(Checkpoint),
    MigrationSnapshot {
        workspace_id: WorkspaceId,
        snapshot: MigrationSnapshot,
    },
    Error {
        workspace_id: WorkspaceId,
        message: String,
    },
}

/// Handle returned from spawn — holds senders for both channels.
pub struct WorkspaceHandle {
    pub id: WorkspaceId,
    pub coordinator_tx: mpsc::Sender<CoordinatorCommand>,
    pub agent_tx: mpsc::Sender<AgentMessage>,
}

/// The workspace actor.
pub struct WorkspaceActor {
    state: WorkspaceState,
    coordinator_rx: mpsc::Receiver<CoordinatorCommand>,
    agent_rx: mpsc::Receiver<AgentMessage>,
    event_tx: mpsc::Sender<WorkspaceEvent>,
    first_envelope_received: bool,
}

impl WorkspaceActor {
    /// Spawn the workspace actor as a tokio task.
    pub fn spawn(
        config: WorkspaceConfig,
        event_tx: mpsc::Sender<WorkspaceEvent>,
    ) -> WorkspaceHandle {
        let (coordinator_tx, coordinator_rx) = mpsc::channel(64);
        let (agent_tx, agent_rx) = mpsc::channel(64);
        let id = config.id.clone();

        let mut actor = Self {
            state: WorkspaceState::new(config),
            coordinator_rx,
            agent_rx,
            event_tx,
            first_envelope_received: false,
        };

        tokio::spawn(async move {
            actor.run().await;
        });

        WorkspaceHandle {
            id,
            coordinator_tx,
            agent_tx,
        }
    }

    async fn run(&mut self) {
        loop {
            if self.state.status.is_terminal() {
                let state = std::mem::replace(
                    &mut self.state,
                    WorkspaceState::new(crate::state::WorkspaceConfig {
                        id: WorkspaceId::from("__tombstone__"),
                        role: String::new(),
                        base_role: BaseRole::Worker,
                        parent: WorkspaceId::from(""),
                        owner: UserId::from(""),
                        originator: Originator::System,
                        directive: dummy_envelope(),
                        context: vec![],
                        visibility: Default::default(),
                        authority: Default::default(),
                        delegate: false,
                        budget: None,
                    }),
                );
                let _ = self
                    .event_tx
                    .send(WorkspaceEvent::Terminated(Box::new(state.archive())))
                    .await;
                return;
            }

            tokio::select! {
                biased;
                Some(cmd) = self.coordinator_rx.recv() => {
                    self.handle_coordinator_cmd(cmd).await;
                }
                Some(msg) = self.agent_rx.recv() => {
                    self.handle_agent_msg(msg).await;
                }
                else => break,
            }
        }
    }

    async fn handle_coordinator_cmd(&mut self, cmd: CoordinatorCommand) {
        match cmd {
            CoordinatorCommand::Abort => {
                self.transition(WorkspaceTrigger::CoordinatorAbort).await;
            }
            CoordinatorCommand::Suspend => {
                self.transition(WorkspaceTrigger::CoordinatorSuspend).await;
            }
            CoordinatorCommand::Resume => {
                self.transition(WorkspaceTrigger::CoordinatorResume).await;
            }
            CoordinatorCommand::MigrateBegin => {
                self.transition(WorkspaceTrigger::CoordinatorMigrate).await;
                // Capture and emit snapshot if transition succeeded
                if self.state.status == wacp_types::WorkspaceState::Migrating {
                    let snapshot = self.state.capture_snapshot();
                    let _ = self
                        .event_tx
                        .send(WorkspaceEvent::MigrationSnapshot {
                            workspace_id: self.state.id.clone(),
                            snapshot,
                        })
                        .await;
                }
            }
            CoordinatorCommand::MigrationComplete { restore_blocked } => {
                let trigger = if restore_blocked {
                    WorkspaceTrigger::MigrationSucceededBlocked
                } else {
                    WorkspaceTrigger::MigrationSucceeded
                };
                self.transition(trigger).await;
            }
            CoordinatorCommand::MigrationFailed => {
                self.transition(WorkspaceTrigger::MigrationFailed).await;
            }
            CoordinatorCommand::IntegrationSucceeded => {
                self.transition(WorkspaceTrigger::IntegrationSucceeded).await;
            }
            CoordinatorCommand::IntegrationFailed => {
                self.transition(WorkspaceTrigger::IntegrationFailed).await;
            }
            CoordinatorCommand::ConflictDetected => {
                self.transition(WorkspaceTrigger::ConflictDetected).await;
            }
            CoordinatorCommand::ConflictResolved => {
                self.transition(WorkspaceTrigger::ConflictResolved).await;
            }
            CoordinatorCommand::ConflictUnresolvable => {
                self.transition(WorkspaceTrigger::ConflictUnresolvable).await;
            }
            CoordinatorCommand::GrantVisibility(resources) => {
                self.state.grant_visibility(&resources);
            }
            CoordinatorCommand::UpdateBudget(budget) => {
                self.state.resource_meter.budget = Some(budget);
            }
            CoordinatorCommand::DeliverEnvelope(envelope) => {
                if self.state.is_envelope_delivered(envelope.id.as_ref()) {
                    return; // deduplicate
                }

                // First envelope triggers Idle → Active.
                if !self.first_envelope_received
                    && self.state.status == wacp_types::WorkspaceState::Idle
                {
                    self.first_envelope_received = true;
                    self.transition(WorkspaceTrigger::ReceiveFirstEnvelope)
                        .await;
                }

                self.state.push_inbox(envelope);
            }
            CoordinatorCommand::GracefulTermination { .. } => {
                // Placeholder — the grace period timer is a coordinator concern.
            }
        }
    }

    async fn handle_agent_msg(&mut self, msg: AgentMessage) {
        // Reject agent messages in Migrating state (migration spec §4.3).
        if self.state.status == wacp_types::WorkspaceState::Migrating {
            return;
        }
        match msg {
            AgentMessage::EmitSignal {
                signal_type,
                reason,
                context,
            } => {
                // Map signal to FSM trigger.
                let trigger = match signal_type {
                    SignalType::Started => Some(WorkspaceTrigger::AgentStarted),
                    SignalType::Blocked => Some(WorkspaceTrigger::AgentBlocked),
                    SignalType::Complete => Some(WorkspaceTrigger::AgentComplete),
                    SignalType::Failed => Some(WorkspaceTrigger::AgentFailed),
                    _ => None, // Other signals don't trigger state transitions.
                };

                if let Some(t) = trigger {
                    self.transition(t).await;
                }

                let signal = Signal {
                    signal_type,
                    workspace_id: self.state.id.clone(),
                    timestamp: 0, // placeholder — clock assigned by coordinator
                    reason,
                    context,
                };
                let _ = self.event_tx.send(WorkspaceEvent::Signal(signal)).await;
            }
            AgentMessage::CreateCheckpoint {
                checkpoint_type,
                payload,
                content_hash,
                intent,
                status,
                confidence,
                resource_usage,
            } => {
                self.handle_create_checkpoint(
                    checkpoint_type,
                    payload,
                    content_hash,
                    intent,
                    status,
                    confidence,
                    resource_usage,
                )
                .await;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_create_checkpoint(
        &mut self,
        checkpoint_type: String,
        payload: Vec<u8>,
        content_hash: String,
        intent: String,
        status: CheckpointStatus,
        confidence: Confidence,
        resource_usage: Option<ResourceUsage>,
    ) {
        // Chain validation: parent must be current head.
        let expected_parent = self.state.last_checkpoint().map(|cp| cp.id.clone());

        let cp = Checkpoint {
            id: CheckpointId::from(format!("cp-{}", self.state.checkpoints().len())),
            workspace_id: self.state.id.clone(),
            checkpoint_type,
            payload: payload.clone(),
            content_hash,
            intent,
            parent_checkpoint: expected_parent,
            status,
            confidence,
            timestamp: 0, // placeholder
            resource_usage,
        };

        // Update resource meter.
        self.state.resource_meter.usage.storage_bytes += payload.len() as u64;

        self.state.push_checkpoint(cp.clone());

        // Emit checkpoint created event.
        let _ = self
            .event_tx
            .send(WorkspaceEvent::CheckpointCreated(cp))
            .await;

        // Auto-signal: checkpoint signal (PROTOCOL.md §7.2, rule 5).
        let signal = Signal {
            signal_type: SignalType::Checkpoint,
            workspace_id: self.state.id.clone(),
            timestamp: 0,
            reason: None,
            context: None,
        };
        let _ = self.event_tx.send(WorkspaceEvent::Signal(signal)).await;
    }

    async fn transition(&mut self, trigger: WorkspaceTrigger) {
        let from = self.state.status;
        match WorkspaceFsm::transition(from, &trigger) {
            Ok(to) => {
                self.state.status = to;
                let _ = self
                    .event_tx
                    .send(WorkspaceEvent::StateChanged {
                        workspace_id: self.state.id.clone(),
                        from,
                        to,
                    })
                    .await;
            }
            Err(e) => {
                let _ = self
                    .event_tx
                    .send(WorkspaceEvent::Error {
                        workspace_id: self.state.id.clone(),
                        message: e.to_string(),
                    })
                    .await;
            }
        }
    }
}

fn dummy_envelope() -> Envelope {
    Envelope {
        id: EnvelopeId::from(""),
        from_workspace: WorkspaceId::from(""),
        to_workspace: WorkspaceId::from(""),
        envelope_type: String::new(),
        payload: vec![],
        in_reply_to: None,
        timestamp: 0,
        priority: EnvelopePriority::Normal,
        origin: EnvelopeOrigin::Agent,
        state: EnvelopeState::Created,
    }
}
