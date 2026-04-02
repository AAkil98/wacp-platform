"""Tests for wacp.coordinator Python bindings."""

import pytest
from wacp.coordinator import (
    CoordinatorContext, TaskDefinition, WorkspaceHandle,
    TaskView, SignalEvent, IntegrationResult,
)


def test_task_definition_fields():
    task = TaskDefinition(
        name="implement auth",
        description="Implement authentication",
        depends_on=["plan-1"],
        role="swe:implementer",
        tools=["read_file", "write_file"],
    )
    assert task.name == "implement auth"
    assert len(task.depends_on) == 1
    assert len(task.tools) == 2


def test_workspace_handle():
    handle = WorkspaceHandle(workspace_id="ws-1", task_id="task-1")
    assert handle.workspace_id == "ws-1"


def test_task_view():
    view = TaskView(task_id="t-1", name="plan", status="pending")
    assert view.assigned_workspace is None


def test_signal_event():
    event = SignalEvent(workspace_id="ws-child", signal_type="complete")
    assert event.reason == ""


def test_integration_result():
    result = IntegrationResult(result="accepted", detail="all checks passed")
    assert result.result == "accepted"


def test_coordinator_context_creation():
    ctx = CoordinatorContext(runtime_url="http://localhost:9402")
    assert ctx.connected is True


@pytest.mark.asyncio
async def test_coordinator_context_close():
    ctx = CoordinatorContext(runtime_url="http://localhost:9402")
    await ctx.close()
    assert ctx.connected is False


@pytest.mark.asyncio
async def test_coordinator_methods_raise_not_implemented():
    ctx = CoordinatorContext(runtime_url="http://localhost:9402")
    with pytest.raises(NotImplementedError):
        await ctx.submit_goal("test")
    with pytest.raises(NotImplementedError):
        await ctx.decompose([])
    with pytest.raises(NotImplementedError):
        await ctx.ready_tasks()
