use std::sync::Arc;

use tokio::sync::Mutex;
use wacp_transport::wacp_v1;
use wacp_transport::wacp_v1::agent_service_client::AgentServiceClient;
use wacp_types::*;

use crate::error::Error;

/// Builder for creating checkpoints (sdk-agent spec §4).
pub struct CheckpointBuilder {
    client: Arc<Mutex<AgentServiceClient<tonic::transport::Channel>>>,
    checkpoint_type: Option<String>,
    payload: Option<Vec<u8>>,
    intent: Option<String>,
    status: CheckpointStatus,
    confidence: Confidence,
    resource_usage: Option<ResourceUsage>,
}

impl CheckpointBuilder {
    pub(crate) fn new(
        client: Arc<Mutex<AgentServiceClient<tonic::transport::Channel>>>,
    ) -> Self {
        Self {
            client,
            checkpoint_type: None,
            payload: None,
            intent: None,
            status: CheckpointStatus::Provisional,
            confidence: Confidence::High,
            resource_usage: None,
        }
    }

    pub fn checkpoint_type(mut self, t: &str) -> Self {
        self.checkpoint_type = Some(t.into());
        self
    }

    pub fn payload(mut self, p: &[u8]) -> Self {
        self.payload = Some(p.to_vec());
        self
    }

    pub fn intent(mut self, i: &str) -> Self {
        self.intent = Some(i.into());
        self
    }

    pub fn status(mut self, s: CheckpointStatus) -> Self {
        self.status = s;
        self
    }

    pub fn confidence(mut self, c: Confidence) -> Self {
        self.confidence = c;
        self
    }

    pub fn resource_usage(mut self, r: ResourceUsage) -> Self {
        self.resource_usage = Some(r);
        self
    }

    /// Create the checkpoint. Requires checkpoint_type and payload.
    pub async fn create(self) -> Result<CheckpointResult, Error> {
        let checkpoint_type = self
            .checkpoint_type
            .ok_or(Error::MissingField("checkpoint_type".into()))?;
        let payload = self
            .payload
            .ok_or(Error::MissingField("payload".into()))?;

        let proto_status = match self.status {
            CheckpointStatus::Provisional => wacp_v1::CheckpointStatus::Provisional,
            CheckpointStatus::Final => wacp_v1::CheckpointStatus::Final,
        };

        let proto_confidence = match self.confidence {
            Confidence::High => wacp_v1::Confidence::High,
            Confidence::Medium => wacp_v1::Confidence::Medium,
            Confidence::Low => wacp_v1::Confidence::Low,
        };

        let proto_usage = self.resource_usage.map(|r| wacp_v1::ResourceUsage {
            tokens: r.tokens,
            wall_time_ms: r.wall_time_ms,
            storage_bytes: r.storage_bytes,
            network_bytes: r.network_bytes,
            cost_micros: r.cost_micros,
        });

        let mut client = self.client.lock().await;
        let response = client
            .create_checkpoint(wacp_v1::CreateCheckpointRequest {
                r#type: checkpoint_type,
                payload,
                intent: self.intent.unwrap_or_default(),
                status: proto_status.into(),
                confidence: proto_confidence.into(),
                resource_usage: proto_usage,
                client_request_id: String::new(),
            })
            .await?
            .into_inner();

        Ok(CheckpointResult {
            id: response.checkpoint_id,
            content_hash: response.content_hash,
        })
    }
}

/// Result from creating a checkpoint.
#[derive(Debug)]
pub struct CheckpointResult {
    pub id: String,
    pub content_hash: String,
}

/// Builder for sending envelopes (sdk-agent spec §4).
pub struct EnvelopeBuilder {
    client: Arc<Mutex<AgentServiceClient<tonic::transport::Channel>>>,
    to: Option<WorkspaceId>,
    envelope_type: Option<String>,
    payload: Option<Vec<u8>>,
    in_reply_to: Option<EnvelopeId>,
    priority: EnvelopePriority,
}

impl EnvelopeBuilder {
    pub(crate) fn new(
        client: Arc<Mutex<AgentServiceClient<tonic::transport::Channel>>>,
    ) -> Self {
        Self {
            client,
            to: None,
            envelope_type: None,
            payload: None,
            in_reply_to: None,
            priority: EnvelopePriority::Normal,
        }
    }

    pub fn to(mut self, workspace: &WorkspaceId) -> Self {
        self.to = Some(workspace.clone());
        self
    }

    pub fn envelope_type(mut self, t: &str) -> Self {
        self.envelope_type = Some(t.into());
        self
    }

    pub fn payload(mut self, p: &[u8]) -> Self {
        self.payload = Some(p.to_vec());
        self
    }

    pub fn in_reply_to(mut self, id: &EnvelopeId) -> Self {
        self.in_reply_to = Some(id.clone());
        self
    }

    pub fn priority(mut self, p: EnvelopePriority) -> Self {
        self.priority = p;
        self
    }

    /// Send the envelope. Requires `to` and `envelope_type`.
    pub async fn send(self) -> Result<EnvelopeResult, Error> {
        let to = self
            .to
            .ok_or(Error::MissingField("to".into()))?;
        let envelope_type = self
            .envelope_type
            .ok_or(Error::MissingField("envelope_type".into()))?;

        let proto_priority = match self.priority {
            EnvelopePriority::Normal => wacp_v1::EnvelopePriority::Normal,
            EnvelopePriority::Urgent => wacp_v1::EnvelopePriority::Urgent,
            EnvelopePriority::Blocking => wacp_v1::EnvelopePriority::Blocking,
        };

        let mut client = self.client.lock().await;
        let response = client
            .send_envelope(wacp_v1::SendEnvelopeRequest {
                to_workspace: to.to_string(),
                r#type: envelope_type,
                payload: self.payload.unwrap_or_default(),
                in_reply_to: self
                    .in_reply_to
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                priority: proto_priority.into(),
                client_request_id: String::new(),
            })
            .await?
            .into_inner();

        Ok(EnvelopeResult {
            id: response.envelope_id,
        })
    }
}

/// Result from sending an envelope.
#[derive(Debug)]
pub struct EnvelopeResult {
    pub id: String,
}

