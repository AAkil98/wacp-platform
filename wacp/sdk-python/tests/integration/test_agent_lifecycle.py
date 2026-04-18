"""Integration: agent lifecycle — connect → signal → checkpoint → disconnect (3 tests)

Uses a mock gRPC stub to verify the full agent lifecycle sequence
without a running server.
"""

import pytest
from unittest.mock import AsyncMock, MagicMock

import wacp.v1 as pb
from wacp.agent import Agent, CheckpointResult, EnvelopeResult


def _make_agent():
    """Create an Agent with mocked channel and stub, fully bound."""
    channel = MagicMock()
    stub = MagicMock()

    bind_resp = pb.BindResponse(
        workspace_id="ws-lifecycle",
        state=2,  # ACTIVE
        role="worker",
        directive=pb.Envelope(
            id="env-dir",
            type="directive",
            payload=b"do work",
        ),
        context=b"ctx",
        visibility=["res-a"],
        authority=["res-a"],
    )

    stub.emit_signal = AsyncMock(return_value=pb.EmitSignalResponse())
    stub.create_checkpoint = AsyncMock(
        return_value=pb.CreateCheckpointResponse(
            checkpoint_id="cp-1",
            content_hash="sha256-abc",
        )
    )
    stub.send_envelope = AsyncMock(
        return_value=pb.SendEnvelopeResponse(envelope_id="env-out-1")
    )
    stub.query_trail = AsyncMock(
        return_value=pb.QueryTrailResponse(entries=[])
    )

    agent = Agent(channel, stub, bind_resp, "ws-lifecycle")
    return agent, stub, channel


@pytest.mark.asyncio
async def test_full_lifecycle_signal_checkpoint_disconnect():
    """Agent can signal ready, create checkpoint, signal complete, disconnect."""
    agent, stub, channel = _make_agent()

    # Signal ready
    await agent.signal("ready")
    assert stub.emit_signal.call_count == 1
    req = stub.emit_signal.call_args[0][0]
    assert req.type == 1  # READY

    # Create checkpoint
    result = await agent.checkpoint(b"output data", type="artifact", status="final")
    assert isinstance(result, CheckpointResult)
    assert result.id == "cp-1"
    assert result.content_hash == "sha256-abc"

    # Signal complete
    await agent.signal("complete")
    assert stub.emit_signal.call_count == 2
    complete_req = stub.emit_signal.call_args[0][0]
    assert complete_req.type == 5  # COMPLETE

    # Disconnect
    await agent.disconnect()
    channel.close.assert_called_once()


@pytest.mark.asyncio
async def test_lifecycle_with_envelope_exchange():
    """Agent can send an envelope as part of the lifecycle."""
    agent, stub, _ = _make_agent()

    await agent.signal("ready")
    result = await agent.send_envelope(
        to="ws-peer",
        type="feedback",
        payload=b"looks good",
        priority="urgent",
    )
    assert isinstance(result, EnvelopeResult)
    assert result.id == "env-out-1"

    req = stub.send_envelope.call_args[0][0]
    assert req.to_workspace == "ws-peer"
    assert req.type == "feedback"
    assert req.priority == 2  # URGENT


@pytest.mark.asyncio
async def test_lifecycle_query_trail():
    """Agent can query the trail during its lifecycle."""
    agent, stub, _ = _make_agent()

    stub.query_trail = AsyncMock(
        return_value=pb.QueryTrailResponse(
            entries=[
                pb.TrailEntry(id="te-1", event_type="signal_emitted"),
                pb.TrailEntry(id="te-2", event_type="checkpoint_created"),
            ]
        )
    )

    entries = await agent.query_trail(workspace_id="ws-lifecycle", limit=50)
    assert len(entries) == 2
    assert entries[0].id == "te-1"
    assert entries[1].event_type == "checkpoint_created"
