import { describe, it, expect } from "vitest";
import { AutonomyManager } from "../src/autonomy.js";

describe("AutonomyManager", () => {
  // --- Presets ---
  it("supervised: only llm_call trusted", () => {
    const mgr = new AutonomyManager("supervised");
    expect(mgr.check("llm_call")).toBe(true);
    expect(mgr.check("file_read")).toBe(false);
    expect(mgr.check("shell_exec")).toBe(false);
  });

  it("assisted: file_read + git_read + web_fetch + llm_call", () => {
    const mgr = new AutonomyManager("assisted");
    expect(mgr.check("file_read")).toBe(true);
    expect(mgr.check("git_read")).toBe(true);
    expect(mgr.check("web_fetch")).toBe(true);
    expect(mgr.check("llm_call")).toBe(true);
    expect(mgr.check("file_write")).toBe(false);
    expect(mgr.check("shell_exec")).toBe(false);
  });

  it("autonomous: most operations trusted", () => {
    const mgr = new AutonomyManager("autonomous");
    expect(mgr.check("file_read")).toBe(true);
    expect(mgr.check("file_write")).toBe(true);
    expect(mgr.check("shell_exec")).toBe(true);
    expect(mgr.check("git_write")).toBe(true);
    // Destructive ops still not trusted
    expect(mgr.check("file_delete")).toBe(false);
    expect(mgr.check("shell_exec_destructive")).toBe(false);
  });

  // --- Grant / Revoke ---
  it("grant adds to trust surface", () => {
    const mgr = new AutonomyManager("supervised");
    expect(mgr.check("file_read")).toBe(false);
    mgr.grant("file_read");
    expect(mgr.check("file_read")).toBe(true);
  });

  it("revoke removes from trust surface", () => {
    const mgr = new AutonomyManager("assisted");
    expect(mgr.check("file_read")).toBe(true);
    mgr.revoke("file_read");
    expect(mgr.check("file_read")).toBe(false);
  });

  it("grantAlways persists through revoke", () => {
    const mgr = new AutonomyManager("supervised");
    mgr.grantAlways("file_read");
    expect(mgr.check("file_read")).toBe(true);
    mgr.revoke("file_read"); // should not revoke "always" grants
    expect(mgr.check("file_read")).toBe(true);
  });

  it("grantAlways persists through resetToPreset", () => {
    const mgr = new AutonomyManager("autonomous");
    mgr.grantAlways("file_delete");
    mgr.resetToPreset("supervised");
    expect(mgr.check("file_delete")).toBe(true); // always grant preserved
    expect(mgr.check("file_write")).toBe(false); // reset to supervised
  });

  // --- Trust surface ---
  it("trustSurface returns current set", () => {
    const mgr = new AutonomyManager("supervised");
    const surface = mgr.trustSurface();
    expect(surface.size).toBe(1);
    expect(surface.has("llm_call")).toBe(true);
  });

  it("trustSurface is a copy (mutations don't affect manager)", () => {
    const mgr = new AutonomyManager("supervised");
    const surface = mgr.trustSurface();
    surface.add("file_write");
    expect(mgr.check("file_write")).toBe(false); // original unchanged
  });

  // --- Reset ---
  it("resetToPreset clears grants and applies preset", () => {
    const mgr = new AutonomyManager("supervised");
    mgr.grant("file_write");
    mgr.grant("shell_exec");
    mgr.resetToPreset("assisted");
    expect(mgr.check("file_write")).toBe(false); // not in assisted
    expect(mgr.check("file_read")).toBe(true); // in assisted
  });
});
