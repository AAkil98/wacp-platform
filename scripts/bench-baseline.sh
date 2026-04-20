#!/usr/bin/env bash
# Run the backend perf-baseline bench suite end-to-end and emit a summary.
#
# HEALTH-LOG §8 / backend-perf-baseline-plan C1. Baseline target:
#   - session_monitor broadcast: p99 < 1 ms at 16 subs / 1000 frames
#   - argon2_verify: p99 < 100 ms
#   - csrf_compare: < 100 µs
#   - stub serialize_for_match: documented; used as C5 before/after anchor
#
# Not in CI — bench wallclocks are too noisy for CI-grade alerting without
# dedicated hardware. Run locally; commit the resulting summary doc as the
# numerical baseline for post-v0.1.0 regression checks.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# Run each bench in its owning crate. `--bench` filters to the named bench;
# criterion emits HTML + plain-text to target/criterion/.
echo "=== session_monitor_bench ==="
cargo bench -p console-core --bench session_monitor_bench

echo "=== session_launcher_bench ==="
cargo bench -p console-core --bench session_launcher_bench

echo "=== middleware_bench ==="
cargo bench -p console-api --bench middleware_bench

echo "=== stub_bench ==="
cargo bench -p wacp-llm --bench stub_bench

echo
echo "HTML reports at: target/criterion/*/report/index.html"
echo "See docs/perf-baseline-2026-04-20.md for the human summary."
