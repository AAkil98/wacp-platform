# WACP Platform — Seed Context

> Compressed summary of the full design, current implementation state, and next steps for the unified `wacp-platform` monorepo.
> For detail on any topic, follow the spec references. For task-level implementation detail, see `wacp/IMPLEMENTATION.md` (runtime) and `wacp-console/IMPLEMENTATION.md` (console).
> For the wiring strategy (runtime ↔ console integration plan), see `impl/wiring-strategy.md`.
> For the monorepo merge procedure (M0–M7), see `impl/merge-plan.md` and `impl/merge-execution-log.md`.
> For the latest codebase health + testing strategy, see `AUDIT-2026-04-15.md`.

## What This Is

The `wacp-platform` monorepo houses two binaries that ship together:

- **WACP runtime (`wacp/`)** — the protocol reference implementation. Rust workspace (15 crates) + TypeScript CLI/SDK/verticals + Python SDK. Serves gRPC (Agent, Highway, Coordinator) + REST + WebSocket for the 7 verticals.
- **WACP Console (`wacp-console/`)** — the operator workbench. Rust/Axum backend (6 crates) + React 19 SPA. Discovers agent roles, manages agent profiles, launches coordination sessions, and oversees agent work in real time. The Console is a **client** of the runtime — it connects via gRPC and REST, never modifies protocol behavior, never executes LLM calls.

They are shipped as two binaries with gRPC between them; the monorepo exists for development ergonomics (shared proto codegen, version-locked types, unified CI/fmt/clippy).

**Specs:** runtime protocol lives in the sibling `wacp-protocol` repo (CC BY-SA 4.0). Console design specs under `wacp-console/specs/` (12 finalized). Anchor spec for the Console: `wcon-vision`.

## Current State (Post M0–M7 merge + W1–W7 wiring + post-audit workstream)

**Branch:** `dev`, 5 commits ahead of `main` @ `743c9bd`. The dev→main batched merge ran 2026-04-17 morning (fast-forward, 21 commits), then §13.7.6b WA1/WA2/WA3 + strategy-doc update + WA3.5/WA3.6 + deferral docs landed on dev. Working tree clean. CI green on all five workflows (`ci-lint`, `ci-wacp`, `ci-console`, `release-runtime`, `release-console`) plus the new `coverage` workflow.

**Since the 2026-04-15 audit (from `a6773d6`)**, these have landed in order — all 25 through `743c9bd` now on `main`; the last 4 (WA1–WA3 + strategy update) still on `dev`:

