"""Integration: stream handling — inbox + commands async iteration (3 tests)

Tests that the Agent's async iterator properties correctly yield
items from the mock stub's async generators.
"""

import pytest
from unittest.mock import MagicMock, AsyncMock

import wacp.v1 as pb
from wacp.agent import Agent


async def _async_iter(items):
    """Helper: create an async generator from a list."""
    for item in items:
        yield item


def _make_agent(stub_overrides=None):
    channel = MagicMock()
    stub = MagicMock()
    stub.emit_signal = AsyncMock(return_value=pb.EmitSignalResponse())

    bind_resp = pb.BindResponse(
        workspace_id="ws-stream",
        state=2,
        role="worker",
        visibility=["res-a"],
        authority=[],
    )

    if stub_overrides:
        for k, v in stub_overrides.items():
            setattr(stub, k, v)

    return Agent(channel, stub, bind_resp, "ws-stream"), stub, channel


@pytest.mark.asyncio
async def test_inbox_receives_envelopes():
    """Inbox yields envelopes from the stub's receive_envelopes stream."""
    envelopes = [
        pb.Envelope(id="env-1", type="feedback", payload=b"ok"),
        pb.Envelope(id="env-2", type="query", payload=b"?"),
    ]
    agent, _, _ = _make_agent({
        "receive_envelopes": lambda *a, **kw: _async_iter(envelopes),
    })

    received = []
    async for env in agent.inbox:
        received.append(env)
    assert len(received) == 2
    assert received[0].id == "env-1"
    assert received[1].id == "env-2"


@pytest.mark.asyncio
async def test_commands_receives_visibility_grant():
    """Commands yields coordinator commands from receive_commands stream."""
    commands = [
        pb.Command(visibility_grant=pb.VisibilityGrant(resource_ids=["res-b", "res-c"])),
    ]
    agent, _, _ = _make_agent({
        "receive_commands": lambda *a, **kw: _async_iter(commands),
    })

    received = []
    async for cmd in agent.commands:
        received.append(cmd)
    assert len(received) == 1
    assert received[0].visibility_grant.resource_ids == ["res-b", "res-c"]


@pytest.mark.asyncio
async def test_streams_end_on_empty():
    """Streams complete cleanly when the server sends no items."""
    agent, _, _ = _make_agent({
        "receive_envelopes": lambda *a, **kw: _async_iter([]),
        "receive_commands": lambda *a, **kw: _async_iter([]),
    })

    inbox_items = [env async for env in agent.inbox]
    assert inbox_items == []

    cmd_items = [cmd async for cmd in agent.commands]
    assert cmd_items == []
