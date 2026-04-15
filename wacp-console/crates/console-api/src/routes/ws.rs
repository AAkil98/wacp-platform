//! WebSocket endpoint — 7-channel multiplexed JSON protocol.
//!
//! Spec: `wcon-api` §12, `wcon-highway` §2.2

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use std::sync::Arc;
use tracing::{info, warn};

use console_core::error::ConsoleError;
use console_db::queries::sessions;

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::Auth;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/sessions/{id}/ws", get(ws_upgrade))
}

/// WebSocket upgrade handler. Authenticates, then upgrades the connection.
async fn ws_upgrade(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ApiError> {
    // Verify session exists and user has access
    let session = sessions::get_by_id(&state.db, &id)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?
        .ok_or_else(|| ApiError::not_found("session", &id))?;

    if auth.console_role != console_core::ConsoleRole::Admin
        && session.owner_user_id != auth.user_id
    {
        return Err(ApiError::from(ConsoleError::Forbidden(
            "You do not have access to this session".into(),
        )));
    }

    let session_id = id.clone();
    let user_id = auth.user_id.clone();

    Ok(ws.on_upgrade(move |socket| handle_ws(socket, session_id, user_id)))
}

/// Handle a WebSocket connection. Subscribes to session events and forwards them.
async fn handle_ws(mut socket: WebSocket, session_id: String, user_id: String) {
    info!(session_id = %session_id, user_id = %user_id, "WebSocket connected");

    // Send initial connection confirmation
    let welcome = serde_json::json!({
        "channel": "session",
        "session_id": session_id,
        "event": {
            "type": "connected",
            "message": "WebSocket connected to session",
        }
    });

    if let Err(e) = socket
        .send(Message::Text(
            serde_json::to_string(&welcome).unwrap_or_default().into(),
        ))
        .await
    {
        warn!(error = %e, "Failed to send welcome message");
        return;
    }

    // The session monitor (task 4.7) will provide a broadcast channel per session.
    // Subscribers receive events on all 7 channels. For now, we hold the connection
    // open and listen for client messages (ping/pong, close).
    loop {
        match socket.recv().await {
            Some(Ok(Message::Text(_))) => {
                // Client messages are currently ignored (no client→server commands defined)
            }
            Some(Ok(Message::Ping(data))) => {
                if socket.send(Message::Pong(data)).await.is_err() {
                    break;
                }
            }
            Some(Ok(Message::Close(_))) | None => {
                break;
            }
            Some(Err(e)) => {
                warn!(error = %e, session_id = %session_id, "WebSocket error");
                break;
            }
            _ => {}
        }
    }

    info!(session_id = %session_id, user_id = %user_id, "WebSocket disconnected");
}
