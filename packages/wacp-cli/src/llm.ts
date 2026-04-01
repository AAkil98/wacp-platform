import type { CliConfig } from "./config.js";
import type { ToolDefinition } from "./tools.js";

/** Message in the conversation. */
export interface Message {
  role: "system" | "user" | "assistant" | "tool";
  content: string | ContentBlock[];
  tool_call_id?: string;
}

export interface ContentBlock {
  type: string;
  [key: string]: unknown;
}

/** Parsed tool call from the LLM. */
export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

/** Result of an LLM completion. */
export interface LlmResponse {
  content: string;
  toolCalls: ToolCall[];
  inputTokens: number;
  outputTokens: number;
}

/** Get the provider API URL. */
export function providerUrl(config: CliConfig): string {
  switch (config.provider) {
    case "anthropic":
      return `${config.baseUrl ?? "https://api.anthropic.com"}/v1/messages`;
    case "openai":
    case "generic":
      return `${config.baseUrl ?? "https://api.openai.com"}/v1/chat/completions`;
  }
}

/** Get the provider request headers. */
export function providerHeaders(config: CliConfig): Record<string, string> {
  switch (config.provider) {
    case "anthropic":
      return {
        "content-type": "application/json",
        "x-api-key": config.apiKey,
        "anthropic-version": "2023-06-01",
      };
    case "openai":
    case "generic":
      return {
        "content-type": "application/json",
        "authorization": `Bearer ${config.apiKey}`,
      };
  }
}

/** Build the provider-specific request body. */
export function providerBody(
  config: CliConfig,
  messages: Message[],
  tools: ToolDefinition[],
): Record<string, unknown> {
  switch (config.provider) {
    case "anthropic":
      return buildAnthropicBody(config, messages, tools);
    case "openai":
    case "generic":
      return buildOpenaiBody(config, messages, tools);
  }
}

function buildAnthropicBody(
  config: CliConfig,
  messages: Message[],
  tools: ToolDefinition[],
): Record<string, unknown> {
  const systemMessages = messages
    .filter((m) => m.role === "system")
    .map((m) => (typeof m.content === "string" ? m.content : ""))
    .join("\n\n");

  const apiMessages = messages
    .filter((m) => m.role !== "system")
    .map((m) => ({ role: m.role === "tool" ? "user" : m.role, content: m.content }));

  const body: Record<string, unknown> = {
    model: config.model,
    max_tokens: config.maxTokens,
    messages: apiMessages,
  };

  if (systemMessages) body.system = systemMessages;
  if (config.temperature !== undefined) body.temperature = config.temperature;
  if (tools.length > 0) {
    body.tools = tools.map((t) => ({
      name: t.name,
      description: t.description,
      input_schema: t.input_schema,
    }));
  }

  return body;
}

function buildOpenaiBody(
  config: CliConfig,
  messages: Message[],
  tools: ToolDefinition[],
): Record<string, unknown> {
  const apiMessages = messages.map((m) => {
    if (m.role === "tool") {
      return { role: "tool", tool_call_id: m.tool_call_id, content: m.content };
    }
    return { role: m.role, content: m.content };
  });

  const body: Record<string, unknown> = {
    model: config.model,
    messages: apiMessages,
  };

  if (config.maxTokens) body.max_completion_tokens = config.maxTokens;
  if (config.temperature !== undefined) body.temperature = config.temperature;
  if (tools.length > 0) {
    body.tools = tools.map((t) => ({
      type: "function",
      function: { name: t.name, description: t.description, parameters: t.input_schema },
    }));
  }

  return body;
}

/** Parse an Anthropic response body into LlmResponse. */
export function parseAnthropicResponse(body: Record<string, unknown>): LlmResponse {
  const content = body.content as ContentBlock[] | undefined;
  let text = "";
  const toolCalls: ToolCall[] = [];

  if (Array.isArray(content)) {
    for (const block of content) {
      if (block.type === "text") text += block.text;
      if (block.type === "tool_use") {
        toolCalls.push({
          id: block.id as string,
          name: block.name as string,
          arguments: block.input as Record<string, unknown>,
        });
      }
    }
  }

  const usage = body.usage as Record<string, number> | undefined;
  return {
    content: text,
    toolCalls,
    inputTokens: usage?.input_tokens ?? 0,
    outputTokens: usage?.output_tokens ?? 0,
  };
}

/** Parse an OpenAI response body into LlmResponse. */
export function parseOpenaiResponse(body: Record<string, unknown>): LlmResponse {
  const choices = body.choices as Record<string, unknown>[] | undefined;
  const choice = choices?.[0];
  const message = choice?.message as Record<string, unknown> | undefined;

  const text = (message?.content as string) ?? "";
  const toolCalls: ToolCall[] = [];

  const rawCalls = message?.tool_calls as Record<string, unknown>[] | undefined;
  if (rawCalls) {
    for (const tc of rawCalls) {
      const fn = tc.function as Record<string, unknown>;
      toolCalls.push({
        id: tc.id as string,
        name: fn.name as string,
        arguments: JSON.parse((fn.arguments as string) ?? "{}"),
      });
    }
  }

  const usage = body.usage as Record<string, number> | undefined;
  return {
    content: text,
    toolCalls,
    inputTokens: usage?.prompt_tokens ?? 0,
    outputTokens: usage?.completion_tokens ?? 0,
  };
}
