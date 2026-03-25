# Tasks 13.1–13.3: Request Handler

## Scope

Add `RequestHandler` to `wacp-coordinator` — handles agent and highway RPCs using coordinator state. Domain-level request/response types (no tonic/gRPC dependency). Testable without transport layer.

Tasks 13.1 (agent), 13.2 (highway), 13.3 (gate/escalation) combined into one module since they share the handler struct.

## Types

### New: Agent request/response types

```rust
pub struct BindRequest { workspace_id: WorkspaceId, auth_token: String }
pub struct BindResult { workspace_id: WorkspaceId, state: WorkspaceState, role: String, owner: UserId }
pub struct SendEnvelopeResult { envelope_id: EnvelopeId }
pub struct EmitSignalResult { }
pub struct CreateCheckpointResult { checkpoint_id: CheckpointId, content_hash: String }
```

### New: Highway request/response types

```rust
pub struct WorkspaceView { id, state, parent, owner, originator, task_id }
pub struct TaskGraphView { tasks: Vec<TaskView> }
pub struct TaskView { id, name, status, workspace_ref, depends_on }
```

### New: `RequestHandler`

Holds references to all coordinator structures. Provides methods for each RPC.

## Functions (agent — 13.1)

- `handle_bind(ws_id) -> Result<BindResult>`
- `handle_send_envelope(from_ws, to_ws, type, payload, priority) -> Result<SendEnvelopeResult>`
- `handle_emit_signal(ws_id, signal_type, reason) -> Result<EmitSignalResult>`
- `handle_create_checkpoint(ws_id, type, payload, intent, status, confidence) -> Result<CreateCheckpointResult>`

## Functions (highway — 13.2)

- `handle_get_workspace(ws_id) -> Result<WorkspaceView>`
- `handle_get_task_graph() -> TaskGraphView`
- `handle_inject_envelope(to_ws, type, payload, priority) -> Result<SendEnvelopeResult>`

## Functions (gate/escalation — 13.3)

- `handle_gate_response(gate_id, decision) -> Result<bool>`
- `handle_escalation_response(escalation_id, resolution) -> Result<()>`

## Tests

13.1: bind returns workspace state, send_envelope validates port rights, emit_signal for unknown workspace rejected, create_checkpoint returns id+hash
13.2: get_workspace returns view, get_task_graph returns all tasks, inject_envelope creates with human origin
13.3: gate_response resolves pending gate, gate_response for unknown gate returns false