| SHA | Subject | Audit anchor |
|-----|---------|--------------|
| `71b44b6` | docs(audit-04-15): land audit, move prior audit, fix Phase 4.6 step names | §11 #4, §11 #5 |
| `60d2245` | ci(deny): add cargo-deny workspace gate | §11 #1 |
| `7ba4ae8` | ci(release): SBOM (CycloneDX) + Trivy scan on published OCI images | §11 #2 |
| `d10ed29` | feat(transport): re-key auth HashMaps by SHA-256 digest, constant-time ws check | §11 #3 |
| `7f0736b` | ci(coverage): land cargo-llvm-cov, vitest v8, coverage.py + Codecov | §12.1 |
| `840450a` | test(coverage): Rust branch-coverage sweep — §12.2 T1–T10 | §12.2 T1–T10 |
| `82a4213` | test(frontend): add isolated per-file runner + tighten V8 heap cap | new §13.6 |
| `d71c4fe` | fix(frontend): a11y label bindings + client double-read | test-adjacent prod fixes |
| `fe48c7b` | test(frontend): F1–F6/F9 RTL suite + ProfilesPage monolith split | §12.3 F1–F6, F9 |
| `a510249` | docs(audit-04-15): append §13 post-audit progress | audit update |
| `5e5733b` | docs(audit-04-15): expand §13.7 — task breakdown with deliverables | §13.7 task packages |
| `b061c71` | docs(seed): refresh post-audit progress + point at §13.7 packages | seed refresh |
| `d63648a` | fix(frontend): ProfilesPage.actions render loop + global RTL cleanup | §13.7.1 |
| `b17ae49` | docs(audit-04-15): close §13.7.1 — actions.test render loop resolved | §13.7.1 |
| `8487f2f` | docs(wacp-console): performance-optimization notes from §13.7.1 work | perf-opt doc seeded |
| `e870018` | test(frontend): F7 Wizard.test.tsx — 41 tests across 6 steps + cross-cutting | §13.7.2 |
| `19e4203` | docs(audit-04-15): close §13.7.2 — F7 Wizard tests landed | §13.7.2 |
| `78512c1` | docs(wacp-console): fold §13.7.2 learnings into performance-optimization notes | perf-opt update |
| `92b3ddb` | test(frontend): F8 rest-of-oversight — 52 tests across 4 surfaces | §13.7.3 |
| `ef7940d` | docs(audit-04-15): close §13.7.3 — F8 oversight tests landed | §13.7.3 |
| `eec5486` | docs(wacp-console): §2.5 — spec-vs-impl drift lesson from F8 | perf-opt update |
| `543c295` | test(frontend): F10 Notifications.test.tsx — 16 tests, drift documented | §13.7.4 |
| `c519201` | docs: close §13.7.4 + fold F10 Notifications stub finding into §2.5 | §13.7.4 |
| `2fdf191` | test(console-db): §13.7.5 — fault-injection harness + 83 coverage tests | §13.7.5 / §12.2 T11 |
| `f75a2a7` | docs(wacp-console): perf-opt §9 — backend drifts surfaced by §13.7.5 | perf-opt update |
| `d8ca8ff` | docs(audit-04-15): close §13.7.5 — console-db T11 landed | §13.7.5 / §12.2 T11 |
| `abfbb99` | feat(wacp-llm): §13.7.6 — stub LlmAdapter + fixture + build_adapter factory | §13.7.6 |
| `afe98f6` | test(console-integration): §13.7.6 — I6 llm_stub_e2e + tighten T7.* ignore reasons | §13.7.6 / §12.5 I6 |
| `140fcc2` | docs(wacp-console): §13.7.6 — wcon-llm-stub spec + W7 deviation note + perf-opt §10 | §13.7.6 |
| `2f7b7ae` | docs(impl): §13.7.6b — wiring-strategy-b for runtime agent-service wiring | §13.7.6b |
| `743c9bd` | docs(audit-04-15): close §13.7.6 partial; carve out §13.7.6b; seed refresh | §13.7.6 |
| `b01757c` | feat(wacp-runtime): §13.7.6b WA1 — Bind projects WorkspaceConfig | §13.7.6b / WA1 |
| `69bcde0` | feat(wacp-runtime+wacp-sdk): §13.7.6b WA2 — EmitSignal drives workspace FSM | §13.7.6b / WA2 |
| `822674c` | feat(wacp-runtime): §13.7.6b WA3 — CreateCheckpoint forwards to workspace actor | §13.7.6b / WA3 |
| `7782d78` | docs(impl): wiring-strategy-b — carve WA3.5 + WA3.6; revise total effort | §13.7.6b / strategy |
| (this commit) | feat(§13.7.6b): WA3.5 checkpoint-approval gates + WA3.6 auto-integration; defer WA5 + un-ignore sweep | §13.7.6b / WA3.5 / WA3.6 |

What this delivered, in English: supply-chain scanning (cargo-deny, SBOM, Trivy) is in CI; runtime auth is constant-time via SHA-256 digest rekey; the full coverage-tooling stack (cargo-llvm-cov, Vitest v8, coverage.py, Codecov with per-component flags) is wired; Rust branch-coverage tests landed for T1–T11 (~11,900 lines; T11 `console-db` brought that crate from 55.6 % → 98.3 % region coverage via a new `src/testing.rs` fault-injection harness and 83 tests); frontend RTL tests landed for F1–F10 save F9 which was already green (~5,200 + ~2,100 additional lines for F7/F8/F10); a per-file isolated vitest runner plus a 1536 MB V8 heap cap keeps the now-much-larger frontend suite from crashing WSL; and `wacp-console/performance-optimization.md` aggregates the frontend-side `useEffect`-dep + spec-vs-impl drifts (§2.5) and the backend-side schema-vs-struct drifts (§9) that each session surfaces.

**Runtime (`wacp/`).** 15 Rust crates, ~1,280 Rust tests + TS matrix (10 packages + 7 verticals, ~1,000 tests) + Python SDK (104 tests across 3.11–3.13). All 35 gRPC RPCs fully wired across `AgentService`, `HighwayService`, `CoordinatorService`. REST gateway exposes 16 `/v1/*` endpoints + `/v1/ws`. OpenAPI drift-checked in CI. Stream A (A1–A9) closed all 8 Console-facing integration gaps; the 17 runtime-side stub/placeholder gaps identified in the subsequent implementation audit are all resolved. Port map canonicalized to `9090/9091/9092/9093/9094/9095`.

