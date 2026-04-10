import { describe, it, expect, beforeEach } from "vitest";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as os from "node:os";

import { AutonomyManager, LocalResources } from "@wacp/local";
import { buildToolDefinitions, executeTool } from "../src/tools.js";

describe("Tools", () => {
  // --- Tool definitions ---

  it("all 15 legacy tools (built-in + SWE) have valid definitions", () => {
    // Legacy buildToolDefinitions = 7 universal built-in + 8 SWE-specific (incl. dependency_check).
    // For multi-vertical composition, callers should use buildToolDefinitionsForEcosystem.
    const tools = buildToolDefinitions();
    expect(tools).toHaveLength(15);
    for (const tool of tools) {
      expect(tool.name).toBeTruthy();
      expect(tool.description).toBeTruthy();
      expect(tool.input_schema).toBeDefined();
      expect(tool.input_schema.type).toBe("object");
    }
  });

  it("tool names are unique", () => {
    const tools = buildToolDefinitions();
    const names = tools.map((t) => t.name);
    expect(new Set(names).size).toBe(names.length);
  });

  it("read_file requires path", () => {
    const tools = buildToolDefinitions();
    const readFile = tools.find((t) => t.name === "read_file")!;
    expect(readFile.input_schema.required).toContain("path");
  });

  it("write_file requires path and content", () => {
    const tools = buildToolDefinitions();
    const writeFile = tools.find((t) => t.name === "write_file")!;
    expect(writeFile.input_schema.required).toContain("path");
    expect(writeFile.input_schema.required).toContain("content");
  });

  // --- Tool execution ---

  let tmpDir: string;
  let autonomy: AutonomyManager;
  let resources: LocalResources;

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "wacp-tools-test-"));
    autonomy = new AutonomyManager("autonomous");
    resources = new LocalResources(tmpDir, autonomy);
  });

  it("read_file executor reads file", async () => {
    await fs.writeFile(path.join(tmpDir, "test.txt"), "hello");
    const result = await executeTool(resources, autonomy, "read_file", "call-1", { path: "test.txt" });
    expect(result.isError).toBe(false);
    expect(result.content).toBe("hello");
  });

  it("write_file executor writes file", async () => {
    const result = await executeTool(resources, autonomy, "write_file", "call-2", { path: "out.txt", content: "data" });
    expect(result.isError).toBe(false);
    const content = await fs.readFile(path.join(tmpDir, "out.txt"), "utf-8");
    expect(content).toBe("data");
  });

  it("run_command executor runs command", async () => {
    const result = await executeTool(resources, autonomy, "run_command", "call-3", { command: "echo hello" });
    expect(result.isError).toBe(false);
    expect(result.content.trim()).toBe("hello");
  });

  it("unknown tool returns error", async () => {
    const result = await executeTool(resources, autonomy, "unknown_tool", "call-4", {});
    expect(result.isError).toBe(true);
    expect(result.content).toContain("Unknown tool");
  });

  it("autonomy denial returns structured error", async () => {
    const supervised = new AutonomyManager("supervised");
    const restrictedResources = new LocalResources(tmpDir, supervised);
    const result = await executeTool(restrictedResources, supervised, "read_file", "call-5", { path: "x.txt" });
    expect(result.isError).toBe(true);
    expect(result.content).toContain("denied");
  });

  it("list_dir executor lists directory", async () => {
    await fs.writeFile(path.join(tmpDir, "a.txt"), "");
    await fs.writeFile(path.join(tmpDir, "b.txt"), "");
    const result = await executeTool(resources, autonomy, "list_dir", "call-6", { path: "." });
    expect(result.isError).toBe(false);
    expect(result.content).toContain("a.txt");
    expect(result.content).toContain("b.txt");
  });
});
