import { describe, it, expect } from "vitest";
import { LlmCallError } from "../src/llm.js";

describe("Agent Loop helpers", () => {
  // Agent loop itself requires a real LLM — tested via integration.
  // Here we test the supporting types and error handling.

  it("LlmCallError is retryable for 429", () => {
    const err = new LlmCallError(429, "rate limited");
    expect(err.retryable).toBe(true);
    expect(err.status).toBe(429);
  });

  it("LlmCallError is retryable for 500", () => {
    expect(new LlmCallError(500, "server error").retryable).toBe(true);
  });

  it("LlmCallError is retryable for 502", () => {
    expect(new LlmCallError(502, "bad gateway").retryable).toBe(true);
  });

  it("LlmCallError is retryable for 503", () => {
    expect(new LlmCallError(503, "overloaded").retryable).toBe(true);
  });

  it("LlmCallError is NOT retryable for 400", () => {
    expect(new LlmCallError(400, "bad request").retryable).toBe(false);
  });

  it("LlmCallError is NOT retryable for 401", () => {
    expect(new LlmCallError(401, "unauthorized").retryable).toBe(false);
  });

  it("LlmCallError is NOT retryable for 404", () => {
    expect(new LlmCallError(404, "not found").retryable).toBe(false);
  });

  it("LlmCallError has correct name", () => {
    const err = new LlmCallError(500, "test");
    expect(err.name).toBe("LlmCallError");
    expect(err.message).toBe("test");
  });
});
