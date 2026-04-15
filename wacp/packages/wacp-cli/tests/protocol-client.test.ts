import { describe, it, expect } from "vitest";
import { SignalType, CheckpointStatus } from "../src/protocol-client.js";

describe("Protocol Client Constants", () => {
  it("signal types match proto enum values", () => {
    expect(SignalType.READY).toBe(1);
    expect(SignalType.STARTED).toBe(2);
    expect(SignalType.BLOCKED).toBe(3);
    expect(SignalType.CHECKPOINT).toBe(4);
    expect(SignalType.COMPLETE).toBe(5);
    expect(SignalType.FAILED).toBe(6);
    expect(SignalType.INTEGRATE).toBe(7);
    expect(SignalType.ACKNOWLEDGED).toBe(8);
    expect(SignalType.ESCALATION).toBe(9);
  });

  it("checkpoint status matches proto enum values", () => {
    expect(CheckpointStatus.PROVISIONAL).toBe(1);
    expect(CheckpointStatus.FINAL).toBe(2);
  });

  // Note: CoordinatorClient and AgentClient require a running gRPC server
  // to test. Those are verified via the E2E integration tests with the
  // Rust runtime. Here we verify the type contracts and constants only.
});
