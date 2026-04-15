//! Spawn `wacp-runtime` as a child process for end-to-end tests.
//!
//! The harness binds random ephemeral ports (chosen by querying the OS
//! before spawn), passes them to the runtime via `WACP_SERVER__*_LISTEN`
//! env overrides + a unique `WACP_STORAGE__DATA_DIR` per harness, and
//! waits for the runtime's `/healthz` endpoint to flip to `"ready"`.
//!
//! The `Drop` impl sends SIGKILL to the child if it's still running so
//! a panic'ing test can't leak runtime processes.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeHarnessError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("port allocation: {0}")]
    PortAlloc(String),
    #[error("runtime did not become ready within {0:?}")]
    NotReady(Duration),
    #[error("/healthz check failed: {0}")]
    Health(#[from] reqwest::Error),
}

pub struct RuntimeHarness {
    child: Option<Child>,
    pub agent_port: u16,
    pub highway_port: u16,
    pub coordinator_port: u16,
    pub rest_port: u16,
    pub health_port: u16,
    pub metrics_port: u16,
    /// Path to the captured runtime stderr log. Surfaced on spawn
    /// failure so diagnostics show up in `cargo test` output.
    pub stderr_path: std::path::PathBuf,
    /// Held to keep the data dir alive for the harness's lifetime.
    _data_dir: TempDir,
}

