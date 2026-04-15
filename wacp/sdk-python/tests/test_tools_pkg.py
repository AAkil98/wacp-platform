"""Tests for wacp.tools Python bindings."""

import pytest
from wacp.tools import ToolDescriptor, Capability, ToolPackage, ToolContext, ToolError, tool


def test_descriptor_valid():
    desc = ToolDescriptor(
        name="read_file",
        version="1.0.0",
        description="Read a file",
        capabilities=[Capability(
            name="read",
            description="Read file contents",
            input_schema={"type": "object"},
            output_schema={"type": "object"},
        )],
    )
    assert desc.validate() == []


def test_descriptor_invalid_name():
    desc = ToolDescriptor(name="ReadFile", version="1.0.0", description="x", capabilities=[
        Capability(name="read", description="x", input_schema={"type": "object"}, output_schema={"type": "object"}),
    ])
    errors = desc.validate()
    assert any("name" in e.lower() for e in errors)


def test_descriptor_invalid_version():
    desc = ToolDescriptor(name="tool", version="bad", description="x", capabilities=[
        Capability(name="run", description="x", input_schema={"type": "object"}, output_schema={"type": "object"}),
    ])
    assert any("version" in e.lower() for e in desc.validate())


def test_descriptor_no_capabilities():
    desc = ToolDescriptor(name="tool", version="1.0.0", description="x", capabilities=[])
    assert any("capability" in e.lower() for e in desc.validate())


def test_descriptor_duplicate_capabilities():
    cap = Capability(name="run", description="x", input_schema={"type": "object"}, output_schema={"type": "object"})
    desc = ToolDescriptor(name="tool", version="1.0.0", description="x", capabilities=[cap, cap])
    assert any("duplicate" in e.lower() for e in desc.validate())


def test_package_valid():
    cap = Capability(name="run", description="x", input_schema={"type": "object"}, output_schema={"type": "object"})
    desc = ToolDescriptor(name="tool", version="1.0.0", description="x", capabilities=[cap])
    pkg = ToolPackage(descriptor=desc, handlers={"run": lambda ctx, args: {}})
    assert pkg.validate() == []


def test_package_missing_handler():
    cap = Capability(name="run", description="x", input_schema={"type": "object"}, output_schema={"type": "object"})
    desc = ToolDescriptor(name="tool", version="1.0.0", description="x", capabilities=[cap])
    pkg = ToolPackage(descriptor=desc, handlers={})
    assert any("missing" in e.lower() for e in pkg.validate())


def test_package_extra_handler():
    cap = Capability(name="run", description="x", input_schema={"type": "object"}, output_schema={"type": "object"})
    desc = ToolDescriptor(name="tool", version="1.0.0", description="x", capabilities=[cap])
    pkg = ToolPackage(descriptor=desc, handlers={"run": lambda c, a: {}, "extra": lambda c, a: {}})
    assert any("extra" in e.lower() for e in pkg.validate())


def test_tool_decorator():
    @tool(name="echo", description="Echo args", input_schema={"type": "object"})
    async def echo_handler(ctx, args):
        return args

    assert isinstance(echo_handler, ToolPackage)
    assert echo_handler.descriptor.name == "echo"
    assert "echo" in echo_handler.handlers


def test_tool_error_display():
    err = ToolError(code="timeout", message="took too long")
    assert str(err) == "timeout: took too long"


def test_tool_context_fields():
    ctx = ToolContext(tool_name="test", capability_name="run")
    assert ctx.workspace_id is None
    assert ctx.timeout_ms == 30_000