**Console (`wacp-console/`).** 6 Rust crates, 66+ REST endpoints, 99+ backend unit tests. React 19 + Vite + TanStack Query + Zustand frontend (37 TS files, 9,367 lines). Now fully wired to the real runtime after W1–W7:

- **W1** gRPC pool in `AppState`; `/api/health` queries live pool + REST gateway (not mocks).
- **W2** Launch flow: real `CoordinatorService` sequence (`SubmitGoal` → `Decompose` → `Dispatch×N` → envelope send), rollback via `AbortWorkspace` on partial failure.
- **W3** `SessionMonitor`: one Tokio task per session, bounded `broadcast` fan-out (cap 256), four stream drivers (Trail, Gates, Escalations, WorkspaceChanges).
- **W4** Highway forwarding: gate resolve, escalation respond, directive inject all hit real `HighwayService` gRPC.
- **W5** Cancel calls `AbortWorkspace`; startup recovery scans `state='active'` and respawns monitors.
- **W6** Cross-session pending endpoints (`/api/gates/pending`, `/api/escalations/pending`, `/api/refusals/pending`) aggregate from live monitor state.
- **W7** Integration harness (`integration/tests/`) — `lifecycle`, `cross_session`, `chaos`, and `llm_stub_e2e` all passing on their active tests. Six scenarios remain `#[ignore]`-ed (`T7.2`, `T7.3`, `T7.5`, `T7.7`, `T7.8`, `T7.10`); five are now **structurally** unblocked by WA3.5 (gates) + WA3.6 (auto-integration) — un-ignore is a separate ~10–20 h follow-up that wires Console-level orchestration on top of the runtime-side primitives. T7.5 still needs WA5 (deferred). Ignore reasons updated to reflect the new state.

**Working end-to-end against a live runtime:** discovery (roles, tools, verticals, types, search), profile CRUD with validation/versioning/export/import/clone, multi-user auth (Argon2id, CSRF double-submit, rate limiting, 256-bit bootstrap credential at 0o600), session launch + oversight (trail stream, gate queue, escalation inbox, refusal panel, workspace tree, injection bar across 7 WebSocket channels), startup recovery, cross-session pending aggregation.

**Not yet present (tracked, not regressions):** Playwright E2E suite (Phase 7.6–7.10, audit §13.7.7), the remaining §13.7.6b pieces — **WA5** (harness-side dispatch-failure proxy; blocks T7.5; revised estimate 3–4 h after deeper analysis showed the original 2 h plan needs either a generic GrpcPool refactor or a 13-RPC tonic mock-server proxy), the **un-ignore sweep** for T7.2/T7.3/T7.7/T7.8/T7.10 (~10–20 h total — Console-level integration tests on top of WA3.5/WA3.6 primitives that already pass at the runtime-unit-test layer), the five new Rust integration + chaos suites I1–I5 (§13.7.8; I6 landed via §13.7.6), mutation-testing workflow (§13.7.9), and Codecov monthly ratchet (§13.7.10, deferred until the new baseline settles). All broken out with deliverables in `AUDIT-2026-04-15.md` §13.7 and `impl/wiring-strategy-b.md`. Supply-chain scanning, the F-series frontend sweep, the Rust branch-coverage sweep (T1–T11), the deterministic LLM stub provider + I6 integration test, and §13.7.6b WA1/WA2/WA3/WA3.5/WA3.6 (Bind projection, EmitSignal→FSM, CreateCheckpoint→actor, checkpoint-approval gates, auto-integration on Complete) are all landed (WA3.5/WA3.6 in working tree pending commit).

### Milestone history

All milestones passed evaluation. Reports at `wacp-console/impl/phase-{1..6}-eval.md`, `wacp-console/impl/wiring-eval.md` (per-phase W1–W7), and `impl/merge-execution-log.md` (M0–M7).

| Track | Phases | Outcome |
|---|---|---|
| Console design | 0–4 (backend) + 5–6 (frontend) | 99 backend tests, 66 endpoints, full SPA |
| Runtime Stream A | A1–A9 | Port map, CI matrix, cargo-dist, crates.io metadata, OpenAPI, workspace listing, mock runtime |
| Monorepo merger | M0–M7 | Umbrella workspace, `wacp-proto` extraction, unified CI, hoisted tooling, docker-compose |
| Wiring | W1–W7 | Real gRPC+REST integration, session monitor, cancel + recovery, cross-session pending, integration tests |

## Four Surfaces

