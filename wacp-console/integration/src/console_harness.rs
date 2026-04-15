//! In-process console harness — builds an `AppState` wired to a running
//! `RuntimeHarness`, mounts the API router via `console_api::api_router`,
//! and serves on a random ephemeral port.
//!
//! Per W7 spec §3.2 we default to in-process for tighter observability
//! (DB pool reachable from the test, monitor handles inspectable). A
//! child-process console variant is intentionally out-of-scope.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use arc_swap::ArcSwap;
use console_api::{AppState, api_router};
use console_core::config::RuntimeConfig as ConsoleRuntimeConfig;
use console_core::recovery;
use console_core::session_monitor::MonitorConfig;
use console_core::taxonomy_builder;
use console_db::DbPool;
use console_runtime::grpc_pool::GrpcPool;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::runtime_harness::RuntimeHarness;

pub struct ConsoleHarness {
    pub addr: SocketAddr,
    pub state: Arc<AppState>,
    handle: Option<JoinHandle<()>>,
}

impl ConsoleHarness {
    /// Build the AppState, run startup recovery, and serve. Returns once
    /// the listener is bound and the serve task is spawned. If
    /// `pre_seeded_db` is provided, that DB is used as-is — for T7.6
    /// (console restart), the test passes the same DB pool back in to
    /// simulate state surviving the restart.
    pub async fn spawn(rt: &RuntimeHarness) -> std::io::Result<Self> {
        let db = console_db::create_test_pool()
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Self::spawn_with_db(rt, db).await
    }

    pub async fn spawn_with_db(rt: &RuntimeHarness, db: DbPool) -> std::io::Result<Self> {
        let pool = GrpcPool::new(&rt.agent_addr(), &rt.highway_addr(), &rt.coordinator_addr());
        pool.connect().await;

        let taxonomy = Arc::new(ArcSwap::from_pointee(
            taxonomy_builder::build_index(None, &[], &[]).index,
        ));
        let active_sessions = Arc::new(tokio::sync::RwLock::new(HashMap::new()));

        // Run startup recovery against the (possibly reused) DB. For a
        // freshly-seeded DB this is a no-op (no ACTIVE rows); for the
        // restart simulation it respawns the W3 monitor for any session
        // still ACTIVE.
        let _report = recovery::run(
            db.clone(),
            pool.clone(),
            console_core::event_enricher::EventEnricher::new(taxonomy.clone()),
            console_core::refusal_synthesizer::RefusalSynthesizer::new(),
            active_sessions.clone(),
            MonitorConfig::default(),
        )
        .await;

        let state = Arc::new(AppState {
            db,
            taxonomy,
            runtime_config: ConsoleRuntimeConfig {
                agent_address: rt.agent_addr(),
                highway_address: rt.highway_addr(),
                coordinator_address: rt.coordinator_addr(),
                rest_address: rt.rest_url(),
            },
            grpc_pool: pool,
            active_sessions,
        });

        let listener = TcpListener::bind("[::1]:0").await?;
        let addr = listener.local_addr()?;
        // api_router takes AppState by value, not Arc<AppState>. Clone
        // the inner contents — Arc fields share state.
        let state_for_router = (*state).clone();
        let app = api_router(state_for_router);
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Ok(ConsoleHarness {
            addr,
            state,
            handle: Some(handle),
        })
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for ConsoleHarness {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}
