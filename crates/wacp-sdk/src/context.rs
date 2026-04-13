use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use wacp_transport::wacp_v1;
use wacp_types::*;

use crate::builder::{CheckpointResult, EnvelopeResult};
use crate::connection::Agent;
use crate::error::Error;
use crate::streams::InboxStream;

/// Middleware-level agent API. Wraps Agent + optional ToolRegistry.
///
/// AgentContext adds tool integration, convenience methods, and cancellation
/// on top of the protocol-level Agent.
pub struct AgentContext {
    agent: Agent,
    tools: Option<Arc<wacp_tools::ToolRegistry>>,
    cancellation: CancellationToken,
}

impl AgentContext {
    /// Create from an existing Agent, optionally with a tool registry.
    pub fn new(agent: Agent, tools: Option<Arc<wacp_tools::ToolRegistry>>) -> Self {
        Self {
            agent,
            tools,
            cancellation: CancellationToken::new(),
        }
    }

    /// Create with an explicit cancellation token (for external cancellation).
    pub fn with_cancellation(
        agent: Agent,
        tools: Option<Arc<wacp_tools::ToolRegistry>>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            agent,
            tools,
            cancellation,
        }
    }

    // ── Identity & directive ────────────────────────────────

    /// The directive envelope from the bind response.
    pub fn directive(&self) -> Option<&wacp_v1::Envelope> {
        self.agent.directive()
    }

    /// Context bytes from the bind response.
    pub fn context(&self) -> &[u8] {
        self.agent.context()
    }

    /// The role assigned to this workspace.
    pub fn role(&self) -> &str {
        self.agent.role()
    }

    /// The workspace ID.
    pub fn workspace_id(&self) -> &WorkspaceId {
        self.agent.workspace_id()
    }

    /// Resources this workspace can read.
    pub fn visibility(&self) -> &[String] {
        self.agent.visibility()
    }

    /// Resources this workspace can modify.
    pub fn authority(&self) -> &[String] {
        self.agent.authority()
    }

    // ── Lifecycle convenience ───────────────────────────────

    /// Mark work as complete. Optionally creates a final checkpoint first.
    pub async fn complete(&self, final_payload: Option<&[u8]>) -> Result<(), Error> {
        if let Some(payload) = final_payload {
            self.agent
                .checkpoint()
                .checkpoint_type("artifact")
                .payload(payload)
                .intent("task complete")
                .status(CheckpointStatus::Final)
                .confidence(Confidence::High)
                .create()
                .await?;
        }
        self.agent.signal(SignalType::Complete).await
    }

    /// Signal that this workspace is blocked.
    pub async fn blocked(&self, reason: &str) -> Result<(), Error> {
        self.agent.signal_blocked(reason).await
    }

    /// Signal an escalation to the highway.
    pub async fn escalate(&self, context: &[u8]) -> Result<(), Error> {
        self.agent.signal_escalation(context).await
    }

    // ── Checkpoints ─────────────────────────────────────────

    /// Start building a checkpoint (full control).
    pub fn checkpoint(&self) -> crate::builder::CheckpointBuilder {
        self.agent.checkpoint()
    }

    /// Create a provisional artifact checkpoint (the most common pattern).
    pub async fn quick_checkpoint(
        &self,
        payload: &[u8],
        intent: &str,
    ) -> Result<CheckpointResult, Error> {
        self.agent
            .checkpoint()
            .checkpoint_type("artifact")
            .payload(payload)
            .intent(intent)
            .status(CheckpointStatus::Provisional)
            .confidence(Confidence::High)
            .create()
            .await
    }

    // ── Communication ───────────────────────────────────────

    /// Send a query to the coordinator and wait for the response.
    pub async fn query(
        &self,
        content: &[u8],
        timeout_ms: Option<u64>,
    ) -> Result<wacp_v1::Envelope, Error> {
        // Send query envelope to coordinator (first visibility entry is typically coordinator)
        let coordinator_ws = self
            .agent
            .visibility()
            .first()
            .map(|s| WorkspaceId::from(s.as_str()))
            .ok_or_else(|| Error::MissingField("no coordinator in visibility".into()))?;

        let envelope_result = self
            .agent
            .send_envelope()
            .to(&coordinator_ws)
            .envelope_type("query")
            .payload(content)
            .send()
            .await?;

        // Wait for response on inbox
        let timeout = timeout_ms.unwrap_or(30_000);
        let mut inbox = self.agent.inbox().await?;
        let target_id = envelope_result.id.clone();

        let response = tokio::time::timeout(Duration::from_millis(timeout), async {
            while let Some(item) = inbox.next().await {
                let env = item?;
                if env.in_reply_to == target_id {
                    return Ok(env);
                }
                // Non-matching envelope — skip (in production, would buffer)
            }
            Err(Error::StreamEnded)
        })
        .await
        .map_err(|_| Error::QueryTimeout(timeout))?;

        response
    }

    /// Send an envelope to a target workspace.
    pub async fn send(
        &self,
        target: &WorkspaceId,
        envelope_type: &str,
        payload: &[u8],
    ) -> Result<EnvelopeResult, Error> {
        self.agent
            .send_envelope()
            .to(target)
            .envelope_type(envelope_type)
            .payload(payload)
            .send()
            .await
    }

    /// Open the inbox stream.
    pub async fn inbox(&self) -> Result<InboxStream, Error> {
        self.agent.inbox().await
    }

    // ── Tools ───────────────────────────────────────────────

    /// Invoke a tool by name. Delegates to the local registry if available,
    /// otherwise sends a query envelope to the coordinator.
    pub async fn tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, Error> {
        if let Some(registry) = &self.tools {
            let result = registry
                .execute(
                    name,
                    name, // single-capability tools: capability name = tool name
                    args,
                    wacp_tools::ExecutionOptions {
                        workspace_id: Some(self.agent.workspace_id().clone()),
                        cancellation_token: Some(self.cancellation.clone()),
                        ..Default::default()
                    },
                )
                .await?;
            Ok(result)
        } else {
            // Remote path: encode as query envelope
            let payload = serde_json::json!({"tool": name, "args": args});
            let payload_bytes =
                serde_json::to_vec(&payload).map_err(|e| Error::MissingField(e.to_string()))?;
            let response = self.query(&payload_bytes, None).await?;
            serde_json::from_slice(&response.payload).map_err(|e| {
                wacp_tools::ToolError::internal(format!("invalid tool response: {e}")).into()
            })
        }
    }

    /// List available tool descriptors (empty if no registry).
    pub fn tools(&self) -> Vec<&wacp_tools::ToolDescriptor> {
        self.tools
            .as_ref()
            .map(|r| r.list_tools())
            .unwrap_or_default()
    }

    // ── Observation ─────────────────────────────────────────

    /// Query the trail.
    pub async fn trail(
        &self,
        event_type: Option<&str>,
        limit: u32,
    ) -> Result<Vec<wacp_v1::TrailEntry>, Error> {
        self.agent.query_trail(None, event_type, limit).await
    }

    // ── Cancellation ────────────────────────────────────────

    /// The cancellation token. Pass to tool invocations and long-running loops.
    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation
    }

    /// Check if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Access the underlying protocol-level Agent.
    pub fn agent(&self) -> &Agent {
        &self.agent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wacp_tools::{
        PackageBuilder, RegistryConfig, ToolDescriptor, ToolRegistry,
        descriptor::Capability,
        handler::{ToolContext, ToolHandler},
    };

    fn test_descriptor(name: &str) -> ToolDescriptor {
        ToolDescriptor {
            name: name.into(),
            version: "1.0.0".into(),
            description: format!("{name} tool"),
            capabilities: vec![Capability {
                name: name.into(),
                description: format!("{name} capability"),
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: serde_json::json!({"type": "object"}),
                timeout_ms: None,
                idempotent: false,
                side_effects: false,
            }],
            config_schema: None,
            tags: vec![],
        }
    }

    fn echo_handler() -> impl ToolHandler {
        |_ctx: &ToolContext, args: serde_json::Value| async move { Ok(args) }
    }

    async fn test_registry() -> Arc<ToolRegistry> {
        let mut registry = ToolRegistry::new(RegistryConfig::default());
        let pkg = PackageBuilder::new(test_descriptor("echo"))
            .handler("echo", echo_handler())
            .build()
            .unwrap();
        registry.register(pkg).await.unwrap();
        Arc::new(registry)
    }

    // --- Tools ---

    #[tokio::test]
    async fn tools_with_registry_returns_descriptors() {
        let registry = test_registry().await;
        assert_eq!(registry.list_tools().len(), 1);
        // Can't create AgentContext without a real Agent (needs gRPC),
        // so test the registry integration directly
        let result = registry
            .execute(
                "echo",
                "echo",
                serde_json::json!({"hello": "world"}),
                wacp_tools::ExecutionOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(result, serde_json::json!({"hello": "world"}));
    }

    #[tokio::test]
    async fn tools_without_registry_returns_empty() {
        // Simulate: tools() with None registry
        let tools: Option<Arc<ToolRegistry>> = None;
        let list: Vec<&wacp_tools::ToolDescriptor> =
            tools.as_ref().map(|r| r.list_tools()).unwrap_or_default();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn tool_execution_through_registry() {
        let registry = test_registry().await;
        let result = registry
            .execute(
                "echo",
                "echo",
                serde_json::json!({"key": "value"}),
                wacp_tools::ExecutionOptions {
                    workspace_id: Some(WorkspaceId::from("ws-test")),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(result["key"], "value");
    }

    #[tokio::test]
    async fn tool_not_found_returns_error() {
        let registry = test_registry().await;
        let result = registry
            .execute(
                "nonexistent",
                "nonexistent",
                serde_json::json!({}),
                wacp_tools::ExecutionOptions::default(),
            )
            .await;
        assert!(result.is_err());
    }

    // --- Cancellation ---

    #[test]
    fn cancellation_token_not_cancelled_initially() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancellation_token_cancelled_after_cancel() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    // --- Error variants ---

    #[test]
    fn tool_error_variant() {
        let tool_err = wacp_tools::ToolError::validation("bad input");
        let err: Error = tool_err.into();
        assert!(err.to_string().contains("tool error"));
    }

    #[test]
    fn query_timeout_variant() {
        let err = Error::QueryTimeout(5000);
        assert_eq!(err.to_string(), "query timed out after 5000ms");
    }
}
