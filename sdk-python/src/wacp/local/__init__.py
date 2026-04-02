"""WACP Local SDK — Python bindings.

Session management, autonomy, and local resources for co-located agents.
"""

from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Optional
import asyncio
import subprocess


class SessionState(Enum):
    OPEN = "open"
    ACTIVE = "active"
    SUSPENDED = "suspended"
    CLOSED = "closed"


class AutonomyPreset(Enum):
    SUPERVISED = "supervised"
    ASSISTED = "assisted"
    AUTONOMOUS = "autonomous"


PRESET_DEFAULTS: dict[AutonomyPreset, set[str]] = {
    AutonomyPreset.SUPERVISED: {"llm_call"},
    AutonomyPreset.ASSISTED: {"file_read", "git_read", "web_fetch", "llm_call"},
    AutonomyPreset.AUTONOMOUS: {
        "file_read", "file_write", "shell_exec",
        "git_read", "git_write", "web_fetch", "llm_call",
    },
}

VALID_TRANSITIONS: list[tuple[SessionState, SessionState]] = [
    (SessionState.OPEN, SessionState.ACTIVE),
    (SessionState.ACTIVE, SessionState.SUSPENDED),
    (SessionState.SUSPENDED, SessionState.ACTIVE),
    (SessionState.ACTIVE, SessionState.CLOSED),
    (SessionState.SUSPENDED, SessionState.CLOSED),
    (SessionState.OPEN, SessionState.CLOSED),
]


class AutonomyManager:
    """Dynamic trust surface for operation gating."""

    def __init__(self, preset: AutonomyPreset):
        self._trusted: set[str] = set(PRESET_DEFAULTS[preset])
        self._always: set[str] = set()

    def check(self, op: str) -> bool:
        return op in self._trusted

    def grant(self, op: str) -> None:
        self._trusted.add(op)

    def grant_always(self, op: str) -> None:
        self._trusted.add(op)
        self._always.add(op)

    def revoke(self, op: str) -> None:
        if op not in self._always:
            self._trusted.discard(op)

    def trust_surface(self) -> set[str]:
        return set(self._trusted)

    def reset_to_preset(self, preset: AutonomyPreset) -> None:
        self._trusted = set(PRESET_DEFAULTS[preset]) | self._always


class AutonomyError(Exception):
    """Raised when an operation is blocked by autonomy."""
    def __init__(self, operation: str):
        self.operation = operation
        super().__init__(f"Operation '{operation}' requires human approval")


class LocalResources:
    """Local filesystem, shell, and git access with autonomy gating."""

    def __init__(self, working_dir: str, autonomy: AutonomyManager):
        self._working_dir = Path(working_dir).resolve()
        self._autonomy = autonomy

    def _require(self, op: str) -> None:
        if not self._autonomy.check(op):
            raise AutonomyError(op)

    def _resolve(self, path: str) -> Path:
        resolved = (self._working_dir / path).resolve()
        if not str(resolved).startswith(str(self._working_dir)):
            raise AutonomyError("file_read")
        return resolved

    async def read_file(self, path: str) -> str:
        self._require("file_read")
        return self._resolve(path).read_text()

    async def write_file(self, path: str, content: str) -> None:
        self._require("file_write")
        target = self._resolve(path)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)

    async def exec(self, command: str, timeout: int = 30) -> tuple[str, str, int]:
        self._require("shell_exec")
        proc = await asyncio.create_subprocess_shell(
            command,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=str(self._working_dir),
        )
        stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout)
        return (
            stdout.decode() if stdout else "",
            stderr.decode() if stderr else "",
            proc.returncode or 0,
        )

    async def git_status(self) -> str:
        self._require("git_read")
        stdout, _, _ = await self.exec("git status --porcelain")
        return stdout


@dataclass
class SessionContext:
    """Cross-task continuity within a session."""
    history: list[str] = field(default_factory=list)
    trust_log: list[dict] = field(default_factory=list)
    metadata: dict = field(default_factory=dict)

    def add_history(self, entry: str) -> None:
        self.history.append(entry)

    def record_trust(self, operation: str, granted: bool) -> None:
        self.trust_log.append({"operation": operation, "granted": granted})


class LocalSession:
    """A co-located human-agent session."""

    _counter = 0

    def __init__(self, working_dir: str, preset: AutonomyPreset):
        LocalSession._counter += 1
        self.id = f"session-{LocalSession._counter}"
        self._state = SessionState.OPEN
        self.autonomy = AutonomyManager(preset)
        self.resources = LocalResources(working_dir, self.autonomy)
        self.context = SessionContext()

    @property
    def state(self) -> SessionState:
        return self._state

    async def activate(self) -> None:
        self._transition(SessionState.ACTIVE)

    async def suspend(self) -> None:
        self._transition(SessionState.SUSPENDED)

    async def close(self) -> None:
        self._transition(SessionState.CLOSED)

    def _transition(self, to: SessionState) -> None:
        if self._state == SessionState.CLOSED:
            raise RuntimeError("Session is already closed")
        if (self._state, to) not in VALID_TRANSITIONS:
            raise RuntimeError(f"Cannot transition from {self._state.value} to {to.value}")
        self._state = to
