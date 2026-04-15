import { describe, it, expect } from "vitest";
import { formatToolCall, formatToolResult, formatGatePrompt, formatBanner, formatTrustSurface, formatHelp } from "../src/display.js";

describe("Display", () => {
  it("formats tool call with args", () => {
    const output = formatToolCall("read_file", { path: "src/main.ts" });
    expect(output).toBe('⟡ read_file(path: "src/main.ts")');
  });

  it("formats tool call with multiple args", () => {
    const output = formatToolCall("write_file", { path: "out.txt", content: "hello" });
    expect(output).toContain("write_file");
    expect(output).toContain("path");
    expect(output).toContain("content");
  });

  it("formats successful tool result (short)", () => {
    const result = formatToolResult({ toolCallId: "1", content: "file contents here", isError: false });
    expect(result).toContain("→");
    expect(result).toContain("file contents here");
  });

  it("formats successful tool result (long → byte count)", () => {
    const longContent = "line\n".repeat(100);
    const result = formatToolResult({ toolCallId: "1", content: longContent, isError: false });
    expect(result).toContain("bytes");
    expect(result).toContain("lines");
  });

  it("formats error tool result", () => {
    const result = formatToolResult({ toolCallId: "1", content: "permission denied", isError: true });
    expect(result).toContain("✗");
    expect(result).toContain("permission denied");
  });

  it("formats gate prompt", () => {
    const prompt = formatGatePrompt("write_file", "file_write", "write to src/main.ts");
    expect(prompt).toContain("⚠");
    expect(prompt).toContain("file_write");
    expect(prompt).toContain("[y]es");
    expect(prompt).toContain("[a]lways");
  });

  it("formats banner with config", () => {
    const banner = formatBanner("/home/user/project", "anthropic", "claude-sonnet-4", "assisted");
    expect(banner).toContain("WACP CLI");
    expect(banner).toContain("anthropic");
    expect(banner).toContain("assisted");
  });

  it("formats trust surface", () => {
    const surface = formatTrustSurface(new Set(["file_read", "llm_call"]));
    expect(surface).toContain("file_read");
    expect(surface).toContain("llm_call");
  });

  it("formats empty trust surface", () => {
    const surface = formatTrustSurface(new Set());
    expect(surface).toContain("empty");
  });

  it("formats help text", () => {
    const help = formatHelp();
    expect(help).toContain("/help");
    expect(help).toContain("/trust");
    expect(help).toContain("/exit");
    expect(help).toContain("file_read");
  });
});
