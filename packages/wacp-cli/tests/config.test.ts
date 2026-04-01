import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { parseConfig, mergeConfig, validateConfig, resolveEnvVars } from "../src/config.js";

describe("Config", () => {
  // --- YAML parsing ---

  it("parses basic YAML config", () => {
    const config = parseConfig(`
provider: anthropic
model: claude-sonnet-4-20250514
api_key: sk-test
autonomy: supervised
    `);
    expect(config.provider).toBe("anthropic");
    expect(config.model).toBe("claude-sonnet-4-20250514");
    expect(config.apiKey).toBe("sk-test");
    expect(config.autonomy).toBe("supervised");
  });

  it("parses optional fields", () => {
    const config = parseConfig(`
provider: openai
model: gpt-4o
api_key: sk-test
max_tokens: 8192
temperature: 0.7
base_url: https://custom.api.com
    `);
    expect(config.maxTokens).toBe(8192);
    expect(config.temperature).toBe(0.7);
    expect(config.baseUrl).toBe("https://custom.api.com");
  });

  it("returns empty config for invalid YAML", () => {
    const config = parseConfig("");
    expect(Object.keys(config)).toHaveLength(0);
  });

  // --- Env var substitution ---

  it("resolves env vars in values", () => {
    process.env.TEST_KEY = "secret123";
    const result = resolveEnvVars("${TEST_KEY}");
    expect(result).toBe("secret123");
    delete process.env.TEST_KEY;
  });

  it("throws on missing env var", () => {
    delete process.env.NONEXISTENT_VAR;
    expect(() => resolveEnvVars("${NONEXISTENT_VAR}")).toThrow("not set");
  });

  it("resolves multiple env vars", () => {
    process.env.VAR_A = "hello";
    process.env.VAR_B = "world";
    const result = resolveEnvVars("${VAR_A} ${VAR_B}");
    expect(result).toBe("hello world");
    delete process.env.VAR_A;
    delete process.env.VAR_B;
  });

  it("passes through strings without env vars", () => {
    expect(resolveEnvVars("plain-string")).toBe("plain-string");
  });

  // --- Merge order ---

  it("flags override file config", () => {
    const fileConfig = { provider: "openai" as const, model: "gpt-4o" };
    const merged = mergeConfig({ provider: "anthropic" }, fileConfig);
    expect(merged.provider).toBe("anthropic");
  });

  it("file config overrides defaults", () => {
    const merged = mergeConfig({}, { model: "custom-model" });
    expect(merged.model).toBe("custom-model");
  });

  it("defaults used when nothing specified", () => {
    const merged = mergeConfig({}, {});
    expect(merged.provider).toBe("anthropic");
    expect(merged.autonomy).toBe("assisted");
    expect(merged.maxTokens).toBe(16384);
    expect(merged.temperature).toBe(0);
  });

  // --- Validation ---

  it("validates valid config", () => {
    const config = mergeConfig({}, { apiKey: "sk-test" });
    const errors = validateConfig(config);
    expect(errors).toHaveLength(0);
  });

  it("rejects missing API key", () => {
    // Save and clear env vars that could auto-fill the key
    const saved = { ...process.env };
    delete process.env.ANTHROPIC_API_KEY;
    delete process.env.OPENAI_API_KEY;

    const config = mergeConfig({}, {});
    const errors = validateConfig(config);
    expect(errors.some((e) => e.includes("API key"))).toBe(true);

    // Restore
    Object.assign(process.env, saved);
  });

  it("rejects invalid provider", () => {
    const config = mergeConfig({}, { provider: "invalid" as any, apiKey: "key" });
    const errors = validateConfig(config);
    expect(errors.some((e) => e.includes("Invalid provider"))).toBe(true);
  });
});
