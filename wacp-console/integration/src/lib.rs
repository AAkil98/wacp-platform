//! Integration test harness for the WACP console.
//!
//! See `wacp-console/specs/coding/wcon-w7-integration-tests.md` for the
//! full design. The harness provides three composable pieces:
//!
//! - [`RuntimeHarness`] — spawns the real `wacp-runtime` binary in a
//!   tempdir on random ports, blocks until `/healthz` returns ready.
//! - [`ConsoleHarness`] — runs the console *in-process* (an `axum::serve`
//!   on a random port) wired to the runtime harness's gRPC ports.
//! - [`TestClient`] — REST + WebSocket helpers using bearer-token auth
//!   (CSRF-exempt) so tests don't have to wrangle cookies.
//!
//! ## Driving sessions to completion
//!
//! The runtime expects a real LLM provider to make forward progress on
//! coordinator state (decompose, dispatch, integrate). There is no built-in
//! mock LLM — see W7's `5.1 Happy-path lifecycle` deviation note in
//! `impl/wiring-phases.md`. Tests that need a live LLM are marked
//! `#[ignore]` with a `// ignored: needs LLM stub` reason.

pub mod console_harness;
pub mod runtime_harness;
pub mod test_client;

pub use console_harness::ConsoleHarness;
pub use runtime_harness::{RuntimeHarness, RuntimeHarnessError};
pub use test_client::{TestClient, WsClient};
