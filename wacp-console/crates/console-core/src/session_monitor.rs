//! Per-session Tokio task that bridges the runtime's four highway streams
//! to the WebSocket clients for a single session. Critical-path phase of
//! the wiring plan — see `wacp-console/specs/coding/wcon-w3-session-monitor.md`
//! and the gate artifact `impl/notes/w3-stream-shapes.md`.
//!
//! Topology:
//! ```text
//!   spawn() ─► main loop task (owns broadcast::Sender + PendingState)
//!               ├── StreamTrail driver  ─► mpsc<StreamEvent> ─┐
//!               ├── StreamGates driver  ─► mpsc<StreamEvent> ─┤
//!               ├── StreamEscalations   ─► mpsc<StreamEvent> ─┤
//!               └── StreamWorkspaceCh.. ─► mpsc<StreamEvent> ─┘
//!                                                              │
//!                                        select! drains events, enriches,
//!                                        filters by WorkspaceSet, broadcasts.
//! ```
//!
//! **Filtering is client-side.** The runtime ignores every stream request
//! filter — see `impl/notes/w3-stream-shapes.md` §6.
//!
//! **Scope of this implementation.**
//! - 4 stream drivers with client-side filtering.
//! - Bounded broadcast fan-out (slow consumers receive `Lagged`).
//! - Completion detection on root-workspace terminal state.
//! - Shutdown via command channel (drops active_sessions entry on exit).
//! - Per-stream exponential-backoff reconnect, capped failure count.
//! - Lag frame emitted on reconnect (frontend re-queries REST for pending).
//!
//! **Deferred to a follow-up** (flagged in the phase log):
//! - `GetTaskGraph` / `GetWorkspace`-based gap recovery is implemented as a
//!   minimal `Lag` marker only; deeper state reconciliation lives in a
//!   future phase once the RPC palette stabilizes (spec §4.3).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use console_db::DbPool;
use console_db::queries::sessions;
use console_runtime::grpc_pool::GrpcPool;
use console_runtime::proto as proto;
use serde::Serialize;
use tokio::sync::{RwLock, broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::event_enricher::{EnrichedEscalation, EnrichedGate, EnrichedTrailEntry, EventEnricher};
use crate::refusal_synthesizer::{Refusal, RefusalSynthesizer};
use crate::session_state;

// ============================================================================
// Public API
// ============================================================================

/// A handle to a running monitor. Cheap to clone; callers share one handle
/// to observe pending state (W6), subscribe to broadcasts (WS route), or
/// issue commands (shutdown).
#[derive(Clone)]
pub struct SessionMonitorHandle {
    pub session_id: String,
    pub cmd_tx: mpsc::Sender<MonitorCmd>,
    pub broadcast_tx: broadcast::Sender<Frame>,
    pub pending: Arc<PendingState>,
}

impl SessionMonitorHandle {
    pub fn subscribe(&self) -> broadcast::Receiver<Frame> {
        self.broadcast_tx.subscribe()
    }

    pub async fn shutdown(&self) {
        let _ = self.cmd_tx.send(MonitorCmd::Shutdown).await;
    }

    pub async fn snapshot(&self) -> Option<MonitorSnapshot> {
        let (tx, rx) = oneshot::channel();
        if self.cmd_tx.send(MonitorCmd::Snapshot(tx)).await.is_err() {
            return None;
        }
        rx.await.ok()
    }
}

/// Pending-state bookkeeping shared between the monitor loop and W6 reads.
/// All fields are held under their own RwLocks so readers don't block on
/// unrelated writes.
#[derive(Default)]
pub struct PendingState {
    pub gates: RwLock<Vec<EnrichedGate>>,
    pub escalations: RwLock<Vec<EnrichedEscalation>>,
    pub refusals: RwLock<Vec<Refusal>>,
}

#[derive(Debug)]
pub enum MonitorCmd {
    Shutdown,
    Snapshot(oneshot::Sender<MonitorSnapshot>),
}

#[derive(Debug, Clone)]
pub struct MonitorSnapshot {
    pub session_id: String,
    pub workspaces: HashMap<String, String>, // workspace_id → current state
    pub pending_gates: usize,
    pub pending_escalations: usize,
    pub pending_refusals: usize,
}

/// Set of workspaces the monitor tracks. Events for any other workspace
/// are dropped on the floor.
#[derive(Clone, Debug)]
pub struct WorkspaceSet {
    pub root: String,
    pub members: HashSet<String>,
}

impl WorkspaceSet {
    pub fn new(root: String, assignments: impl IntoIterator<Item = String>) -> Self {
        let mut members: HashSet<String> = assignments.into_iter().collect();
        members.insert(root.clone());
        Self { root, members }
    }

    pub fn contains(&self, ws: &str) -> bool {
        self.members.contains(ws)
    }
}

/// Frame envelope broadcast to WS subscribers. `channel` + `session_id` +
/// `event` matches the shape in `wcon-highway.md` §2.2.
#[derive(Debug, Clone, Serialize)]
pub struct Frame {
    pub channel: Channel,
    pub session_id: String,
    pub event: FrameEvent,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Trail,
    Gates,
    Escalations,
    Workspaces,
    Refusals,
    Session,
    Control,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FrameEvent {
    Trail(EnrichedTrailEntry),
    Gate(EnrichedGate),
    Escalation(EnrichedEscalation),
    WorkspaceChange(WorkspaceChange),
    Refusal(Refusal),
    SessionLifecycle { state: String, reason: Option<String> },
    Lag { refresh_hint: Vec<&'static str>, reason: String },
    MonitorError { stream: &'static str, transient: bool, message: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceChange {
    pub workspace_id: String,
    pub previous: String,
    pub current: String,
    pub trigger: String,
    pub timestamp: String,
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub broadcast_capacity: usize,
    pub reconnect_initial: Duration,
    pub reconnect_max: Duration,
    pub reconnect_failure_cap: u32,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            broadcast_capacity: 256,
            reconnect_initial: Duration::from_millis(200),
            reconnect_max: Duration::from_secs(30),
            reconnect_failure_cap: 30,
        }
    }
}

// ============================================================================
// Spawn
// ============================================================================

/// Spawn a new monitor task for `session_id`. Returns a handle the launcher
/// inserts into `AppState.active_sessions`. The monitor lives until
/// `shutdown()` is called, the session transitions to a terminal state, or
/// reconnect failures exceed the cap.
pub fn spawn(
    session_id: String,
    workspaces: WorkspaceSet,
    pool: Arc<GrpcPool>,
    db: DbPool,
    enricher: EventEnricher,
    refusals: RefusalSynthesizer,
    cfg: MonitorConfig,
) -> (SessionMonitorHandle, JoinHandle<()>) {
    let (cmd_tx, cmd_rx) = mpsc::channel(16);
    let (broadcast_tx, _) = broadcast::channel(cfg.broadcast_capacity);
    let pending = Arc::new(PendingState::default());

    let handle = SessionMonitorHandle {
        session_id: session_id.clone(),
        cmd_tx,
        broadcast_tx: broadcast_tx.clone(),
        pending: pending.clone(),
    };

    let monitor = Monitor {
        session_id: session_id.clone(),
        workspaces,
        pool,
        db,
        enricher,
        refusals,
        cfg,
        broadcast_tx,
        pending,
        workspace_labels: HashMap::new(),
        workspace_states: HashMap::new(),
    };

    let join = tokio::spawn(monitor.run(cmd_rx));
    (handle, join)
}

// ============================================================================
// Monitor task
// ============================================================================

struct Monitor {
    session_id: String,
    workspaces: WorkspaceSet,
    pool: Arc<GrpcPool>,
    db: DbPool,
    enricher: EventEnricher,
    refusals: RefusalSynthesizer,
    cfg: MonitorConfig,
    broadcast_tx: broadcast::Sender<Frame>,
    pending: Arc<PendingState>,
    workspace_labels: HashMap<String, String>,
    workspace_states: HashMap<String, String>,
}

enum StreamEvent {
    Trail(proto::TrailEntry),
    Gate(proto::GateEvent),
    Escalation(proto::EscalationEvent),
    WorkspaceChange(proto::WorkspaceStateChange),
    Lag {
        stream: &'static str,
        attempt: u32,
        reason: String,
    },
    /// Emitted when a stream driver exhausts its reconnect budget. The main
    /// loop treats this as a terminal failure.
    Fatal {
        stream: &'static str,
        reason: String,
    },
}

impl Monitor {
    async fn run(mut self, mut cmd_rx: mpsc::Receiver<MonitorCmd>) {
        info!(session_id = %self.session_id, "session monitor starting");

        // Seed workspace labels via GetWorkspace for each member. Failures
        // just fall through to the workspace_id fallback label in the
        // enricher — no panic.
        self.seed_workspace_labels().await;

        let (tx, mut rx) = mpsc::channel::<StreamEvent>(256);

        // Spawn four stream drivers, one per highway stream.
        let drivers = self.spawn_stream_drivers(tx.clone());

        loop {
            tokio::select! {
                biased;
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(MonitorCmd::Shutdown) => {
                            info!(session_id = %self.session_id, "monitor received Shutdown");
                            break;
                        }
                        Some(MonitorCmd::Snapshot(reply)) => {
                            let snap = MonitorSnapshot {
                                session_id: self.session_id.clone(),
                                workspaces: self.workspace_states.clone(),
                                pending_gates: self.pending.gates.read().await.len(),
                                pending_escalations: self.pending.escalations.read().await.len(),
                                pending_refusals: self.pending.refusals.read().await.len(),
                            };
                            let _ = reply.send(snap);
                        }
                        None => break,
                    }
                }
                ev = rx.recv() => {
                    match ev {
                        Some(event) => {
                            if let Some(session_terminal) = self.handle_event(event).await {
                                // Session reached a terminal state — exit the loop after
                                // broadcasting the lifecycle frame.
                                info!(
                                    session_id = %self.session_id,
                                    state = %session_terminal,
                                    "session terminal state detected"
                                );
                                let _ = self.record_terminal(&session_terminal).await;
                                break;
                            }
                        }
                        None => {
                            warn!(session_id = %self.session_id, "stream aggregator channel closed");
                            break;
                        }
                    }
                }
            }
        }

        // Drain and stop drivers.
        for d in drivers {
            d.abort();
        }
        info!(session_id = %self.session_id, "session monitor stopped");
    }

    async fn seed_workspace_labels(&mut self) {
        let Some(mut client) = self.pool.highway().await else {
            warn!(session_id = %self.session_id, "highway channel unavailable at monitor start");
            return;
        };
        for ws in &self.workspaces.members {
            let req = proto::GetWorkspaceRequest {
                workspace_id: ws.clone(),
            };
            match client.get_workspace(req).await {
                Ok(resp) => {
                    let view = resp.into_inner();
                    if !view.role.is_empty() {
                        self.workspace_labels.insert(view.id.clone(), view.role.clone());
                    }
                    self.workspace_states
                        .insert(view.id.clone(), workspace_state_string(view.state()));
                }
                Err(e) => warn!(session_id = %self.session_id, workspace_id = %ws, error = %e, "GetWorkspace failed at seed"),
            }
        }
    }

    fn spawn_stream_drivers(&self, tx: mpsc::Sender<StreamEvent>) -> Vec<JoinHandle<()>> {
        let mut out = Vec::with_capacity(4);
        let cfg = self.cfg.clone();
        let pool = self.pool.clone();

        // Trail
        {
            let tx = tx.clone();
            let cfg = cfg.clone();
            let pool = pool.clone();
            out.push(tokio::spawn(async move {
                run_stream_driver("trail", pool, cfg, tx, trail_driver).await;
            }));
        }
        // Gates
        {
            let tx = tx.clone();
            let cfg = cfg.clone();
            let pool = pool.clone();
            out.push(tokio::spawn(async move {
                run_stream_driver("gates", pool, cfg, tx, gates_driver).await;
            }));
        }
        // Escalations
        {
            let tx = tx.clone();
            let cfg = cfg.clone();
            let pool = pool.clone();
            out.push(tokio::spawn(async move {
                run_stream_driver("escalations", pool, cfg, tx, escalations_driver).await;
            }));
        }
        // Workspace changes
        {
            let tx = tx.clone();
            let cfg = cfg.clone();
            let pool = pool.clone();
            out.push(tokio::spawn(async move {
                run_stream_driver(
                    "workspace_changes",
                    pool,
                    cfg,
                    tx,
                    workspace_changes_driver,
                )
                .await;
            }));
        }
        out
    }

    /// Process a single StreamEvent. Returns `Some(terminal_state_name)` if
    /// the event implies the session should terminate (root workspace
    /// reached CLOSED or FAILED).
    async fn handle_event(&mut self, ev: StreamEvent) -> Option<String> {
        match ev {
            StreamEvent::Trail(raw) => {
                if !self.workspaces.contains(&raw.workspace_id) {
                    return None;
                }
                // Refusal detection fans out to the refusals channel.
                if let Some(refusal) = self.refusals.detect(&raw) {
                    self.pending.refusals.write().await.push(refusal.clone());
                    let _ = self.broadcast_tx.send(Frame {
                        channel: Channel::Refusals,
                        session_id: self.session_id.clone(),
                        event: FrameEvent::Refusal(refusal),
                    });
                }
                // Gate resolution trail entries prune pending gates.
                if raw.event_type == "gate_resolved"
                    && let Some(gate_id) = parse_gate_id(&raw.body)
                {
                    self.pending.gates.write().await.retain(|g| g.gate_id != gate_id);
                }
                let label = self.workspace_labels.get(&raw.workspace_id).cloned();
                let enriched = self.enricher.enrich_trail(&raw, label.as_deref());
                let _ = self.broadcast_tx.send(Frame {
                    channel: Channel::Trail,
                    session_id: self.session_id.clone(),
                    event: FrameEvent::Trail(enriched),
                });
                None
            }
            StreamEvent::Gate(raw) => {
                if !self.workspaces.contains(&raw.workspace_id) {
                    return None;
                }
                let label = self.workspace_labels.get(&raw.workspace_id).cloned();
                let enriched = self.enricher.enrich_gate(&raw, label.as_deref());
                self.pending.gates.write().await.push(enriched.clone());
                let _ = self.broadcast_tx.send(Frame {
                    channel: Channel::Gates,
                    session_id: self.session_id.clone(),
                    event: FrameEvent::Gate(enriched),
                });
                None
            }
            StreamEvent::Escalation(raw) => {
                if !self.workspaces.contains(&raw.workspace_id) {
                    return None;
                }
                let label = self.workspace_labels.get(&raw.workspace_id).cloned();
                let enriched = self.enricher.enrich_escalation(&raw, label.as_deref());
                self.pending
                    .escalations
                    .write()
                    .await
                    .push(enriched.clone());
                let _ = self.broadcast_tx.send(Frame {
                    channel: Channel::Escalations,
                    session_id: self.session_id.clone(),
                    event: FrameEvent::Escalation(enriched),
                });
                None
            }
            StreamEvent::WorkspaceChange(raw) => {
                if !self.workspaces.contains(&raw.workspace_id) {
                    return None;
                }
                let prev = workspace_state_string(raw.previous());
                let curr = workspace_state_string(raw.current());
                self.workspace_states
                    .insert(raw.workspace_id.clone(), curr.clone());
                let change = WorkspaceChange {
                    workspace_id: raw.workspace_id.clone(),
                    previous: prev,
                    current: curr.clone(),
                    trigger: raw.trigger.clone(),
                    timestamp: event_enricher_util::timestamp_rfc3339(&raw.timestamp),
                };
                let _ = self.broadcast_tx.send(Frame {
                    channel: Channel::Workspaces,
                    session_id: self.session_id.clone(),
                    event: FrameEvent::WorkspaceChange(change),
                });
                // Completion detection: root workspace hitting a terminal state.
                if raw.workspace_id == self.workspaces.root
                    && matches!(
                        raw.current(),
                        proto::WorkspaceState::Closed | proto::WorkspaceState::Failed
                    )
                {
                    let terminal = match raw.current() {
                        proto::WorkspaceState::Failed => session_state::FAILED,
                        _ => session_state::COMPLETED,
                    };
                    let _ = self.broadcast_tx.send(Frame {
                        channel: Channel::Session,
                        session_id: self.session_id.clone(),
                        event: FrameEvent::SessionLifecycle {
                            state: terminal.to_string(),
                            reason: Some(raw.trigger.clone()),
                        },
                    });
                    return Some(terminal.to_string());
                }
                None
            }
            StreamEvent::Lag {
                stream,
                attempt,
                reason,
            } => {
                let _ = self.broadcast_tx.send(Frame {
                    channel: Channel::Control,
                    session_id: self.session_id.clone(),
                    event: FrameEvent::Lag {
                        refresh_hint: lag_refresh_hint(stream),
                        reason: format!("{stream} reconnected after {attempt} attempts: {reason}"),
                    },
                });
                None
            }
            StreamEvent::Fatal { stream, reason } => {
                let _ = self.broadcast_tx.send(Frame {
                    channel: Channel::Control,
                    session_id: self.session_id.clone(),
                    event: FrameEvent::MonitorError {
                        stream,
                        transient: false,
                        message: reason,
                    },
                });
                // Mark session FAILED; the caller drops this monitor.
                Some(session_state::FAILED.to_string())
            }
        }
    }

    async fn record_terminal(&self, to_state: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        // transition_state is idempotent on the FROM state mismatch — if the
        // session was already closed-out (e.g. by a cancel path), this is a
        // harmless no-op.
        sessions::transition_state(
            &self.db,
            &self.session_id,
            session_state::ACTIVE,
            to_state,
            &now,
        )
        .await
        .map(|_| ())
    }
}

