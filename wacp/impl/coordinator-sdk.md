# WACP Implementation: Coordinator SDK

```yaml
id: wacp-impl-coordinator-sdk
type: implementation-spec
status: draft
created: 2026-04-01
lineage: LAYER-MAPPING.md (M2)
protocol_sections:
  - §4.5 (task — lifecycle, graph)
  - §5.2 (coordinator role)
  - §6 (workspace lifecycle — create, terminate)
  - §7 (integration — merge, conflict)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-protocol-interface
  - wacp-impl-agent-sdk-v2
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, middleware, sdk, coordinator, orchestration]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Design Principles](#2-design-principles)
3. [CoordinatorService Proto](#3-coordinatorservice-proto)
4. [CoordinatorContext API](#4-coordinatorcontext-api)
5. [Task Graph Operations](#5-task-graph-operations)
6. [Workspace Lifecycle](#6-workspace-lifecycle)
7. [Signal Consumption](#7-signal-consumption)
8. [Integration](#8-integration)
9. [Server-Side Implementation](#9-server-side-implementation)
10. [Crate Structure](#10-crate-structure)
11. [Test Requirements](#11-test-requirements)
12. [References](#12-references)

---

## 1. Purpose

This spec defines the coordinator SDK — the client-facing interface for driving WACP coordination externally. It answers "how does a custom coordinator interact with the runtime" — not "how does the internal coordinator make decisions" (that's the coordinator crate) or "how do agents interact" (that's the agent SDK).

The WACP runtime includes a built-in coordinator actor that manages the workspace tree, task graph, and all orchestration. The coordinator SDK does not replace this — it provides a typed client that drives it via gRPC. The internal coordinator remains the authority; the SDK provides ergonomic access.

**Scope.** New `CoordinatorService` protobuf definition. Server-side implementation bridging RPCs to `wacp-coordinator` internals. `CoordinatorContext` client struct in a new `wacp-coordinator-sdk` crate. 15+ methods for task graph management, workspace lifecycle, signal consumption, and integration.

**Not in scope.** Modifying the internal coordinator logic. Agent-side operations (agent SDK). Human oversight (highway service).

**Key decision.** The CoordinatorService is a new gRPC service alongside AgentService and HighwayService. It runs on its own port (default 9092). This separation ensures coordinator clients don't accidentally call agent or highway RPCs.

---

## 2. Design Principles

**Principle 1: Client, not replacement.** The SDK is a gRPC client. It sends requests; the runtime's coordinator processes them. If the coordinator rejects an operation (wrong state, missing precondition), the SDK surfaces the error. It does not second-guess the coordinator.

**Principle 2: Operations, not decisions.** The SDK provides `dispatch()`, `integrate()`, `abort()` — operations. It does not provide `decide_what_to_do()` — that's the custom coordinator's job. The SDK is the hands; the application is the brain.

**Principle 3: Streams for signals, unary for actions.** Reading is streaming (signals arrive asynchronously). Writing is unary (each action is a request-response). This matches the coordinator's interaction pattern: observe continuously, act discretely.

---

## 3. CoordinatorService Proto

New file: `proto/coordinator.proto`

```protobuf
service CoordinatorService {
  // --- Task graph ---
  rpc SubmitGoal(SubmitGoalRequest) returns (SubmitGoalResponse);
  rpc Decompose(DecomposeRequest) returns (DecomposeResponse);
  rpc GetReadyTasks(GetReadyTasksRequest) returns (GetReadyTasksResponse);
  rpc GetTaskGraph(GetTaskGraphRequest) returns (TaskGraphView);
  rpc CancelTask(CancelTaskRequest) returns (CancelTaskResponse);

  // --- Workspace lifecycle ---
  rpc Dispatch(DispatchRequest) returns (DispatchResponse);
  rpc AbortWorkspace(AbortWorkspaceRequest) returns (AbortWorkspaceResponse);
  rpc SuspendWorkspace(SuspendWorkspaceRequest) returns (SuspendWorkspaceResponse);
  rpc ResumeWorkspace(ResumeWorkspaceRequest) returns (ResumeWorkspaceResponse);

  // --- Communication ---
  rpc SendDirective(SendDirectiveRequest) returns (SendDirectiveResponse);
  rpc SendFeedback(SendFeedbackRequest) returns (SendFeedbackResponse);

  // --- Integration ---
  rpc TriggerIntegration(TriggerIntegrationRequest) returns (TriggerIntegrationResponse);

  // --- Observation ---
  rpc GetAllocatable(GetAllocatableRequest) returns (GetAllocatableResponse);
  rpc StreamSignals(StreamSignalsRequest) returns (stream SignalEvent);
}
```

**15 RPCs:** 5 task graph, 4 workspace lifecycle, 2 communication, 1 integration, 1 budget, 1 streaming + 1 from highway (GetTaskGraph reused).

---

## 4. CoordinatorContext API

```rust
/// Client-facing coordinator API.
pub struct CoordinatorContext {
    client: CoordinatorServiceClient<tonic::transport::Channel>,
    session_id: String,
    cancellation: CancellationToken,
}

