//! REST + WebSocket helpers for integration tests.
//!
//! Auth uses bearer tokens (CSRF-exempt per `console-api/src/middleware.rs`).
//! The fixture user is seeded directly into the test DB — going through
//! the bootstrap → invite → first-login flow would multiply the number of
//! moving parts a test must understand without adding signal.

use std::sync::Arc;
use std::time::Duration;

use console_api::AppState;
use console_core::authenticator;
use console_db::queries::api_tokens;
use futures::StreamExt;
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

pub struct TestClient {
    base_url: String,
    pub token: String,
    http: reqwest::Client,
}

impl TestClient {
    /// Seed a user + bearer token in the console DB and return a client
    /// configured to use the bearer token. `state` is the in-process
    /// console's `AppState` so we can write to the same DB the API
    /// reads from.
    pub async fn seed_user(state: &Arc<AppState>, base_url: &str, user_id: &str, role: &str) -> Self {
        sqlx::query(
            "INSERT OR IGNORE INTO users (id, username, username_lower, display_name, password_hash,
                console_role, must_change_password, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind("h")
        .bind(role)
        .bind(0_i64)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await
        .expect("seed user");

        let token = format!("wcon_t_{}", uuid::Uuid::new_v4());
        let hash = authenticator::hash_token(&token);
        api_tokens::insert_token(
            &state.db,
            &format!("tok-{}", uuid::Uuid::new_v4()),
            user_id,
            "integration-test",
            &hash,
            &chrono::Utc::now().to_rfc3339(),
            None,
        )
        .await
        .expect("insert token");

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest client");

        TestClient {
            base_url: base_url.to_string(),
            token,
            http,
        }
    }

    pub async fn get(&self, path: &str) -> reqwest::Response {
        self.http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await
            .expect("GET")
    }

    pub async fn post_json(&self, path: &str, body: Value) -> reqwest::Response {
        self.http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .expect("POST")
    }

    /// Connect to the WS endpoint at `path` using the bearer token in
    /// the `Authorization` header. Returns a `WsClient` thin wrapper.
    pub async fn open_ws(&self, path: &str) -> WsClient {
        // ws scheme requires the base URL be rewritten.
        let ws_url = self
            .base_url
            .replacen("http://", "ws://", 1)
            .replacen("https://", "wss://", 1);
        let url = format!("{ws_url}{path}");
        // IntoClientRequest for &str fills in the WS handshake headers
        // (sec-websocket-key, sec-websocket-version, host, …); building
        // a bare http::Request would skip them and the upgrade fails.
        let mut req = url.as_str().into_client_request().expect("ws request");
        req.headers_mut().insert(
            "authorization",
            format!("Bearer {}", self.token)
                .parse()
                .expect("auth header"),
        );
        let (stream, _) = connect_async(req).await.expect("ws connect");
        WsClient { stream }
    }
}

pub struct WsClient {
    stream: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
}

impl WsClient {
    /// Receive frames until `pred` returns true or `timeout` elapses.
    /// Returns the matching frame (parsed as JSON) or panics on timeout
    /// — integration tests prefer loud failures over silent skips.
    pub async fn assert_frame_within<F>(&mut self, timeout: Duration, pred: F) -> Value
    where
        F: Fn(&Value) -> bool,
    {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                panic!("WS frame predicate not met within {timeout:?}");
            }
            let frame = match tokio::time::timeout(remaining, self.stream.next()).await {
                Ok(Some(Ok(Message::Text(t)))) => {
                    serde_json::from_str::<Value>(&t).expect("ws json")
                }
                Ok(Some(Ok(Message::Close(_)))) | Ok(None) => {
                    panic!("WS closed before predicate matched")
                }
                Ok(Some(Ok(Message::Binary(_) | Message::Ping(_) | Message::Pong(_)))) => continue,
                Ok(Some(Ok(Message::Frame(_)))) => continue,
                Ok(Some(Err(e))) => panic!("WS recv error: {e}"),
                Err(_) => panic!("WS recv timeout"),
            };
            if pred(&frame) {
                return frame;
            }
        }
    }

    pub async fn close(mut self) {
        let _ = self.stream.close(None).await;
    }
}

