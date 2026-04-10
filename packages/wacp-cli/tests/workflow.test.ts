import { describe, it, expect } from "vitest";
import { detectTaskType } from "../src/workflow.js";
import { loadSweVertical } from "../src/vertical.js";
import { filterToolsByProfile } from "../src/agent.js";
import { buildToolDefinitions } from "../src/tools.js";
import type { AgentProfile } from "@wacp/local";

describe("Task Type Detection", () => {
  it("detects debug from 'fix the bug'", () => {
    expect(detectTaskType("fix the bug in auth.ts").id).toBe("swe:debug");
  });

  it("detects debug from 'error in login'", () => {
    expect(detectTaskType("there's an error in the login flow").id).toBe("swe:debug");
  });

  it("detects refactor from 'refactor the database'", () => {
    expect(detectTaskType("refactor the database module").id).toBe("swe:refactor");
  });

  it("detects test from 'add tests for auth'", () => {
    expect(detectTaskType("write tests for the auth module").id).toBe("swe:test");
  });

  it("detects implement from 'add authentication'", () => {
    expect(detectTaskType("add authentication to the API").id).toBe("swe:implement");
  });

  it("detects implement from 'build a login page'", () => {
    expect(detectTaskType("build a login page").id).toBe("swe:implement");
  });

  it("detects investigate from 'how does the cache work'", () => {
    expect(detectTaskType("investigate how the cache works").id).toBe("swe:investigate");
  });

  it("detects document from 'write docs for the API'", () => {
    expect(detectTaskType("document the REST API").id).toBe("swe:document");
  });

  it("defaults to implement for ambiguous input", () => {
    expect(detectTaskType("make the app faster").id).toBe("swe:implement");
  });

  it("maps task types to correct workflows", () => {
    expect(detectTaskType("fix the bug").workflowId).toBe("swe:fix-bug");
    expect(detectTaskType("add feature").workflowId).toBe("swe:implement-feature");
    expect(detectTaskType("refactor code").workflowId).toBe("swe:refactor");
    expect(detectTaskType("write tests").workflowId).toBe("swe:write-tests");
  });
});

describe("SWE Vertical Loading", () => {
  it("loads 4 workflows", () => {
    const vertical = loadSweVertical();
    expect(vertical.workflows).toHaveLength(4);
  });

  it("loads 4 profiles", () => {
    const vertical = loadSweVertical();
    expect(vertical.profiles).toHaveLength(4);
  });

  it("all workflows have stages", () => {
    const vertical = loadSweVertical();
    for (const wf of vertical.workflows) {
      expect(wf.stages.length).toBeGreaterThan(0);
    }
  });

  it("all profiles have system prompts", () => {
    const vertical = loadSweVertical();
    for (const profile of vertical.profiles) {
      expect(profile.systemPrompt.length).toBeGreaterThan(50);
    }
  });

  it("every workflow stage has a matching profile", () => {
    const vertical = loadSweVertical();
    const profileRoles = new Set(vertical.profiles.map((p) => p.roleId));
    for (const wf of vertical.workflows) {
      for (const stage of wf.stages) {
        expect(profileRoles.has(stage.roleId)).toBe(true);
      }
    }
  });
});

describe("Tool Filtering by Profile", () => {
  const allTools = buildToolDefinitions();

  it("planner gets only read-only tools", () => {
    const planner: AgentProfile = {
      roleId: "swe:planner",
      systemPrompt: "",
      tools: ["read_file", "list_dir", "search_files", "code_search", "git_status", "git_diff"],
      autonomy: "gated",
    };
    const filtered = filterToolsByProfile(allTools, planner);
    expect(filtered.every((t) => ["read_file", "list_dir", "search_files", "code_search", "git_status", "git_diff"].includes(t.name))).toBe(true);
    expect(filtered.some((t) => t.name === "write_file")).toBe(false);
    expect(filtered.some((t) => t.name === "run_command")).toBe(false);
  });

  it("implementer gets read + write tools", () => {
    const impl: AgentProfile = {
      roleId: "swe:implementer",
      systemPrompt: "",
      tools: ["read_file", "write_file", "code_edit", "run_command", "git_commit"],
      autonomy: "gated",
    };
    const filtered = filterToolsByProfile(allTools, impl);
    expect(filtered.some((t) => t.name === "write_file")).toBe(true);
    expect(filtered.some((t) => t.name === "code_edit")).toBe(true);
    expect(filtered.some((t) => t.name === "git_commit")).toBe(true);
  });

  it("empty whitelist returns no tools", () => {
    const empty: AgentProfile = {
      roleId: "empty",
      systemPrompt: "",
      tools: [],
      autonomy: "gated",
    };
    expect(filterToolsByProfile(allTools, empty)).toHaveLength(0);
  });

  it("legacy buildToolDefinitions returns 15 tools (7 built-in + 8 SWE)", () => {
    expect(allTools.length).toBe(15);
  });
});
