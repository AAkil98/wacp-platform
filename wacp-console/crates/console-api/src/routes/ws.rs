//! WebSocket endpoint — 7-channel multiplexed JSON protocol.
//!
//! Spec: `wcon-api` §12, `wcon-highway` §2.2
//!
//! The WS connection subscribes to the per-session `broadcast::Receiver`
//! exposed by the session monitor. Frame shape: `{ "channel", "session_id",
//! "event" }`. Slow consumers that fall behind the monitor's bounded
//! buffer receive a control-channel `lag` frame and may continue (tokio
//! broadcast keeps the receiver usable after `Lagged`).

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tracing::{info, warn};

use console_core::error::ConsoleError;
use console_core::session_monitor::{Channel, Frame, SessionMonitorHandle};
use console_db::queries::sessions;

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::Auth;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/sessions/{id}/ws", get(ws_upgrade))
}

/// WebSocket upgrade handler. Authenticates, verifies session access, looks
/// up the monitor handle, then upgrades.
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
    let monitor_handle = state.active_sessions.read().await.get(&session_id).cloned();

    Ok(ws.on_upgrade(move |socket| handle_ws(socket, session_id, user_id, monitor_handle)))
}

/// Handle a WebSocket connection. Subscribes to the session monitor's
/// broadcast; if no monitor is running for the session, the socket still
/// connects but only receives the welcome + any future frames if a monitor
/// spawns later (post-recovery — not W3 scope).
async fn handle_ws(
    mut socket: WebSocket,
    session_id: String,
    user_id: String,
    monitor: Option<SessionMonitorHandle>,
) {
    info!(session_id = %session_id, user_id = %user_id, "WebSocket connected");

    let welcome = serde_json::json!({
        "channel": "session",
        "session_id": session_id,
        "event": {
            "type": "connected",
            "monitor_running": monitor.is_some(),
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

    let mut rx = monitor.as_ref().map(|h| h.subscribe());

    loop {
        tokio::select! {
            // Client frames: ping/pong, close, text (ignored).
            maybe = socket.recv() => match maybe {
                Some(Ok(Message::Text(_))) => {},
                Some(Ok(Message::Ping(data))) => {
                    if socket.send(Message::Pong(data)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(e)) => {
                    warn!(error = %e, session_id = %session_id, "WebSocket error");
                    break;
                }
                _ => {},
            },
            // Monitor broadcasts: forward to the client, emit lag markers.
            maybe = recv_broadcast(&mut rx), if rx.is_some() => match maybe {
                Ok(frame) => {
                    let text = match serde_json::to_string(&frame_as_envelope(&frame)) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(missed)) => {
                    let lag = serde_json::json!({
                        "channel": "control",
                        "session_id": session_id,
                        "event": { "type": "lag", "missed": missed },
                    });
                    if socket
                        .send(Message::Text(lag.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    warn!(session_id = %session_id, missed, "WebSocket consumer lagged");
                }
                Err(RecvError::Closed) => {
                    rx = None;
                }
            },
        }
    }

    info!(session_id = %session_id, user_id = %user_id, "WebSocket disconnected");
}

/// Broadcast recv wrapper: returns `Closed` if the receiver is `None`.
async fn recv_broadcast(
    rx: &mut Option<tokio::sync::broadcast::Receiver<Frame>>,
) -> Result<Frame, RecvError> {
    match rx.as_mut() {
        Some(r) => r.recv().await,
        None => Err(RecvError::Closed),
    }
}

/// Wrap a `Frame` into the flat `{channel, session_id, event}` envelope
/// expected by the frontend. `event` inlines the typed payload variant; the
/// variant discriminator is carried in `event.kind` via the enum's serde
/// attribute.
fn frame_as_envelope(frame: &Frame) -> serde_json::Value {
    let channel = match frame.channel {
        Channel::Trail => "trail",
        Channel::Gates => "gates",
        Channel::Escalations => "escalations",
        Channel::Workspaces => "workspaces",
        Channel::Refusals => "refusals",
        Channel::Session => "session",
        Channel::Control => "control",
    };
    // event is already serializable as an externally-tagged enum; pull the
    // value out via round-trip to maintain the existing frame shape.
    let event = serde_json::to_value(&frame.event).unwrap_or_default();
    serde_json::json!({
        "channel": channel,
        "session_id": frame.session_id,
        "event": event,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use console_core::session_monitor::FrameEvent;

    #[test]
    fn frame_envelope_carries_channel_and_event() {
        let frame = Frame {
            channel: Channel::Control,
            session_id: "s".into(),
            event: FrameEvent::Lag {
                refresh_hint: vec!["gates"],
                reason: "reconnect".into(),
            },
        };
        let v = frame_as_envelope(&frame);
        assert_eq!(v["channel"], "control");
        assert_eq!(v["session_id"], "s");
        assert!(v["event"].is_object());
    }
}
