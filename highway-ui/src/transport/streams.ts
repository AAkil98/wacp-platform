import { type ConnectError } from "@connectrpc/connect";
import { getClient } from "./client.js";
import { classifyError } from "./errors.js";
import { useStore } from "../store/index.js";
import type { TrailEntry as ProtoTrailEntry } from "../gen/primitives_pb.js";
import { WorkspaceState } from "../gen/primitives_pb.js";
import type { WorkspaceStateChange } from "../gen/highway_pb.js";
import type { WorkspaceView as ProtoWorkspaceView } from "../gen/highway_pb.js";
import type { TrailEntry, WorkspaceView } from "../store/index.js";

// ── Proto enum → string mapping ──

const WORKSPACE_STATE_NAMES: Record<number, string> = {
  [WorkspaceState.UNSPECIFIED]: "WORKSPACE_STATE_UNSPECIFIED",
  [WorkspaceState.IDLE]: "WORKSPACE_STATE_IDLE",
  [WorkspaceState.ACTIVE]: "WORKSPACE_STATE_ACTIVE",
  [WorkspaceState.BLOCKED]: "WORKSPACE_STATE_BLOCKED",
  [WorkspaceState.SUSPENDED]: "WORKSPACE_STATE_SUSPENDED",
  [WorkspaceState.MIGRATING]: "WORKSPACE_STATE_MIGRATING",
  [WorkspaceState.INTEGRATING]: "WORKSPACE_STATE_INTEGRATING",
  [WorkspaceState.CONFLICTED]: "WORKSPACE_STATE_CONFLICTED",
  [WorkspaceState.CLOSED]: "WORKSPACE_STATE_CLOSED",
  [WorkspaceState.FAILED]: "WORKSPACE_STATE_FAILED",
};

function workspaceStateName(state: WorkspaceState): string {
  return WORKSPACE_STATE_NAMES[state] ?? "WORKSPACE_STATE_UNSPECIFIED";
}

// ── Proto → domain converters ──

function toTrailEntry(proto: ProtoTrailEntry): TrailEntry {
  const ts = proto.timestamp;
  return {
    timestamp: ts ? ts.physicalUs : 0n,
    eventType: proto.eventType,
    actor: proto.actor,
    workspaceId: proto.workspaceId,
    body: new TextDecoder().decode(proto.body),
    sequenceNumber: proto.sequenceNumber,
  };
}

function toWorkspaceView(proto: ProtoWorkspaceView): WorkspaceView {
  return {
    id: proto.id,
    state: workspaceStateName(proto.state),
    role: proto.role,
    parent: proto.parent,
    owner: proto.owner,
    originator: proto.originator,
    taskId: proto.taskId,
    checkpointCount: proto.checkpointCount,
    currentUsage: proto.currentUsage
      ? {
          tokens: proto.currentUsage.tokens,
          wallTimeMs: proto.currentUsage.wallTimeMs,
          storageBytes: proto.currentUsage.storageBytes,
          networkBytes: proto.currentUsage.networkBytes,
          costMicros: proto.currentUsage.costMicros,
        }
      : undefined,
    budget: proto.budget
      ? {
          maxTokens: proto.budget.maxTokens,
          maxWallTimeMs: proto.budget.maxWallTimeMs,
          maxStorageBytes: proto.budget.maxStorageBytes,
          maxNetworkBytes: proto.budget.maxNetworkBytes,
          maxCostMicros: proto.budget.maxCostMicros,
          warningThreshold: proto.budget.warningThreshold,
        }
      : undefined,
    createdAt: proto.createdAt?.physicalUs,
    lastActivity: proto.lastActivity?.physicalUs,
  };
}

// ── Stream wrappers ──

export interface StreamOptions {
  signal: AbortSignal;
}

export interface TrailStreamOptions extends StreamOptions {
  workspaceId?: string;
  fromBeginning?: boolean;
}

export interface WorkspaceStreamOptions extends StreamOptions {
  workspaceId?: string;
}

/**
 * Open a StreamTrail RPC and feed entries into the store.
 * Runs until the signal is aborted or the server closes the stream.
 * Rethrows classified errors for the session manager to handle.
 */
export async function startTrailStream(options: TrailStreamOptions): Promise<void> {
  const client = getClient();
  const { appendTrailEntry } = useStore.getState();

  try {
    for await (const entry of client.streamTrail(
      {
        workspaceId: options.workspaceId ?? "",
        eventType: "",
        fromBeginning: options.fromBeginning ?? true,
      },
      { signal: options.signal },
    )) {
      appendTrailEntry(toTrailEntry(entry));
    }
  } catch (err: unknown) {
    if (options.signal.aborted) return;
    const category = classifyError(err);
    throw Object.assign(err as ConnectError, { category });
  }
}

/**
 * Open a StreamWorkspaceChanges RPC and feed updates into the store.
 * Runs until the signal is aborted or the server closes the stream.
 */
export async function startWorkspaceStream(options: WorkspaceStreamOptions): Promise<void> {
  const client = getClient();

  try {
    for await (const change of client.streamWorkspaceChanges(
      { workspaceId: options.workspaceId ?? "" },
      { signal: options.signal },
    )) {
      handleWorkspaceChange(change);
    }
  } catch (err: unknown) {
    if (options.signal.aborted) return;
    const category = classifyError(err);
    throw Object.assign(err as ConnectError, { category });
  }
}

function handleWorkspaceChange(change: WorkspaceStateChange): void {
  const { addWorkspaceChange, upsertWorkspace } = useStore.getState();
  const views = useStore.getState().workspaces.views;

  addWorkspaceChange({
    workspaceId: change.workspaceId,
    previous: workspaceStateName(change.previous),
    current: workspaceStateName(change.current),
    timestamp: change.timestamp?.physicalUs ?? 0n,
  });

  // Update the workspace view's state if we have it
  const existing = views.get(change.workspaceId);
  if (existing) {
    upsertWorkspace({
      ...existing,
      state: workspaceStateName(change.current),
      lastActivity: change.timestamp?.physicalUs,
    });
  }
}

/**
 * Fetch a single workspace view and upsert it into the store.
 */
export async function fetchWorkspace(workspaceId: string, signal?: AbortSignal): Promise<void> {
  const client = getClient();
  const response = await client.getWorkspace({ workspaceId }, signal ? { signal } : undefined);
  useStore.getState().upsertWorkspace(toWorkspaceView(response));
}

/**
 * Fetch the task graph and set it in the store.
 */
export async function fetchTaskGraph(signal?: AbortSignal): Promise<void> {
  const client = getClient();
  const response = await client.getTaskGraph({}, signal ? { signal } : undefined);
  useStore.getState().setTaskGraph(
    response.tasks.map((t) => ({
      id: t.id,
      name: t.name,
      status: t.status.toString(),
      workspaceRef: t.workspaceRef,
      dependsOn: t.dependsOn,
      parentTask: t.parentTask,
    })),
  );
}

// Re-export converters for testing
export { toTrailEntry, toWorkspaceView, workspaceStateName };
