import { describe, it, expect } from "vitest";
import { sweToolDefinitions } from "../src/tools/swe-tools.js";

describe("SWE Tools", () => {
  const tools = sweToolDefinitions();

  it("defines 8 tools", () => {
    expect(tools).toHaveLength(8);
  });

  it("all tools have unique names", () => {
    const names = tools.map((t) => t.name);
    expect(new Set(names).size).toBe(names.length);
  });

  it("all tools have non-empty descriptions", () => {
    for (const tool of tools) {
      expect(tool.description.length).toBeGreaterThan(0);
    }
  });

  it("all tools have object input schemas", () => {
    for (const tool of tools) {
      expect(tool.input_schema.type).toBe("object");
    }
  });

  it("code_search requires pattern", () => {
    const cs = tools.find((t) => t.name === "code_search")!;
    expect(cs.input_schema.required).toContain("pattern");
  });

  it("code_edit requires path, old_text, new_text", () => {
    const ce = tools.find((t) => t.name === "code_edit")!;
    const required = ce.input_schema.required as string[];
    expect(required).toContain("path");
    expect(required).toContain("old_text");
    expect(required).toContain("new_text");
  });

  it("git_commit requires message", () => {
    const gc = tools.find((t) => t.name === "git_commit")!;
    expect(gc.input_schema.required).toContain("message");
  });

  it("test_run has optional fields", () => {
    const tr = tools.find((t) => t.name === "test_run")!;
    expect(tr.input_schema.required).toBeUndefined(); // all optional
  });
});
