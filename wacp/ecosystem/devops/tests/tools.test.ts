import { describe, it, expect } from "vitest";
import { devopsToolDefinitions } from "../src/tools/devops-tools.js";

describe("DevOps Tools", () => {
  const tools = devopsToolDefinitions();

  it("defines 10 tools", () => {
    expect(tools).toHaveLength(10);
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

  it("deploy_execute requires environment", () => {
    const de = tools.find((t) => t.name === "deploy_execute")!;
    expect(de.input_schema.required).toContain("environment");
  });

  it("rollback requires environment", () => {
    const rb = tools.find((t) => t.name === "rollback")!;
    expect(rb.input_schema.required).toContain("environment");
  });

  it("health_check requires endpoints", () => {
    const hc = tools.find((t) => t.name === "health_check")!;
    expect(hc.input_schema.required).toContain("endpoints");
  });

  it("secret_rotate requires name and environment", () => {
    const sr = tools.find((t) => t.name === "secret_rotate")!;
    const required = sr.input_schema.required as string[];
    expect(required).toContain("name");
    expect(required).toContain("environment");
  });

  it("environment-aware tools are deploy_execute, rollback, secret_rotate", () => {
    const envTools = tools.filter((t) => {
      const props = t.input_schema.properties as Record<string, Record<string, unknown>>;
      return props.environment !== undefined;
    });
    const envNames = envTools.map((t) => t.name).sort();
    expect(envNames).toEqual(["deploy_execute", "rollback", "secret_rotate"]);
  });

  it("infra_plan has optional fields only", () => {
    const ip = tools.find((t) => t.name === "infra_plan")!;
    expect(ip.input_schema.required).toBeUndefined();
  });
});