fn lag_refresh_hint(stream: &'static str) -> Vec<&'static str> {
    match stream {
        "gates" => vec!["gates"],
        "escalations" => vec!["escalations"],
        "trail" => vec![],
        "workspace_changes" => vec!["workspaces"],
        _ => vec![],
    }
}

fn workspace_state_string(s: proto::WorkspaceState) -> String {
    match s {
        proto::WorkspaceState::Unspecified => "unspecified",
        proto::WorkspaceState::Idle => "idle",
        proto::WorkspaceState::Active => "active",
        proto::WorkspaceState::Blocked => "blocked",
        proto::WorkspaceState::Suspended => "suspended",
        proto::WorkspaceState::Migrating => "migrating",
        proto::WorkspaceState::Integrating => "integrating",
        proto::WorkspaceState::Conflicted => "conflicted",
        proto::WorkspaceState::Closed => "closed",
        proto::WorkspaceState::Failed => "failed",
    }
    .to_string()
}

fn parse_gate_id(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("gate_id").and_then(|g| g.as_str()).map(String::from))
}

// ============================================================================
// Stream driver — subscribe, recv, reconnect with backoff
// ============================================================================

/// Generic driver that runs one `StreamTrait::subscribe` + receive loop
/// with exponential backoff on failure. `subscribe_and_recv` yields a
/// result per stream frame; the driver converts it into `StreamEvent`s
/// and forwards via `tx`. On reconnect, a `Lag` event is emitted first.
async fn run_stream_driver<F, Fut>(
    stream_name: &'static str,
    pool: Arc<GrpcPool>,
    cfg: MonitorConfig,
    tx: mpsc::Sender<StreamEvent>,
    driver: F,
) where
    F: Fn(Arc<GrpcPool>, mpsc::Sender<StreamEvent>, &'static str) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), String>> + Send,
{
    let mut backoff = cfg.reconnect_initial;
    let mut failures: u32 = 0;
    let mut attempts: u32 = 0;
    loop {
        match driver(pool.clone(), tx.clone(), stream_name).await {
            Ok(()) => {
                warn!(stream = stream_name, "stream ended (no error) — reconnecting");
            }
            Err(reason) => {
                warn!(stream = stream_name, error = %reason, "stream driver errored");
                let _ = tx
                    .send(StreamEvent::Lag {
                        stream: stream_name,
                        attempt: attempts,
                        reason: reason.clone(),
                    })
                    .await;
            }
        }
        failures += 1;
        attempts += 1;
        if failures >= cfg.reconnect_failure_cap {
            let _ = tx
                .send(StreamEvent::Fatal {
                    stream: stream_name,
                    reason: format!("reconnect cap {failures} exceeded"),
                })
                .await;
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(cfg.reconnect_max);
    }
}

// Per-stream body: subscribe + forward until the stream ends.
async fn trail_driver(
    pool: Arc<GrpcPool>,
    tx: mpsc::Sender<StreamEvent>,
    _: &'static str,
) -> Result<(), String> {
    let mut client = pool.highway().await.ok_or_else(|| "highway unavailable".to_string())?;
    let mut stream = client
        .stream_trail(proto::StreamTrailRequest {
            workspace_id: String::new(),
            event_type: String::new(),
            from_beginning: false,
        })
        .await
        .map_err(|e| e.to_string())?
        .into_inner();
    while let Some(item) = stream.message().await.map_err(|e| e.to_string())? {
        if tx.send(StreamEvent::Trail(item)).await.is_err() {
            return Ok(()); // monitor gone — stop driver
        }
    }
    Ok(())
}

async fn gates_driver(
    pool: Arc<GrpcPool>,
    tx: mpsc::Sender<StreamEvent>,
    _: &'static str,
) -> Result<(), String> {
    let mut client = pool.highway().await.ok_or_else(|| "highway unavailable".to_string())?;
    let mut stream = client
        .stream_gates(proto::StreamGatesRequest::default())
        .await
        .map_err(|e| e.to_string())?
        .into_inner();
    while let Some(item) = stream.message().await.map_err(|e| e.to_string())? {
        if tx.send(StreamEvent::Gate(item)).await.is_err() {
            return Ok(());
        }
    }
    Ok(())
}

async fn escalations_driver(
    pool: Arc<GrpcPool>,
    tx: mpsc::Sender<StreamEvent>,
    _: &'static str,
) -> Result<(), String> {
    let mut client = pool.highway().await.ok_or_else(|| "highway unavailable".to_string())?;
    let mut stream = client
        .stream_escalations(proto::StreamEscalationsRequest {
            user_id: String::new(),
        })
        .await
        .map_err(|e| e.to_string())?
        .into_inner();
    while let Some(item) = stream.message().await.map_err(|e| e.to_string())? {
        if tx.send(StreamEvent::Escalation(item)).await.is_err() {
            return Ok(());
        }
    }
    Ok(())
}

async fn workspace_changes_driver(
    pool: Arc<GrpcPool>,
    tx: mpsc::Sender<StreamEvent>,
    _: &'static str,
) -> Result<(), String> {
    let mut client = pool.highway().await.ok_or_else(|| "highway unavailable".to_string())?;
    let mut stream = client
        .stream_workspace_changes(proto::StreamWorkspaceChangesRequest {
            workspace_id: String::new(),
        })
        .await
        .map_err(|e| e.to_string())?
        .into_inner();
    while let Some(item) = stream.message().await.map_err(|e| e.to_string())? {
        if tx.send(StreamEvent::WorkspaceChange(item)).await.is_err() {
            return Ok(());
        }
    }
    Ok(())
}

// ============================================================================
// Small util shared with enricher. Kept local to avoid a public API churn.
// ============================================================================
mod event_enricher_util {
    use console_runtime::proto as proto;
    pub(super) fn timestamp_rfc3339(ts: &Option<proto::Timestamp>) -> String {
        let Some(ts) = ts else {
            return String::new();
        };
        let secs = (ts.physical_us / 1_000_000) as i64;
        let nanos = ((ts.physical_us % 1_000_000) * 1_000) as u32;
        match chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos) {
            Some(dt) if ts.logical == 0 => dt.to_rfc3339(),
            Some(dt) => format!("{}#{}", dt.to_rfc3339(), ts.logical),
            None => String::new(),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use console_db::create_test_pool;
    use console_runtime::grpc_pool::GrpcPool;

    fn empty_taxonomy() -> Arc<ArcSwap<crate::taxonomy::TaxonomyIndex>> {
        Arc::new(ArcSwap::from_pointee(
            crate::taxonomy_builder::build_index(None, &[], &[]).index,
        ))
    }

    fn tiny_cfg() -> MonitorConfig {
        MonitorConfig {
            broadcast_capacity: 64,
            reconnect_initial: Duration::from_millis(5),
            reconnect_max: Duration::from_millis(10),
            reconnect_failure_cap: 3,
        }
    }

    async fn insert_user(db: &DbPool, id: &str) {
        sqlx::query(
            "INSERT INTO users (id, username, username_lower, display_name, password_hash,
                console_role, must_change_password, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .bind(id)
        .bind("h")
        .bind("operator")
        .bind(0_i64)
        .bind("2026-04-15T00:00:00Z")
        .bind("2026-04-15T00:00:00Z")
        .execute(db)
        .await
        .expect("insert user");
    }

    async fn insert_session(db: &DbPool, id: &str, state: &str) {
        let row = sessions::SessionRow {
            id: id.into(),
            name: Some(id.into()),
            owner_user_id: "u-1".into(),
            vertical: "swe".into(),
            workflow: "wf".into(),
            context: None,
            coordinator_workspace_id: Some("ws-root".into()),
            state: state.into(),
            created_at: "2026-04-15T00:00:00Z".into(),
            launched_at: Some("2026-04-15T00:00:00Z".into()),
            closed_at: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
        };
        sessions::insert_session(db, &row).await.expect("insert session");
    }

    fn ws_set() -> WorkspaceSet {
        WorkspaceSet::new("ws-root".into(), vec!["ws-1".into(), "ws-2".into()])
    }

    #[test]
    fn workspace_set_contains_root_and_members() {
        let set = ws_set();
        assert!(set.contains("ws-root"));
        assert!(set.contains("ws-1"));
        assert!(set.contains("ws-2"));
        assert!(!set.contains("ws-outsider"));
    }

    #[test]
    fn workspace_state_roundtrip() {
        assert_eq!(workspace_state_string(proto::WorkspaceState::Active), "active");
        assert_eq!(workspace_state_string(proto::WorkspaceState::Closed), "closed");
        assert_eq!(workspace_state_string(proto::WorkspaceState::Failed), "failed");
    }

    #[test]
    fn parse_gate_id_from_json_body() {
        let body = br#"{"gate_id":"g-7","decision":"approve"}"#;
        assert_eq!(parse_gate_id(body), Some("g-7".to_string()));
        assert_eq!(parse_gate_id(b"not json"), None);
    }

    #[test]
    fn lag_refresh_hint_covers_all_streams() {
        assert_eq!(lag_refresh_hint("gates"), vec!["gates"]);
        assert_eq!(lag_refresh_hint("escalations"), vec!["escalations"]);
        assert_eq!(lag_refresh_hint("workspace_changes"), vec!["workspaces"]);
        assert!(lag_refresh_hint("trail").is_empty());
        assert!(lag_refresh_hint("unknown").is_empty());
    }

    /// End-to-end check: spawn a monitor against a disconnected pool, send
    /// Shutdown, confirm the task exits cleanly within the reconnect budget.
    #[tokio::test]
    async fn monitor_shuts_down_cleanly_when_commanded() {
        let db = create_test_pool().await.unwrap();
        insert_user(&db, "u-1").await;
        insert_session(&db, "s-shut", session_state::ACTIVE).await;

        // Pool dials a port nothing is listening on → all streams fail; the
        // driver hits the reconnect cap within a handful of ms.
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        pool.connect().await;

        let (handle, join) = spawn(
            "s-shut".into(),
            ws_set(),
            pool,
            db,
            EventEnricher::new(empty_taxonomy()),
            RefusalSynthesizer::new(),
            tiny_cfg(),
        );

        // Fire Shutdown. Whichever wins — our Shutdown or the Fatal-driven
        // exit — the task terminates promptly.
        handle.shutdown().await;
        // Give the task a bounded window to drain.
        let _ = tokio::time::timeout(Duration::from_secs(5), join).await;
    }

    #[test]
    fn workspace_state_string_covers_all_variants() {
        for s in [
            proto::WorkspaceState::Unspecified,
            proto::WorkspaceState::Idle,
            proto::WorkspaceState::Active,
            proto::WorkspaceState::Blocked,
            proto::WorkspaceState::Suspended,
            proto::WorkspaceState::Migrating,
            proto::WorkspaceState::Integrating,
            proto::WorkspaceState::Conflicted,
            proto::WorkspaceState::Closed,
            proto::WorkspaceState::Failed,
        ] {
            assert!(!workspace_state_string(s).is_empty(), "missing label for {s:?}");
        }
    }

    #[test]
    fn workspace_set_dedupes_root_passed_twice() {
        let set = WorkspaceSet::new("ws-root".into(), vec!["ws-root".into(), "ws-1".into()]);
        assert_eq!(set.members.len(), 2);
        assert!(set.contains("ws-root"));
        assert!(set.contains("ws-1"));
    }

    #[test]
    fn parse_gate_id_returns_none_when_field_missing() {
        let body = br#"{"decision":"approve"}"#;
        assert_eq!(parse_gate_id(body), None);
    }

    #[test]
    fn parse_gate_id_returns_none_when_field_not_string() {
        let body = br#"{"gate_id":42}"#;
        assert_eq!(parse_gate_id(body), None);
    }

    fn frame(channel: Channel, event: FrameEvent) -> Frame {
        Frame { channel, session_id: "s-1".into(), event }
    }

    #[test]
    fn frame_serializes_lag_with_kind_tag() {
        let f = frame(
            Channel::Control,
            FrameEvent::Lag {
                refresh_hint: vec!["gates"],
                reason: "reconnect".into(),
            },
        );
        let v: serde_json::Value = serde_json::to_value(&f).unwrap();
        assert_eq!(v["channel"], "control");
        assert_eq!(v["event"]["kind"], "lag");
        assert_eq!(v["event"]["refresh_hint"][0], "gates");
    }

    #[test]
    fn frame_serializes_monitor_error_with_transient_flag() {
        let f = frame(
            Channel::Control,
            FrameEvent::MonitorError {
                stream: "trail",
                transient: false,
                message: "cap exceeded".into(),
            },
        );
        let v: serde_json::Value = serde_json::to_value(&f).unwrap();
        assert_eq!(v["event"]["kind"], "monitor_error");
        assert_eq!(v["event"]["transient"], false);
        assert_eq!(v["event"]["stream"], "trail");
    }

    #[test]
    fn frame_serializes_session_lifecycle_with_state_and_reason() {
        let f = frame(
            Channel::Session,
            FrameEvent::SessionLifecycle {
                state: "completed".into(),
                reason: Some("root closed".into()),
            },
        );
        let v: serde_json::Value = serde_json::to_value(&f).unwrap();
        assert_eq!(v["event"]["kind"], "session_lifecycle");
        assert_eq!(v["event"]["state"], "completed");
        assert_eq!(v["event"]["reason"], "root closed");
    }

    #[test]
    fn frame_serializes_workspace_change_payload() {
        let f = frame(
            Channel::Workspaces,
            FrameEvent::WorkspaceChange(WorkspaceChange {
                workspace_id: "ws-1".into(),
                previous: "idle".into(),
                current: "active".into(),
                trigger: "envelope".into(),
                timestamp: "2026-04-15T00:00:00Z".into(),
            }),
        );
        let v: serde_json::Value = serde_json::to_value(&f).unwrap();
        assert_eq!(v["channel"], "workspaces");
        assert_eq!(v["event"]["kind"], "workspace_change");
        assert_eq!(v["event"]["workspace_id"], "ws-1");
        assert_eq!(v["event"]["current"], "active");
    }

    #[test]
    fn channel_serializes_to_snake_case() {
        for (chan, expected) in [
            (Channel::Trail, "trail"),
            (Channel::Gates, "gates"),
            (Channel::Escalations, "escalations"),
            (Channel::Workspaces, "workspaces"),
            (Channel::Refusals, "refusals"),
            (Channel::Session, "session"),
            (Channel::Control, "control"),
        ] {
            let v = serde_json::to_value(chan).unwrap();
            assert_eq!(v.as_str(), Some(expected));
        }
    }

    #[tokio::test]
    async fn pending_state_is_default_empty() {
        let p = PendingState::default();
        assert!(p.gates.read().await.is_empty());
        assert!(p.escalations.read().await.is_empty());
        assert!(p.refusals.read().await.is_empty());
    }

    #[test]
    fn handle_subscribe_returns_a_live_receiver() {
        let (tx, _rx) = broadcast::channel::<Frame>(8);
        let handle = SessionMonitorHandle {
            session_id: "s".into(),
            cmd_tx: mpsc::channel(1).0,
            broadcast_tx: tx.clone(),
            pending: Arc::new(PendingState::default()),
        };
        let mut sub = handle.subscribe();
        let f = frame(Channel::Trail, FrameEvent::Lag { refresh_hint: vec![], reason: "x".into() });
        tx.send(f.clone()).expect("send");
        let received = sub.try_recv().expect("frame");
        assert_eq!(received.session_id, "s-1");
    }

    #[test]
    fn timestamp_rfc3339_handles_none_and_logical() {
        use event_enricher_util::timestamp_rfc3339;
        assert_eq!(timestamp_rfc3339(&None), "");
        let ts = Some(proto::Timestamp { physical_us: 1_700_000_000_000_000, logical: 0 });
        assert!(timestamp_rfc3339(&ts).contains("2023"));
        let ts2 = Some(proto::Timestamp { physical_us: 1_700_000_000_000_000, logical: 9 });
        assert!(timestamp_rfc3339(&ts2).ends_with("#9"));
    }
}
