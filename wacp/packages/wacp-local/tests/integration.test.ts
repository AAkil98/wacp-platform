/**
 * Cross-module integration tests for @wacp/local.
 * Tests the full session lifecycle with real modules interacting.
 */
import { describe, it, expect, beforeEach } from "vitest";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as os from "node:os";

import { LocalSession, type SessionConfig } from "../src/session.js";
import { AutonomyManager } from "../src/autonomy.js";
import { InteractionStream, type Gate } from "../src/interaction.js";
import { LocalResources } from "../src/resources.js";
import { SessionContext } from "../src/context.js";
import { AutonomyError } from "../src/errors.js";

describe("Integration: Full Session Lifecycle", () => {
  let tmpDir: string;

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "wacp-integ-"));
  });

  it("session create → activate → resource ops → context → close", async () => {
    const config: SessionConfig = {
      workingDir: tmpDir,
      autonomyPreset: "assisted",
    };

    // 1. Create session
    const session = await LocalSession.create(config);
    expect(session.state).toBe("open");

    // 2. Activate
    await session.activate();
    expect(session.state).toBe("active");

    // 3. Read a file (assisted allows file_read)
    await fs.writeFile(path.join(tmpDir, "test.txt"), "hello world");
    const content = await session.resources.readFile("test.txt");
    expect(content).toBe("hello world");

    // 4. Write should be gated (assisted doesn't allow file_write)
    await expect(
      session.resources.writeFile("output.txt", "data"),
    ).rejects.toThrow(AutonomyError);

    // 5. Grant file_write
    session.autonomy.grant("file_write");
    await session.resources.writeFile("output.txt", "data");
    const written = await fs.readFile(path.join(tmpDir, "output.txt"), "utf-8");
    expect(written).toBe("data");

    // 6. Record in context
    session.context.addHistory("user: fix the bug");
    session.context.addHistory("assistant: I'll read test.txt");
    session.context.recordTrust("file_write", true);

    expect(session.context.history).toHaveLength(2);
    expect(session.context.trustLog).toHaveLength(1);

    // 7. Snapshot
    const snap = session.context.snapshot();
    expect(snap.historyLength).toBe(2);

    // 8. Close
    await session.close();
    expect(session.state).toBe("closed");
  });

  it("interaction stream classifies input and gates resolve through autonomy", async () => {
    const config: SessionConfig = { workingDir: tmpDir, autonomyPreset: "supervised" };
    const session = await LocalSession.create(config);
    await session.activate();

    const stream = new InteractionStream();

    // 1. Goal
    const goal = stream.classify("fix the authentication bug");
    expect(goal.type).toBe("goal");

    // 2. Agent tries to read a file → gated in supervised mode
    await expect(session.resources.readFile("any.txt")).rejects.toThrow(AutonomyError);

    // 3. Human approves → gate shown
    const gates: Gate[] = [{ id: "gate-1", operation: "file_read" }];
    const approval = stream.classify("yes", gates);
    expect(approval.type).toBe("approval");
    expect(approval.gateId).toBe("gate-1");

    // 4. Grant based on approval
    session.autonomy.grant("file_read");
    session.context.recordTrust("file_read", true);

    // 5. Now file read works
    await fs.writeFile(path.join(tmpDir, "code.ts"), "export const x = 1;");
    const code = await session.resources.readFile("code.ts");
    expect(code).toBe("export const x = 1;");

    await session.close();
  });

  it("autonomy preset escalation: supervised → assisted → autonomous", async () => {
    const mgr = new AutonomyManager("supervised");

    // Supervised: only llm_call
    expect(mgr.trustSurface().size).toBe(1);

    // Escalate to assisted
    mgr.resetToPreset("assisted");
    expect(mgr.check("file_read")).toBe(true);
    expect(mgr.check("file_write")).toBe(false);

    // Escalate to autonomous
    mgr.resetToPreset("autonomous");
    expect(mgr.check("file_write")).toBe(true);
    expect(mgr.check("shell_exec")).toBe(true);

    // Destructive ops still gated even in autonomous
    expect(mgr.check("file_delete")).toBe(false);
    expect(mgr.check("shell_exec_destructive")).toBe(false);
  });

  it("session context survives suspend/resume", async () => {
    const config: SessionConfig = { workingDir: tmpDir, autonomyPreset: "assisted" };
    const session = await LocalSession.create(config);
    await session.activate();

    // Add context
    session.context.addHistory("entry-1");
    session.context.recordTrust("file_read", true);

    // Suspend
    await session.suspend();
    expect(session.state).toBe("suspended");

    // Context survives
    expect(session.context.history).toHaveLength(1);
    expect(session.context.trustLog).toHaveLength(1);

    // Resume
    await session.activate();
    expect(session.state).toBe("active");
    expect(session.context.history).toHaveLength(1);

    await session.close();
  });

  it("path scoping prevents escape across all resource operations", async () => {
    const config: SessionConfig = { workingDir: tmpDir, autonomyPreset: "autonomous" };
    const session = await LocalSession.create(config);
    await session.activate();

    // All these should throw AutonomyError for path escape
    await expect(session.resources.readFile("../../etc/passwd")).rejects.toThrow(
      "escapes working directory",
    );
    session.autonomy.grant("file_delete");
    await expect(session.resources.deleteFile("../../important")).rejects.toThrow(
      "escapes working directory",
    );

    await session.close();
  });

  it("shell exec works end-to-end with autonomy gating", async () => {
    const config: SessionConfig = { workingDir: tmpDir, autonomyPreset: "autonomous" };
    const session = await LocalSession.create(config);
    await session.activate();

    // Create a file via shell
    const result = await session.resources.exec("echo hello > test.txt");
    expect(result.exitCode).toBe(0);

    // Verify file was created
    const content = await session.resources.readFile("test.txt");
    expect(content.trim()).toBe("hello");

    await session.close();
  });
});
