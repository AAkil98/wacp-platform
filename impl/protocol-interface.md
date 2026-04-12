# WACP Implementation: Protocol Interface

```yaml
id: wacp-impl-protocol-interface
type: implementation-spec
status: complete
created: 2026-03-18
lineage: PROTOCOL.md (wacp-v0.1)
protocol_sections:
  - §4 (core primitives)
  - §5 (roles and permissions)
  - §6 (workspace lifecycle)
  - §8 (human highway)
  - §9 (trail)
depends_on:
  - wacp-impl-runtime
  - wacp-impl-storage
  - wacp-spec-workspace
  - wacp-spec-envelope
  - wacp-spec-signal
  - wacp-spec-checkpoint
  - wacp-spec-task
  - wacp-spec-trail
  - wacp-spec-identity
  - wacp-spec-user
  - wacp-spec-roles
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, implementation, protobuf, grpc, interface, transport, serialization]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Interface Principles](#2-interface-principles)
3. [Protobuf Type Definitions](#3-protobuf-type-definitions)
4. [Agent Service Contract](#4-agent-service-contract)
5. [Highway Service Contract](#5-highway-service-contract)
6. [Serialization Rules](#6-serialization-rules)
7. [Authentication at the Boundary](#7-authentication-at-the-boundary)
8. [Transport Trait](#8-transport-trait)
9. [gRPC Implementation](#9-grpc-implementation)
10. [Versioning and Compatibility](#10-versioning-and-compatibility)
11. [References](#11-references)

## 1. Purpose

This spec defines the formal boundary between the Rust runtime and everything outside it — agent SDKs and highway clients. It answers "what crosses the wire" — the protobuf message definitions, gRPC service contracts, and serialization rules that all three languages (Rust, Python, TypeScript) share.

The runtime spec defines what happens inside the trust root. The storage spec defines how data is persisted. This spec defines how external clients talk to the runtime. It is the single source of truth for message shapes — the `.proto` files that generate code in all three languages.

**Scope.** Protobuf type definitions for all protocol primitives. Two gRPC service definitions (agent-facing, highway-facing). Serialization rules that map protocol concepts to wire format. Authentication at the boundary. The transport trait that abstracts over gRPC and alternative transports. Versioning strategy for protocol evolution.

**Not in scope.** Agent SDK surface area and developer ergonomics (sdk-agent spec). Highway UI design and rendering. Runtime internals — this spec describes the external surface, not the internal implementation.

**Design constraint.** The `.proto` files are the contract. The Rust runtime generates server stubs from them. The Python SDK and TypeScript highway UI generate client stubs from them. If it's not in a `.proto` file, it doesn't cross the boundary. No side-channel communication, no out-of-band messages, no implicit behavior.

---

## 2. Interface Principles

Five principles govern the protocol interface. They resolve ambiguity when designing message shapes and service contracts.

**Principle 1: The runtime assigns, the client proposes.** Identifiers (workspace id, envelope id, checkpoint id, trail entry id) are always runtime-assigned (identity spec, rule 1). A client may include a `client_request_id` for correlation — the runtime echoes it back in responses — but this is a client-side convenience, not a protocol identifier. The runtime ignores client-supplied protocol identifiers and overwrites them.

**Principle 2: Closed sets are enums, open sets are strings.** Signal types (11, closed) are a protobuf `enum`. Workspace states (9, closed) are a protobuf `enum`. Envelope types (3 base + taxonomy extensions, open) are a `string`. Checkpoint types (2 base + taxonomy extensions, open) are a `string`. Role names (3 base + taxonomy extensions, open) are a `string`. This reflects the protocol's extensibility model: enums for things the protocol fixes, strings for things the taxonomy extends.

**Principle 3: One message per protocol action.** Each client action maps to exactly one request message. Sending an envelope is one request. Emitting a signal is one request. Creating a checkpoint is one request. No multi-step client-side transactions. The runtime handles all internal sequencing (permission check → trail write → state update) atomically from the client's perspective.

**Principle 4: Errors are structured, not codes.** Error responses carry a typed error with a machine-readable category, the protocol rule that was violated, and a human-readable explanation. Clients can programmatically distinguish "permission denied" from "illegal transition" from "budget exceeded" without parsing strings.

**Principle 5: Streaming for observation, unary for action.** Actions (send envelope, emit signal, create checkpoint) are unary RPCs — one request, one response. Observations (trail streaming, gate events, inbox delivery) are server-streaming RPCs — the server pushes events as they occur. The client does not poll. This maps to the protocol's distinction between agent actions (discrete) and runtime notifications (continuous).

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
## 3. Protobuf Type Definitions

This section defines the protobuf messages for all protocol primitives. These live in `proto/primitives.proto` — the foundational type definitions that both service contracts import.

**File organization.** Four `.proto` files:

| File | Contains |
|------|----------|
| `proto/primitives.proto` | Enums, identifiers, and message types for all six core primitives |
| `proto/agent.proto` | Agent-facing gRPC service definition (§4) |
| `proto/highway.proto` | Highway-facing gRPC service definition (§5) |
| `proto/taxonomy.proto` | Taxonomy configuration messages (loaded at init, not a runtime service) |

**Enums (closed sets).**

```protobuf
syntax = "proto3";
package wacp.v1;

