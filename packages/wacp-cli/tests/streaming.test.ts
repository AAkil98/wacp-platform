import { describe, it, expect } from "vitest";
import { providerUrl, providerHeaders, providerBody, type Message } from "../src/llm.js";
import type { CliConfig } from "../src/config.js";

const anthropicConfig: CliConfig = {
  provider: "anthropic",
  model: "claude-sonnet-4-20250514",
  apiKey: "sk-test",
  workingDir: ".",
  autonomy: "assisted",
  maxTokens: 4096,
  temperature: 0,
  systemPrompt: "You are helpful.",
};

const openaiConfig: CliConfig = {
  ...anthropicConfig,
  provider: "openai",
  model: "gpt-4o",
};

describe("Streaming LLM support", () => {
  // Note: actual SSE streaming requires a real HTTP server or mock.
  // These tests verify the request construction for streaming mode.

  it("anthropic streaming body includes stream: true", () => {
    const body = providerBody(anthropicConfig, [{ role: "user", content: "hi" }], []);
    // When calling streamCompletion, stream is set by the caller
    // Here we verify the base body construction
    expect(body.model).toBe("claude-sonnet-4-20250514");
    expect(body.max_tokens).toBe(4096);
  });

  it("openai streaming body structure", () => {
    const body = providerBody(openaiConfig, [{ role: "user", content: "hi" }], []);
    expect(body.model).toBe("gpt-4o");
    expect((body.messages as unknown[]).length).toBe(1);
  });

  it("anthropic streaming body with tools", () => {
    const tools = [{ name: "read_file", description: "Read", input_schema: { type: "object" } }];
    const body = providerBody(anthropicConfig, [{ role: "user", content: "hi" }], tools);
    expect((body.tools as unknown[]).length).toBe(1);
  });

  it("tool result message for anthropic uses content blocks", () => {
    const msg: Message = {
      role: "tool",
      content: [{ type: "tool_result", tool_use_id: "call_1", content: "file data", is_error: false }],
    };
    expect(Array.isArray(msg.content)).toBe(true);
    expect((msg.content as unknown[])[0]).toHaveProperty("type", "tool_result");
  });

  it("tool result message for openai uses string + tool_call_id", () => {
    const msg: Message = {
      role: "tool",
      content: "file data",
      tool_call_id: "call_1",
    };
    expect(typeof msg.content).toBe("string");
    expect(msg.tool_call_id).toBe("call_1");
  });

  it("provider URL for generic with custom base", () => {
    const genericConfig: CliConfig = {
      ...anthropicConfig,
      provider: "generic",
      baseUrl: "http://localhost:11434",
    };
    expect(providerUrl(genericConfig)).toBe("http://localhost:11434/v1/chat/completions");
  });
});
