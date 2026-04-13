use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::proto::wacp_v1;
use crate::rest_gateway::GatewayBackend;

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
    #[serde(default)]
    id: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl JsonRpcResponse {
    fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            result: Some(result),
            error: None,
            id,
        }
    }

    fn error(id: Option<serde_json::Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
            id,
        }
    }

    fn notification(method: &str, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            result: Some(serde_json::json!({"method": method, "params": params})),
            error: None,
            id: None,
        }
    }
}

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

/// Axum handler for WebSocket upgrade on /v1/ws.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(backend): State<Arc<dyn GatewayBackend>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, backend))
}

async fn handle_socket(mut socket: WebSocket, backend: Arc<dyn GatewayBackend>) {
    while let Some(msg) = socket.recv().await {
        let text = match msg {
            Ok(Message::Text(text)) => text,
            Ok(Message::Ping(data)) => {
                let _ = socket.send(Message::Pong(data)).await;
                continue;
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(_) => break,
        };

        // Parse JSON-RPC request
        let request: JsonRpcRequest = match serde_json::from_str(&text) {
            Ok(req) => req,
            Err(e) => {
                let resp = JsonRpcResponse::error(None, -32700, format!("Parse error: {e}"));
                let _ = socket
                    .send(Message::Text(serde_json::to_string(&resp).unwrap().into()))
                    .await;
                continue;
            }
        };

        if request.jsonrpc != "2.0" {
            let resp = JsonRpcResponse::error(
                request.id,
                -32600,
                "Invalid Request: jsonrpc must be '2.0'",
            );
            let _ = socket
                .send(Message::Text(serde_json::to_string(&resp).unwrap().into()))
                .await;
            continue;
        }

        // Dispatch method
        let response = dispatch_method(&request, &backend).await;
        let _ = socket
            .send(Message::Text(
                serde_json::to_string(&response).unwrap().into(),
            ))
            .await;
    }
}

async fn dispatch_method(
    request: &JsonRpcRequest,
    backend: &Arc<dyn GatewayBackend>,
) -> JsonRpcResponse {
    let id = request.id.clone();
    let params = &request.params;

    match request.method.as_str() {
        "submit_goal" => {
            let description = params
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match backend
                .submit_goal(wacp_v1::SubmitGoalRequest {
                    description: description.into(),
                    context: vec![],
                    client_request_id: String::new(),
                })
                .await
            {
                Ok(resp) => JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "goal_id": resp.goal_id,
                        "workspace_id": resp.root_workspace_id,
                    }),
                ),
                Err(e) => JsonRpcResponse::error(id, -32000, e.message),
            }
        }

        "get_ready_tasks" => match backend.get_ready_tasks().await {
            Ok(resp) => {
                let tasks: Vec<serde_json::Value> = resp.tasks.iter().map(|t| {
                        serde_json::json!({"task_id": t.task_id, "name": t.name, "status": t.status})
                    }).collect();
                JsonRpcResponse::success(id, serde_json::json!(tasks))
            }
            Err(e) => JsonRpcResponse::error(id, -32000, e.message),
        },

        "get_workspace" => {
            let ws_id = params
                .get("workspace_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match backend.get_workspace(ws_id).await {
                Ok(resp) => JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "workspace_id": resp.id, "state": resp.state, "role": resp.role,
                    }),
                ),
                Err(e) => JsonRpcResponse::error(id, -32000, e.message),
            }
        }

        "dispatch" => {
            let task_id = params.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
            let role = params.get("role").and_then(|v| v.as_str()).unwrap_or("");
            match backend
                .dispatch(wacp_v1::DispatchRequest {
                    task_id: task_id.into(),
                    role: role.into(),
                    directive_payload: vec![],
                    tools: vec![],
                    budget: None,
                    client_request_id: String::new(),
                })
                .await
            {
                Ok(resp) => JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "workspace_id": resp.workspace_id, "task_id": resp.task_id,
                    }),
                ),
                Err(e) => JsonRpcResponse::error(id, -32000, e.message),
            }
        }

        "abort_workspace" => {
            let ws_id = params
                .get("workspace_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let reason = params.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            match backend
                .abort_workspace(wacp_v1::AbortWorkspaceRequest {
                    workspace_id: ws_id.into(),
                    reason: reason.into(),
                    client_request_id: String::new(),
                })
                .await
            {
                Ok(_) => JsonRpcResponse::success(id, serde_json::json!({"ok": true})),
                Err(e) => JsonRpcResponse::error(id, -32000, e.message),
            }
        }

        "inject_envelope" => {
            let ws_id = params
                .get("workspace_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let env_type = params
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("directive");
            let payload = params.get("payload").and_then(|v| v.as_str()).unwrap_or("");
            match backend
                .inject_envelope(wacp_v1::InjectEnvelopeRequest {
                    to_workspace: ws_id.into(),
                    r#type: env_type.into(),
                    payload: payload.as_bytes().to_vec(),
                    priority: 0,
                    client_request_id: String::new(),
                })
                .await
            {
                Ok(resp) => JsonRpcResponse::success(
                    id,
                    serde_json::json!({"envelope_id": resp.envelope_id}),
                ),
                Err(e) => JsonRpcResponse::error(id, -32000, e.message),
            }
        }

        "query_trail" => {
            let mut req = wacp_v1::HighwayQueryTrailRequest::default();
            req.limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as u32;
            match backend.query_trail(req).await {
                Ok(resp) => {
                    let entries: Vec<serde_json::Value> = resp
                        .entries
                        .iter()
                        .map(|e| serde_json::json!({"id": e.id, "event_type": e.event_type}))
                        .collect();
                    JsonRpcResponse::success(id, serde_json::json!(entries))
                }
                Err(e) => JsonRpcResponse::error(id, -32000, e.message),
            }
        }

        "get_allocatable" => match backend.get_allocatable().await {
            Ok(resp) => {
                let b = resp.remaining.unwrap_or_default();
                JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "max_tokens": b.max_tokens, "max_cost_micros": b.max_cost_micros,
                    }),
                )
            }
            Err(e) => JsonRpcResponse::error(id, -32000, e.message),
        },

        "trigger_integration" => {
            let ws_id = params
                .get("workspace_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match backend
                .trigger_integration(wacp_v1::TriggerIntegrationRequest {
                    workspace_id: ws_id.into(),
                    client_request_id: String::new(),
                })
                .await
            {
                Ok(resp) => JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "result": resp.result, "detail": resp.detail,
                    }),
                ),
                Err(e) => JsonRpcResponse::error(id, -32000, e.message),
            }
        }

        "subscribe_session_trail" => {
            let session_id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if session_id.is_empty() {
                return JsonRpcResponse::error(id, -32602, "session_id is required");
            }
            // Validate the session exists via the workspace backend.
            match backend.get_workspace(session_id).await {
                Ok(_) => JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "subscribed": true,
                        "session_id": session_id,
                    }),
                ),
                Err(e) => JsonRpcResponse::error(id, -32000, e.message),
            }
        }

        _ => JsonRpcResponse::error(id, -32601, format!("Method not found: {}", request.method)),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- JSON-RPC response types ---

    #[test]
    fn jsonrpc_success_format() {
        let resp =
            JsonRpcResponse::success(Some(serde_json::json!(1)), serde_json::json!({"ok": true}));
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["result"]["ok"], true);
        assert_eq!(json["id"], 1);
        assert!(json.get("error").is_none());
    }

    #[test]
    fn jsonrpc_error_format() {
        let resp = JsonRpcResponse::error(Some(serde_json::json!(2)), -32600, "Invalid Request");
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["error"]["code"], -32600);
        assert_eq!(json["error"]["message"], "Invalid Request");
        assert_eq!(json["id"], 2);
        assert!(json.get("result").is_none());
    }

    #[test]
    fn jsonrpc_notification_has_no_id() {
        let resp =
            JsonRpcResponse::notification("trail_entry", serde_json::json!({"event": "test"}));
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("id").is_none());
    }

    // --- Method dispatch (unit tests via dispatch_method) ---

    use crate::rest_gateway::tests::MockBackend;

    #[tokio::test]
    async fn dispatch_submit_goal() {
        let backend: Arc<dyn GatewayBackend> = Arc::new(MockBackend);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "submit_goal".into(),
            params: serde_json::json!({"description": "implement auth"}),
            id: Some(serde_json::json!(1)),
        };
        let resp = dispatch_method(&req, &backend).await;
        assert!(resp.result.is_some());
        assert_eq!(resp.result.unwrap()["goal_id"], "goal-1");
    }

    #[tokio::test]
    async fn dispatch_get_ready_tasks() {
        let backend: Arc<dyn GatewayBackend> = Arc::new(MockBackend);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "get_ready_tasks".into(),
            params: serde_json::json!({}),
            id: Some(serde_json::json!(2)),
        };
        let resp = dispatch_method(&req, &backend).await;
        let tasks = resp.result.unwrap();
        assert!(tasks.as_array().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn dispatch_get_workspace() {
        let backend: Arc<dyn GatewayBackend> = Arc::new(MockBackend);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "get_workspace".into(),
            params: serde_json::json!({"workspace_id": "ws-1"}),
            id: Some(serde_json::json!(3)),
        };
        let resp = dispatch_method(&req, &backend).await;
        assert_eq!(resp.result.unwrap()["workspace_id"], "ws-1");
    }

    #[tokio::test]
    async fn dispatch_unknown_method() {
        let backend: Arc<dyn GatewayBackend> = Arc::new(MockBackend);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "nonexistent_method".into(),
            params: serde_json::json!({}),
            id: Some(serde_json::json!(99)),
        };
        let resp = dispatch_method(&req, &backend).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn dispatch_abort_workspace() {
        let backend: Arc<dyn GatewayBackend> = Arc::new(MockBackend);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "abort_workspace".into(),
            params: serde_json::json!({"workspace_id": "ws-1", "reason": "timeout"}),
            id: Some(serde_json::json!(4)),
        };
        let resp = dispatch_method(&req, &backend).await;
        assert!(resp.result.is_some());
    }

    #[tokio::test]
    async fn dispatch_get_allocatable() {
        let backend: Arc<dyn GatewayBackend> = Arc::new(MockBackend);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "get_allocatable".into(),
            params: serde_json::json!({}),
            id: Some(serde_json::json!(5)),
        };
        let resp = dispatch_method(&req, &backend).await;
        let budget = resp.result.unwrap();
        assert_eq!(budget["max_tokens"], 100_000);
    }

    #[tokio::test]
    async fn dispatch_subscribe_session_trail() {
        let backend: Arc<dyn GatewayBackend> = Arc::new(MockBackend);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "subscribe_session_trail".into(),
            params: serde_json::json!({"session_id": "ws-1"}),
            id: Some(serde_json::json!(7)),
        };
        let resp = dispatch_method(&req, &backend).await;
        let result = resp.result.unwrap();
        assert_eq!(result["subscribed"], true);
        assert_eq!(result["session_id"], "ws-1");
    }

    #[tokio::test]
    async fn dispatch_subscribe_session_trail_missing_id() {
        let backend: Arc<dyn GatewayBackend> = Arc::new(MockBackend);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "subscribe_session_trail".into(),
            params: serde_json::json!({}),
            id: Some(serde_json::json!(8)),
        };
        let resp = dispatch_method(&req, &backend).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn dispatch_trigger_integration() {
        let backend: Arc<dyn GatewayBackend> = Arc::new(MockBackend);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "trigger_integration".into(),
            params: serde_json::json!({"workspace_id": "ws-impl"}),
            id: Some(serde_json::json!(6)),
        };
        let resp = dispatch_method(&req, &backend).await;
        assert_eq!(resp.result.unwrap()["result"], "accepted");
    }
}