enum SignalType {
    SIGNAL_TYPE_UNSPECIFIED = 0;
    SIGNAL_TYPE_READY = 1;
    SIGNAL_TYPE_STARTED = 2;
    SIGNAL_TYPE_BLOCKED = 3;
    SIGNAL_TYPE_CHECKPOINT = 4;
    SIGNAL_TYPE_COMPLETE = 5;
    SIGNAL_TYPE_FAILED = 6;
    SIGNAL_TYPE_INTEGRATE = 7;
    SIGNAL_TYPE_ACKNOWLEDGED = 8;
    SIGNAL_TYPE_ESCALATION = 9;
    SIGNAL_TYPE_SUSPEND = 10;
    SIGNAL_TYPE_MIGRATE = 11;
}

enum WorkspaceState {
    WORKSPACE_STATE_UNSPECIFIED = 0;
    WORKSPACE_STATE_IDLE = 1;
    WORKSPACE_STATE_ACTIVE = 2;
    WORKSPACE_STATE_BLOCKED = 3;
    WORKSPACE_STATE_SUSPENDED = 4;
    WORKSPACE_STATE_MIGRATING = 5;
    WORKSPACE_STATE_INTEGRATING = 6;
    WORKSPACE_STATE_CONFLICTED = 7;
    WORKSPACE_STATE_CLOSED = 8;
    WORKSPACE_STATE_FAILED = 9;
}

enum EnvelopeState {
    ENVELOPE_STATE_UNSPECIFIED = 0;
    ENVELOPE_STATE_CREATED = 1;
    ENVELOPE_STATE_VALIDATED = 2;
    ENVELOPE_STATE_DELIVERED = 3;
    ENVELOPE_STATE_ACKNOWLEDGED = 4;
    ENVELOPE_STATE_REJECTED = 5;
}

enum TaskStatus {
    TASK_STATUS_UNSPECIFIED = 0;
    TASK_STATUS_DRAFT = 1;
    TASK_STATUS_PENDING = 2;
    TASK_STATUS_ASSIGNED = 3;
    TASK_STATUS_IN_PROGRESS = 4;
    TASK_STATUS_COMPLETED = 5;
    TASK_STATUS_FAILED = 6;
    TASK_STATUS_INTEGRATED = 7;
    TASK_STATUS_CANCELLED = 8;
}

enum CheckpointStatus {
    CHECKPOINT_STATUS_UNSPECIFIED = 0;
    CHECKPOINT_STATUS_PROVISIONAL = 1;
    CHECKPOINT_STATUS_FINAL = 2;
}

enum Confidence {
    CONFIDENCE_UNSPECIFIED = 0;
    CONFIDENCE_HIGH = 1;
    CONFIDENCE_MEDIUM = 2;
    CONFIDENCE_LOW = 3;
}

enum EnvelopePriority {
    ENVELOPE_PRIORITY_UNSPECIFIED = 0;
    ENVELOPE_PRIORITY_NORMAL = 1;
    ENVELOPE_PRIORITY_URGENT = 2;
    ENVELOPE_PRIORITY_BLOCKING = 3;
}

enum EnvelopeOrigin {
    ENVELOPE_ORIGIN_UNSPECIFIED = 0;
    ENVELOPE_ORIGIN_AGENT = 1;
    ENVELOPE_ORIGIN_HUMAN = 2;
}
```

**Core messages.**

```protobuf
message Timestamp {
    uint64 physical_us = 1;     // microseconds since Unix epoch
    uint32 logical = 2;         // logical counter (16-bit range, stored as uint32)
}

message Envelope {
    string id = 1;              // runtime-assigned
    string from_workspace = 2;
    string to_workspace = 3;
    string type = 4;            // open set: "directive", "feedback", "query", or taxonomy-registered
    bytes payload = 5;
    string in_reply_to = 6;     // envelope id, empty if unsolicited
    Timestamp timestamp = 7;    // runtime-assigned
    EnvelopePriority priority = 8;
    EnvelopeOrigin origin = 9;  // runtime-assigned, immutable
}

message Signal {
    SignalType type = 1;
    string workspace_id = 2;
    Timestamp timestamp = 3;    // runtime-assigned
    string reason = 4;          // for BLOCKED and FAILED
    bytes context = 5;          // for ESCALATION
}

message Checkpoint {
    string id = 1;              // runtime-assigned
    string workspace_id = 2;
    string type = 3;            // open set: "artifact", "observation", or taxonomy-registered
    bytes payload = 4;          // content or reference to content-addressed blob
    string content_hash = 5;    // SHA-256 hex of payload
    string intent = 6;
    string parent_checkpoint = 7; // checkpoint id, empty for first
    CheckpointStatus status = 8;
    Confidence confidence = 9;
    Timestamp timestamp = 10;   // runtime-assigned
    ResourceUsage resource_usage = 11;
}

message Task {
    string id = 1;              // runtime-assigned
    string name = 2;
    string description = 3;
    repeated string depends_on = 4;
    string parent_task = 5;     // empty for root tasks
    TaskStatus status = 6;
    string workspace_ref = 7;
    repeated string workspace_history = 8;
    string checkpoint_ref = 9;
}

message TrailEntry {
    string id = 1;              // runtime-assigned
    Timestamp timestamp = 2;
    string workspace_id = 3;    // empty for system-level events
    string actor = 4;           // role name, "protocol", or user_id
    string event_type = 5;
    bytes body = 6;             // type-specific payload
    uint64 sequence_number = 7; // global sequence
    bytes chain_hash = 8;       // SHA-256 hash chain link
}

