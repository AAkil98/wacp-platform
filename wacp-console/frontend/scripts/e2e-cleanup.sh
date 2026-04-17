#!/usr/bin/env bash
# Wipe the per-run Playwright state dir so every `pnpm exec playwright test`
# starts against a freshly-bootstrapped console.
#
# Must run BEFORE `playwright test` — cleanup from inside the Playwright
# config module is unsafe because the config module is imported twice (runner
# + worker); the second import wipes the state the webServer just populated.
#
# Opt out with `PLAYWRIGHT_REUSE_STATE=1` for fast iterative local loops.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FRONTEND_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
STATE_DIR="$FRONTEND_DIR/.e2e-state"

if [[ -n "${PLAYWRIGHT_REUSE_STATE:-}" ]]; then
  echo "[e2e-cleanup] PLAYWRIGHT_REUSE_STATE set — skipping"
  exit 0
fi

if [[ -d "$STATE_DIR" ]]; then
  rm -rf "$STATE_DIR"
fi
mkdir -p "$STATE_DIR"
