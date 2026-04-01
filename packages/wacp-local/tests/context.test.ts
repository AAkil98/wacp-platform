import { describe, it, expect } from "vitest";
import { SessionContext } from "../src/context.js";

describe("SessionContext", () => {
  it("starts empty", () => {
    const ctx = new SessionContext();
    expect(ctx.history).toEqual([]);
    expect(ctx.trustLog).toEqual([]);
  });

  it("records trust decisions", () => {
    const ctx = new SessionContext();
    ctx.recordTrust("file_read", true);
    ctx.recordTrust("shell_exec", false);
    expect(ctx.trustLog).toHaveLength(2);
    expect(ctx.trustLog[0].operation).toBe("file_read");
    expect(ctx.trustLog[0].granted).toBe(true);
    expect(ctx.trustLog[1].granted).toBe(false);
  });

  it("accumulates history", () => {
    const ctx = new SessionContext();
    ctx.addHistory("user: fix the bug");
    ctx.addHistory("assistant: I'll look at auth.ts");
    expect(ctx.history).toHaveLength(2);
  });

  it("snapshot captures state", () => {
    const ctx = new SessionContext();
    ctx.addHistory("entry-1");
    ctx.recordTrust("file_read", true);
    ctx.metadata["key"] = "value";

    const snap = ctx.snapshot();
    expect(snap.historyLength).toBe(1);
    expect(snap.trustLog).toHaveLength(1);
    expect(snap.metadata["key"]).toBe("value");
  });

  it("restore rebuilds trust log and metadata", () => {
    const ctx = new SessionContext();
    const snap = {
      historyLength: 5,
      trustLog: [{ operation: "file_write", granted: true, timestamp: 123 }],
      metadata: { restored: true },
    };
    ctx.restore(snap);
    expect(ctx.trustLog).toHaveLength(1);
    expect(ctx.metadata["restored"]).toBe(true);
  });

  it("snapshot is a copy (mutations don't affect context)", () => {
    const ctx = new SessionContext();
    ctx.recordTrust("x", true);
    const snap = ctx.snapshot();
    snap.trustLog.push({ operation: "y", granted: false, timestamp: 0 });
    expect(ctx.trustLog).toHaveLength(1); // unchanged
  });

  it("prune removes old history", () => {
    const ctx = new SessionContext();
    for (let i = 0; i < 10; i++) ctx.addHistory(`entry-${i}`);
    const removed = ctx.prune(5);
    expect(removed).toBe(5);
    expect(ctx.history).toHaveLength(5);
    expect(ctx.history[0]).toBe("entry-5"); // oldest kept
  });

  it("prune does nothing when under budget", () => {
    const ctx = new SessionContext();
    ctx.addHistory("a");
    ctx.addHistory("b");
    expect(ctx.prune(10)).toBe(0);
    expect(ctx.history).toHaveLength(2);
  });
});
