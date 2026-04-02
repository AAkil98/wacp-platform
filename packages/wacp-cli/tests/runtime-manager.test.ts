import { describe, it, expect } from "vitest";
import { RuntimeManager, DEFAULT_RUNTIME_CONFIG } from "../src/runtime-manager.js";

describe("RuntimeManager", () => {
  it("has correct default config", () => {
    const mgr = new RuntimeManager();
    expect(mgr.config.agentPort).toBe(9400);
    expect(mgr.config.coordinatorPort).toBe(9402);
    expect(mgr.config.binaryPath).toBe("wacp-runtime");
  });

  it("accepts custom config", () => {
    const mgr = new RuntimeManager({ agentPort: 19400, coordinatorPort: 19402 });
    expect(mgr.config.agentPort).toBe(19400);
    expect(mgr.config.coordinatorPort).toBe(19402);
    expect(mgr.config.binaryPath).toBe("wacp-runtime"); // default preserved
  });

  it("generates correct URLs", () => {
    const mgr = new RuntimeManager({ agentPort: 9400, coordinatorPort: 9402, highwayPort: 9401 });
    expect(mgr.agentUrl).toBe("127.0.0.1:9400");
    expect(mgr.coordinatorUrl).toBe("127.0.0.1:9402");
    expect(mgr.highwayUrl).toBe("127.0.0.1:9401");
  });

  it("starts as not ready", () => {
    const mgr = new RuntimeManager();
    expect(mgr.ready).toBe(false);
  });

  it("start with missing binary fails with timeout", async () => {
    // /bin/false exits immediately — runtime won't become ready
    const mgr = new RuntimeManager({ binaryPath: "/bin/false" });
    try {
      await mgr.start(500);
      expect.unreachable("should have thrown");
    } catch (err) {
      expect((err as Error).message).toContain("failed to start");
    }
  });

  it("stop on unstarted is safe", async () => {
    const mgr = new RuntimeManager();
    await mgr.stop(); // should not throw
    expect(mgr.ready).toBe(false);
  });
});
