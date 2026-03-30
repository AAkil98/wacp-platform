import { describe, it, expect } from "vitest";
import { toTrailEntry, toWorkspaceView, workspaceStateName } from "./streams.js";
import { WorkspaceState } from "../gen/primitives_pb.js";
import { create } from "@bufbuild/protobuf";
import { TrailEntrySchema, TimestampSchema, ResourceUsageSchema, ResourceBudgetSchema } from "../gen/primitives_pb.js";
import { WorkspaceViewSchema } from "../gen/highway_pb.js";

describe("workspaceStateName", () => {
  it("maps all WorkspaceState enum values", () => {
    expect(workspaceStateName(WorkspaceState.ACTIVE)).toBe("WORKSPACE_STATE_ACTIVE");
    expect(workspaceStateName(WorkspaceState.BLOCKED)).toBe("WORKSPACE_STATE_BLOCKED");
    expect(workspaceStateName(WorkspaceState.IDLE)).toBe("WORKSPACE_STATE_IDLE");
    expect(workspaceStateName(WorkspaceState.FAILED)).toBe("WORKSPACE_STATE_FAILED");
    expect(workspaceStateName(WorkspaceState.CLOSED)).toBe("WORKSPACE_STATE_CLOSED");
    expect(workspaceStateName(WorkspaceState.SUSPENDED)).toBe("WORKSPACE_STATE_SUSPENDED");
    expect(workspaceStateName(WorkspaceState.MIGRATING)).toBe("WORKSPACE_STATE_MIGRATING");
    expect(workspaceStateName(WorkspaceState.INTEGRATING)).toBe("WORKSPACE_STATE_INTEGRATING");
    expect(workspaceStateName(WorkspaceState.CONFLICTED)).toBe("WORKSPACE_STATE_CONFLICTED");
  });

  it("returns UNSPECIFIED for unknown values", () => {
    expect(workspaceStateName(99 as WorkspaceState)).toBe("WORKSPACE_STATE_UNSPECIFIED");
  });
});

describe("toTrailEntry", () => {
  it("converts proto TrailEntry to store TrailEntry", () => {
    const ts = create(TimestampSchema, { physicalUs: 1711900000000000n, logical: 42 });
    const proto = create(TrailEntrySchema, {
      id: "entry-1",
      timestamp: ts,
      workspaceId: "ws-1",
      actor: "agent-A",
      eventType: "signal_emitted",
      body: new TextEncoder().encode('{"signal":"ready"}'),
      sequenceNumber: 7n,
      chainHash: new Uint8Array(32),
    });

    const result = toTrailEntry(proto);
    expect(result.timestamp).toBe(1711900000000000n);
    expect(result.eventType).toBe("signal_emitted");
    expect(result.actor).toBe("agent-A");
    expect(result.workspaceId).toBe("ws-1");
    expect(result.body).toBe('{"signal":"ready"}');
    expect(result.sequenceNumber).toBe(7n);
  });

  it("handles missing timestamp gracefully", () => {
    const proto = create(TrailEntrySchema, {
      id: "entry-no-ts",
      eventType: "test",
      actor: "system",
      workspaceId: "",
      body: new Uint8Array(),
      sequenceNumber: 0n,
    });

    const result = toTrailEntry(proto);
    expect(result.timestamp).toBe(0n);
  });
});

describe("toWorkspaceView", () => {
  it("converts proto WorkspaceView to store WorkspaceView", () => {
    const ts = create(TimestampSchema, { physicalUs: 1711900000000000n, logical: 0 });
    const usage = create(ResourceUsageSchema, {
      tokens: 5000n,
      wallTimeMs: 12000n,
      storageBytes: 1024n,
      networkBytes: 2048n,
      costMicros: 100n,
    });
    const budget = create(ResourceBudgetSchema, {
      maxTokens: 10000n,
      maxWallTimeMs: 60000n,
      maxStorageBytes: 1048576n,
      maxNetworkBytes: 1048576n,
      maxCostMicros: 1000000n,
      warningThreshold: 0.8,
    });
    const proto = create(WorkspaceViewSchema, {
      id: "ws-1",
      state: WorkspaceState.ACTIVE,
      role: "worker",
      parent: "root",
      owner: "user-1",
      originator: "System",
      taskId: "t-1",
      currentUsage: usage,
      budget: budget,
      checkpointCount: 3,
      createdAt: ts,
      lastActivity: ts,
    });

    const result = toWorkspaceView(proto);
    expect(result.id).toBe("ws-1");
    expect(result.state).toBe("WORKSPACE_STATE_ACTIVE");
    expect(result.role).toBe("worker");
    expect(result.parent).toBe("root");
    expect(result.owner).toBe("user-1");
    expect(result.originator).toBe("System");
    expect(result.taskId).toBe("t-1");
    expect(result.checkpointCount).toBe(3);
    expect(result.currentUsage?.tokens).toBe(5000n);
    expect(result.budget?.maxTokens).toBe(10000n);
    expect(result.budget?.warningThreshold).toBe(0.8);
    expect(result.createdAt).toBe(1711900000000000n);
    expect(result.lastActivity).toBe(1711900000000000n);
  });

  it("handles missing optional fields", () => {
    const proto = create(WorkspaceViewSchema, {
      id: "ws-minimal",
      state: WorkspaceState.IDLE,
      role: "observer",
      parent: "",
      owner: "",
      originator: "",
      taskId: "",
      checkpointCount: 0,
    });

    const result = toWorkspaceView(proto);
    expect(result.id).toBe("ws-minimal");
    expect(result.state).toBe("WORKSPACE_STATE_IDLE");
    expect(result.currentUsage).toBeUndefined();
    expect(result.budget).toBeUndefined();
    expect(result.createdAt).toBeUndefined();
    expect(result.lastActivity).toBeUndefined();
  });
});