impl CoordinatorContext {
    pub async fn connect(url: &str, auth_token: &str) -> Result<Self, Error>;

    // --- Task graph ---
    pub async fn submit_goal(&self, description: &str, context: &[u8]) -> Result<String, Error>;
    pub async fn decompose(&self, tasks: Vec<TaskDefinition>) -> Result<Vec<String>, Error>;
    pub async fn ready_tasks(&self) -> Result<Vec<TaskView>, Error>;
    pub async fn task_graph(&self) -> Result<TaskGraphView, Error>;
    pub async fn cancel_task(&self, task_id: &str, reason: &str) -> Result<(), Error>;

    // --- Workspace lifecycle ---
    pub async fn dispatch(&self, task_id: &str, config: WorkspaceConfig) -> Result<WorkspaceHandle, Error>;
    pub async fn abort(&self, workspace_id: &str, reason: &str) -> Result<(), Error>;
    pub async fn suspend(&self, workspace_id: &str) -> Result<(), Error>;
    pub async fn resume(&self, workspace_id: &str) -> Result<(), Error>;

    // --- Communication ---
    pub async fn send_directive(&self, workspace_id: &str, content: &[u8]) -> Result<String, Error>;
    pub async fn feedback(&self, workspace_id: &str, content: &[u8]) -> Result<String, Error>;

    // --- Signals ---
    pub async fn signals(&self) -> Result<SignalStream, Error>;
    pub async fn wait_for_signal(&self, filter: SignalFilter) -> Result<SignalEvent, Error>;

    // --- Integration ---
    pub async fn integrate(&self, workspace_id: &str) -> Result<IntegrationResult, Error>;

    // --- Budget ---
    pub async fn allocatable(&self) -> Result<ResourceBudget, Error>;

    // --- Cancellation ---
    pub fn cancellation_token(&self) -> &CancellationToken;
}
```

**Helper types:**

```rust
pub struct TaskDefinition {
    pub name: String,
    pub description: String,
    pub depends_on: Vec<String>,  // task IDs
    pub config: serde_json::Value,
}

pub struct WorkspaceConfig {
    pub role: String,
    pub directive: Vec<u8>,
    pub tools: Vec<String>,     // tool names to mount
    pub budget: Option<ResourceBudget>,
}

pub struct WorkspaceHandle {
    pub workspace_id: String,
    pub task_id: String,
}

