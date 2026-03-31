import { describe, it, expect, beforeEach, vi } from "vitest";

describe("client", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it("getClient before init throws", async () => {
    const { getClient } = await import("./client.js");
    expect(() => getClient()).toThrow("Transport not initialized");
  });

  it("getTransport before init throws", async () => {
    const { getTransport } = await import("./client.js");
    expect(() => getTransport()).toThrow("Transport not initialized");
  });

  it("initTransport sets transport so getTransport succeeds", async () => {
    const { initTransport, getTransport } = await import("./client.js");
    initTransport("http://localhost:9091");
    expect(() => getTransport()).not.toThrow();
  });
});
