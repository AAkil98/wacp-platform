import { describe, it, expect } from "vitest";
import { providerUrl, providerHeaders, providerBody, parseAnthropicResponse, parseOpenaiResponse } from "../src/llm.js";
import type { CliConfig } from "../src/config.js";

const baseConfig: CliConfig = {
  provider: "anthropic",
  model: "claude-sonnet-4-20250514",
  apiKey: "sk-test",
  workingDir: ".",
  autonomy: "assisted",
  maxTokens: 4096,
  temperature: 0,
  systemPrompt: "You are helpful.",
};

describe("LLM", () => {
  // --- Provider URL ---

  it("anthropic URL", () => {
    expect(providerUrl(baseConfig)).toBe("https://api.anthropic.com/v1/messages");
  });

  it("openai URL", () => {
    expect(providerUrl({ ...baseConfig, provider: "openai" })).toBe("https://api.openai.com/v1/chat/completions");
  });

  it("generic URL with custom base", () => {
    expect(providerUrl({ ...baseConfig, provider: "generic", baseUrl: "http://localhost:11434" })).toBe("http://localhost:11434/v1/chat/completions");
  });

  // --- Headers ---

  it("anthropic headers include x-api-key", () => {
    const headers = providerHeaders(baseConfig);
    expect(headers["x-api-key"]).toBe("sk-test");
    expect(headers["anthropic-version"]).toBeDefined();
  });

  it("openai headers include bearer token", () => {
    const headers = providerHeaders({ ...baseConfig, provider: "openai" });
    expect(headers["authorization"]).toBe("Bearer sk-test");
  });

  // --- Request body ---

  it("anthropic body extracts system message", () => {
    const body = providerBody(baseConfig, [
      { role: "system", content: "system prompt" },
      { role: "user", content: "hello" },
    ], []);
    expect(body.system).toBe("system prompt");
    expect((body.messages as any[]).length).toBe(1);
    expect((body.messages as any[])[0].role).toBe("user");
  });

  it("openai body includes system in messages", () => {
    const body = providerBody({ ...baseConfig, provider: "openai" }, [
      { role: "system", content: "system prompt" },
      { role: "user", content: "hello" },
    ], []);
    expect((body.messages as any[]).length).toBe(2);
    expect((body.messages as any[])[0].role).toBe("system");
  });

  it("anthropic body includes tools", () => {
    const tools = [{ name: "read_file", description: "Read", input_schema: { type: "object" } }];
    const body = providerBody(baseConfig, [{ role: "user", content: "hi" }], tools);
    expect((body.tools as any[]).length).toBe(1);
    expect((body.tools as any[])[0].name).toBe("read_file");
  });

  // --- Response parsing ---

  it("parses anthropic text response", () => {
    const response = parseAnthropicResponse({
      content: [{ type: "text", text: "Hello!" }],
      usage: { input_tokens: 10, output_tokens: 5 },
    });
    expect(response.content).toBe("Hello!");
    expect(response.toolCalls).toHaveLength(0);
    expect(response.inputTokens).toBe(10);
    expect(response.outputTokens).toBe(5);
  });

  it("parses anthropic tool use response", () => {
    const response = parseAnthropicResponse({
      content: [
        { type: "text", text: "I'll read that." },
        { type: "tool_use", id: "call_1", name: "read_file", input: { path: "/tmp/x" } },
      ],
      usage: { input_tokens: 20, output_tokens: 15 },
    });
    expect(response.content).toBe("I'll read that.");
    expect(response.toolCalls).toHaveLength(1);
    expect(response.toolCalls[0].name).toBe("read_file");
    expect(response.toolCalls[0].arguments.path).toBe("/tmp/x");
  });

  it("parses openai text response", () => {
    const response = parseOpenaiResponse({
      choices: [{ message: { role: "assistant", content: "Hello!" } }],
      usage: { prompt_tokens: 10, completion_tokens: 5 },
    });
    expect(response.content).toBe("Hello!");
    expect(response.toolCalls).toHaveLength(0);
  });

  it("parses openai tool call response", () => {
    const response = parseOpenaiResponse({
      choices: [{
        message: {
          role: "assistant",
          content: null,
          tool_calls: [{ id: "call_1", function: { name: "read_file", arguments: '{"path":"/tmp/x"}' } }],
        },
      }],
      usage: { prompt_tokens: 20, completion_tokens: 10 },
    });
    expect(response.toolCalls).toHaveLength(1);
    expect(response.toolCalls[0].name).toBe("read_file");
  });
});
