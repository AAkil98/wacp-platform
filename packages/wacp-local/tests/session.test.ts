import { describe, it, expect } from "vitest";
import { LocalSession, type SessionConfig } from "../src/session.js";
import { SessionError } from "../src/errors.js";

const defaultConfig: SessionConfig = {
  workingDir: "/tmp/test-workspace",
  autonomyPreset: "assisted",
};

describe("LocalSession", () => {
  it("creates with open state", async () => {
    const session = await LocalSession.create(defaultConfig);
    expect(session.state).toBe("open");
    expect(session.id).toMatch(/^session-/);
  });

  it("activates from open", async () => {
    const session = await LocalSession.create(defaultConfig);
    await session.activate();
    expect(session.state).toBe("active");
  });

  it("suspends from active", async () => {
    const session = await LocalSession.create(defaultConfig);
    await session.activate();
    await session.suspend();
    expect(session.state).toBe("suspended");
  });

  it("resumes from suspended", async () => {
    const session = await LocalSession.create(defaultConfig);
    await session.activate();
    await session.suspend();
    await session.activate();
    expect(session.state).toBe("active");
  });

  it("closes from active", async () => {
    const session = await LocalSession.create(defaultConfig);
    await session.activate();
    await session.close();
    expect(session.state).toBe("closed");
  });

  it("closes from suspended", async () => {
    const session = await LocalSession.create(defaultConfig);
    await session.activate();
    await session.suspend();
    await session.close();
    expect(session.state).toBe("closed");
  });

  it("closes from open", async () => {
    const session = await LocalSession.create(defaultConfig);
    await session.close();
    expect(session.state).toBe("closed");
  });

  it("rejects invalid transition: open → suspended", async () => {
    const session = await LocalSession.create(defaultConfig);
    await expect(session.suspend()).rejects.toThrow(SessionError);
  });

  it("rejects transition from closed", async () => {
    const session = await LocalSession.create(defaultConfig);
    await session.close();
    await expect(session.activate()).rejects.toThrow("already closed");
  });

  it("rejects missing workingDir", async () => {
    await expect(
      LocalSession.create({ workingDir: "", autonomyPreset: "supervised" }),
    ).rejects.toThrow("workingDir is required");
  });

  it("has autonomy manager from preset", async () => {
    const session = await LocalSession.create(defaultConfig);
    expect(session.autonomy.check("file_read")).toBe(true); // assisted preset
    expect(session.autonomy.check("file_write")).toBe(false);
  });
});
