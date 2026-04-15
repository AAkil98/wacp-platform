import { describe, it, expect } from "vitest";
import { AutonomyManager } from "@wacp/local";
import { handleCommand } from "../src/commands.js";

describe("Commands", () => {
  it("/help returns help text", () => {
    const mgr = new AutonomyManager("assisted");
    const result = handleCommand("/help", mgr);
    expect(result.exit).toBe(false);
    expect(result.output).toContain("/trust");
    expect(result.output).toContain("/exit");
  });

  it("/exit signals exit", () => {
    const mgr = new AutonomyManager("assisted");
    const result = handleCommand("/exit", mgr);
    expect(result.exit).toBe(true);
  });

  it("/quit signals exit", () => {
    const mgr = new AutonomyManager("assisted");
    const result = handleCommand("/quit", mgr);
    expect(result.exit).toBe(true);
  });

  it("/trust grants operation", () => {
    const mgr = new AutonomyManager("supervised");
    expect(mgr.check("file_write")).toBe(false);
    const result = handleCommand("/trust file_write", mgr);
    expect(result.output).toContain("Granted");
    expect(mgr.check("file_write")).toBe(true);
  });

  it("/trust with invalid op shows error", () => {
    const mgr = new AutonomyManager("supervised");
    const result = handleCommand("/trust invalid_op", mgr);
    expect(result.output).toContain("Unknown operation");
  });

  it("/trust without arg shows usage", () => {
    const mgr = new AutonomyManager("supervised");
    const result = handleCommand("/trust", mgr);
    expect(result.output).toContain("Usage");
  });

  it("/revoke removes trust", () => {
    const mgr = new AutonomyManager("assisted");
    expect(mgr.check("file_read")).toBe(true);
    handleCommand("/revoke file_read", mgr);
    expect(mgr.check("file_read")).toBe(false);
  });

  it("/preset switches preset", () => {
    const mgr = new AutonomyManager("supervised");
    handleCommand("/preset autonomous", mgr);
    expect(mgr.check("file_write")).toBe(true);
    expect(mgr.check("shell_exec")).toBe(true);
  });

  it("/preset with invalid name shows error", () => {
    const mgr = new AutonomyManager("supervised");
    const result = handleCommand("/preset invalid", mgr);
    expect(result.output).toContain("Unknown preset");
  });

  it("/surface shows trust surface", () => {
    const mgr = new AutonomyManager("supervised");
    const result = handleCommand("/surface", mgr);
    expect(result.output).toContain("llm_call");
  });

  it("unknown command shows error", () => {
    const mgr = new AutonomyManager("supervised");
    const result = handleCommand("/unknown", mgr);
    expect(result.output).toContain("Unknown command");
  });
});
