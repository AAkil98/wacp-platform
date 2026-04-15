use wacp_transport::TransportError;
use wacp_transport::in_process::InProcessAgentClient;
use wacp_transport::messages::{AgentInbound, AgentOutbound};
use wacp_types::*;

/// Test agent — wraps in-process transport for clean API in tests.
pub struct TestAgent {
    client: InProcessAgentClient,
    pub workspace_id: WorkspaceId,
}

impl TestAgent {
    pub fn new(client: InProcessAgentClient, workspace_id: WorkspaceId) -> Self {
        Self {
            client,
            workspace_id,
        }
    }

    /// Emit a signal.
    pub async fn emit_signal(
        &self,
        signal_type: SignalType,
        reason: Option<String>,
    ) -> Result<(), TransportError> {
        self.client
            .send(AgentInbound::EmitSignal {
                signal_type,
                reason,
                context: None,
            })
            .await
    }

    /// Create a checkpoint.
    pub async fn create_checkpoint(
        &self,
        checkpoint_type: &str,
        payload: &[u8],
        intent: &str,
    ) -> Result<(), TransportError> {
        self.client
            .send(AgentInbound::CreateCheckpoint {
                checkpoint_type: checkpoint_type.into(),
                payload: payload.to_vec(),
                intent: intent.into(),
                status: CheckpointStatus::Provisional,
                confidence: Confidence::High,
                resource_usage: None,
            })
            .await
    }

    /// Receive the next message from the runtime.
    pub async fn recv(&mut self) -> Result<AgentOutbound, TransportError> {
        self.client.recv().await
    }
}
