"""Integration: error propagation — server errors → agent exceptions (2 tests)

Tests that gRPC errors from the stub propagate correctly through
the Agent methods as Python exceptions.
"""

import pytest
from unittest.mock import MagicMock, AsyncMock

import wacp.proto.v1 as pb
from wacp.agent import Agent


def _make_agent():
    channel = MagicMock()
    stub = MagicMock()

    bind_resp = pb.BindResponse(
        workspace_id="ws-err",
        state=2,
        role="worker",
        visibility=[],
        authority=[],
    )

    stub.emit_signal = AsyncMock(return_value=pb.EmitSignalResponse())
    stub.create_checkpoint = AsyncMock(
        return_value=pb.CreateCheckpointResponse(
            checkpoint_id="cp-1",
            content_hash="hash",
        )
    )

    return Agent(channel, stub, bind_resp, "ws-err"), stub


@pytest.mark.asyncio
async def test_signal_rpc_error_propagates():
    """When emit_signal raises, Agent.signal() propagates the exception."""
    agent, stub = _make_agent()
    stub.emit_signal = AsyncMock(side_effect=Exception("server unavailable"))

    with pytest.raises(Exception, match="server unavailable"):
        await agent.signal("ready")


@pytest.mark.asyncio
async def test_checkpoint_rpc_error_propagates():
    """When create_checkpoint raises, Agent.checkpoint() propagates the exception."""
    agent, stub = _make_agent()
    stub.create_checkpoint = AsyncMock(side_effect=Exception("budget exceeded"))

    with pytest.raises(Exception, match="budget exceeded"):
        await agent.checkpoint(b"data", type="artifact")