impl RuntimeHarness {
    /// Spawn `wacp-runtime` and wait up to `ready_timeout` for it to come
    /// up. Returns once `/healthz` returns ready, otherwise an error.
    ///
    /// `bin_path` should be the absolute path to the runtime binary —
    /// integration tests pass `env!("CARGO_BIN_EXE_wacp-runtime")`, which
    /// Cargo populates only inside the test binary's compilation unit
    /// (not at library build time). The `WACP_RUNTIME_BIN` env var
    /// overrides for callers running outside Cargo's test wrapper.
    pub async fn spawn_with_timeout(
        bin_path: &str,
        ready_timeout: Duration,
    ) -> Result<Self, RuntimeHarnessError> {
        let agent_port = pick_port()?;
        let highway_port = pick_port()?;
        let coordinator_port = pick_port()?;
        let rest_port = pick_port()?;
        let health_port = pick_port()?;
        let metrics_port = pick_port()?;

        let data_dir = TempDir::new()?;
        let bin_path = std::env::var("WACP_RUNTIME_BIN").unwrap_or_else(|_| bin_path.to_string());

        // Assemble env-var overrides. The runtime supports `WACP_…`-prefixed
        // doubles-underscore-separated overrides on every config field
        // (`apply_env_overrides` in wacp-runtime/src/config.rs).
        let mut cmd = Command::new(Path::new(&bin_path));
        cmd.arg("serve");
        cmd.envs([
            ("WACP_SERVER__AGENT_LISTEN", &fmt_addr(agent_port)),
            ("WACP_SERVER__HIGHWAY_LISTEN", &fmt_addr(highway_port)),
            (
                "WACP_SERVER__COORDINATOR_LISTEN",
                &fmt_addr(coordinator_port),
            ),
            ("WACP_SERVER__REST_LISTEN", &fmt_addr(rest_port)),
            ("WACP_OBSERVABILITY__HEALTH__LISTEN", &fmt_addr(health_port)),
            (
                "WACP_OBSERVABILITY__METRICS__LISTEN",
                &fmt_addr(metrics_port),
            ),
            (
                "WACP_STORAGE__DATA_DIR",
                &data_dir.path().to_string_lossy().into_owned(),
            ),
            // Quiet the runtime's logs unless caller overrides — the
            // harness already captures stderr. Format must be "json" or
            // "pretty"; leave it at default.
            ("WACP_LOGGING__LEVEL", &"warn".to_string()),
        ]);
        // Capture stderr so wait_ready failures can be diagnosed; route
        // it to a file inside the data dir.
        let stderr_path = data_dir.path().join("runtime.stderr.log");
        let stderr_file = std::fs::File::create(&stderr_path)?;
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr_file));

        let child = cmd.spawn()?;
        let harness = RuntimeHarness {
            child: Some(child),
            agent_port,
            highway_port,
            coordinator_port,
            rest_port,
            health_port,
            metrics_port,
            stderr_path,
            _data_dir: data_dir,
        };
        if let Err(e) = harness.wait_ready(ready_timeout).await {
            let log = std::fs::read_to_string(&harness.stderr_path).unwrap_or_default();
            eprintln!("--- runtime stderr ---\n{log}\n--- end runtime stderr ---");
            return Err(e);
        }
        Ok(harness)
    }

    /// Convenience — spawn with a 30 s ready timeout.
    pub async fn spawn(bin_path: &str) -> Result<Self, RuntimeHarnessError> {
        Self::spawn_with_timeout(bin_path, Duration::from_secs(30)).await
    }

    /// Spawn using `discover_bin()` to locate the runtime binary.
    /// Use this from integration tests — `CARGO_BIN_EXE_wacp-runtime`
    /// is unavailable because wacp-runtime exposes no lib target and
    /// can't be declared as a dev-dep on this crate.
    pub async fn spawn_default() -> Result<Self, RuntimeHarnessError> {
        let bin = Self::discover_bin();
        Self::spawn(&bin.to_string_lossy()).await
    }

    /// Resolve the wacp-runtime binary path from the conventional Cargo
    /// layout: `<workspace target>/debug/wacp-runtime` (or `release/`).
    /// `CARGO_BIN_EXE_*` is only set for same-package bins; this helper
    /// works for any workspace member that depends on wacp-runtime as a
    /// dev-dep (Cargo builds the bin alongside the test).
    pub fn discover_bin() -> std::path::PathBuf {
        if let Ok(p) = std::env::var("WACP_RUNTIME_BIN") {
            return std::path::PathBuf::from(p);
        }
        // CARGO_MANIFEST_DIR points at the integration crate's directory.
        // Walk up until we find `target/debug/wacp-runtime`, falling back
        // to `target/release/wacp-runtime`.
        let manifest = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR is set during cargo test");
        let mut cur = std::path::PathBuf::from(manifest);
        for _ in 0..6 {
            for profile in ["debug", "release"] {
                let candidate = cur.join("target").join(profile).join("wacp-runtime");
                if candidate.exists() {
                    return candidate;
                }
            }
            if !cur.pop() {
                break;
            }
        }
        // Last resort — return the canonical workspace path; if the
        // binary really isn't there the spawn will fail with a clear
        // io::Error pointing at the path.
        cur.join("target").join("debug").join("wacp-runtime")
    }

    /// Poll the runtime's `/healthz` until it returns 200 with body
    /// `"ready"`, or `timeout` elapses.
    pub async fn wait_ready(&self, timeout: Duration) -> Result<(), RuntimeHarnessError> {
        let url = format!("http://[::1]:{}/healthz", self.health_port);
        let client = reqwest::Client::new();
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(resp) = client.get(&url).send().await
                && resp.status().is_success()
                && resp
                    .text()
                    .await
                    .map(|t| t.contains("ready"))
                    .unwrap_or(false)
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(RuntimeHarnessError::NotReady(timeout));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub fn agent_addr(&self) -> String {
        fmt_addr(self.agent_port)
    }
    pub fn highway_addr(&self) -> String {
        fmt_addr(self.highway_port)
    }
    pub fn coordinator_addr(&self) -> String {
        fmt_addr(self.coordinator_port)
    }
    pub fn rest_addr(&self) -> String {
        fmt_addr(self.rest_port)
    }
    pub fn rest_url(&self) -> String {
        format!("http://[::1]:{}", self.rest_port)
    }

    /// Send SIGKILL (the only stop signal `Child::kill` exposes
    /// portably). Used by tests that explicitly assert on runtime-down
    /// behaviour (T7.4).
    pub fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for RuntimeHarness {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Bind a random ephemeral port, then close — we hand the number to the
/// runtime via env-var. There's a small TOCTOU window where two
/// harnesses race on the same port; mitigated by the ports staying
/// "in use" via TIME_WAIT briefly after close, plus the random spread.
fn pick_port() -> Result<u16, RuntimeHarnessError> {
    let listener = std::net::TcpListener::bind("[::1]:0")
        .map_err(|e| RuntimeHarnessError::PortAlloc(e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| RuntimeHarnessError::PortAlloc(e.to_string()))?
        .port();
    Ok(port)
}

fn fmt_addr(port: u16) -> String {
    format!("[::1]:{port}")
}
