//! Standalone mock-runtime binary for Playwright E2E webServer use.
//!
//! Wraps `console_test_support::MockRuntime` with deterministic ports so
//! browser-based tests can connect to known addresses. Ports come from env
//! vars (or the listed defaults) — see `§13.7.7` in `AUDIT-2026-04-15.md`.
//!
//! Env vars (all optional; defaults target `[::1]` loopback):
//!   WACP_MOCK_AGENT_ADDR        — default `[::1]:9190`
//!   WACP_MOCK_HIGHWAY_ADDR      — default `[::1]:9191`
//!   WACP_MOCK_COORDINATOR_ADDR  — default `[::1]:9192`
//!   WACP_MOCK_REST_ADDR         — default `[::1]:9193`
//!
//! Exits with code 1 if any address fails to parse or bind. Blocks on
//! SIGTERM / SIGINT; the inner service tasks are aborted on drop.

use std::net::SocketAddr;
use std::process::ExitCode;

use console_test_support::{MockAddrs, MockRuntime};

const DEFAULT_AGENT: &str = "[::1]:9190";
const DEFAULT_HIGHWAY: &str = "[::1]:9191";
const DEFAULT_COORDINATOR: &str = "[::1]:9192";
const DEFAULT_REST: &str = "[::1]:9193";

fn env_addr(var: &str, default: &str) -> Result<SocketAddr, String> {
    let raw = std::env::var(var).unwrap_or_else(|_| default.to_string());
    raw.parse::<SocketAddr>()
        .map_err(|e| format!("{var}='{raw}' is not a valid SocketAddr: {e}"))
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let addrs = match (
        env_addr("WACP_MOCK_AGENT_ADDR", DEFAULT_AGENT),
        env_addr("WACP_MOCK_HIGHWAY_ADDR", DEFAULT_HIGHWAY),
        env_addr("WACP_MOCK_COORDINATOR_ADDR", DEFAULT_COORDINATOR),
        env_addr("WACP_MOCK_REST_ADDR", DEFAULT_REST),
    ) {
        (Ok(a), Ok(h), Ok(c), Ok(r)) => MockAddrs {
            agent: Some(a),
            highway: Some(h),
            coordinator: Some(c),
            rest: Some(r),
        },
        results => {
            for r in [results.0, results.1, results.2, results.3] {
                if let Err(msg) = r {
                    eprintln!("wacp-mock-runtime: {msg}");
                }
            }
            return ExitCode::from(1);
        }
    };

    let rt = match MockRuntime::start_on_addrs(addrs, None).await {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("wacp-mock-runtime: failed to start: {e}");
            return ExitCode::from(1);
        }
    };

    // Print bound addresses on stdout so Playwright/webServer invocations can
    // parse them if needed. The fixed-port case makes this redundant but the
    // output is also a useful log line for debugging.
    println!("agent_addr={}", rt.agent_addr);
    println!("highway_addr={}", rt.highway_addr);
    println!("coordinator_addr={}", rt.coordinator_addr);
    println!("rest_addr={}", rt.rest_addr);
    println!("ready");

    // Block until SIGINT/SIGTERM. The MockRuntime's service tasks run on
    // tokio::spawn handles held in `_handles`; they shut down when this
    // runtime is dropped at process exit.
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    let sigterm = async {
        let mut s =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        s.recv().await;
    };
    #[cfg(unix)]
    tokio::select! {
        _ = ctrl_c => {}
        _ = sigterm => {}
    }
    #[cfg(not(unix))]
    let _ = ctrl_c.await;

    drop(rt);
    ExitCode::SUCCESS
}