message ResourceUsage {
    uint64 tokens = 1;
    uint64 wall_time_ms = 2;
    uint64 storage_bytes = 3;
    uint64 network_bytes = 4;
    uint64 cost_micros = 5;     // cost in microdollars
}

message ProtocolError {
    ErrorCategory category = 1;
    string rule = 2;            // protocol section reference (e.g., "§5.5")
    string message = 3;         // human-readable explanation
    string request_id = 4;      // echoed client_request_id
}

enum ErrorCategory {
    ERROR_CATEGORY_UNSPECIFIED = 0;
    ERROR_CATEGORY_PERMISSION_DENIED = 1;
    ERROR_CATEGORY_ILLEGAL_TRANSITION = 2;
    ERROR_CATEGORY_VALIDATION_FAILED = 3;
    ERROR_CATEGORY_BUDGET_EXCEEDED = 4;
    ERROR_CATEGORY_TIMEOUT = 5;
    ERROR_CATEGORY_NOT_FOUND = 6;
    ERROR_CATEGORY_DELIVERY_FAILED = 7;
    ERROR_CATEGORY_INTERNAL = 8;
}
```

**The `UNSPECIFIED` value.** Every enum has a zero value named `*_UNSPECIFIED`. Protobuf 3 uses zero as the default for unset fields. A message with `signal_type = 0` means "not set," not "ready." The runtime rejects any message where a required enum field is `UNSPECIFIED`. This prevents ambiguity between "the client explicitly chose the first enum value" and "the client forgot to set the field."

## 4. Agent Service Contract

The agent service is the gRPC interface that agent SDKs connect to. One connection per workspace. The agent authenticates, binds to a workspace, and interacts through the RPCs defined here.

```protobuf
// proto/agent.proto
syntax = "proto3";
package wacp.v1;

import "primitives.proto";

service AgentService {
    // --- Connection lifecycle ---

    // Bind to a workspace. Called once after connection.
    // Returns the workspace's directive, context, role, and current state.
    rpc Bind(BindRequest) returns (BindResponse);

    // --- Actions (unary) ---

    // Send an envelope to another workspace.
    rpc SendEnvelope(SendEnvelopeRequest) returns (SendEnvelopeResponse);

    // Emit a signal declaring a state change.
    rpc EmitSignal(EmitSignalRequest) returns (EmitSignalResponse);

    // Create a checkpoint recording progress.
    rpc CreateCheckpoint(CreateCheckpointRequest) returns (CreateCheckpointResponse);

    // Query the local trail.
    rpc QueryTrail(QueryTrailRequest) returns (QueryTrailResponse);

    // Read a resource within the visibility set.
    rpc ReadResource(ReadResourceRequest) returns (ReadResourceResponse);

    // --- Observations (server-streaming) ---

    // Receive envelopes as they arrive in the inbox.
    // The stream stays open for the workspace's lifetime.
    rpc ReceiveEnvelopes(ReceiveEnvelopesRequest) returns (stream Envelope);

    // Receive coordinator commands (feedback, visibility grants).
    rpc ReceiveCommands(ReceiveCommandsRequest) returns (stream Command);
}
```

**Request and response messages:**

```protobuf
message BindRequest {
    string workspace_id = 1;    // which workspace to bind to
    string auth_token = 2;      // authentication credential
    string client_request_id = 3;
}

message BindResponse {
    string workspace_id = 1;
    WorkspaceState state = 2;
    string role = 3;            // effective role name
    Envelope directive = 4;     // the task assignment
    bytes context = 5;          // read-only info from coordinator
    repeated string visibility = 6;  // resource ids this workspace can read
    repeated string authority = 7;   // resource ids this workspace can modify
    ResourceBudget budget = 8;  // current budget limits, if any
}

message SendEnvelopeRequest {
    string to_workspace = 1;
    string type = 2;
    bytes payload = 3;
    string in_reply_to = 4;
    EnvelopePriority priority = 5;
    string client_request_id = 6;
}

message SendEnvelopeResponse {
    string envelope_id = 1;     // runtime-assigned
    Timestamp timestamp = 2;
    string client_request_id = 3;
}

message EmitSignalRequest {
    SignalType type = 1;
    string reason = 2;          // required for BLOCKED and FAILED
    bytes context = 3;          // required for ESCALATION
    string client_request_id = 4;
}

message EmitSignalResponse {
    Timestamp timestamp = 1;
    string client_request_id = 2;
}

message CreateCheckpointRequest {
    string type = 1;            // "artifact", "observation", or taxonomy-registered
    bytes payload = 2;
    string intent = 3;
    CheckpointStatus status = 4;
    Confidence confidence = 5;
    ResourceUsage resource_usage = 6;
    string client_request_id = 7;
}

message CreateCheckpointResponse {
    string checkpoint_id = 1;   // runtime-assigned
    string content_hash = 2;    // SHA-256 of payload
    Timestamp timestamp = 3;
    string client_request_id = 4;
}

message QueryTrailRequest {
    string workspace_id = 1;    // must be within visibility set, or own workspace
    string event_type = 2;      // filter by event type (empty = all)
    Timestamp from = 3;         // filter by time range start
    Timestamp to = 4;           // filter by time range end
    uint32 limit = 5;           // max entries to return (0 = default 100)
    string client_request_id = 6;
}

message QueryTrailResponse {
    repeated TrailEntry entries = 1;
    bool has_more = 2;
    string client_request_id = 3;
}

message ReadResourceRequest {
    string resource_id = 1;     // must be within visibility set
    string client_request_id = 2;
}

