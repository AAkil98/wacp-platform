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

// ---------------------------------------------------------------------------
// Streaming completion — the primary LLM call path
// ---------------------------------------------------------------------------

export interface StreamCallbacks {
  /** Called for each text token. */
  onToken?: (token: string) => void;
  /** Called when a tool call starts. */
  onToolCallStart?: (name: string) => void;
}

/**
 * Call the LLM with streaming enabled. Parses SSE, invokes callbacks
 * for real-time output, and returns the assembled response.
 */
export async function streamCompletion(
  config: CliConfig,
  messages: Message[],
  tools: ToolDefinition[],
  callbacks: StreamCallbacks = {},
  signal?: AbortSignal,
): Promise<LlmResponse> {
  const body = providerBody(config, messages, tools);
  body.stream = true;
  if (config.provider !== "anthropic") {
    body.stream_options = { include_usage: true };
  }

  const response = await fetch(providerUrl(config), {
    method: "POST",
    headers: providerHeaders(config),
    body: JSON.stringify(body),
    signal,
  });

  if (!response.ok) {
    const errorBody = await response.text();
    throw new LlmCallError(
      response.status,
      `LLM API error ${response.status}: ${errorBody.slice(0, 500)}`,
    );
  }

  if (!response.body) {
    throw new LlmCallError(0, "No response body from LLM API");
  }

  if (config.provider === "anthropic") {
    return parseAnthropicStream(response.body, callbacks);
  } else {
    return parseOpenaiStream(response.body, callbacks);
  }
}

/** Non-streaming completion (simpler, for testing or fallback). */
export async function completion(
  config: CliConfig,
  messages: Message[],
  tools: ToolDefinition[],
  signal?: AbortSignal,
): Promise<LlmResponse> {
  const body = providerBody(config, messages, tools);

  const response = await fetch(providerUrl(config), {
    method: "POST",
    headers: providerHeaders(config),
    body: JSON.stringify(body),
    signal,
  });

  if (!response.ok) {
    const errorBody = await response.text();
    throw new LlmCallError(
      response.status,
      `LLM API error ${response.status}: ${errorBody.slice(0, 500)}`,
    );
  }

  const json = await response.json() as Record<string, unknown>;

  if (config.provider === "anthropic") {
    return parseAnthropicResponse(json);
  } else {
    return parseOpenaiResponse(json);
  }
}

export class LlmCallError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = "LlmCallError";
  }

  get retryable(): boolean {
    return [408, 429, 500, 502, 503].includes(this.status);
  }
}

// ---------------------------------------------------------------------------
// SSE stream parsers
// ---------------------------------------------------------------------------

async function parseAnthropicStream(
  body: ReadableStream<Uint8Array>,
  callbacks: StreamCallbacks,
): Promise<LlmResponse> {
  let content = "";
  const toolCalls: ToolCall[] = [];
  let inputTokens = 0;
  let outputTokens = 0;
  let currentEvent = "";

  // Track tool call assembly
  const toolCallBuilders: Map<number, { id: string; name: string; args: string }> = new Map();

  for await (const line of sseLines(body)) {
    if (line.startsWith("event:")) {
      currentEvent = line.slice(6).trim();
      continue;
    }

    if (!line.startsWith("data:")) continue;
    const data = line.slice(5).trim();
    if (data === "[DONE]") break;

    let json: Record<string, unknown>;
    try {
      json = JSON.parse(data);
    } catch {
      continue;
    }

    switch (currentEvent) {
      case "message_start": {
        const usage = (json.message as Record<string, unknown>)?.usage as Record<string, number> | undefined;
        if (usage) inputTokens = usage.input_tokens ?? 0;
        break;
      }
      case "content_block_start": {
        const block = json.content_block as Record<string, unknown>;
        const index = json.index as number;
        if (block?.type === "tool_use") {
          toolCallBuilders.set(index, {
            id: block.id as string,
            name: block.name as string,
            args: "",
          });
          callbacks.onToolCallStart?.(block.name as string);
        }
        break;
      }
      case "content_block_delta": {
        const delta = json.delta as Record<string, unknown>;
        const index = json.index as number;
        if (delta?.type === "text_delta") {
          const text = delta.text as string;
          content += text;
          callbacks.onToken?.(text);
        } else if (delta?.type === "input_json_delta") {
          const builder = toolCallBuilders.get(index);
          if (builder) {
            builder.args += delta.partial_json as string;
          }
        }
        break;
      }
      case "content_block_stop": {
        const index = json.index as number;
        const builder = toolCallBuilders.get(index);
        if (builder) {
          let args: Record<string, unknown>;
          try {
            args = JSON.parse(builder.args || "{}");
          } catch {
            args = {};
          }
          toolCalls.push({ id: builder.id, name: builder.name, arguments: args });
          toolCallBuilders.delete(index);
        }
        break;
      }
      case "message_delta": {
        const usage = json.usage as Record<string, number> | undefined;
        if (usage) outputTokens = usage.output_tokens ?? 0;
        break;
      }
      case "message_stop":
        break;
    }
    currentEvent = "";
  }

  return { content, toolCalls, inputTokens, outputTokens };
}

