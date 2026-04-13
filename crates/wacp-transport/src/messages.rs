use wacp_types::*;

/// Inbound messages from an agent to the runtime.
#[derive(Debug)]
pub enum AgentInbound {
    Bind {
        workspace_id: WorkspaceId,
        auth_token: String,
    },
    SendEnvelope {
        to_workspace: WorkspaceId,
        envelope_type: String,
        payload: Vec<u8>,
        in_reply_to: Option<EnvelopeId>,
        priority: EnvelopePriority,
    },
    EmitSignal {
        signal_type: SignalType,
        reason: Option<String>,
        context: Option<Vec<u8>>,
    },
    CreateCheckpoint {
        checkpoint_type: String,
        payload: Vec<u8>,
        intent: String,
        status: CheckpointStatus,
        confidence: Confidence,
        resource_usage: Option<ResourceUsage>,
    },
    QueryTrail {
        workspace_id: Option<WorkspaceId>,
        event_type: Option<String>,
        limit: u32,
    },
}

/// Outbound messages from the runtime to an agent.
#[derive(Debug)]
pub enum AgentOutbound {
    BindResponse {
        workspace_id: WorkspaceId,
        state: WorkspaceState,
        role: String,
    },
    EnvelopeDelivery(Envelope),
    Error(ProtocolError),
}

/// Inbound messages from highway (human) to the runtime.
#[derive(Debug)]
pub enum HighwayInbound {
    Authenticate {
        auth_token: String,
    },
    InjectEnvelope {
        to_workspace: WorkspaceId,
        envelope_type: String,
        payload: Vec<u8>,
        priority: EnvelopePriority,
    },
    RespondToGate {
        gate_id: GateId,
        decision: GateDecision,
    },
    QueryTrail {
        workspace_id: Option<WorkspaceId>,
        event_type: Option<String>,
        limit: u32,
    },
}

/// Outbound messages from the runtime to highway.
#[derive(Debug)]
pub enum HighwayOutbound {
    TrailEntry(TrailEntry),
    GateEvent(GateEvent),
    EscalationEvent(EscalationEvent),
    WorkspaceStateChange {
        workspace_id: WorkspaceId,
        from: WorkspaceState,
        to: WorkspaceState,
    },
}
