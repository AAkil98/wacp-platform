"""Tests for wacp.local Python bindings."""

import os
import tempfile
import pytest
from wacp.local import (
    LocalSession, SessionState, AutonomyPreset,
    AutonomyManager, AutonomyError, LocalResources, SessionContext,
)


# --- Session lifecycle ---

def test_session_creates_open():
    session = LocalSession("/tmp", AutonomyPreset.ASSISTED)
    assert session.state == SessionState.OPEN


@pytest.mark.asyncio
async def test_session_activate():
    session = LocalSession("/tmp", AutonomyPreset.ASSISTED)
    await session.activate()
    assert session.state == SessionState.ACTIVE


@pytest.mark.asyncio
async def test_session_close():
    session = LocalSession("/tmp", AutonomyPreset.ASSISTED)
    await session.close()
    assert session.state == SessionState.CLOSED


@pytest.mark.asyncio
async def test_session_invalid_transition():
    session = LocalSession("/tmp", AutonomyPreset.ASSISTED)
    with pytest.raises(RuntimeError, match="Cannot transition"):
        await session.suspend()  # can't suspend from open


@pytest.mark.asyncio
async def test_session_closed_rejects_activate():
    session = LocalSession("/tmp", AutonomyPreset.ASSISTED)
    await session.close()
    with pytest.raises(RuntimeError, match="already closed"):
        await session.activate()


# --- Autonomy manager ---

def test_supervised_preset():
    mgr = AutonomyManager(AutonomyPreset.SUPERVISED)
    assert mgr.check("llm_call") is True
    assert mgr.check("file_read") is False


def test_assisted_preset():
    mgr = AutonomyManager(AutonomyPreset.ASSISTED)
    assert mgr.check("file_read") is True
    assert mgr.check("file_write") is False


def test_autonomous_preset():
    mgr = AutonomyManager(AutonomyPreset.AUTONOMOUS)
    assert mgr.check("file_write") is True
    assert mgr.check("shell_exec") is True
    assert mgr.check("file_delete") is False


def test_grant_and_revoke():
    mgr = AutonomyManager(AutonomyPreset.SUPERVISED)
    mgr.grant("file_read")
    assert mgr.check("file_read") is True
    mgr.revoke("file_read")
    assert mgr.check("file_read") is False


def test_grant_always_survives_revoke():
    mgr = AutonomyManager(AutonomyPreset.SUPERVISED)
    mgr.grant_always("file_read")
    mgr.revoke("file_read")
    assert mgr.check("file_read") is True


def test_reset_to_preset():
    mgr = AutonomyManager(AutonomyPreset.AUTONOMOUS)
    mgr.reset_to_preset(AutonomyPreset.SUPERVISED)
    assert mgr.check("file_write") is False
    assert mgr.check("llm_call") is True


# --- Local resources ---

@pytest.mark.asyncio
async def test_read_file_when_trusted():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = os.path.join(tmpdir, "test.txt")
        with open(path, "w") as f:
            f.write("hello")
        res = LocalResources(tmpdir, AutonomyManager(AutonomyPreset.AUTONOMOUS))
        content = await res.read_file("test.txt")
        assert content == "hello"


@pytest.mark.asyncio
async def test_read_file_denied():
    with tempfile.TemporaryDirectory() as tmpdir:
        res = LocalResources(tmpdir, AutonomyManager(AutonomyPreset.SUPERVISED))
        with pytest.raises(AutonomyError):
            await res.read_file("test.txt")


@pytest.mark.asyncio
async def test_exec_runs_command():
    with tempfile.TemporaryDirectory() as tmpdir:
        res = LocalResources(tmpdir, AutonomyManager(AutonomyPreset.AUTONOMOUS))
        stdout, stderr, code = await res.exec("echo hello")
        assert stdout.strip() == "hello"
        assert code == 0


# --- Session context ---

def test_session_context():
    ctx = SessionContext()
    ctx.add_history("user: fix bug")
    ctx.record_trust("file_read", True)
    assert len(ctx.history) == 1
    assert len(ctx.trust_log) == 1
    assert ctx.trust_log[0]["granted"] is True