message ReadResourceResponse {
    bytes content = 1;
    string client_request_id = 2;
}

message ReceiveEnvelopesRequest {
    // Empty — the workspace is implicit from the bound connection.
}

message ReceiveCommandsRequest {
    // Empty — the workspace is implicit from the bound connection.
}

message Command {
    oneof command {
        Envelope feedback = 1;
        VisibilityGrant visibility_grant = 2;
        GracefulTermination graceful_termination = 3;
        BudgetUpdate budget_update = 4;
    }
}

message VisibilityGrant {
    repeated string resource_ids = 1;
}

message GracefulTermination {
    uint64 grace_period_ms = 1;
}

message BudgetUpdate {
    ResourceBudget new_budget = 1;
}

message ResourceBudget {
    uint64 max_tokens = 1;
    uint64 max_wall_time_ms = 2;
    uint64 max_storage_bytes = 3;
    uint64 max_network_bytes = 4;
    uint64 max_cost_micros = 5;
    float warning_threshold = 6;  // 0.0–1.0, default 0.8
}
```

**Connection semantics.** A client connects, calls `Bind` once, then uses the action RPCs and opens the streaming RPCs. The connection is bound to exactly one workspace for its lifetime. If the connection drops, the workspace enters `Blocked` if no new connection arrives within the liveness interval. Reconnecting requires a new `Bind` call. The runtime resumes the workspace from its current state — no state is lost on reconnection because the trail and workspace actor hold all state server-side.

**Implicit workspace.** After `Bind`, the runtime associates the connection with the workspace. Subsequent RPCs do not need to specify the workspace id — it is implicit. The `from_workspace` field on envelopes is set by the runtime, not the client. This prevents impersonation — a client cannot claim to be a different workspace.

## 5. Highway Service Contract

The highway service is the gRPC interface that human-facing clients connect to. Unlike the agent service (one connection per workspace), the highway service supports multiple simultaneous connections — multiple humans may observe and interact with the same run.

```protobuf
// proto/highway.proto
syntax = "proto3";
package wacp.v1;

import "primitives.proto";

service HighwayService {
    // --- Connection lifecycle ---

    // Authenticate a human user.
    rpc Authenticate(AuthenticateRequest) returns (AuthenticateResponse);

    // --- Actions (unary) ---

    // Inject an envelope into any workspace.
    rpc InjectEnvelope(InjectEnvelopeRequest) returns (InjectEnvelopeResponse);

    // Respond to a gate event (approve, reject, modify).
    rpc RespondToGate(GateResponse) returns (GateResponseAck);

    // Respond to an escalation.
    rpc RespondToEscalation(EscalationResponse) returns (EscalationResponseAck);

    // Query the global trail.
    rpc QueryTrail(HighwayQueryTrailRequest) returns (QueryTrailResponse);

    // Read a workspace's current state.
    rpc GetWorkspace(GetWorkspaceRequest) returns (WorkspaceView);

    // Read the task graph.
    rpc GetTaskGraph(GetTaskGraphRequest) returns (TaskGraphView);

    // Read a checkpoint's payload.
    rpc GetCheckpoint(GetCheckpointRequest) returns (CheckpointView);

    // --- Observations (server-streaming) ---

    // Stream trail entries in real time.
    rpc StreamTrail(StreamTrailRequest) returns (stream TrailEntry);

    // Stream gate events as they occur.
    rpc StreamGates(StreamGatesRequest) returns (stream GateEvent);

    // Stream escalation events.
    rpc StreamEscalations(StreamEscalationsRequest) returns (stream EscalationEvent);

    // Stream workspace state changes.
    rpc StreamWorkspaceChanges(StreamWorkspaceChangesRequest) returns (stream WorkspaceStateChange);
}
```

**Request and response messages:**

```protobuf
message AuthenticateRequest {
    string auth_token = 1;
}

message AuthenticateResponse {
    string user_id = 1;
    repeated string capabilities = 2;  // what this user can do
}

message InjectEnvelopeRequest {
    string to_workspace = 1;
    string type = 2;
    bytes payload = 3;
    EnvelopePriority priority = 4;
    string client_request_id = 5;
}

message InjectEnvelopeResponse {
    string envelope_id = 1;
    Timestamp timestamp = 2;
    string client_request_id = 3;
}

message GateEvent {
    string gate_id = 1;
    GateType type = 2;
    bytes subject = 3;          // the full object awaiting approval (serialized)
    string workspace_id = 4;
    string task_id = 5;
    uint64 timeout_ms = 6;
    string fallback_action = 7; // what happens if no response
    Timestamp created_at = 8;
}

enum GateType {
    GATE_TYPE_UNSPECIFIED = 0;
    GATE_TYPE_TASK_APPROVAL = 1;
    GATE_TYPE_WORKSPACE_CREATE = 2;
    GATE_TYPE_ENVELOPE_DELIVERY = 3;
    GATE_TYPE_INTEGRATION = 4;
    GATE_TYPE_CONFLICT_RESOLUTION = 5;
    GATE_TYPE_WORKSPACE_ABORT = 6;
}

message GateResponse {
    string gate_id = 1;
    GateDecision decision = 2;
    bytes modifications = 3;    // for MODIFY: the altered fields
    string client_request_id = 4;
}

enum GateDecision {
    GATE_DECISION_UNSPECIFIED = 0;
    GATE_DECISION_APPROVE = 1;
    GATE_DECISION_REJECT = 2;
    GATE_DECISION_MODIFY = 3;
}

