"""WACP Coordinator SDK — Python bindings.

Async client for driving WACP coordination via gRPC.
"""

from dataclasses import dataclass, field
from typing import Optional, AsyncIterator


@dataclass
class TaskDefinition:
    """Definition for a task in the task graph."""
    name: str
    description: str
    depends_on: list[str] = field(default_factory=list)
    role: str = "worker"
    directive_payload: bytes = b""
    tools: list[str] = field(default_factory=list)


@dataclass
class WorkspaceHandle:
    """Handle to a dispatched workspace."""
    workspace_id: str
    task_id: str


@dataclass
class TaskView:
    """View of a task's current state."""
    task_id: str
    name: str
    status: str
    assigned_workspace: Optional[str] = None
    depends_on: list[str] = field(default_factory=list)


@dataclass
class SignalEvent:
    """A signal from a child workspace."""
    workspace_id: str
    signal_type: str
    reason: str = ""
    timestamp: int = 0


@dataclass
class IntegrationResult:
    """Result of integration pipeline."""
    result: str  # "accepted", "revised", "rejected"
    detail: str = ""


class CoordinatorContext:
    """Client-facing coordinator API.

    Drives the runtime's coordinator via gRPC. In this initial Python
    implementation, methods are typed stubs that define the contract.
    The gRPC wiring uses grpclib or betterproto generated client.
    """

    def __init__(self, runtime_url: str):
        self._url = runtime_url
        # gRPC channel would be established here in production
        self._connected = True

    @property
    def connected(self) -> bool:
        return self._connected

    async def submit_goal(
        self, description: str, context: bytes = b""
    ) -> tuple[str, str]:
        """Submit a goal. Returns (goal_id, root_workspace_id)."""
        # In production: call SubmitGoal RPC
        raise NotImplementedError("Requires gRPC connection to runtime")

    async def decompose(self, tasks: list[TaskDefinition]) -> list[str]:
        """Create task graph. Returns assigned task IDs."""
        raise NotImplementedError("Requires gRPC connection to runtime")

    async def ready_tasks(self) -> list[TaskView]:
        """Get tasks ready for dispatch."""
        raise NotImplementedError("Requires gRPC connection to runtime")

    async def dispatch(
        self, task_id: str, role: str, directive: bytes = b"", tools: list[str] | None = None
    ) -> WorkspaceHandle:
        """Create workspace for a task."""
        raise NotImplementedError("Requires gRPC connection to runtime")

    async def abort(self, workspace_id: str, reason: str = "") -> None:
        """Terminate a workspace."""
        raise NotImplementedError("Requires gRPC connection to runtime")

    async def suspend(self, workspace_id: str) -> None:
        """Pause a workspace."""
        raise NotImplementedError("Requires gRPC connection to runtime")

    async def resume(self, workspace_id: str) -> None:
        """Resume a paused workspace."""
        raise NotImplementedError("Requires gRPC connection to runtime")

    async def send_directive(self, workspace_id: str, payload: bytes) -> str:
        """Send directive envelope. Returns envelope ID."""
        raise NotImplementedError("Requires gRPC connection to runtime")

    async def feedback(
        self, workspace_id: str, payload: bytes, in_reply_to: str | None = None
    ) -> str:
        """Send feedback envelope. Returns envelope ID."""
        raise NotImplementedError("Requires gRPC connection to runtime")

    async def integrate(self, workspace_id: str) -> IntegrationResult:
        """Trigger integration pipeline."""
        raise NotImplementedError("Requires gRPC connection to runtime")

    async def close(self) -> None:
        """Close the gRPC connection."""
        self._connected = False
