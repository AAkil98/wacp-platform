# Console Playwright E2E suite

End-to-end coverage of the five user-journey scenarios from `AUDIT-2026-04-15.md` §12.4. Each spec drives a real browser against a real `wacp-console` binary talking to a real `wacp-mock-runtime` sidecar — no mocks at the component layer.

## Scenarios

| File | Scenario | State |
|---|---|---|
| `auth-flows.spec.ts` | Bad password, lockout, forced change, logout, API token create + use | **5/5 unskipped** |
| `golden-path.spec.ts` | Bootstrap → login → discover → profile → launch → approve → complete | **2/3 unskipped** (launch step skipped — deterministic LLM wiring still open) |
| `multi-user.spec.ts` | admin/operator/viewer role-gated UI | **skipped** — pending admin seed helper |
| `cancel.spec.ts` | Cancel from wizard + from dashboard | **skipped** — pending reliable pre-launch fixture |
| `profile-roundtrip.spec.ts` | YAML export → delete → import → verify | **skipped** — pending YAML download-handle helper |

Each skipped spec has an in-file `// UNSKIP WHEN:` note explaining what prereq is missing. Removing `test.skip` is the only change needed once each prereq lands.

## Running locally

From `wacp-console/frontend/`:

```bash
# First-time setup — installs Chromium + system deps
pnpm exec playwright install --with-deps chromium

# Run the suite (spawns both binaries as Playwright webServer blocks)
pnpm test:e2e
```

The `test:e2e` script first runs `scripts/e2e-cleanup.sh` to wipe `.e2e-state/` (per-run SQLite + bootstrap-token dir), then invokes `playwright test`.

The two binaries must already be built:

```bash
cargo build -p wacp-console --bin wacp-console \
            -p console-test-support --bin wacp-mock-runtime
```

The Playwright config resolves both from `target/debug/` relative to the repo root.

## Debugging

```bash
pnpm test:e2e:ui     # Playwright UI — step through, time-travel, inspect DOM
pnpm test:e2e:debug  # Single-test stepper via Playwright Inspector
```

On CI failures, two artifacts are uploaded (retention 14d):

- **`playwright-report`** — HTML report (screenshots, logs, config). Always uploaded.
- **`playwright-traces`** — trace.zip per failed test (load via `npx playwright show-trace <path>`) plus attached videos. Only uploaded on failure.

### Fast local iteration

```bash
PLAYWRIGHT_REUSE_STATE=1 pnpm test:e2e
```

Keeps `.e2e-state/` and the running webServers between runs — ~10× faster. The binaries must still be pre-built (or rebuild manually when you change Rust). Use for iterating on a single spec; wipe state before committing.

## Port map

| Service | Address | Exposed by |
|---|---|---|
| Mock runtime — AgentService (gRPC) | `[::1]:9190` | `wacp-mock-runtime` |
| Mock runtime — HighwayService (gRPC) | `[::1]:9191` | `wacp-mock-runtime` |
| Mock runtime — CoordinatorService (gRPC) | `[::1]:9192` | `wacp-mock-runtime` |
| Mock runtime — REST gateway | `[::1]:9193` | `wacp-mock-runtime` |
| Console — REST + WebSocket + SPA | `[::1]:8787` | `wacp-console serve` |

Shifted by 100 from the canonical production ports (9090–9093 + 8080) so an e2e run can't clobber a locally-running dev console.

## Readiness probes

Playwright blocks `test` start until:

- `http://[::1]:9193/v1/verticals` returns 200 (mock runtime is up).
- `http://[::1]:8787/api/health` returns 200 (console is up, SPA assets embedded, DB migrated).

If either probe times out (15s / 30s), check the `stdout`/`stderr` streams Playwright surfaces in the report — usually a port collision from a prior run or a missing binary build.

## Snapshots

The suite is assertion-based, not snapshot-based. No golden PNGs. Adding a screenshot assertion in the future: `await expect(page).toHaveScreenshot("...");` and update on first-run with `pnpm test:e2e -- --update-snapshots`.

## CI

The `e2e` job in `.github/workflows/ci-console.yml` runs the full suite on every push to `main`/`dev` and every PR targeting `main`. Cold-cache runs take ~5–8 min (Rust build + Chromium download + suite); cached reruns ~3–4 min.

Coverage instrumentation (merging Playwright runs into the frontend lcov) is deferred — see `AUDIT-2026-04-15.md` §13.7.7 deliverable 1.