pub struct SignalFilter {
    pub workspace_id: Option<String>,
    pub signal_type: Option<String>,
}
```

---

## 5. Task Graph Operations

**`submit_goal(description, context)`:** Registers a top-level goal with the coordinator. Returns a goal ID. The coordinator creates the root workspace if one doesn't exist.

**`decompose(tasks)`:** Creates a task graph (DAG) from the given task definitions. Each task has a name, description, dependencies (as task IDs), and configuration. The coordinator validates the DAG (no cycles, all dependency IDs valid) and returns assigned task IDs.

**`ready_tasks()`:** Returns tasks whose dependencies are all satisfied (completed or integrated). These are eligible for dispatch. The coordinator tracks readiness counters internally; this method is a read.

**`cancel_task(task_id, reason)`:** Cancels a task. If the task is assigned to a workspace, the workspace is aborted. If the task has dependents, they are cascade-cancelled.

---

## 6. Workspace Lifecycle

**`dispatch(task_id, config)`:** Creates a workspace for the given task. The coordinator assigns the workspace to the task, sets up the directive, mounts tools, allocates budget. Returns a handle with the workspace ID.

**`abort(workspace_id, reason)`:** Terminates a workspace. The workspace transitions to Failed. In-flight agent work is cancelled.

**`suspend(workspace_id)` / `resume(workspace_id)`:** Pauses and resumes a workspace. The agent's work is paused (commands stream delivers Suspend/Resume).

---

## 7. Signal Consumption

**`signals()`:** Opens a streaming RPC. Returns a `SignalStream` that yields `SignalEvent` as child workspaces emit signals (started, blocked, complete, failed, escalation).

**`wait_for_signal(filter)`:** Convenience — opens the stream, waits for the first matching event, returns it. Useful for synchronous orchestration patterns (dispatch → wait for complete → integrate).

---

## 8. Integration

**`integrate(workspace_id)`:** Triggers the integration pipeline for a completed workspace. The coordinator runs: find final checkpoint → select strategy → execute merge → evaluate quality → accept/revise/reject. Returns the `IntegrationResult`.

---

## 9. Server-Side Implementation

The `CoordinatorService` gRPC server is implemented in `crates/wacp-transport/`. Each RPC maps to a `wacp-coordinator` internal operation:

| RPC | Coordinator operation |
|-----|----------------------|
| `SubmitGoal` | Create root task + workspace |
| `Decompose` | `task_graph.add_tasks()` |
| `GetReadyTasks` | `task_graph.ready_tasks()` |
| `Dispatch` | `coordinator.dispatch()` |
| `AbortWorkspace` | `coordinator.abort()` |
| `SuspendWorkspace` | Send `Suspend` command to workspace actor |
| `ResumeWorkspace` | Send `Resume` command to workspace actor |
| `SendDirective` | `handler.inject_envelope()` with type "directive" |
| `SendFeedback` | `handler.inject_envelope()` with type "feedback" |
| `TriggerIntegration` | `integration_queue.enqueue()` |
| `StreamSignals` | `event_bus.subscribe()` → filter signal events |
| `GetAllocatable` | `budget_enforcer.remaining()` |
| `CancelTask` | `task_graph.cancel()` + cascade abort |

---

## 10. Crate Structure

**New crate:** `crates/wacp-coordinator-sdk/`

```
crates/wacp-coordinator-sdk/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Public exports
│   ├── context.rs      # CoordinatorContext struct + methods
│   ├── types.rs        # TaskDefinition, WorkspaceConfig, WorkspaceHandle, SignalFilter
│   ├── error.rs        # Error enum
│   └── streams.rs      # SignalStream wrapper
└── tests/
```

**New proto:** `proto/coordinator.proto`

**Transport changes:** `crates/wacp-transport/` adds `CoordinatorServiceImpl` alongside existing `AgentServiceImpl` and `HighwayServiceImpl`.

**Dependencies:** `wacp-types`, `tonic`, `tokio`, `tokio-util`, `futures`, `thiserror`.

---

## 11. Test Requirements

| Area | Tests |
|------|-------|
| `connect` | Success, auth failure. |
| `submit_goal` | Returns goal ID. |
| `decompose` | Valid DAG accepted. Cycle rejected. Missing dep rejected. |
| `ready_tasks` | Returns only tasks with satisfied deps. |
| `dispatch` | Creates workspace, returns handle. Task not ready → error. |
| `abort` | Workspace transitions to Failed. |
| `suspend` / `resume` | Workspace state changes. |
| `send_directive` / `feedback` | Envelope delivered. |
| `signals` | Stream yields events from child workspaces. |
| `wait_for_signal` | Returns matching signal. Timeout if none. |
| `integrate` | Returns accept/revise/reject result. |
| `allocatable` | Returns remaining budget. |
| `cancel_task` | Task cancelled, dependents cascade. |

**Total target: ~20 tests.** Tests use `InProcessTransport` with a real coordinator — no mock coordinator.

---

## 12. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Runtime spec | §3 (process model) | §9 | Coordinator actor internals |
| Coordinator modules | §10 (task_graph.rs, dispatch.rs) | §5, §6, §9 | Internal operations mapped to RPCs |
| Protocol interface spec | §4–5 | §3 | gRPC service patterns |
| LAYER-MAPPING.md | M2 | §1 | CoordinatorContext design |
| Agent SDK v2 spec | §4 | §4 | Tool integration pattern |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
