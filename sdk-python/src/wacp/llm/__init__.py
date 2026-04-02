"""WACP LLM Adapter — Python bindings.

Provider-agnostic LLM inference with raw HTTP (no SDK dependencies).
"""

from dataclasses import dataclass, field
from typing import Optional, AsyncIterator, Protocol
import json


@dataclass
class Message:
    """A message in a conversation."""
    role: str  # "system", "user", "assistant", "tool"
    content: str
    tool_call_id: Optional[str] = None


@dataclass
class ToolCall:
    """A tool call from the LLM."""
    id: str
    name: str
    arguments: dict


@dataclass
class TokenUsage:
    """Token usage: input + output."""
    input_tokens: int = 0
    output_tokens: int = 0

    @property
    def total(self) -> int:
        return self.input_tokens + self.output_tokens


@dataclass
class CompletionResult:
    """Result of a completion request."""
    content: str
    tool_calls: list[ToolCall] = field(default_factory=list)
    usage: TokenUsage = field(default_factory=TokenUsage)
    model: str = ""
    truncated: bool = False


@dataclass
class LlmConfig:
    """LLM provider configuration."""
    provider: str  # "anthropic", "openai", "generic"
    api_key: str
    model: str = ""
    base_url: Optional[str] = None
    max_tokens: int = 4096
    temperature: float = 0.0


class LlmAdapter(Protocol):
    """Provider-agnostic LLM adapter protocol."""

    async def complete(
        self, messages: list[Message], config: LlmConfig
    ) -> CompletionResult: ...


class AnthropicAdapter:
    """Anthropic Claude adapter — raw HTTP via httpx or urllib."""

    async def complete(
        self, messages: list[Message], config: LlmConfig
    ) -> CompletionResult:
        import urllib.request

        url = f"{config.base_url or 'https://api.anthropic.com'}/v1/messages"

        system = "\n\n".join(m.content for m in messages if m.role == "system")
        api_messages = [
            {"role": m.role, "content": m.content}
            for m in messages if m.role != "system"
        ]

        body = {
            "model": config.model,
            "max_tokens": config.max_tokens,
            "messages": api_messages,
        }
        if system:
            body["system"] = system
        if config.temperature is not None:
            body["temperature"] = config.temperature

        req = urllib.request.Request(
            url,
            data=json.dumps(body).encode(),
            headers={
                "content-type": "application/json",
                "x-api-key": config.api_key,
                "anthropic-version": "2023-06-01",
            },
        )

        with urllib.request.urlopen(req) as resp:
            data = json.loads(resp.read())

        content = ""
        tool_calls = []
        for block in data.get("content", []):
            if block.get("type") == "text":
                content += block.get("text", "")
            elif block.get("type") == "tool_use":
                tool_calls.append(ToolCall(
                    id=block["id"],
                    name=block["name"],
                    arguments=block.get("input", {}),
                ))

        usage_data = data.get("usage", {})
        return CompletionResult(
            content=content,
            tool_calls=tool_calls,
            usage=TokenUsage(
                input_tokens=usage_data.get("input_tokens", 0),
                output_tokens=usage_data.get("output_tokens", 0),
            ),
            model=data.get("model", ""),
            truncated=data.get("stop_reason") == "max_tokens",
        )


class OpenAiAdapter:
    """OpenAI adapter — raw HTTP."""

    async def complete(
        self, messages: list[Message], config: LlmConfig
    ) -> CompletionResult:
        import urllib.request

        url = f"{config.base_url or 'https://api.openai.com'}/v1/chat/completions"

        api_messages = [{"role": m.role, "content": m.content} for m in messages]
        body: dict = {"model": config.model, "messages": api_messages}
        if config.max_tokens:
            body["max_completion_tokens"] = config.max_tokens
        if config.temperature is not None:
            body["temperature"] = config.temperature

        req = urllib.request.Request(
            url,
            data=json.dumps(body).encode(),
            headers={
                "content-type": "application/json",
                "authorization": f"Bearer {config.api_key}",
            },
        )

        with urllib.request.urlopen(req) as resp:
            data = json.loads(resp.read())

        choice = data.get("choices", [{}])[0]
        message = choice.get("message", {})
        content = message.get("content", "") or ""

        tool_calls = []
        for tc in message.get("tool_calls", []):
            fn = tc.get("function", {})
            tool_calls.append(ToolCall(
                id=tc.get("id", ""),
                name=fn.get("name", ""),
                arguments=json.loads(fn.get("arguments", "{}")),
            ))

        usage_data = data.get("usage", {})
        return CompletionResult(
            content=content,
            tool_calls=tool_calls,
            usage=TokenUsage(
                input_tokens=usage_data.get("prompt_tokens", 0),
                output_tokens=usage_data.get("completion_tokens", 0),
            ),
            model=data.get("model", ""),
            truncated=choice.get("finish_reason") == "length",
        )