| Surface | What it does | Spec |
|---------|-------------|------|
| **Discovery Browser** | Browse taxonomy: roles, tools, verticals, types. Read-only, search, filter, drill-down. | `wcon-discovery`, `wcon-ui` §4 |
| **Profile Studio** | Create/edit/clone/delete agent profiles. YAML import/export. Validates against taxonomy. | `wcon-profiles`, `wcon-ui` §5 |
| **Session Launcher** | 6-step wizard: vertical → workflow → assign profiles → vertical context → budgets → review & launch. | `wcon-sessions` §2, `wcon-ui` §6 |
| **Oversight Dashboard** | Real-time: trail stream, gate queue, escalation inbox, refusal panel, workspace tree, injection bar. 7 WebSocket channels. | `wcon-highway`, `wcon-ui` §7 |

## Architecture

Two tiers: Rust backend + React SPA. Backend owns persistence, taxonomy index, session orchestration, highway bridge. Frontend is a rendering layer consuming REST + WebSocket.

**Spec:** `wcon-architecture`

### Backend Components

| Component | Responsibility |
|-----------|---------------|
| **Taxonomy Index** | In-memory, built from protocol taxonomy YAML + runtime REST (`GET /v1/verticals[/{id}]`). Atomic swap via `ArcSwap`. |
| **Profile Store** | CRUD, versioning (append-only), soft delete, validation, import/export. SQLite. |
| **Session Manager** | Lifecycle from configuring → active → terminal. Translates UI config into gRPC calls. Monitors 4 streams. |
| **Highway Bridge** | Proxies trail/gates/escalations/workspace changes from gRPC to 7 WebSocket channels. Synthesizes `refusals`, `session`, `notification` channels. |
| **Auth Service** | Users, browser sessions, API tokens, login attempts. `LocalAuthenticator` + `RoleAuthorizer`. |
| **Audit Service** | Append-only mutation log. |

### Runtime Connection

Four endpoints, three independent Tonic channels + one REST client:

| Service | Default | Config key |
|---------|---------|------------|
| AgentService (gRPC) | `[::1]:9090` | `runtime.agent_address` |
| HighwayService (gRPC) | `[::1]:9091` | `runtime.highway_address` |
| CoordinatorService (gRPC) | `[::1]:9092` | `runtime.coordinator_address` |
| REST gateway | `http://[::1]:9093` | `runtime.rest_address` |

**NOT multiplexed.** Per-service reconnection and health tracking.

**Spec:** `wcon-architecture` §1, §4, §7; ADR-003

## Data Model

**Storage:** SQLite (`console.db`), WAL mode, single file.

### Tables

| Table | Purpose | Spec |
|-------|---------|------|
| `profiles` | Agent config bundles. Versioned (append-only), soft-delete. PK: `(id, version)`. | `wcon-data-model` §3 |
| `sessions` | Coordination runs. Optional `name`. State machine: configuring → validating → launching → active → completed/failed/cancelled. Cancel from any non-terminal state. | `wcon-data-model` §4 |
| `session_assignments` | Profile-to-role-slot bindings per session. Supports Mode A (stage-aware) and Mode B (role-aware fallback). | `wcon-data-model` §4.2 |
| `settings` | Key-value config. JSON-encoded values. | `wcon-data-model` §5.1 |
| `users` | Local identity store. Argon2id hashing. Console roles: admin ⊃ operator ⊃ viewer. Never deleted, only disabled. | `wcon-data-model` §5.3 |
| `user_sessions` | Cookie-based browser sessions. SHA-256 hashed token. 24h TTL. | `wcon-data-model` §5.4 |
| `api_tokens` | Bearer tokens for programmatic access. SHA-256 hashed. | `wcon-data-model` §5.5 |
| `audit_log` | Append-only mutation record. Admin-readable. | `wcon-data-model` §5.6 |
| `login_attempts` | Rate-limit tracking. GC'd after 24h. | `wcon-data-model` §5.7 |

### In-Memory

| Structure | Contents |
|-----------|----------|
| `TaxonomyIndex` | Roles, tools, envelope types, checkpoint types, verticals (with full `VerticalEntry` including context_schema, tool_policies, checkpoint_types, quality_criteria, task_types, workflows). |
| Active session state | Workspace states, task states, trail buffer, pending gates/escalations/refusals per session. |

**Spec:** `wcon-data-model`

## Next Steps: Close §13.7 work packages

