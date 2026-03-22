use tokio::sync::mpsc;

use crate::messages::{AgentInbound, AgentOutbound};
use crate::TransportError;

/// Server-side agent session for in-process transport.
pub struct InProcessAgentSession {
    pub inbound_rx: mpsc::Receiver<AgentInbound>,
    pub outbound_tx: mpsc::Sender<AgentOutbound>,
}

/// Client-side agent handle for in-process transport.
pub struct InProcessAgentClient {
    pub inbound_tx: mpsc::Sender<AgentInbound>,
    pub outbound_rx: mpsc::Receiver<AgentOutbound>,
}

impl InProcessAgentSession {
    /// Receive the next message from the agent.
    pub async fn recv(&mut self) -> Result<AgentInbound, TransportError> {
        self.inbound_rx
            .recv()
            .await
            .ok_or(TransportError::ConnectionClosed)
    }

    /// Send a message to the agent.
    pub async fn send(&self, msg: AgentOutbound) -> Result<(), TransportError> {
        self.outbound_tx
            .send(msg)
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))
    }
}

impl InProcessAgentClient {
    /// Send a message to the server.
    pub async fn send(&self, msg: AgentInbound) -> Result<(), TransportError> {
        self.inbound_tx
            .send(msg)
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))
    }

    /// Receive a message from the server.
    pub async fn recv(&mut self) -> Result<AgentOutbound, TransportError> {
        self.outbound_rx
            .recv()
            .await
            .ok_or(TransportError::ConnectionClosed)
    }
}

/// In-process transport for testing — channel-based, zero serialization.
pub struct InProcessTransport;

impl InProcessTransport {
    /// Create a connected agent session pair (server-side, client-side).
    pub fn connect_agent() -> (InProcessAgentSession, InProcessAgentClient) {
        let (inbound_tx, inbound_rx) = mpsc::channel(64);
        let (outbound_tx, outbound_rx) = mpsc::channel(64);

        let server = InProcessAgentSession {
            inbound_rx,
            outbound_tx,
        };
        let client = InProcessAgentClient {
            inbound_tx,
            outbound_rx,
        };

        (server, client)
    }
}
