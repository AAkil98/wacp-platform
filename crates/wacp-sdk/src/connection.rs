use std::sync::Arc;

use tokio::sync::Mutex;
use wacp_transport::wacp_v1;
use wacp_transport::wacp_v1::agent_service_client::AgentServiceClient;
use wacp_types::*;

use crate::AgentConfig;
use crate::builder::{CheckpointBuilder, EnvelopeBuilder};
use crate::error::Error;
use crate::streams::{CommandStream, InboxStream};

/// A connected WACP agent — the primary SDK type.
///
/// Created via `Agent::connect()`. Provides the full agent API:
/// signals, checkpoints, envelopes, inbox, commands.
pub struct Agent {
    client: Arc<Mutex<AgentServiceClient<tonic::transport::Channel>>>,
    workspace_id: WorkspaceId,
    bind_response: wacp_v1::BindResponse,
}

impl Agent {
    /// Connect to the runtime and bind to a workspace.
    pub async fn connect(config: AgentConfig) -> Result<Self, Error> {
        let channel = tonic::transport::Channel::from_shared(config.runtime_url.clone())
            .map_err(|e| Error::ConnectionFailed(e.to_string()))?
            .connect()
            .await?;

        let mut client = AgentServiceClient::new(channel);

        // Call Bind RPC.
        let bind_response = client
            .bind(wacp_v1::BindRequest {
                workspace_id: config.workspace_id.to_string(),
                auth_token: config.auth_token,
                client_request_id: String::new(),
            })
            .await?
            .into_inner();

        Ok(Agent {
            client: Arc::new(Mutex::new(client)),
            workspace_id: config.workspace_id,
            bind_response,
        })
    }

    /// The workspace this agent is bound to.
    pub fn workspace_id(&self) -> &WorkspaceId {
        &self.workspace_id
    }

    /// The directive envelope from the bind response.
    pub fn directive(&self) -> Option<&wacp_v1::Envelope> {
        self.bind_response.directive.as_ref()
    }

    /// Context bytes from the bind response.
    pub fn context(&self) -> &[u8] {
        &self.bind_response.context
    }

    /// The role assigned to this workspace.
    pub fn role(&self) -> &str {
        &self.bind_response.role
    }

    /// Resources this workspace can read.
    pub fn visibility(&self) -> &[String] {
        &self.bind_response.visibility
    }

    /// Resources this workspace can modify.
    pub fn authority(&self) -> &[String] {
        &self.bind_response.authority
    }

    /// Emit a signal.
    pub async fn signal(&self, signal_type: SignalType) -> Result<(), Error> {
        self.signal_with(signal_type, None, None).await
    }

    /// Emit a blocked signal with a reason.
    pub async fn signal_blocked(&self, reason: &str) -> Result<(), Error> {
        self.signal_with(SignalType::Blocked, Some(reason.into()), None)
            .await
    }

    /// Emit a failed signal with a reason.
    pub async fn signal_failed(&self, reason: &str) -> Result<(), Error> {
        self.signal_with(SignalType::Failed, Some(reason.into()), None)
            .await
    }

    /// Emit an escalation signal with context.
    pub async fn signal_escalation(&self, context: &[u8]) -> Result<(), Error> {
        self.signal_with(SignalType::Escalation, None, Some(context.to_vec()))
            .await
    }

    async fn signal_with(
        &self,
        signal_type: SignalType,
        reason: Option<String>,
        context: Option<Vec<u8>>,
    ) -> Result<(), Error> {
        let proto_type = match signal_type {
            SignalType::Ready => wacp_v1::SignalType::Ready,
            SignalType::Started => wacp_v1::SignalType::Started,
            SignalType::Blocked => wacp_v1::SignalType::Blocked,
            SignalType::Checkpoint => wacp_v1::SignalType::Checkpoint,
            SignalType::Complete => wacp_v1::SignalType::Complete,
            SignalType::Failed => wacp_v1::SignalType::Failed,
            SignalType::Integrate => wacp_v1::SignalType::Integrate,
            SignalType::Acknowledged => wacp_v1::SignalType::Acknowledged,
            SignalType::Escalation => wacp_v1::SignalType::Escalation,
            SignalType::Suspend => wacp_v1::SignalType::Suspend,
            SignalType::Migrate => wacp_v1::SignalType::Migrate,
        };

        let mut client = self.client.lock().await;
        client
            .emit_signal(wacp_v1::EmitSignalRequest {
                r#type: proto_type.into(),
                reason: reason.unwrap_or_default(),
                context: context.unwrap_or_default(),
                client_request_id: String::new(),
            })
            .await?;

        Ok(())
    }

    /// Start building a checkpoint.
    pub fn checkpoint(&self) -> CheckpointBuilder {
        CheckpointBuilder::new(self.client.clone())
    }

    /// Start building an envelope to send.
    pub fn send_envelope(&self) -> EnvelopeBuilder {
        EnvelopeBuilder::new(self.client.clone())
    }

    /// Open the inbox stream — receives envelopes as they arrive.
    pub async fn inbox(&self) -> Result<InboxStream, Error> {
        let mut client = self.client.lock().await;
        let stream = client
            .receive_envelopes(wacp_v1::ReceiveEnvelopesRequest {})
            .await?
            .into_inner();
        Ok(InboxStream::new(stream))
    }

    /// Open the commands stream — receives coordinator commands.
    pub async fn commands(&self) -> Result<CommandStream, Error> {
        let mut client = self.client.lock().await;
        let stream = client
            .receive_commands(wacp_v1::ReceiveCommandsRequest {})
            .await?
            .into_inner();
        Ok(CommandStream::new(stream))
    }

    /// Query the trail.
    pub async fn query_trail(
        &self,
        workspace_id: Option<&str>,
        event_type: Option<&str>,
        limit: u32,
    ) -> Result<Vec<wacp_v1::TrailEntry>, Error> {
        let mut client = self.client.lock().await;
        let response = client
            .query_trail(wacp_v1::QueryTrailRequest {
                workspace_id: workspace_id.unwrap_or_default().into(),
                event_type: event_type.unwrap_or_default().into(),
                from: None,
                to: None,
                limit,
                client_request_id: String::new(),
            })
            .await?
            .into_inner();
        Ok(response.entries)
    }

    /// Disconnect from the runtime.
    pub async fn disconnect(self) -> Result<(), Error> {
        // Dropping the client closes the gRPC channel.
        drop(self.client);
        Ok(())
    }
}