The merger (M0–M7) and wiring (W1–W7) are done. The 2026-04-15 audit's §11 pre-release punch list (items 1–5) is landed, along with §12.1 tooling, §12.2 T1–T10, and §12.3 F1–F6/F9. Remaining work is tracked as ten numbered packages in `AUDIT-2026-04-15.md` §13.7, each with status, blockers, effort, deliverables, and acceptance criterion.

### Pre-`v0.1.0` punch list (from `AUDIT-2026-04-15.md` §11)

| # | Item | Status |
|---|---|---|
| 1 | ~~CI — add `cargo-deny` to `ci-lint.yml` + `deny.toml` at platform root~~ | **done** — `60d2245` |
| 2 | ~~CI — add SBOM + Trivy OCI scanning to `release-runtime.yml` + `release-console.yml`~~ | **done** — `7ba4ae8` |
| 3 | ~~Runtime auth — wrap post-lookup equality with `subtle::ConstantTimeEq` + re-key HashMap by hashed-token~~ | **done** (exceeded spec) — `d10ed29` |
| 4 | ~~Doc fix — `wacp-console/IMPLEMENTATION.md` Phase 4.6 step names~~ | **done** — `71b44b6` |
| 5 | ~~Move/cross-link `wacp/AUDIT-2026-04-12.md`~~ | **done** — `71b44b6` (now at `AUDIT-2026-04-12.md`) |
| 6 | Schedule the LLM stub that unblocks W7 T7.2/T7.3 | **stub landed + §13.7.6b WA1–WA3 landed** — `abfbb99` / `afe98f6` / `140fcc2` / `2f7b7ae` / `743c9bd` (§13.7.6) + `b01757c` / `69bcde0` / `822674c` (§13.7.6b WA1/WA2/WA3). Un-ignore of the six T7.* tests still blocked on WA3.5 (checkpoint gates) + WA3.6 (auto-integration) + WA5 (dispatch-failure harness) — ~9–11 h remaining |
| 7 | Plan the frontend test build-out (Phase 7.5–7.10 Playwright E2E) | F-series **complete** — `d63648a` + `e870018` + `92b3ddb` + `543c295`; Playwright open — audit §13.7.7 |

### Testing coverage initiative (audit §12) — progress snapshot

| Weeks | Phase | Status |
|---|---|---|
| 1–2 | Tooling — `cargo-llvm-cov`, Vitest v8, Python `coverage --branch`, Codecov | **landed** — `7f0736b` |
| 3–4 | Rust branch gap — T1–T11 | **landed** — T1–T10 in `840450a` (+~9,800 lines); T11 `console-db` in `2fdf191` (+2,102 lines; region 55.6 → 98.3 %, line 63.4 → 99.1 %). |
| 5–6 | Frontend + E2E | F-series **complete** — `fe48c7b` (F1–F6/F9, ~5,200 lines) + `e870018` (F7, 41 tests) + `92b3ddb` (F8, 52 tests) + `543c295` (F10, 16 tests). Playwright scenarios not started — §13.7.7. Blocked partly on LLM stub (§13.7.6) for golden-path + multi-user. |

Mutation testing (`cargo-mutants`, `stryker`) still pending — audit §13.7.9. Codecov monthly ratchet deferred until baseline stabilizes — audit §13.7.10. Integration + chaos additions I1–I5 ready (§13.7.8); I6 waits on the LLM stub.

**End-state targets unchanged:** Rust 95% branch, Console frontend 95% branch, TS verticals 95% branch, Python SDK 95% branch.

### New workstream added this session — frontend test-execution stability

Not in the original audit. Running the newly-written F-series RTL suite under `pool: "forks"` with `maxWorkers: 1` and `--max-old-space-size=6144` climbed one worker's RSS past 5 GB across files and killed WSL twice. Two fixes landed:

- `wacp-console/frontend/vitest.config.ts` — `execArgv` lowered from 6144 MB to 1536 MB. Forces V8 GC pressure early; single-file leaks OOM cleanly inside vitest instead of escaping into system memory.
- `wacp-console/frontend/scripts/run-tests-isolated.sh` + `npm run test:isolated` — per-file process isolation via a shell loop. Commits `82a4213`, `d71c4fe`, `fe48c7b` (suite itself).

One outstanding known issue from that work — `ProfilesPage.actions.test.tsx` OOMing inside its own per-file cap — was **resolved in `d63648a` (§13.7.1)**: root cause was an infinite render loop driven by a `mockImplementation` returning a fresh object per call, not a memory leak. The RTL `cleanup()` hook was also missing from `test-setup.ts` and was added globally in the same commit. Full post-mortem + extended pattern library live at `wacp-console/performance-optimization.md`. `npm run test:isolated` now exits 0 across all 17 files with session peak ~291 MB RSS and walltime ~62 s.

