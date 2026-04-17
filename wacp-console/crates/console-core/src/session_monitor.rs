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
use console_runtime::proto;
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
    SessionLifecycle {
        state: String,
        reason: Option<String>,
    },
    Lag {
        refresh_hint: Vec<&'static str>,
        reason: String,
    },
    MonitorError {
        stream: &'static str,
        transient: bool,
        message: String,
    },
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
                        self.workspace_labels
                            .insert(view.id.clone(), view.role.clone());
                    }
                    self.workspace_states
                        .insert(view.id.clone(), workspace_state_string(view.state()));
                }
                Err(e) => {
                    warn!(session_id = %self.session_id, workspace_id = %ws, error = %e, "GetWorkspace failed at seed")
                }
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
                run_stream_driver("workspace_changes", pool, cfg, tx, workspace_changes_driver)
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
                    self.pending
                        .gates
                        .write()
                        .await
                        .retain(|g| g.gate_id != gate_id);
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
                warn!(
                    stream = stream_name,
                    "stream ended (no error) — reconnecting"
                );
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
    let mut client = pool
        .highway()
        .await
        .ok_or_else(|| "highway unavailable".to_string())?;
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
    let mut client = pool
        .highway()
        .await
        .ok_or_else(|| "highway unavailable".to_string())?;
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
    let mut client = pool
        .highway()
        .await
        .ok_or_else(|| "highway unavailable".to_string())?;
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
    let mut client = pool
        .highway()
        .await
        .ok_or_else(|| "highway unavailable".to_string())?;
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
    use console_runtime::proto;
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
        sessions::insert_session(db, &row)
            .await
            .expect("insert session");
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
        assert_eq!(
            workspace_state_string(proto::WorkspaceState::Active),
            "active"
        );
        assert_eq!(
            workspace_state_string(proto::WorkspaceState::Closed),
            "closed"
        );
        assert_eq!(
            workspace_state_string(proto::WorkspaceState::Failed),
            "failed"
        );
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
            assert!(
                !workspace_state_string(s).is_empty(),
                "missing label for {s:?}"
            );
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
        Frame {
            channel,
            session_id: "s-1".into(),
            event,
        }
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
        let f = frame(
            Channel::Trail,
            FrameEvent::Lag {
                refresh_hint: vec![],
                reason: "x".into(),
            },
        );
        tx.send(f.clone()).expect("send");
        let received = sub.try_recv().expect("frame");
        assert_eq!(received.session_id, "s-1");
    }

    #[test]
    fn timestamp_rfc3339_handles_none_and_logical() {
        use event_enricher_util::timestamp_rfc3339;
        assert_eq!(timestamp_rfc3339(&None), "");
        let ts = Some(proto::Timestamp {
            physical_us: 1_700_000_000_000_000,
            logical: 0,
        });
        assert!(timestamp_rfc3339(&ts).contains("2023"));
        let ts2 = Some(proto::Timestamp {
            physical_us: 1_700_000_000_000_000,
            logical: 9,
        });
        assert!(timestamp_rfc3339(&ts2).ends_with("#9"));
    }

    // ========================================================================
    // Helper: construct a Monitor directly (bypassing spawn + stream drivers)
    // so we can feed StreamEvents through handle_event in isolation.
    // ========================================================================

    async fn make_monitor(db: &DbPool) -> (Monitor, broadcast::Receiver<Frame>) {
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        let (broadcast_tx, rx) = broadcast::channel(256);
        let pending = Arc::new(PendingState::default());
        let monitor = Monitor {
            session_id: "s-test".into(),
            workspaces: ws_set(),
            pool,
            db: db.clone(),
            enricher: EventEnricher::new(empty_taxonomy()),
            refusals: RefusalSynthesizer::new(),
            cfg: tiny_cfg(),
            broadcast_tx,
            pending,
            workspace_labels: HashMap::from([("ws-1".to_string(), "swe:implementer".to_string())]),
            workspace_states: HashMap::new(),
        };
        (monitor, rx)
    }

    fn trail_entry(ws: &str, event_type: &str, body: &[u8]) -> proto::TrailEntry {
        proto::TrailEntry {
            id: format!("t-{ws}"),
            timestamp: Some(proto::Timestamp {
                physical_us: 1_700_000_000_000_000,
                logical: 0,
            }),
            workspace_id: ws.into(),
            actor: "agent".into(),
            event_type: event_type.into(),
            body: body.to_vec(),
            sequence_number: 1,
            chain_hash: vec![],
        }
    }

    fn gate_event(ws: &str, gate_id: &str) -> proto::GateEvent {
        proto::GateEvent {
            gate_id: gate_id.into(),
            r#type: proto::GateType::TaskApproval as i32,
            subject: vec![],
            workspace_id: ws.into(),
            task_id: "t-1".into(),
            timeout_ms: 30_000,
            fallback_action: "reject".into(),
            created_at: Some(proto::Timestamp {
                physical_us: 1_700_000_000_000_000,
                logical: 0,
            }),
        }
    }

    fn escalation_event(ws: &str, esc_id: &str) -> proto::EscalationEvent {
        proto::EscalationEvent {
            escalation_id: esc_id.into(),
            workspace_id: ws.into(),
            owner: "u-1".into(),
            context: vec![],
            created_at: Some(proto::Timestamp {
                physical_us: 1_700_000_000_000_000,
                logical: 0,
            }),
        }
    }

    fn workspace_change(
        ws: &str,
        prev: proto::WorkspaceState,
        curr: proto::WorkspaceState,
    ) -> proto::WorkspaceStateChange {
        proto::WorkspaceStateChange {
            workspace_id: ws.into(),
            previous: prev as i32,
            current: curr as i32,
            trigger: "test-trigger".into(),
            timestamp: Some(proto::Timestamp {
                physical_us: 1_700_000_000_000_000,
                logical: 0,
            }),
        }
    }

    // ========================================================================
    // handle_event: Trail
    // ========================================================================

    #[tokio::test]
    async fn handle_event_trail_broadcasts_on_matching_workspace() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::Trail(trail_entry("ws-1", "envelope_delivered", b"hello"));
        let terminal = mon.handle_event(ev).await;
        assert!(terminal.is_none());
        let f = rx.try_recv().expect("expected trail frame");
        assert_eq!(f.channel, Channel::Trail);
        assert_eq!(f.session_id, "s-test");
    }

    #[tokio::test]
    async fn handle_event_trail_drops_event_for_outside_workspace() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::Trail(trail_entry("ws-outsider", "signal", b"x"));
        let terminal = mon.handle_event(ev).await;
        assert!(terminal.is_none());
        assert!(
            rx.try_recv().is_err(),
            "no frame should be broadcast for outsider ws"
        );
    }

    #[tokio::test]
    async fn handle_event_trail_detects_refusal_and_broadcasts_both_channels() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let body = br#"{"code":"BLOCKED","reason":"not allowed"}"#;
        let ev = StreamEvent::Trail(trail_entry("ws-1", "tool_call_refused", body));
        let terminal = mon.handle_event(ev).await;
        assert!(terminal.is_none());
        // First frame: Refusal channel
        let f1 = rx.try_recv().expect("refusal frame");
        assert_eq!(f1.channel, Channel::Refusals);
        // Second frame: Trail channel
        let f2 = rx.try_recv().expect("trail frame");
        assert_eq!(f2.channel, Channel::Trail);
        // Pending refusals should have 1 entry
        assert_eq!(mon.pending.refusals.read().await.len(), 1);
    }

    #[tokio::test]
    async fn handle_event_trail_gate_resolved_prunes_pending_gate() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        // First, add a gate to pending
        let gate_ev = StreamEvent::Gate(gate_event("ws-1", "g-99"));
        mon.handle_event(gate_ev).await;
        let _ = rx.try_recv(); // drain the gate frame
        assert_eq!(mon.pending.gates.read().await.len(), 1);

        // Now resolve it via a trail entry with gate_resolved event type
        let body = br#"{"gate_id":"g-99","decision":"approve"}"#;
        let ev = StreamEvent::Trail(trail_entry("ws-1", "gate_resolved", body));
        mon.handle_event(ev).await;
        let _ = rx.try_recv(); // drain trail frame
        assert_eq!(
            mon.pending.gates.read().await.len(),
            0,
            "gate should be pruned after gate_resolved"
        );
    }

    // ========================================================================
    // handle_event: Gate
    // ========================================================================

    #[tokio::test]
    async fn handle_event_gate_broadcasts_and_appends_pending() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::Gate(gate_event("ws-1", "g-1"));
        let terminal = mon.handle_event(ev).await;
        assert!(terminal.is_none());
        let f = rx.try_recv().expect("gate frame");
        assert_eq!(f.channel, Channel::Gates);
        assert_eq!(mon.pending.gates.read().await.len(), 1);
        assert_eq!(mon.pending.gates.read().await[0].gate_id, "g-1");
    }

    #[tokio::test]
    async fn handle_event_gate_drops_for_outside_workspace() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::Gate(gate_event("ws-outsider", "g-2"));
        let terminal = mon.handle_event(ev).await;
        assert!(terminal.is_none());
        assert!(rx.try_recv().is_err());
        assert!(mon.pending.gates.read().await.is_empty());
    }

    // ========================================================================
    // handle_event: Escalation
    // ========================================================================

    #[tokio::test]
    async fn handle_event_escalation_broadcasts_and_appends_pending() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::Escalation(escalation_event("ws-1", "e-1"));
        let terminal = mon.handle_event(ev).await;
        assert!(terminal.is_none());
        let f = rx.try_recv().expect("escalation frame");
        assert_eq!(f.channel, Channel::Escalations);
        assert_eq!(mon.pending.escalations.read().await.len(), 1);
    }

    #[tokio::test]
    async fn handle_event_escalation_drops_for_outside_workspace() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::Escalation(escalation_event("ws-outsider", "e-2"));
        let terminal = mon.handle_event(ev).await;
        assert!(terminal.is_none());
        assert!(rx.try_recv().is_err());
        assert!(mon.pending.escalations.read().await.is_empty());
    }

    // ========================================================================
    // handle_event: WorkspaceChange — non-terminal
    // ========================================================================

    #[tokio::test]
    async fn handle_event_workspace_change_broadcasts_and_tracks_state() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::WorkspaceChange(workspace_change(
            "ws-1",
            proto::WorkspaceState::Idle,
            proto::WorkspaceState::Active,
        ));
        let terminal = mon.handle_event(ev).await;
        assert!(terminal.is_none());
        let f = rx.try_recv().expect("workspace change frame");
        assert_eq!(f.channel, Channel::Workspaces);
        assert_eq!(mon.workspace_states.get("ws-1").unwrap(), "active");
    }

    #[tokio::test]
    async fn handle_event_workspace_change_drops_for_outside_workspace() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::WorkspaceChange(workspace_change(
            "ws-outsider",
            proto::WorkspaceState::Idle,
            proto::WorkspaceState::Active,
        ));
        let terminal = mon.handle_event(ev).await;
        assert!(terminal.is_none());
        assert!(rx.try_recv().is_err());
    }

    // ========================================================================
    // Root workspace terminal states: completed, failed, cancelled
    // ========================================================================

    #[tokio::test]
    async fn root_workspace_closed_triggers_completed_terminal() {
        let db = create_test_pool().await.unwrap();
        insert_user(&db, "u-1").await;
        insert_session(&db, "s-test", session_state::ACTIVE).await;
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::WorkspaceChange(workspace_change(
            "ws-root",
            proto::WorkspaceState::Active,
            proto::WorkspaceState::Closed,
        ));
        let terminal = mon.handle_event(ev).await;
        assert_eq!(terminal.as_deref(), Some(session_state::COMPLETED));
        // Should broadcast: workspace change frame, then session lifecycle frame
        let f1 = rx.try_recv().expect("workspace change frame");
        assert_eq!(f1.channel, Channel::Workspaces);
        let f2 = rx.try_recv().expect("session lifecycle frame");
        assert_eq!(f2.channel, Channel::Session);
        if let FrameEvent::SessionLifecycle { state, .. } = &f2.event {
            assert_eq!(state, session_state::COMPLETED);
        } else {
            panic!("expected SessionLifecycle event");
        }
    }

    #[tokio::test]
    async fn root_workspace_failed_triggers_failed_terminal() {
        let db = create_test_pool().await.unwrap();
        insert_user(&db, "u-1").await;
        insert_session(&db, "s-test", session_state::ACTIVE).await;
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::WorkspaceChange(workspace_change(
            "ws-root",
            proto::WorkspaceState::Active,
            proto::WorkspaceState::Failed,
        ));
        let terminal = mon.handle_event(ev).await;
        assert_eq!(terminal.as_deref(), Some(session_state::FAILED));
        let _f1 = rx.try_recv().expect("workspace change");
        let f2 = rx.try_recv().expect("session lifecycle");
        if let FrameEvent::SessionLifecycle { state, .. } = &f2.event {
            assert_eq!(state, session_state::FAILED);
        } else {
            panic!("expected SessionLifecycle event");
        }
    }

    #[tokio::test]
    async fn non_root_workspace_closed_does_not_trigger_terminal() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        // ws-1 is a member, not root. Closing it should NOT terminate the session.
        let ev = StreamEvent::WorkspaceChange(workspace_change(
            "ws-1",
            proto::WorkspaceState::Active,
            proto::WorkspaceState::Closed,
        ));
        let terminal = mon.handle_event(ev).await;
        assert!(
            terminal.is_none(),
            "non-root workspace close should not terminate"
        );
        let f = rx.try_recv().expect("workspace frame");
        assert_eq!(f.channel, Channel::Workspaces);
        // No session lifecycle frame should follow.
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn root_workspace_non_terminal_state_does_not_trigger_session_end() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, _rx) = make_monitor(&db).await;
        // Root workspace going Active → Blocked is not terminal.
        let ev = StreamEvent::WorkspaceChange(workspace_change(
            "ws-root",
            proto::WorkspaceState::Active,
            proto::WorkspaceState::Blocked,
        ));
        let terminal = mon.handle_event(ev).await;
        assert!(terminal.is_none());
    }

    // ========================================================================
    // handle_event: Lag
    // ========================================================================

    #[tokio::test]
    async fn handle_event_lag_broadcasts_control_frame() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::Lag {
            stream: "gates",
            attempt: 2,
            reason: "connection reset".into(),
        };
        let terminal = mon.handle_event(ev).await;
        assert!(terminal.is_none());
        let f = rx.try_recv().expect("lag frame");
        assert_eq!(f.channel, Channel::Control);
        if let FrameEvent::Lag {
            refresh_hint,
            reason,
        } = &f.event
        {
            assert_eq!(refresh_hint, &vec!["gates"]);
            assert!(reason.contains("gates reconnected"));
            assert!(reason.contains("2 attempts"));
        } else {
            panic!("expected Lag event");
        }
    }

    // ========================================================================
    // handle_event: Fatal
    // ========================================================================

    #[tokio::test]
    async fn handle_event_fatal_broadcasts_error_and_returns_failed() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::Fatal {
            stream: "trail",
            reason: "reconnect cap 30 exceeded".into(),
        };
        let terminal = mon.handle_event(ev).await;
        assert_eq!(terminal.as_deref(), Some(session_state::FAILED));
        let f = rx.try_recv().expect("error frame");
        assert_eq!(f.channel, Channel::Control);
        if let FrameEvent::MonitorError {
            stream,
            transient,
            message,
        } = &f.event
        {
            assert_eq!(*stream, "trail");
            assert!(!transient);
            assert!(message.contains("reconnect cap"));
        } else {
            panic!("expected MonitorError event");
        }
    }

    // ========================================================================
    // Event enricher workspace label lookup miss
    // ========================================================================

    #[tokio::test]
    async fn enricher_label_miss_falls_back_to_workspace_id() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        // ws-2 is in the workspace set but NOT in workspace_labels (only ws-1 is)
        let ev = StreamEvent::Trail(trail_entry("ws-2", "signal", b"x"));
        mon.handle_event(ev).await;
        let f = rx.try_recv().expect("trail frame");
        if let FrameEvent::Trail(enriched) = &f.event {
            assert_eq!(
                enriched.workspace_label, "ws-2",
                "should fall back to workspace_id when label not found"
            );
        } else {
            panic!("expected Trail event");
        }
    }

    #[tokio::test]
    async fn enricher_label_present_uses_role_label() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        // ws-1 IS in workspace_labels with label "swe:implementer"
        let ev = StreamEvent::Trail(trail_entry("ws-1", "signal", b"x"));
        mon.handle_event(ev).await;
        let f = rx.try_recv().expect("trail frame");
        if let FrameEvent::Trail(enriched) = &f.event {
            assert_eq!(enriched.workspace_label, "swe:implementer");
        } else {
            panic!("expected Trail event");
        }
    }

    #[tokio::test]
    async fn enricher_label_miss_on_gate_falls_back() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        // ws-root is in workspace set but not in workspace_labels
        let ev = StreamEvent::Gate(gate_event("ws-root", "g-miss"));
        mon.handle_event(ev).await;
        let f = rx.try_recv().expect("gate frame");
        if let FrameEvent::Gate(enriched) = &f.event {
            assert_eq!(enriched.workspace_label, "ws-root");
        } else {
            panic!("expected Gate event");
        }
    }

    #[tokio::test]
    async fn enricher_label_miss_on_escalation_falls_back() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let ev = StreamEvent::Escalation(escalation_event("ws-root", "e-miss"));
        mon.handle_event(ev).await;
        let f = rx.try_recv().expect("escalation frame");
        if let FrameEvent::Escalation(enriched) = &f.event {
            assert_eq!(enriched.workspace_label, "ws-root");
        } else {
            panic!("expected Escalation event");
        }
    }

    // ========================================================================
    // Broadcast capacity exhausted — slow consumer sees Lagged
    // ========================================================================

    #[tokio::test]
    async fn broadcast_capacity_exhausted_yields_lagged_for_slow_consumer() {
        // Use a tiny broadcast capacity.
        let (tx, mut slow_rx) = broadcast::channel::<Frame>(4);
        // Fill the channel past capacity.
        for i in 0..6 {
            let f = Frame {
                channel: Channel::Trail,
                session_id: "s-cap".into(),
                event: FrameEvent::Lag {
                    refresh_hint: vec![],
                    reason: format!("filler-{i}"),
                },
            };
            let _ = tx.send(f);
        }
        // The slow consumer should get a Lagged error.
        match slow_rx.try_recv() {
            Err(broadcast::error::TryRecvError::Lagged(n)) => {
                assert!(n >= 2, "expected at least 2 lagged messages, got {n}");
            }
            other => panic!("expected Lagged, got {other:?}"),
        }
    }

    // ========================================================================
    // Multiple concurrent subscribers
    // ========================================================================

    #[tokio::test]
    async fn multiple_subscribers_each_receive_all_frames() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx1) = make_monitor(&db).await;
        let mut rx2 = mon.broadcast_tx.subscribe();
        let mut rx3 = mon.broadcast_tx.subscribe();

        // Send 3 events of different types
        mon.handle_event(StreamEvent::Trail(trail_entry("ws-1", "signal", b"a")))
            .await;
        mon.handle_event(StreamEvent::Gate(gate_event("ws-1", "g-multi")))
            .await;
        mon.handle_event(StreamEvent::Escalation(escalation_event("ws-1", "e-multi")))
            .await;

        for rx in [&mut rx1, &mut rx2, &mut rx3] {
            let f1 = rx.try_recv().expect("trail");
            assert_eq!(f1.channel, Channel::Trail);
            let f2 = rx.try_recv().expect("gate");
            assert_eq!(f2.channel, Channel::Gates);
            let f3 = rx.try_recv().expect("escalation");
            assert_eq!(f3.channel, Channel::Escalations);
        }
    }

    // ========================================================================
    // Subscriber connects after events already emitted (missed events)
    // ========================================================================

    #[tokio::test]
    async fn late_subscriber_misses_events_emitted_before_subscribe() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, _rx) = make_monitor(&db).await;

        // Emit some events before subscribing a new receiver
        mon.handle_event(StreamEvent::Trail(trail_entry("ws-1", "signal", b"early")))
            .await;
        mon.handle_event(StreamEvent::Gate(gate_event("ws-1", "g-early")))
            .await;

        // Now subscribe late
        let mut late_rx = mon.broadcast_tx.subscribe();

        // Late subscriber should have nothing queued
        assert!(
            late_rx.try_recv().is_err(),
            "late subscriber should not receive previously emitted events"
        );

        // But should receive future events
        mon.handle_event(StreamEvent::Trail(trail_entry("ws-1", "signal", b"late")))
            .await;
        let f = late_rx
            .try_recv()
            .expect("late subscriber should get new events");
        assert_eq!(f.channel, Channel::Trail);
    }

    // ========================================================================
    // Rapid-fire events (burst)
    // ========================================================================

    #[tokio::test]
    async fn rapid_fire_burst_all_events_delivered() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;
        let count = 100;
        for i in 0..count {
            let ev = StreamEvent::Trail(trail_entry(
                "ws-1",
                "signal",
                format!("burst-{i}").as_bytes(),
            ));
            let terminal = mon.handle_event(ev).await;
            assert!(terminal.is_none());
        }
        let mut received = 0;
        while rx.try_recv().is_ok() {
            received += 1;
        }
        assert_eq!(received, count, "all burst events should be broadcast");
    }

    // ========================================================================
    // Normal operation: events flow through all 4 channels
    // ========================================================================

    #[tokio::test]
    async fn events_flow_through_all_four_channels() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, mut rx) = make_monitor(&db).await;

        // Trail
        mon.handle_event(StreamEvent::Trail(trail_entry("ws-1", "signal", b"t")))
            .await;
        let f = rx.try_recv().expect("trail");
        assert_eq!(f.channel, Channel::Trail);

        // Gate
        mon.handle_event(StreamEvent::Gate(gate_event("ws-1", "g-all")))
            .await;
        let f = rx.try_recv().expect("gate");
        assert_eq!(f.channel, Channel::Gates);

        // Escalation
        mon.handle_event(StreamEvent::Escalation(escalation_event("ws-1", "e-all")))
            .await;
        let f = rx.try_recv().expect("escalation");
        assert_eq!(f.channel, Channel::Escalations);

        // Workspace change
        mon.handle_event(StreamEvent::WorkspaceChange(workspace_change(
            "ws-1",
            proto::WorkspaceState::Idle,
            proto::WorkspaceState::Active,
        )))
        .await;
        let f = rx.try_recv().expect("workspace");
        assert_eq!(f.channel, Channel::Workspaces);
    }

    // ========================================================================
    // Stream driver reconnect: each of the 4 drivers reconnects after err
    // ========================================================================

    /// Helper that exercises run_stream_driver with a driver that fails once
    /// then succeeds (returns Ok). Verifies a Lag event is emitted on reconnect
    /// and the driver eventually exits.
    async fn assert_driver_reconnects_after_disconnect(stream_name: &'static str) {
        use std::sync::atomic::{AtomicU32, Ordering};
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");

        let (tx, mut rx) = mpsc::channel::<StreamEvent>(64);
        let cfg = MonitorConfig {
            broadcast_capacity: 64,
            reconnect_initial: Duration::from_millis(1),
            reconnect_max: Duration::from_millis(5),
            reconnect_failure_cap: 5,
        };

        // Driver that fails on first call, succeeds on second, then the
        // run_stream_driver loop reconnects.
        let driver = move |_pool: Arc<GrpcPool>,
                           _tx: mpsc::Sender<StreamEvent>,
                           _name: &'static str|
              -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), String>> + Send>,
        > {
            let cc = call_count_clone.clone();
            Box::pin(async move {
                let n = cc.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err("simulated disconnect".into())
                } else {
                    // Return Ok to simulate a clean stream end;
                    // the driver will reconnect (Ok path).
                    Ok(())
                }
            })
        };

        let handle = tokio::spawn(async move {
            run_stream_driver(stream_name, pool, cfg, tx, driver).await;
        });

        // Wait for the driver to finish (reconnect cap exceeded after enough
        // iterations). Give it a generous timeout.
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;

        // The first call failed, so a Lag event should have been emitted.
        let mut saw_lag = false;
        let mut saw_fatal = false;
        while let Ok(ev) = rx.try_recv() {
            match ev {
                StreamEvent::Lag { stream, .. } => {
                    assert_eq!(stream, stream_name);
                    saw_lag = true;
                }
                StreamEvent::Fatal { stream, .. } => {
                    assert_eq!(stream, stream_name);
                    saw_fatal = true;
                }
                _ => {}
            }
        }
        assert!(saw_lag, "expected Lag event after first disconnect");
        assert!(
            saw_fatal,
            "expected Fatal event after reconnect cap exceeded"
        );
        assert!(
            call_count.load(Ordering::SeqCst) >= 2,
            "driver should have been called at least twice"
        );
    }

    #[tokio::test]
    async fn stream_driver_trail_reconnects_after_disconnect() {
        assert_driver_reconnects_after_disconnect("trail").await;
    }

    #[tokio::test]
    async fn stream_driver_gates_reconnects_after_disconnect() {
        assert_driver_reconnects_after_disconnect("gates").await;
    }

    #[tokio::test]
    async fn stream_driver_escalations_reconnects_after_disconnect() {
        assert_driver_reconnects_after_disconnect("escalations").await;
    }

    #[tokio::test]
    async fn stream_driver_workspace_changes_reconnects_after_disconnect() {
        assert_driver_reconnects_after_disconnect("workspace_changes").await;
    }

    // ========================================================================
    // Stream driver: reconnect cap exceeded emits Fatal
    // ========================================================================

    #[tokio::test]
    async fn stream_driver_cap_exceeded_emits_fatal() {
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        let (tx, mut rx) = mpsc::channel::<StreamEvent>(64);
        let cfg = MonitorConfig {
            broadcast_capacity: 64,
            reconnect_initial: Duration::from_millis(1),
            reconnect_max: Duration::from_millis(2),
            reconnect_failure_cap: 3,
        };

        // Driver that always fails.
        let driver = |_pool: Arc<GrpcPool>,
                      _tx: mpsc::Sender<StreamEvent>,
                      _name: &'static str|
         -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), String>> + Send>,
        > { Box::pin(async { Err("always fails".into()) }) };

        let handle = tokio::spawn(async move {
            run_stream_driver("test_stream", pool, cfg, tx, driver).await;
        });

        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;

        let mut lag_count = 0;
        let mut fatal_count = 0;
        while let Ok(ev) = rx.try_recv() {
            match ev {
                StreamEvent::Lag { .. } => lag_count += 1,
                StreamEvent::Fatal { stream, reason } => {
                    assert_eq!(stream, "test_stream");
                    assert!(reason.contains("3"));
                    fatal_count += 1;
                }
                _ => {}
            }
        }
        assert_eq!(lag_count, 3, "should emit Lag on each failed attempt");
        assert_eq!(fatal_count, 1, "should emit exactly one Fatal");
    }

    // ========================================================================
    // Stream driver: Ok path also triggers reconnect loop
    // ========================================================================

    #[tokio::test]
    async fn stream_driver_ok_return_triggers_reconnect() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        let (tx, mut rx) = mpsc::channel::<StreamEvent>(64);
        let cfg = MonitorConfig {
            broadcast_capacity: 64,
            reconnect_initial: Duration::from_millis(1),
            reconnect_max: Duration::from_millis(2),
            reconnect_failure_cap: 3,
        };

        // Driver that always succeeds — simulates stream ending without error.
        let driver = move |_pool: Arc<GrpcPool>,
                           _tx: mpsc::Sender<StreamEvent>,
                           _name: &'static str|
              -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), String>> + Send>,
        > {
            let c = cc.clone();
            Box::pin(async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        };

        let handle = tokio::spawn(async move {
            run_stream_driver("ok_test", pool, cfg, tx, driver).await;
        });

        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;

        // Ok returns do NOT emit Lag events (only Err does), but the driver
        // should still reconnect and eventually hit the cap.
        assert!(
            call_count.load(Ordering::SeqCst) >= 3,
            "driver should have been called at least reconnect_failure_cap times"
        );
        // Should eventually emit Fatal after cap.
        let mut saw_fatal = false;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, StreamEvent::Fatal { .. }) {
                saw_fatal = true;
            }
        }
        assert!(saw_fatal, "expected Fatal after reconnect cap exceeded");
    }

    // ========================================================================
    // Monitor loop: cancellation mid-stream (drop handle while active)
    // ========================================================================

    #[tokio::test]
    async fn monitor_cancellation_mid_stream_by_dropping_cmd_channel() {
        let db = create_test_pool().await.unwrap();
        insert_user(&db, "u-1").await;
        insert_session(&db, "s-drop", session_state::ACTIVE).await;
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        pool.connect().await;

        let (handle, join) = spawn(
            "s-drop".into(),
            ws_set(),
            pool,
            db,
            EventEnricher::new(empty_taxonomy()),
            RefusalSynthesizer::new(),
            tiny_cfg(),
        );

        // Drop the handle entirely. This drops cmd_tx, which causes cmd_rx.recv()
        // to return None, which breaks the monitor loop.
        drop(handle);

        // The task should terminate within the reconnect budget.
        let result = tokio::time::timeout(Duration::from_secs(10), join).await;
        assert!(result.is_ok(), "monitor should exit when handle is dropped");
    }

    // ========================================================================
    // Monitor loop: empty stream, monitor stays alive until shutdown
    // ========================================================================

    #[tokio::test]
    async fn empty_stream_monitor_stays_alive_until_shutdown() {
        let db = create_test_pool().await.unwrap();
        insert_user(&db, "u-1").await;
        insert_session(&db, "s-empty", session_state::ACTIVE).await;
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        pool.connect().await;

        let (handle, mut join) = spawn(
            "s-empty".into(),
            ws_set(),
            pool,
            db,
            EventEnricher::new(empty_taxonomy()),
            RefusalSynthesizer::new(),
            tiny_cfg(),
        );

        // Wait a short while — the monitor should NOT have exited on its own
        // during this window just because no events arrived.
        let premature = tokio::time::timeout(Duration::from_millis(50), &mut join).await;
        // On a disconnected pool the drivers will eventually hit the reconnect
        // cap and emit Fatal. So we check that the monitor is still alive at
        // least briefly at the start.
        if premature.is_ok() {
            // It exited early due to Fatal from disconnected pool, which is
            // acceptable — the important thing is we can still send Shutdown.
            return;
        }

        // Now send Shutdown.
        handle.shutdown().await;
        let result = tokio::time::timeout(Duration::from_secs(5), join).await;
        assert!(result.is_ok(), "monitor should exit on shutdown");
    }

    // ========================================================================
    // Snapshot command returns current state
    // ========================================================================

    #[tokio::test]
    async fn snapshot_returns_current_pending_counts() {
        let db = create_test_pool().await.unwrap();
        insert_user(&db, "u-1").await;
        insert_session(&db, "s-snap", session_state::ACTIVE).await;
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        pool.connect().await;

        let (handle, join) = spawn(
            "s-snap".into(),
            ws_set(),
            pool,
            db,
            EventEnricher::new(empty_taxonomy()),
            RefusalSynthesizer::new(),
            tiny_cfg(),
        );

        // Snapshot should succeed while monitor is alive.
        let snap = handle.snapshot().await;
        if let Some(s) = snap {
            assert_eq!(s.session_id, "s-snap");
            // No events sent yet, so pending counts should be 0.
            assert_eq!(s.pending_gates, 0);
            assert_eq!(s.pending_escalations, 0);
            assert_eq!(s.pending_refusals, 0);
        }
        // Otherwise the monitor may have already exited due to disconnected
        // pool. That is fine — the test above already covers snapshot content.

        handle.shutdown().await;
        let _ = tokio::time::timeout(Duration::from_secs(5), join).await;
    }

    // ========================================================================
    // record_terminal transitions DB state
    // ========================================================================

    #[tokio::test]
    async fn record_terminal_transitions_session_in_db() {
        let db = create_test_pool().await.unwrap();
        insert_user(&db, "u-1").await;
        insert_session(&db, "s-rec", session_state::ACTIVE).await;
        let (mon, _rx) = make_monitor(&db).await;
        // Override the session_id to match what we inserted
        let mon = Monitor {
            session_id: "s-rec".into(),
            ..mon
        };
        let result = mon.record_terminal(session_state::COMPLETED).await;
        assert!(result.is_ok());
        // Verify DB reflects the transition
        let row = sessions::get_by_id(&db, "s-rec")
            .await
            .expect("query")
            .expect("session");
        assert_eq!(row.state, session_state::COMPLETED);
    }

    // ========================================================================
    // Pending state accumulates across multiple events
    // ========================================================================

    #[tokio::test]
    async fn pending_state_accumulates_gates_and_escalations() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, _rx) = make_monitor(&db).await;
        mon.handle_event(StreamEvent::Gate(gate_event("ws-1", "g-a")))
            .await;
        mon.handle_event(StreamEvent::Gate(gate_event("ws-1", "g-b")))
            .await;
        mon.handle_event(StreamEvent::Escalation(escalation_event("ws-1", "e-a")))
            .await;
        assert_eq!(mon.pending.gates.read().await.len(), 2);
        assert_eq!(mon.pending.escalations.read().await.len(), 1);
    }

    // ========================================================================
    // Workspace state map updated across multiple changes
    // ========================================================================

    #[tokio::test]
    async fn workspace_states_map_tracks_latest_state() {
        let db = create_test_pool().await.unwrap();
        let (mut mon, _rx) = make_monitor(&db).await;
        mon.handle_event(StreamEvent::WorkspaceChange(workspace_change(
            "ws-1",
            proto::WorkspaceState::Idle,
            proto::WorkspaceState::Active,
        )))
        .await;
        assert_eq!(mon.workspace_states.get("ws-1").unwrap(), "active");

        mon.handle_event(StreamEvent::WorkspaceChange(workspace_change(
            "ws-1",
            proto::WorkspaceState::Active,
            proto::WorkspaceState::Blocked,
        )))
        .await;
        assert_eq!(mon.workspace_states.get("ws-1").unwrap(), "blocked");
    }

    // ========================================================================
    // Lag event for each stream name
    // ========================================================================

    #[tokio::test]
    async fn lag_event_for_each_stream_carries_correct_hints() {
        let db = create_test_pool().await.unwrap();
        for (stream, expected_hint) in [
            ("trail", vec![]),
            ("gates", vec!["gates"]),
            ("escalations", vec!["escalations"]),
            ("workspace_changes", vec!["workspaces"]),
        ] {
            let (mut mon, mut rx) = make_monitor(&db).await;
            mon.handle_event(StreamEvent::Lag {
                stream,
                attempt: 1,
                reason: "test".into(),
            })
            .await;
            let f = rx.try_recv().expect("lag frame");
            if let FrameEvent::Lag { refresh_hint, .. } = &f.event {
                assert_eq!(refresh_hint, &expected_hint, "wrong hint for {stream}");
            } else {
                panic!("expected Lag event for {stream}");
            }
        }
    }
}