async function parseOpenaiStream(
  body: ReadableStream<Uint8Array>,
  callbacks: StreamCallbacks,
): Promise<LlmResponse> {
  let content = "";
  const toolCallBuilders: Map<number, { id: string; name: string; args: string }> = new Map();
  let inputTokens = 0;
  let outputTokens = 0;

  for await (const line of sseLines(body)) {
    if (!line.startsWith("data:")) continue;
    const data = line.slice(5).trim();
    if (data === "[DONE]") break;

    let json: Record<string, unknown>;
    try {
      json = JSON.parse(data);
    } catch {
      continue;
    }

    // Usage in final chunk
    const usage = json.usage as Record<string, number> | undefined;
    if (usage) {
      inputTokens = usage.prompt_tokens ?? 0;
      outputTokens = usage.completion_tokens ?? 0;
    }

    const choices = json.choices as Record<string, unknown>[] | undefined;
    if (!choices?.length) continue;
    const delta = choices[0].delta as Record<string, unknown> | undefined;
    if (!delta) continue;

    // Text content
    if (typeof delta.content === "string" && delta.content) {
      content += delta.content;
      callbacks.onToken?.(delta.content);
    }

    // Tool calls
    const tcs = delta.tool_calls as Record<string, unknown>[] | undefined;
    if (tcs) {
      for (const tc of tcs) {
        const index = (tc.index as number) ?? 0;
        const fn = tc.function as Record<string, unknown> | undefined;

        if (!toolCallBuilders.has(index)) {
          toolCallBuilders.set(index, {
            id: (tc.id as string) ?? "",
            name: (fn?.name as string) ?? "",
            args: "",
          });
          if (fn?.name) callbacks.onToolCallStart?.(fn.name as string);
        }

        const builder = toolCallBuilders.get(index)!;
        if (tc.id) builder.id = tc.id as string;
        if (fn?.name) builder.name = fn.name as string;
        if (fn?.arguments) builder.args += fn.arguments as string;
      }
    }
  }

  // Assemble tool calls
  const toolCalls: ToolCall[] = [];
  for (const [, builder] of toolCallBuilders) {
    let args: Record<string, unknown>;
    try {
      args = JSON.parse(builder.args || "{}");
    } catch {
      args = {};
    }
    toolCalls.push({ id: builder.id, name: builder.name, arguments: args });
  }

  return { content, toolCalls, inputTokens, outputTokens };
}

/**
 * Yield SSE lines from a ReadableStream. Handles line buffering
 * across chunk boundaries.
 */
async function* sseLines(body: ReadableStream<Uint8Array>): AsyncGenerator<string> {
  const decoder = new TextDecoder();
  const reader = body.getReader();
  let buffer = "";

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop()!; // keep incomplete line in buffer

      for (const line of lines) {
        const trimmed = line.trimEnd();
        if (trimmed) yield trimmed;
      }
    }

    // Flush remaining buffer
    if (buffer.trim()) yield buffer.trim();
  } finally {
    reader.releaseLock();
  }
}

// ---------------------------------------------------------------------------
// Non-streaming response parsers (kept for fallback / testing)
// ---------------------------------------------------------------------------

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