### Resumption Point

**M0–M7 merger, W1–W7 wiring, runtime implementation audit, §11 pre-release punch list (1–5), §12.1 tooling, §12.2 T1–T11, §12.3 F1–F10, audit §13.7.1–§13.7.5, §13.7.6 (stub provider + I6), and §13.7.6b WA1/WA2/WA3/WA3.5/WA3.6 all complete.** Working tree clean. Test totals after this session: `wacp-coordinator` 387 (+9), `wacp-workspace` 65 (+5), `wacp-runtime` 109 (+6), `wacp-types` 45 (unchanged), `console-integration` 6 passing + 6 ignored (no regression — five of six ignores now structurally unblocked); `console-integration --test llm_stub_e2e`: 2/2 green; workspace clippy + fmt clean across modified crates.

§13.7.6b remaining = **WA5** (dispatch-failure proxy; revised estimate 3–4 h) + **un-ignore sweep** for T7.2/T7.3/T7.7/T7.8/T7.10 (10–20 h Console-level integration tests on top of the runtime-side primitives that already pass via WA3.5/WA3.6 unit tests). Full scope + file-anchored deliverables in `impl/wiring-strategy-b.md` §3.5 + §4. T7.5 is the only test still blocked on a runtime-adjacent piece (WA5).

When resuming:
1. Read `AUDIT-2026-04-15.md` §13 (~10 min) — §13.8 tracking table + the new "13.7.6b — Status snapshot (post-WA3.5/WA3.6)" sub-table are the fastest index. Skip §1–§12 unless you need the underlying rationale.
2. Read `impl/wiring-strategy-b.md` (~5 min) — §3.3.5 (WA3.5) and §3.3.6 (WA3.6) marked LANDED with effort actuals. §3.5 (WA5) carries the deferral rationale and the two implementation paths (generic GrpcPool vs tonic mock-server).
3. Read `wacp-console/performance-optimization.md` (~5 min) — new §11 covers the WA3.5/WA3.6 backend drifts: enum-offset gotcha (§11.1), cross-crate exhaustive-match propagation (§11.2), async cascade (§11.3). Any new performance-adjacent observation should extend this.
4. `cd /home/aakil98/mada/wacp-platform`. **Commit the working tree first** (suggested: two commits — `feat(§13.7.6b WA3.5 ...)` and `feat(§13.7.6b WA3.6 ...)` — plus a doc-update commit, OR a single bundled `feat(§13.7.6b WA3.5 + WA3.6) ...` if the user prefers one atom). Recommended next ordering:
   - **WA5 — dispatch-failure proxy** (~3–4 h). Pick one path: (a) make `GrpcPool` generic over the channel type (cascades to AppState), or (b) implement a tonic mock CoordinatorService that forwards 12 RPCs + 1 streaming RPC to the real runtime, intercepting only Dispatch. Path (b) is simpler-ish but more lines; path (a) is more Rust-idiomatic but more invasive. Either way, T7.5's body is sketched in `chaos.rs` ready to fill.
   - **Un-ignore sweep — T7.2/T7.3/T7.7/T7.8/T7.10** (~10–20 h, parallelizable per test). Each test needs StubLLM fixture authoring + agent orchestration + WS observation + DB assertions. The runtime-side primitives are proven via WA3.5/WA3.6 unit tests; this layer is observability validation. Suggested order: T7.3 (simplest — Complete signal → Closed) → T7.2 (gate flow) → T7.7 (concurrency) → T7.8 (slow consumer) → T7.10 (latency). T7.5 stays ignored until WA5.
   - **In parallel** (independent): **§13.7.9 (mutation testing)** — 2 h setup + ongoing triage.
   - **Then §13.7.7 (Playwright E2E tooling + first two scenarios)** — 4–6 h, `golden-path.spec.ts` + `multi-user.spec.ts` benefit from un-ignore sweep landing first but don't strictly require it.
   - **Then §13.7.8 (Rust integration I1–I5)** — 4–6 h; I6 already landed via §13.7.6.
   - **§13.7.10 (Codecov monthly ratchet)** — deferred until 2–3 `main` merges with §13.7.6b so the new baseline settles.