message GateResponseAck {
    string gate_id = 1;
    bool applied = 2;           // false if gate already resolved (timeout/other user)
    string client_request_id = 3;
}

message EscalationEvent {
    string escalation_id = 1;
    string workspace_id = 2;
    string owner = 3;           // user_id of workspace owner
    bytes context = 4;          // escalation context from agent
    Timestamp created_at = 5;
}

message EscalationResponse {
    string escalation_id = 1;
    oneof action {
        Envelope feedback = 2;          // send feedback to unblock
        bool abort = 3;                 // fail the workspace
        bool delegate_to_coordinator = 4; // let coordinator handle
    }
    string client_request_id = 5;
}

message EscalationResponseAck {
    string escalation_id = 1;
    bool applied = 2;
    string client_request_id = 3;
}

message HighwayQueryTrailRequest {
    string workspace_id = 1;    // empty for global trail
    string event_type = 2;
    string actor = 3;
    Timestamp from = 4;
    Timestamp to = 5;
    uint32 limit = 6;
    string client_request_id = 7;
}

message GetWorkspaceRequest {
    string workspace_id = 1;
}

message WorkspaceView {
    string id = 1;
    WorkspaceState state = 2;
    string role = 3;
    string parent = 4;
    string owner = 5;
    string originator = 6;
    string task_id = 7;
    ResourceUsage current_usage = 8;
    ResourceBudget budget = 9;
    uint32 checkpoint_count = 10;
    Timestamp created_at = 11;
    Timestamp last_activity = 12;
}

message GetTaskGraphRequest {}

message TaskGraphView {
    repeated Task tasks = 1;
}

message GetCheckpointRequest {
    string checkpoint_id = 1;
}

message CheckpointView {
    Checkpoint metadata = 1;
    bytes payload = 2;          // the full payload content
}

message StreamTrailRequest {
    string workspace_id = 1;    // empty for global trail
    string event_type = 2;      // empty for all types
    bool from_beginning = 3;    // true = replay history then stream live
}

message StreamGatesRequest {}

message StreamEscalationsRequest {
    string user_id = 1;         // empty for all escalations the user owns
}

message StreamWorkspaceChangesRequest {
    string workspace_id = 1;    // empty for all workspaces
}

message WorkspaceStateChange {
    string workspace_id = 1;
    WorkspaceState previous = 2;
    WorkspaceState current = 3;
    string trigger = 4;         // what caused the transition
    Timestamp timestamp = 5;
}
```

**Multi-user semantics.** Multiple highway clients can connect simultaneously. Gate events are delivered to all connected clients with visibility. The first response wins — subsequent responses to the same gate receive `applied: false`. Escalations route to the workspace's owner by default — only clients authenticated as the owner (or with explicit capability) receive them.

**Trail streaming.** The `StreamTrail` RPC supports two modes. `from_beginning = false` (default): stream live events only. `from_beginning = true`: replay all historical entries matching the filter, then switch to live streaming. This enables a newly connected dashboard to catch up without a separate query.

## 6. Serialization Rules

Protobuf handles most serialization automatically. This section defines the rules for the cases where the mapping from protocol concept to wire format is not obvious.

**Rule 1: Timestamps are two-field messages, not single integers.** The HLC timestamp (runtime spec, §7) is 80 bits: 64-bit physical + 16-bit logical. Protobuf has no native 80-bit type. The `Timestamp` message uses `uint64 physical_us` and `uint32 logical` as separate fields. This is clearer than packing into a single `bytes` field and enables protobuf-native comparison on the physical component. The 10-byte big-endian encoding (used for storage and trail indexing) is an internal concern — on the wire, the two-field message is canonical.

**Rule 2: Opaque payloads are `bytes`.** Envelope payloads, checkpoint payloads, escalation context, and trail entry bodies are `bytes` on the wire. The protocol does not prescribe their internal structure — that is application-defined (TAXONOMY.md §1: "The taxonomy is a registry, not a schema"). The runtime passes them through without interpretation. SDKs may provide convenience methods to serialize/deserialize common payload formats (JSON, protobuf sub-messages), but the wire format is always raw bytes.

**Rule 3: Identifiers are `string`.** All protocol identifiers (workspace id, envelope id, checkpoint id, task id, trail entry id, user id) are `string` on the wire. The identity spec (rule 2) says identifiers are opaque — UUIDs, ULIDs, integers, URIs are all valid. `string` accommodates all of these. The runtime generates identifiers as ULIDs by default (lexicographically sortable, time-ordered, URL-safe), but the wire format does not assume any structure.

**Rule 4: Empty strings mean absent.** Protobuf 3 does not distinguish between "field not set" and "field set to default value." For `string` fields, the default is `""`. The protocol uses this: `in_reply_to = ""` means unsolicited, `parent_task = ""` means root task, `workspace_id = ""` on a trail entry means system-level event. SDKs should expose these as `Option<String>` or equivalent — the wire format uses empty string, the SDK translates.

**Rule 5: Content hashes are hex-encoded strings.** SHA-256 hashes (checkpoint `content_hash`, trail `chain_hash`) are transmitted as hex-encoded strings in human-facing contexts (responses, trail entries) and as raw `bytes` in performance-sensitive contexts (trail storage, checkpoint store). On the wire (protobuf), `content_hash` is `string` (hex) and `chain_hash` is `bytes` (raw). The distinction: agents and humans read content hashes (they appear in logs and UIs), but chain hashes are internal to trail integrity verification.

**Rule 6: Resource values are unsigned integers in base units.** Tokens are counts. Wall time is milliseconds. Storage and network are bytes. Cost is microdollars (millionths of a dollar). No floating point on the wire — all resource values are `uint64`. This eliminates floating-point comparison issues across languages. SDKs may provide float conversions for display (e.g., dollars from microdollars).

**Rule 7: Taxonomy types are strings validated server-side.** Envelope types, checkpoint types, and role names are `string` on the wire — not enums. The runtime validates them against the loaded taxonomy. An unregistered type string is rejected with `ERROR_CATEGORY_VALIDATION_FAILED`. This keeps the `.proto` files stable across taxonomy changes — adding a new envelope type does not require regenerating protobuf code.

## 7. Authentication at the Boundary

The protocol requires authenticated identity before any action (§11.3). This section defines how authentication works at the gRPC boundary — not the authentication mechanism itself (which is deployment-defined), but how it integrates with the transport.

**Agent authentication.** The `Bind` RPC carries an `auth_token`. The runtime validates this token against a pluggable authenticator before allowing the bind. The authenticator is a trait:

```rust
trait Authenticator: Send + Sync {
    /// Validate a token and return the authenticated identity.
    /// For agents: returns the agent identity bound to a workspace.
    /// For humans: returns the user_id.
    fn authenticate_agent(&self, token: &str, workspace_id: &WorkspaceId)
        -> Result<AgentIdentity, AuthError>;

