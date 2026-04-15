"""WACP Tool Framework — Python bindings.

Provides tool descriptors, packaging, and execution for Python-authored tools.
"""

from dataclasses import dataclass, field
from typing import Any, Callable, Awaitable, Optional
import json


@dataclass
class Capability:
    """A single callable operation within a tool."""
    name: str
    description: str
    input_schema: dict
    output_schema: dict
    timeout_ms: Optional[int] = None
    idempotent: bool = False
    side_effects: bool = False


@dataclass
class ToolDescriptor:
    """A tool's complete declaration."""
    name: str
    version: str
    description: str
    capabilities: list[Capability]
    config_schema: Optional[dict] = None
    tags: list[str] = field(default_factory=list)

    def validate(self) -> list[str]:
        """Validate the descriptor. Returns list of errors (empty = valid)."""
        errors = []
        if not self.name or not self.name.replace("_", "").isalnum() or not self.name.islower():
            errors.append(f"Invalid name: '{self.name}'")
        if len(self.name) > 64:
            errors.append(f"Name exceeds 64 characters")
        parts = self.version.split(".")
        if len(parts) != 3 or not all(p.isdigit() for p in parts):
            errors.append(f"Invalid version: '{self.version}'")
        if not self.capabilities:
            errors.append("Must have at least one capability")
        names = [c.name for c in self.capabilities]
        if len(names) != len(set(names)):
            errors.append("Duplicate capability names")
        return errors


@dataclass
class ToolContext:
    """Invocation context passed to every handler call."""
    tool_name: str
    capability_name: str
    workspace_id: Optional[str] = None
    config: dict = field(default_factory=dict)
    timeout_ms: int = 30_000


@dataclass
class ToolError(Exception):
    """Structured tool error."""
    code: str
    message: str
    retryable: bool = False

    def __str__(self) -> str:
        return f"{self.code}: {self.message}"


ToolHandler = Callable[[ToolContext, dict], Awaitable[dict]]


@dataclass
class ToolPackage:
    """A distributable tool unit: descriptor + handlers."""
    descriptor: ToolDescriptor
    handlers: dict[str, ToolHandler]

    def validate(self) -> list[str]:
        """Validate descriptor + handler-descriptor alignment."""
        errors = self.descriptor.validate()
        cap_names = {c.name for c in self.descriptor.capabilities}
        handler_names = set(self.handlers.keys())
        for name in cap_names - handler_names:
            errors.append(f"Missing handler for capability '{name}'")
        for name in handler_names - cap_names:
            errors.append(f"Extra handler '{name}' has no capability")
        return errors


def tool(
    name: str,
    description: str,
    input_schema: dict,
    output_schema: Optional[dict] = None,
    **kwargs: Any,
) -> Callable[[ToolHandler], ToolPackage]:
    """Decorator to create a ToolPackage from a function."""
    def decorator(fn: ToolHandler) -> ToolPackage:
        cap = Capability(
            name=name,
            description=description,
            input_schema=input_schema,
            output_schema=output_schema or {"type": "object"},
            **kwargs,
        )
        desc = ToolDescriptor(
            name=name,
            version="1.0.0",
            description=description,
            capabilities=[cap],
        )
        return ToolPackage(descriptor=desc, handlers={name: fn})
    return decorator