5. The dev→main batched merge already ran 2026-04-17 morning (21 commits fast-forwarded up through `743c9bd`). Current dev lead = WA1/WA2/WA3 (`7782d78`) + uncommitted WA3.5/WA3.6 changes — candidate for the next merge after committing.
6. Each §13.7 package's "Acceptance criterion" is what closes it. Update the §13 status tables as items land.
7. Tag `wacp-runtime-v0.1.0` and `wacp-console-v0.1.0` independently once the Rust branch-coverage floor clears 85 % and the Playwright golden-path + auth scenarios (§13.7.7 minimum) are green.

### Hollow Code Inventory — closed

All eight hollow components identified pre-wiring are now real. For the record:

| Component | Pre-W state | Post-W state (file:line proof) |
|---|---|---|
| gRPC pool | Built, never instantiated | Instantiated + connected in `wacp-console/crates/console/src/main.rs:150–162`; injected into `AppState` |
| Launch flow | SQLite state transitions only | `SubmitGoal → Decompose → Dispatch` sequence in `wacp-console/crates/console-core/src/session_launcher.rs:154–347`; rollback at `:375–396` |
| Session monitor | Didn't exist | Tokio task with 4 stream drivers in `wacp-console/crates/console-core/src/session_monitor.rs` (drivers at `:644/675/706/737`) |
| Gate resolution | Audit log only | `HighwayService::respond_to_gate` at `wacp-console/crates/console-api/src/routes/highway.rs:73–120` |
| Escalation response | Audit log only | `HighwayService::respond_to_escalation` at `…/highway.rs:260–335` |
| Directive injection | Audit log only | `HighwayService::send_envelope` at `…/highway.rs:409–500` |
| Cancel cleanup | Empty match arms | `CoordinatorService::AbortWorkspace` at `wacp-console/crates/console-api/src/routes/sessions.rs:687` |
| Startup recovery | Query exists, not wired | Scan + probe + respawn at `wacp-console/crates/console-core/src/recovery.rs:67–119` |

## Key Invariants

1. **Console never modifies taxonomy files or runtime state outside of gRPC/REST** (`wcon-vision` BC1, BC2)
2. **Manifest-driven rendering** — no hardcoded per-vertical logic; new vertical works without code change (`wcon-vision` G7, `wcon-ui` §12.6)
3. **Taxonomy index is atomic** — fully built or previous visible, never partial (`wcon-data-model` §10.4)
4. **Profile versions are append-only** — existing rows never mutated except `is_current` and `deleted_at` (`wcon-data-model` §10.1)
5. **At least one active admin** always exists (`wcon-auth` §13)
6. **No default credentials** — bootstrap generates a one-time credential (`wcon-auth` §13)
7. **Audit log is append-only** — no UPDATE/DELETE through the application (`wcon-auth` §13)
8. **Tool-layer policies are never enforced by the Console** — surfaced as warnings and refusal events only (`wcon-discovery` §3.5)

## ADRs

| ADR | Decision |
|-----|----------|
| 001 | Runtime is the vertical registry (REST, not filesystem) |
| 002 | Multi-user auth in Phase 1 |
| 003 | Tech stack: Rust/Axum/Tonic + React/Vite/TS + SQLite/sqlx |
| 004 | ~~Single binary with embedded frontend (rust-embed + cargo-dist)~~ — **superseded by ADR-009** |
| 005 | TLS trust boundary: three modes (plaintext loopback / system CA / explicit CA) |
| 006 | Apache-2.0 license |
| 007 | Profile YAML format versioning (format_version: 1) |
| 008 | OpenAPI as shared contract (utoipa → openapi-typescript) |
| 009 | OCI-only console distribution; cargo-dist deferred; `rust-embed` retained inside Dockerfile cargo build stage |

**Full ADR text:** `wacp-console/SPEC_BUILD.md` + `impl/adr-009-oci-only-console.md`.

## Workspace Layout

