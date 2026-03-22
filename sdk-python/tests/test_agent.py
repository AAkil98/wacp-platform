"""Tests for the WACP Python Agent SDK."""

import pytest
from wacp import Agent, Signal, CheckpointStatus, Confidence


def test_import():
    """Package is importable with all public symbols."""
    from wacp import Agent, Signal, CheckpointStatus, Confidence, Priority
    assert Agent is not None
    assert Signal.READY == "ready"


def test_proto_generated():
    """Generated proto types are importable."""
    from wacp.proto.v1 import (
        SignalType,
        WorkspaceState,
        TaskStatus,
        BindRequest,
        EmitSignalRequest,
        Envelope,
    )
    assert SignalType.SIGNAL_TYPE_READY == 1
    assert WorkspaceState.WORKSPACE_STATE_IDLE == 1
    assert TaskStatus.TASK_STATUS_DRAFT == 1

    # Proto messages are constructable.
    req = BindRequest(workspace_id="ws-1", auth_token="tok", client_request_id="")
    assert req.workspace_id == "ws-1"


@pytest.mark.asyncio
async def test_connect_fails_on_no_server():
    """Connecting to a non-existent server raises an error."""
    with pytest.raises(Exception):
        await Agent.connect(
            runtime_url="127.0.0.1",
            workspace_id="ws-nonexistent",
            auth_token="bad-token",
            port=1,  # nothing listening
        )
