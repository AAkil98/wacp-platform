"""Tests for wacp.llm Python bindings."""

from wacp.llm import Message, ToolCall, TokenUsage, CompletionResult, LlmConfig


def test_message_construction():
    msg = Message(role="user", content="hello")
    assert msg.role == "user"
    assert msg.content == "hello"
    assert msg.tool_call_id is None


def test_tool_call_fields():
    tc = ToolCall(id="call_1", name="read_file", arguments={"path": "/tmp"})
    assert tc.name == "read_file"
    assert tc.arguments["path"] == "/tmp"


def test_token_usage_total():
    usage = TokenUsage(input_tokens=100, output_tokens=50)
    assert usage.total == 150


def test_token_usage_zero():
    usage = TokenUsage()
    assert usage.total == 0


def test_completion_result_fields():
    result = CompletionResult(
        content="Hello!",
        tool_calls=[ToolCall(id="1", name="test", arguments={})],
        usage=TokenUsage(input_tokens=10, output_tokens=5),
        model="claude-sonnet-4",
    )
    assert result.content == "Hello!"
    assert len(result.tool_calls) == 1
    assert result.usage.total == 15
    assert not result.truncated


def test_llm_config():
    config = LlmConfig(
        provider="anthropic",
        api_key="sk-test",
        model="claude-sonnet-4-20250514",
    )
    assert config.provider == "anthropic"
    assert config.max_tokens == 4096
    assert config.temperature == 0.0
