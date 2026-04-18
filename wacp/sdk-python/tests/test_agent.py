"""Tests for the WACP Python Agent SDK."""

import pytest
from unittest.mock import AsyncMock, MagicMock
from wacp import Agent, Signal, CheckpointStatus, Confidence

import wacp.v1 as pb
from wacp.agent import CheckpointResult, EnvelopeResult


def test_import():
    """Package is importable with all public symbols."""
    from wacp import Agent, Signal, CheckpointStatus, Confidence, Priority
    assert Agent is not None
    assert Signal.READY == "ready"


def test_proto_generated():
    """Generated proto types are importable."""
    from wacp.v1 import (
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


# ── Mock infrastructure (Phase T1.3) ────────────────────


async def _async_iter(items):
    """Helper: create an async generator from a list."""
    for item in items:
        yield item


def _make_bind_response(**overrides):
    """Build a BindResponse with sensible defaults."""
    defaults = dict(
        workspace_id="ws-test",
        state=2,  # ACTIVE
        role="worker",
        directive=pb.Envelope(
            id="env-directive",
            type="directive",
            payload=b"implement feature X",
        ),
        context=b"workspace context",
        visibility=["res-a", "res-b"],
        authority=["res-a"],
    )
    defaults.update(overrides)
    return pb.BindResponse(**defaults)


def _make_agent(bind_response=None):
    """Create an Agent with mocked channel and stub."""
    channel = MagicMock()
    stub = MagicMock()
    if bind_response is None:
        bind_response = _make_bind_response()

    stub.emit_signal = AsyncMock(return_value=pb.EmitSignalResponse())
    stub.create_checkpoint = AsyncMock(
        return_value=pb.CreateCheckpointResponse(
            checkpoint_id="cp-001",
            content_hash="sha256-abc123",
        )
    )
    stub.send_envelope = AsyncMock(
        return_value=pb.SendEnvelopeResponse(envelope_id="env-out-001")
    )
    stub.query_trail = AsyncMock(
        return_value=pb.QueryTrailResponse(entries=[])
    )

    agent = Agent(channel, stub, bind_response, bind_response.workspace_id)
    return agent, stub, channel


# ── Agent connect (1 test) ───────────────────────────────


@pytest.mark.asyncio
async def test_agent_connect_with_mock():
    """Agent.connect() calls bind with correct request."""
    mock_channel = MagicMock()
    mock_stub_instance = MagicMock()
    mock_stub_instance.bind = AsyncMock(return_value=_make_bind_response())

    # Patch Channel and AgentServiceStub
    import wacp.agent as agent_mod
    orig_channel = agent_mod.Channel
    orig_stub = agent_mod.AgentServiceStub

    agent_mod.Channel = lambda host, port: mock_channel
    agent_mod.AgentServiceStub = lambda ch: mock_stub_instance
    try:
        agent = await Agent.connect(
            runtime_url="127.0.0.1",
            workspace_id="ws-test",
            auth_token="tok-abc",
            port=9400,
        )
        assert agent.workspace_id == "ws-test"
        mock_stub_instance.bind.assert_called_once()
        req = mock_stub_instance.bind.call_args[0][0]
        assert req.workspace_id == "ws-test"
        assert req.auth_token == "tok-abc"
    finally:
        agent_mod.Channel = orig_channel
        agent_mod.AgentServiceStub = orig_stub


# ── Agent properties (6 tests) ──────────────────────────


def test_agent_workspace_id():
    agent, _, _ = _make_agent()
    assert agent.workspace_id == "ws-test"


def test_agent_directive_present():
    agent, _, _ = _make_agent()
    d = agent.directive
    assert d
    assert d.id == "env-directive"
    assert d.type == "directive"


def test_agent_directive_absent():
    bind = _make_bind_response(directive=None)
    agent, _, _ = _make_agent(bind)
    assert not agent.directive


def test_agent_context():
    agent, _, _ = _make_agent()
    assert agent.context == b"workspace context"


def test_agent_role():
    agent, _, _ = _make_agent()
    assert agent.role == "worker"


def test_agent_visibility_and_authority():
    agent, _, _ = _make_agent()
    assert agent.visibility == ["res-a", "res-b"]
    assert agent.authority == ["res-a"]


# ── Agent.signal() (5 tests) ────────────────────────────


@pytest.mark.asyncio
async def test_agent_signal_ready():
    agent, stub, _ = _make_agent()
    await agent.signal("ready")
    stub.emit_signal.assert_called_once()
    req = stub.emit_signal.call_args[0][0]
    assert req.type == 1  # Signal.to_proto("ready")


@pytest.mark.asyncio
async def test_agent_signal_blocked_with_reason():
    agent, stub, _ = _make_agent()
    await agent.signal("blocked", reason="waiting for input")
    req = stub.emit_signal.call_args[0][0]
    assert req.type == 3  # blocked
    assert req.reason == "waiting for input"


@pytest.mark.asyncio
async def test_agent_signal_failed_with_reason():
    agent, stub, _ = _make_agent()
    await agent.signal("failed", reason="out of memory")
    req = stub.emit_signal.call_args[0][0]
    assert req.type == 6  # failed
    assert req.reason == "out of memory"


@pytest.mark.asyncio
async def test_agent_signal_escalation_with_context():
    agent, stub, _ = _make_agent()
    await agent.signal("escalation", context=b"need human review")
    req = stub.emit_signal.call_args[0][0]
    assert req.type == 9  # escalation
    assert req.context == b"need human review"


@pytest.mark.asyncio
async def test_agent_signal_remaining_types():
    """Checkpoint, integrate, acknowledged, suspend, migrate all succeed."""
    agent, stub, _ = _make_agent()
    types_and_values = [
        ("started", 2), ("checkpoint", 4), ("integrate", 7),
        ("acknowledged", 8), ("suspend", 10), ("migrate", 11),
    ]
    for signal_name, expected_proto in types_and_values:
        stub.emit_signal.reset_mock()
        await agent.signal(signal_name)
        req = stub.emit_signal.call_args[0][0]
        assert req.type == expected_proto, f"{signal_name} should be {expected_proto}"


# ── Agent.checkpoint() (2 tests) ────────────────────────


@pytest.mark.asyncio
async def test_agent_checkpoint_basic():
    agent, stub, _ = _make_agent()
    result = await agent.checkpoint(b"output data")
    assert isinstance(result, CheckpointResult)
    assert result.id == "cp-001"
    assert result.content_hash == "sha256-abc123"
    stub.create_checkpoint.assert_called_once()


@pytest.mark.asyncio
async def test_agent_checkpoint_all_options():
    agent, stub, _ = _make_agent()
    usage = pb.ResourceUsage(tokens=500, wall_time_ms=1000)
    await agent.checkpoint(
        b"metrics",
        type="observation",
        intent="performance record",
        status="final",
        confidence="medium",
        resource_usage=usage,
    )
    req = stub.create_checkpoint.call_args[0][0]
    assert req.type == "observation"
    assert req.payload == b"metrics"
    assert req.intent == "performance record"
    assert req.status == 2   # final
    assert req.confidence == 2  # medium
    assert req.resource_usage.tokens == 500


# ── Agent.send_envelope() (2 tests) ─────────────────────


@pytest.mark.asyncio
async def test_agent_send_envelope_basic():
    agent, stub, _ = _make_agent()
    result = await agent.send_envelope(to="ws-target", type="feedback")
    assert isinstance(result, EnvelopeResult)
    assert result.id == "env-out-001"
    stub.send_envelope.assert_called_once()


@pytest.mark.asyncio
async def test_agent_send_envelope_all_fields():
    agent, stub, _ = _make_agent()
    await agent.send_envelope(
        to="ws-target",
        type="query",
        payload=b"what is the status?",
        in_reply_to="env-prev-1",
        priority="urgent",
    )
    req = stub.send_envelope.call_args[0][0]
    assert req.to_workspace == "ws-target"
    assert req.type == "query"
    assert req.payload == b"what is the status?"
    assert req.in_reply_to == "env-prev-1"
    assert req.priority == 2  # urgent


# ── Agent.query_trail() (2 tests) ───────────────────────


@pytest.mark.asyncio
async def test_agent_query_trail_with_filters():
    agent, stub, _ = _make_agent()
    stub.query_trail = AsyncMock(
        return_value=pb.QueryTrailResponse(
            entries=[pb.TrailEntry(id="te-1", event_type="signal_emitted")]
        )
    )
    entries = await agent.query_trail(
        workspace_id="ws-test", event_type="signal_emitted", limit=10
    )
    assert len(entries) == 1
    assert entries[0].id == "te-1"
    req = stub.query_trail.call_args[0][0]
    assert req.workspace_id == "ws-test"
    assert req.event_type == "signal_emitted"
    assert req.limit == 10


@pytest.mark.asyncio
async def test_agent_query_trail_empty():
    agent, _, _ = _make_agent()
    entries = await agent.query_trail()
    assert entries == []


# ── Agent.inbox / commands (2 tests) ────────────────────


@pytest.mark.asyncio
async def test_agent_inbox_receives():
    agent, stub, _ = _make_agent()
    stub.receive_envelopes = lambda *a, **kw: _async_iter([
        pb.Envelope(id="env-1", type="feedback"),
        pb.Envelope(id="env-2", type="query"),
    ])
    envelopes = []
    async for env in agent.inbox:
        envelopes.append(env)
    assert len(envelopes) == 2
    assert envelopes[0].id == "env-1"
    assert envelopes[1].id == "env-2"


@pytest.mark.asyncio
async def test_agent_commands_receives():
    agent, stub, _ = _make_agent()
    stub.receive_commands = lambda *a, **kw: _async_iter([
        pb.Command(visibility_grant=pb.VisibilityGrant(resource_ids=["res-c"])),
    ])
    commands = []
    async for cmd in agent.commands:
        commands.append(cmd)
    assert len(commands) == 1


# ── Agent.disconnect() (1 test) ─────────────────────────


@pytest.mark.asyncio
async def test_agent_disconnect():
    agent, _, channel = _make_agent()
    await agent.disconnect()
    channel.close.assert_called_once()


# ── Error handling (1 test) ─────────────────────────────


@pytest.mark.asyncio
async def test_agent_signal_invalid_type():
    agent, _, _ = _make_agent()
    with pytest.raises(ValueError, match="unknown signal"):
        await agent.signal("nonexistent")