```
wacp-platform/
├── Cargo.toml                  # umbrella workspace (all Rust crates from both trees)
├── Cargo.lock                  # unified lockfile
├── rust-toolchain.toml         # pin Rust stable
├── docker-compose.yml          # dev stack: runtime + console + postgres
├── SEED.md                     # this file
├── AUDIT-2026-04-15.md         # post-wiring health audit + testing strategy
├── impl/                       # merge-plan, merge-execution-log, wiring-strategy, ADR-009
├── .github/workflows/          # ci-lint / ci-wacp / ci-console / release-runtime / release-console
│
├── wacp/                       # runtime subtree
│   ├── crates/                 # 15 Rust crates
│   │   ├── wacp-proto/         # shared tonic_build codegen (M3)
│   │   ├── wacp-runtime/       # binary — gRPC server, config, TLS
│   │   ├── wacp-coordinator/   # orchestrator: workspace tree, task graph
│   │   ├── wacp-transport/     # gRPC/REST/WS gateway, auth
│   │   ├── wacp-highway/       # signals, gates, escalations
│   │   ├── wacp-tools/         # tool execution, cancellation, retries
│   │   ├── wacp-llm/           # provider adapters (Anthropic, OpenAI)
│   │   ├── wacp-security/      # PII filter, policies
│   │   ├── wacp-taxonomy/      # VerticalManifest + derived roles
│   │   ├── wacp-types/         # primitives
│   │   └── …                   # fsm, clock, permissions, trail, workspace, sdk, coordinator-sdk
│   ├── packages/               # @wacp/cli, @wacp/local (TypeScript)
│   ├── ecosystem/              # 7 verticals: swe, devops, mlops, finance, healthcare, analytics, datasci
│   ├── sdk-python/             # Python bindings
│   ├── highway-ui/             # legacy highway webapp
│   ├── openapi.yaml            # runtime REST spec (16 endpoints)
│   ├── proto/                  # .proto definitions
│   ├── impl/                   # 17 impl specs + phase evals
│   └── IMPLEMENTATION.md       # runtime forward strategy
│
└── wacp-console/               # console subtree
    ├── crates/
    │   ├── console/            # binary — CLI, tracing, startup, taxonomy build, rust-embed assets
    │   ├── console-api/        # Axum routes, handlers, OpenAPI, pagination, WebSocket
    │   ├── console-core/       # auth, profiles, sessions, taxonomy, session_launcher, session_monitor, recovery
    │   ├── console-db/         # sqlx types, queries, migrations
    │   ├── console-runtime/    # GrpcPool, REST client, upstream re-exports
    │   └── console-test-support/ # mock runtime (real Tonic/Axum), fixtures
    ├── migrations/             # 9 SQL tables
    ├── frontend/               # Vite + React 19 + TypeScript SPA
    │   ├── src/api/            # types.ts (generated), client.ts, hooks/
    │   ├── src/store/          # auth.ts, ui.ts, session.ts (Zustand)
    │   ├── src/components/     # Layout, Sidebar, AdminGuard, Notifications
    │   ├── src/realtime/       # useSessionStream.ts (WebSocket hook)
    │   └── src/surfaces/       # auth, discovery, profiles, sessions, oversight, settings, admin
    ├── integration/            # W7 cross-binary integration + chaos tests
    ├── openapi.yaml            # console REST spec (66 operations, 12 tags)
    ├── specs/                  # 12 finalized design specs + 7 wiring coding specs
    ├── impl/                   # phase evals + wiring evals
    └── IMPLEMENTATION.md       # console forward strategy
```

## Design Specs (all final)

| # | ID | Title |
|---|----|-------|
| 1 | `wcon-vision` | Product Vision |
| 2 | `wcon-glossary` | Glossary |
| 3 | `wcon-architecture` | System Architecture |
| 4 | `wcon-data-model` | Data Model |
| 5 | `wcon-discovery` | Agent & Role Discovery |
| 6 | `wcon-profiles` | Profile System |
| 7 | `wcon-sessions` | Session Lifecycle |
| 8 | `wcon-highway` | Highway Integration |
| 9 | `wcon-api` | API Surface |
| 10 | `wcon-ui` | UI Design |
| 11 | `wcon-test` | Test Strategy |
| 12 | `wcon-auth` | Authentication & Authorization |

*WACP Platform — authored by Akil Abderrahim and Claude Opus 4.6. Refreshed 2026-04-16 with post-audit progress and §13.7 task packages; refreshed again same-day by Claude Opus 4.7 (1M context) after §13.7.1–§13.7.5 landed on `dev`; refreshed 2026-04-17 after §13.7.6 (stub provider + I6) landed and §13.7.6b (runtime wiring follow-up) was carved out; refreshed again same-day after dev→main fast-forward and §13.7.6b WA1/WA2/WA3 landed, with WA3.5 + WA3.6 carved out as the remaining gate-fan and auto-integration pieces; refreshed 2026-04-17 (third pass) after §13.7.6b WA3.5 (checkpoint-approval gates) + WA3.6 (auto-integration on Complete) landed in the working tree, WA5 deferred (~3–4 h harness-side proxy), and the un-ignore sweep deferred (~10–20 h Console-level integration tests on top of WA3.5/WA3.6 primitives).*
