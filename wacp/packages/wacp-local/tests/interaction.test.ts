import { describe, it, expect } from "vitest";
import { InteractionStream, type Gate } from "../src/interaction.js";

describe("InteractionStream", () => {
  const stream = new InteractionStream();

  // --- Classification ---
  it("classifies plain text as goal", () => {
    expect(stream.classify("fix the bug in auth.ts").type).toBe("goal");
  });

  it("classifies question as query", () => {
    expect(stream.classify("what files did you change?").type).toBe("query");
    expect(stream.classify("? status").type).toBe("query");
    expect(stream.classify("how does the auth work?").type).toBe("query");
    expect(stream.classify("why did it fail?").type).toBe("query");
  });

  it("classifies amendment", () => {
    expect(stream.classify("actually, use Python instead").type).toBe("amendment");
    expect(stream.classify("instead of React, use Vue").type).toBe("amendment");
    expect(stream.classify("change the approach to TDD").type).toBe("amendment");
  });

  it("classifies approval with pending gate", () => {
    const gates: Gate[] = [{ id: "gate-1", operation: "file_write" }];
    const result = stream.classify("yes", gates);
    expect(result.type).toBe("approval");
    expect(result.gateId).toBe("gate-1");
  });

  it("classifies rejection with pending gate", () => {
    const gates: Gate[] = [{ id: "gate-2", operation: "shell_exec" }];
    expect(stream.classify("no", gates).type).toBe("approval");
    expect(stream.classify("reject", gates).type).toBe("approval");
    expect(stream.classify("deny", gates).type).toBe("approval");
  });

  it("classifies injection with @workspace syntax", () => {
    const result = stream.classify("@ws-child-1: focus on error handling");
    expect(result.type).toBe("injection");
    expect(result.targetWorkspace).toBe("ws-child-1");
    expect(result.content).toBe("focus on error handling");
  });

  it("defaults to goal for unrecognized input", () => {
    expect(stream.classify("implement authentication").type).toBe("goal");
    expect(stream.classify("refactor the database module").type).toBe("goal");
  });

  it("approval without pending gates falls through to goal", () => {
    // "yes" without pending gates is a goal, not an approval
    expect(stream.classify("yes, do that").type).toBe("goal");
  });

  // --- Buffering ---
  it("buffers and flushes input", () => {
    const s = new InteractionStream();
    s.bufferInput("first");
    s.bufferInput("second");
    expect(s.bufferedCount).toBe(2);

    const flushed = s.flush();
    expect(flushed).toEqual(["first", "second"]);
    expect(s.bufferedCount).toBe(0);
  });

  it("flush on empty buffer returns empty array", () => {
    const s = new InteractionStream();
    expect(s.flush()).toEqual([]);
  });

  it("classifies empty input as goal", () => {
    expect(stream.classify("").type).toBe("goal");
  });

  it("classifies whitespace-only as goal", () => {
    expect(stream.classify("   ").type).toBe("goal");
  });

  it("classifies very long input as goal", () => {
    const long = "implement " + "x".repeat(10_000);
    expect(stream.classify(long).type).toBe("goal");
  });

  it("classifies 'y' as approval with pending gate", () => {
    const gates: Gate[] = [{ id: "gate-1", operation: "file_write" }];
    expect(stream.classify("y", gates).type).toBe("approval");
  });

  it("classifies 'n' as approval with pending gate", () => {
    const gates: Gate[] = [{ id: "gate-1", operation: "file_write" }];
    expect(stream.classify("n", gates).type).toBe("approval");
  });

  it("injection with multiword workspace id", () => {
    const result = stream.classify("@my-workspace-123: do something");
    expect(result.type).toBe("injection");
    expect(result.targetWorkspace).toBe("my-workspace-123");
  });
});