    fn authenticate_human(&self, token: &str)
        -> Result<UserId, AuthError>;
}
```

The runtime ships with two implementations:

- **Pre-shared key.** A static mapping from token to identity, loaded from configuration. Suitable for single-machine deployments where agents are launched by the runtime itself. The runtime generates a unique token per workspace at creation time and passes it to the agent through the launch mechanism.
- **External.** Delegates to an external authentication service (HTTP callback). Suitable for deployments where identity is managed by an existing system (OAuth, OIDC, API keys). The runtime calls the external service with the token and receives the identity or a rejection.

**Post-authentication state.** After successful authentication, the gRPC connection carries the authenticated identity as connection-level metadata. Every subsequent RPC on that connection inherits the identity — the client does not re-authenticate per request. The transport actor (runtime spec, §3) associates the identity with the connection and includes it in every message forwarded to workspace or coordinator actors.

**Authentication failure.** A failed `Bind` returns a gRPC `UNAUTHENTICATED` status. A failed `Authenticate` (highway) returns the same. Failed authentications are recorded in the trail as `authentication_failed` entries — with the token redacted but the source IP, timestamp, and claimed workspace/user recorded. Repeated failures from the same source may trigger rate limiting (deployment-configured, not protocol-defined).

**TLS.** All gRPC connections should use TLS in production. The runtime accepts a TLS configuration (certificate, key, CA) at startup. TLS is not enforced by the protocol — it is a deployment concern — but the spec strongly recommends it. Without TLS, authentication tokens travel in cleartext, voiding the security model's message integrity guarantees (§11.4).

**No session tokens.** The runtime does not issue session tokens after authentication. The gRPC connection itself is the session. Connection drop = session end. Reconnection requires re-authentication. This simplifies the security model — there are no session tokens to leak, expire, or revoke. The connection lifecycle is the authentication lifecycle.

## 8. Transport Trait

The runtime's internal logic is transport-agnostic (runtime spec, §2). The transport trait defines the abstraction that decouples the runtime from gRPC — enabling alternative transports for testing, embedded use, and future deployment models.

```rust
/// A connected agent session. The transport creates one per agent connection.
trait AgentSession: Send {
    /// Send a message to the agent (envelope delivery, command).
    fn send(&mut self, msg: AgentOutbound) -> Result<(), TransportError>;

    /// Receive the next message from the agent (action request).
    /// Blocks until a message is available or the connection drops.
    async fn recv(&mut self) -> Result<AgentInbound, TransportError>;

    /// The authenticated identity of this session.
    fn identity(&self) -> &AgentIdentity;

    /// The workspace this session is bound to.
    fn workspace_id(&self) -> &WorkspaceId;
}

/// A connected highway session.
trait HighwaySession: Send {
    fn send(&mut self, msg: HighwayOutbound) -> Result<(), TransportError>;
    async fn recv(&mut self) -> Result<HighwayInbound, TransportError>;
    fn identity(&self) -> &UserId;
}

/// The transport server. Accepts connections and produces sessions.
trait Transport: Send + Sync {
    type AgentSess: AgentSession;
    type HighwaySess: HighwaySession;

    /// Start listening. Returns when the transport is ready to accept connections.
    async fn start(&mut self, config: TransportConfig) -> Result<(), TransportError>;

    /// Accept the next agent connection. Blocks until one arrives.
    async fn accept_agent(&mut self) -> Result<Self::AgentSess, TransportError>;

    /// Accept the next highway connection. Blocks until one arrives.
    async fn accept_highway(&mut self) -> Result<Self::HighwaySess, TransportError>;

    /// Shut down the transport. No new connections accepted.
    async fn shutdown(&mut self) -> Result<(), TransportError>;
}
```

**Message types.** `AgentInbound` and `AgentOutbound` are enums wrapping the typed messages from the agent service contract (§4). `HighwayInbound` and `HighwayOutbound` wrap the highway service contract (§5). These are Rust enums — not protobuf messages. The transport implementation converts between wire format and these enums. The runtime never touches serialized bytes.

```rust
enum AgentInbound {
    Bind(BindRequest),
    SendEnvelope(SendEnvelopeRequest),
    EmitSignal(EmitSignalRequest),
    CreateCheckpoint(CreateCheckpointRequest),
    QueryTrail(QueryTrailRequest),
    ReadResource(ReadResourceRequest),
}

