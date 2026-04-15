"""WACP Agent — the primary SDK class.

Connects to the runtime via gRPC, binds to a workspace, and provides
the full agent API: signals, checkpoints, envelopes, inbox, commands.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import AsyncIterator, Optional

from grpclib.client import Channel

from wacp.proto.v1 import (
    AgentServiceStub,
    BindRequest,
    BindResponse,
    CreateCheckpointRequest,
    CreateCheckpointResponse,
    EmitSignalRequest,
    QueryTrailRequest,
    QueryTrailResponse,
    ReceiveCommandsRequest,
    ReceiveEnvelopesRequest,
    SendEnvelopeRequest,
    SendEnvelopeResponse,
    Envelope,
    Command,
    TrailEntry,
    ResourceUsage,
)
from wacp.types import Signal, CheckpointStatus, Confidence, Priority


@dataclass
class CheckpointResult:
    """Result from creating a checkpoint."""
    id: str
    content_hash: str


@dataclass
class EnvelopeResult:
    """Result from sending an envelope."""
    id: str


class Agent:
    """A connected WACP agent.

    Created via ``Agent.connect()``. Provides the full agent API.
    """

    def __init__(
        self,
        channel: Channel,
        stub: AgentServiceStub,
        bind_response: BindResponse,
        workspace_id: str,
    ):
        self._channel = channel
        self._stub = stub
        self._bind = bind_response
        self._workspace_id = workspace_id

    @classmethod
    async def connect(
        cls,
        runtime_url: str,
        workspace_id: str,
        auth_token: str,
        *,
        port: int = 9400,
    ) -> "Agent":
        """Connect to the runtime and bind to a workspace.

        Args:
            runtime_url: Hostname or IP of the runtime (without port).
            workspace_id: The workspace to bind to.
            auth_token: Authentication credential.
            port: gRPC port (default 9400).
        """
        channel = Channel(host=runtime_url, port=port)
        stub = AgentServiceStub(channel)

        bind_response = await stub.bind(
            BindRequest(
                workspace_id=workspace_id,
                auth_token=auth_token,
                client_request_id="",
            )
        )

        return cls(
            channel=channel,
            stub=stub,
            bind_response=bind_response,
            workspace_id=workspace_id,
        )

    # --- Properties from bind response ---

    @property
    def workspace_id(self) -> str:
        return self._workspace_id

    @property
    def directive(self) -> Optional[Envelope]:
        return self._bind.directive

    @property
    def context(self) -> bytes:
        return self._bind.context

    @property
    def role(self) -> str:
        return self._bind.role

    @property
    def visibility(self) -> list[str]:
        return list(self._bind.visibility)

    @property
    def authority(self) -> list[str]:
        return list(self._bind.authority)

    # --- Signal emission ---

    async def signal(
        self,
        signal_type: str,
        reason: str = "",
        context: bytes = b"",
    ) -> None:
        """Emit a signal declaring a state change.

        Args:
            signal_type: One of Signal.READY, Signal.STARTED, etc.
            reason: Required for BLOCKED and FAILED.
            context: Required for ESCALATION.
        """
        await self._stub.emit_signal(
            EmitSignalRequest(
                type=Signal.to_proto(signal_type),
                reason=reason,
                context=context,
                client_request_id="",
            )
        )

    # --- Checkpoint creation ---

    async def checkpoint(
        self,
        payload: bytes,
        type: str = "artifact",
        intent: str = "",
        status: str = "provisional",
        confidence: str = "high",
        resource_usage: Optional[ResourceUsage] = None,
    ) -> CheckpointResult:
        """Create a checkpoint recording progress.

        Args:
            payload: The work product.
            type: "artifact", "observation", or taxonomy-registered.
            intent: Why this checkpoint exists.
            status: "provisional" or "final".
            confidence: "high", "medium", or "low".
            resource_usage: Optional token/time usage.
        """
        response: CreateCheckpointResponse = await self._stub.create_checkpoint(
            CreateCheckpointRequest(
                type=type,
                payload=payload,
                intent=intent,
                status=CheckpointStatus.to_proto(status),
                confidence=Confidence.to_proto(confidence),
                resource_usage=resource_usage,
                client_request_id="",
            )
        )
        return CheckpointResult(
            id=response.checkpoint_id,
            content_hash=response.content_hash,
        )

    # --- Envelope sending ---

    async def send_envelope(
        self,
        to: str,
        type: str,
        payload: bytes = b"",
        in_reply_to: str = "",
        priority: str = "normal",
    ) -> EnvelopeResult:
        """Send an envelope to another workspace.

        Args:
            to: Target workspace ID.
            type: "query", or taxonomy-registered type.
            payload: The message content.
            in_reply_to: Envelope ID this responds to.
            priority: "normal", "urgent", or "blocking".
        """
        response: SendEnvelopeResponse = await self._stub.send_envelope(
            SendEnvelopeRequest(
                to_workspace=to,
                type=type,
                payload=payload,
                in_reply_to=in_reply_to,
                priority=Priority.to_proto(priority),
                client_request_id="",
            )
        )
        return EnvelopeResult(id=response.envelope_id)

    # --- Trail query ---

    async def query_trail(
        self,
        workspace_id: str = "",
        event_type: str = "",
        limit: int = 100,
    ) -> list[TrailEntry]:
        """Query the local trail."""
        response: QueryTrailResponse = await self._stub.query_trail(
            QueryTrailRequest(
                workspace_id=workspace_id,
                event_type=event_type,
                from_=None,
                to=None,
                limit=limit,
                client_request_id="",
            )
        )
        return list(response.entries)

    # --- Streams ---

    @property
    async def inbox(self) -> AsyncIterator[Envelope]:
        """Receive envelopes as they arrive (async iterator)."""
        async for envelope in self._stub.receive_envelopes(
            ReceiveEnvelopesRequest()
        ):
            yield envelope

    @property
    async def commands(self) -> AsyncIterator[Command]:
        """Receive coordinator commands (async iterator)."""
        async for command in self._stub.receive_commands(
            ReceiveCommandsRequest()
        ):
            yield command

    # --- Disconnect ---

    async def disconnect(self) -> None:
        """Close the gRPC connection."""
        self._channel.close()
