import { describe, it, expect } from "vitest";
import { analyticsToolDefinitions, classifySql } from "../src/tools/analytics-tools.js";

describe("Analytics Tools", () => {
  const tools = analyticsToolDefinitions();

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

  it("sql_query requires query", () => {
    const sq = tools.find((t) => t.name === "sql_query")!;
    expect(sq.input_schema.required).toContain("query");
  });

  it("report_generate requires title and sources", () => {
    const rg = tools.find((t) => t.name === "report_generate")!;
    const required = rg.input_schema.required as string[];
    expect(required).toContain("title");
    expect(required).toContain("sources");
  });

  it("data_reconcile requires two sources", () => {
    const dr = tools.find((t) => t.name === "data_reconcile")!;
    const required = dr.input_schema.required as string[];
    expect(required).toContain("source_a");
    expect(required).toContain("source_b");
  });
});

describe("SQL Safety Classification", () => {
  it("SELECT is read-only", () => {
    expect(classifySql("SELECT * FROM users")).toBe("read");
    expect(classifySql("  select id from orders where status = 'active'")).toBe("read");
  });

  it("DROP is destructive", () => {
    expect(classifySql("DROP TABLE users")).toBe("destructive");
    expect(classifySql("  drop table orders")).toBe("destructive");
  });

  it("TRUNCATE is destructive", () => {
    expect(classifySql("TRUNCATE TABLE logs")).toBe("destructive");
  });

  it("DELETE without WHERE is unscoped", () => {
    expect(classifySql("DELETE FROM users")).toBe("unscoped_mutation");
  });

  it("DELETE with WHERE is scoped", () => {
    expect(classifySql("DELETE FROM users WHERE id = 1")).toBe("scoped_mutation");
  });

  it("UPDATE without WHERE is unscoped", () => {
    expect(classifySql("UPDATE users SET active = false")).toBe("unscoped_mutation");
  });

  it("UPDATE with WHERE is scoped", () => {
    expect(classifySql("UPDATE users SET active = false WHERE id = 1")).toBe("scoped_mutation");
  });

  it("CREATE and ALTER are schema_change", () => {
    expect(classifySql("CREATE TABLE foo (id int)")).toBe("schema_change");
    expect(classifySql("ALTER TABLE users ADD COLUMN email text")).toBe("schema_change");
  });

  it("comments are ignored", () => {
    expect(classifySql("-- harmless comment\nDROP TABLE x")).toBe("destructive");
    expect(classifySql("/* block\nDROP TABLE x */\nSELECT 1")).toBe("read");
  });

  it("INSERT is scoped_mutation", () => {
    expect(classifySql("INSERT INTO users VALUES (1, 'a')")).toBe("scoped_mutation");
  });
});