enum AgentOutbound {
    BindResponse(BindResponse),
    SendEnvelopeResponse(SendEnvelopeResponse),
    EmitSignalResponse(EmitSignalResponse),
    CreateCheckpointResponse(CreateCheckpointResponse),
    QueryTrailResponse(QueryTrailResponse),
    ReadResourceResponse(ReadResourceResponse),
    EnvelopeDelivery(Envelope),
    Command(Command),
    Error(ProtocolError),
}
```

**Three implementations:**

| Implementation | Use case | Session type |
|---------------|----------|-------------|
| `GrpcTransport` | Production | TCP connection, protobuf serialization, TLS |
| `InProcessTransport` | Integration tests | `tokio::mpsc` channels, zero serialization overhead |
| `UnixTransport` | Single-machine deployment | Unix domain sockets, protobuf serialization, no TLS needed |

The `InProcessTransport` is critical for testing. It allows the test harness to create a runtime, connect mock agents, and exercise the full protocol pipeline — without network setup, port allocation, or serialization overhead. Tests run in milliseconds, not seconds.

**The transport actor uses this trait.** The transport actors (runtime spec, §3) are generic over the `Transport` trait. The `wacp-runtime` binary selects the implementation at startup based on configuration. The rest of the runtime is identical regardless of transport.

## 9. gRPC Implementation

The `GrpcTransport` is the production transport implementation. It uses `tonic` (Rust gRPC framework) on the server side, with generated client stubs for Python (`grpcio` or `betterproto`) and TypeScript (`ts-proto`).

**Server setup.** The `wacp-transport` crate contains the `tonic` server implementation. At startup:

1. Load TLS configuration (certificate, key, optional CA for mutual TLS).
2. Build the `tonic::Server` with two services: `AgentServiceServer` and `HighwayServiceServer`.
3. Bind to the configured address and port (default: `0.0.0.0:9090` for agents, `0.0.0.0:9091` for highway, `0.0.0.0:9092` for coordinator). Separate ports enable different firewall rules, rate limits, and TLS requirements per service.
4. Start accepting connections.

**Service implementation.** The `tonic` service implementations are thin adapters. They receive protobuf request messages, convert them to the internal Rust types (`AgentInbound`/`HighwayInbound`), forward them to the transport actor, receive the response, convert back to protobuf, and return. No protocol logic lives in the gRPC layer — it is a serialization/deserialization boundary.

```rust
#[tonic::async_trait]
impl agent_service_server::AgentService for AgentServiceImpl {
    async fn bind(
        &self,
        request: Request<BindRequest>,
    ) -> Result<Response<BindResponse>, Status> {
        let auth = self.authenticator
            .authenticate_agent(&request.get_ref().auth_token, &request.get_ref().workspace_id)
            .map_err(|e| Status::unauthenticated(e.to_string()))?;

        let response = self.coordinator_tx
            .send_and_await(CoordinatorMsg::AgentBind {
                workspace_id: request.get_ref().workspace_id.parse()?,
                identity: auth,
            })
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(response.into()))
    }

    // ... other RPCs follow the same pattern
}
```

**Streaming RPCs.** Server-streaming RPCs (`ReceiveEnvelopes`, `StreamTrail`, `StreamGates`) are implemented using `tokio::mpsc` channels. The service creates a channel, registers the receiver with the appropriate actor (workspace actor for envelopes, coordinator for trail/gates), and returns a `ReceiverStream` to `tonic`. The actor pushes events into the channel as they occur. If the client disconnects, the channel's sender detects the dropped receiver and stops sending.

**Error mapping.** Protocol errors (runtime spec, §15) are mapped to gRPC status codes:

| Protocol error | gRPC status | Details |
|---------------|-------------|---------|
| Permission denied | `PERMISSION_DENIED` | `ProtocolError` in details metadata |
| Illegal transition | `FAILED_PRECONDITION` | `ProtocolError` in details metadata |
| Validation failed | `INVALID_ARGUMENT` | `ProtocolError` in details metadata |
| Budget exceeded | `RESOURCE_EXHAUSTED` | `ProtocolError` in details metadata |
| Not found | `NOT_FOUND` | `ProtocolError` in details metadata |
| Delivery failed | `UNAVAILABLE` | `ProtocolError` in details metadata |
| Internal error | `INTERNAL` | `ProtocolError` in details metadata |
| Authentication failed | `UNAUTHENTICATED` | No protocol error (pre-bind) |

The `ProtocolError` message is always attached as gRPC trailing metadata using `tonic::Status::with_details()`. Clients extract the structured error from metadata — they never need to parse the status message string.

**Code generation pipeline.** The `.proto` files are the input. Code generation runs as a build step:

| Language | Tool | Output | Build integration |
|----------|------|--------|-------------------|
| Rust | `prost` + `tonic-build` | Rust types + server/client stubs | `build.rs` in `wacp-transport` crate |
| Python | `betterproto` | Python dataclasses + async client stubs | `Makefile` / `poetry` script |
| TypeScript | `ts-proto` | TypeScript interfaces + client stubs | `package.json` script |

All three generate from the same `.proto` files. A change to `primitives.proto` regenerates types in all three languages simultaneously. The CI pipeline verifies that generated code is up to date — a modified `.proto` file with stale generated code fails the build.

---

## 10. Versioning and Compatibility

The `.proto` files, the gRPC services, and the protocol itself evolve over time. This section defines how versions are tracked and how compatibility is maintained across changes.

**Three version numbers.**

| Version | What it tracks | Where it lives |
|---------|---------------|----------------|
| Protocol version | The WACP spec (PROTOCOL.md) | `PROTOCOL.md` metadata, taxonomy `protocol_version` field |
| Interface version | The `.proto` file definitions | `package wacp.v1` — the protobuf package name encodes the major version |
| Runtime version | The Rust binary | `Cargo.toml` version, reported at startup in the first trail entry |

The protocol version and the interface version advance together — a change to the protocol that alters a primitive's structure requires a `.proto` change. The runtime version advances independently — bug fixes and performance improvements that don't change the interface increment the runtime version only.

**Protobuf compatibility rules.** Within a major version (`wacp.v1`):

- **Adding fields is safe.** A new optional field in an existing message is backward-compatible. Old clients ignore it. New clients use it if present, use defaults if absent.
- **Adding RPCs is safe.** A new RPC in an existing service is backward-compatible. Old clients don't call it. New clients can.
- **Adding enum values is safe for open sets.** A new `ErrorCategory` value is backward-compatible — old clients see `UNSPECIFIED` for unrecognized values.
- **Adding enum values is NOT safe for closed sets.** Signal types, workspace states, and envelope states are protocol constants (§12.5). Adding a value changes the protocol, not just the interface. This requires a major version bump (`wacp.v2`).
- **Removing or renaming fields is never safe.** Fields can be deprecated (marked `reserved`) but not removed within a major version.
- **Changing field numbers is never safe.** Field numbers are the wire identity. Changing a number is a silent data corruption.

**Major version transitions.** A new major version (`wacp.v2`) means:

- New protobuf package: `package wacp.v2`.
- New gRPC service names: `wacp.v2.AgentService`, `wacp.v2.HighwayService`.
- The runtime may serve both `v1` and `v2` simultaneously during a transition period — two sets of gRPC services on the same ports. Clients declare their version at connection time.
- Trail entries record which interface version produced them. Trail queries across a version boundary may encounter entries from both versions — the query engine handles this transparently.

**Runtime version reporting.** The first trail entry of every run is a `run_started` event containing: runtime version, protocol version, interface version, taxonomy id and version, and startup configuration (excluding secrets). This enables post-hoc analysis to determine exactly what was running.

**SDK version compatibility.** SDKs declare the interface version they were generated against. The runtime rejects connections from SDKs with an incompatible major version. Minor version mismatches are allowed — the SDK may not use features from newer minor versions, but the wire format is compatible.

## 11. References

### PROTOCOL.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §4 (core primitives) | §3 | Primitive definitions — message shapes derived from protocol structures |
| §4.2 (envelope) | §3, §4 | Envelope fields, lifecycle states, delivery guarantees |
| §4.3 (signal) | §3 | Signal types (closed set — protobuf enum) |
| §4.4 (checkpoint) | §3, §4 | Checkpoint fields, status, confidence |
| §4.5 (trail) | §3, §5 | Trail entry structure, query interface |
| §4.6 (task) | §3, §5 | Task fields, status lifecycle |
| §4.7 (identity) | §2, §6 | Runtime-assigned identifiers, opaque format |
| §4.8 (user identity) | §5, §7 | User authentication, originator |
| §5 (roles and permissions) | §4, §7 | Permission matrix, role resolution |
| §6 (workspace lifecycle) | §3, §4, §5 | Workspace states (closed set — protobuf enum) |
| §8 (human highway) | §5 | Gate types, injection, escalation, autonomy spectrum |
| §9 (trail) | §4, §5 | Trail query interface, streaming, access rules |
| §11.3 (identity and authentication) | §7 | Authentication before action requirement |
| §11.4 (message integrity) | §7 | TLS recommendation for transport |
| §12.5 (protocol constants) | §3, §10 | Closed sets — values fixed by protocol |

### Implementation Specs

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| Runtime spec | §2 (boundaries) | §1, §8 | Two external boundaries, transport abstraction |
| Runtime spec | §3 (process model) | §8, §9 | Transport actors, coordinator actor |
| Runtime spec | §5 (permission engine) | §4, §7 | Validation at every action |
| Runtime spec | §15 (error model) | §9 | Protocol errors → gRPC status mapping |
| Storage spec | §5 (checkpoint store) | §3 | Content hash in checkpoint messages |

### TAXONOMY.md

| Section | Referenced in | Topic |
|---------|--------------|-------|
| §1 (purpose) | §2, §6 | Registry not schema — payloads are opaque bytes |
| §4 (envelope type registry) | §2 | Open set — string on wire, validated server-side |
| §5 (checkpoint type registry) | §2 | Open set — string on wire, validated server-side |

### Downstream Specs

| Spec | Relationship | Topic |
|------|-------------|-------|
| Agent SDK spec (`impl/sdk-agent.md`) | Downstream | Python + Rust SDK built on agent service contract and generated stubs |

---

*WACP implementation specification — authored by Akil Abderrahim and Claude Opus 4.6*
*Protocol: [PROTOCOL.md](https://github.com/Madahub-dev/wacp-protocol/blob/main/PROTOCOL.md) | Implementation Journal: [IMPLEMENTATION.md](../IMPLEMENTATION.md)*
