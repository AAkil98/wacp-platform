# Console — Performance Optimization Notes

> Notes collected while stabilizing the Vitest suite (session 2026-04-16) and extrapolated into a guide for runtime-performance work on the React/Axum console. Testing pain is usually a leading indicator of app-performance smells — this document captures both sides.

## 1. Why this doc exists

The F-series RTL test build-out (`AUDIT-2026-04-15.md` §12.3) ran into repeated WSL crashes: RSS climbed past 5 GB during a full `vitest run`, and a single file (`ProfilesPage.actions.test.tsx`) pinned the event loop at 1.4–1.7 GB. Investigation showed the observable symptoms (memory, walltime) were downstream of patterns that also matter in production render paths. This document is the working record of what we saw, what it implies for the shipping app, and where to aim the next optimization pass.

Authoritative references:
- `vitest-oom-notes.md` — chronological investigation log (pre-diagnosis)
- `AUDIT-2026-04-15.md` §13.6 — resolved test-stability workstream
- `d63648a` — fix commit for the infinite-render-loop diagnosis

## 2. Findings from the stabilization work

### 2.1 The headline bug — infinite render loop in `ProfilesPage`

Symptom: `ProfilesPage.actions.test.tsx` ran 9 tests fast (< 150 ms each), then the 10th test (`switches between profiles`) stopped emitting output. RSS climbed to 1669 MB before V8 GC gave up; walltime 340 s.

Root cause: the test used `mockProfile.mockImplementation` whose "p2" branch returned a **freshly-spread object literal on every call**. In the real app `useProfile` is backed by React Query, which memoizes and returns a stable reference for the same key while data doesn't change. In the test, the unstable reference propagated through:

```
useProfile("p2")            → new object every render
  → profileQuery.data       → new reference every render
  → loadedProfile           → new reference every render
ProfilesPage.tsx:205        useEffect([loadedProfile, creating])  ← re-fires every render
  → setForm(...)            → triggers re-render
  → (back to top)           unbounded
```

The fix was a two-liner (`mockReturnValue` instead of `mockImplementation`) but the surrounding implications are real:

- **Any future caller that derives from a hook return without relying on stable `.data` references will trip the same cycle.** React Query gives us stable refs as long as the cache key stays the same and data doesn't change — but if a caller spreads-and-copies the hook result inside a `useMemo` / `useEffect` dep, the stable reference guarantee is lost.
- **`ProfilesPage.tsx:205`'s `useEffect([loadedProfile, ...])` is load-bearing.** It is the only sync between server state and the form's `useState` mirror. Any future change that makes `loadedProfile` unstable (e.g., a `.map()` in the selector, an inline object pass-through) will cause the same symptom in production — except production would show as sluggish form loads or pinned CPU, not test OOM.

### 2.2 The "it's not a leak" lesson

We spent the first investigation round framing this as a memory leak: DOM accumulation, React Query observer retention, missing `cleanup()`. All plausible, all real minor issues, none of them the cause. **RSS growth was a symptom of a pinned event loop, not a leak.** Always verify the mechanism before investing in a fix — add `--reporter=verbose` early, redirect to a file, confirm whether tests are still completing at all before concluding "the cleanup logic is wrong."

For app performance: the same framing error is easy to make in production — "the UI feels slow, must be memory" — when the actual cause is a tight render loop or a synchronous blocking call inside a render. Before scoping a performance fix, attach DevTools Performance profiler and confirm whether frames are being produced.

### 2.3 Missing test-DOM cleanup (real, but not the headline)

`test-setup.ts` imported only `@testing-library/jest-dom/vitest` and did **not** register `afterEach(cleanup)`. Every `render()` across a file's tests left its mounted tree in the jsdom document. This alone would not reach 1.6 GB but compounds with any other leak and inflates the baseline. Fixed by adding `afterEach(cleanup)` globally (`d63648a`).

App-code parallel: nothing exactly equivalent, but worth noting that React 19's `StrictMode` double-invocation of effects surfaces exactly this class of cleanup bug in dev — if a future component mounts subscriptions (WebSocket handlers, interval timers, `ResizeObserver`) without matching cleanup in its `useEffect` return, StrictMode will expose it. Keep StrictMode on in dev.

### 2.4 A11y label-binding gap is now a recurring pattern

Two components in two sessions shipped without `htmlFor`/`id` associations between `<label>` and `<input>`:

- `ProfilesPage.tsx` — Autonomy / Visibility radio groups and Budget Limit / Budget Window inputs; fixed in `d71c4fe`.
- `Wizard.tsx` — three budget inputs (`Max Cost`, `Max Tokens`, `Max Wall Time`) and Session Name; fixed in `e870018`.

The tests surfaced both because RTL's `getByLabelText` enforces the binding contract that a screen reader needs. Behavior for sighted mouse users is unchanged either way, so the gap escapes eyeball review; only the test boundary catches it.

Codebase-wide recommendation (P1): add `eslint-plugin-jsx-a11y` with at least the `label-has-associated-control` rule to the lint stage. That moves this from "caught by tests post-hoc" to "caught at author-time," and incidentally documents the a11y contract in the build. Effort: 15 min. Payoff: kills this class of gap at the door.

### 2.5 Test-writing reveals spec-vs-impl drift

The F8 build-out (§13.7.3, `92b3ddb`) turned up two concrete drifts between the audit deliverables and the shipping components:

- **`RefusalPanel`.** Deliverable called for an "acknowledge action" and per-refusal "expiry". Neither exists — `RefusalPanel.tsx` is purely read-only and the `RefusalEvent` store shape has no `expires_at`. The deliverable was authored against an assumed design.
- **`InjectionBar`.** Deliverable called for send-to-workspace behavior distinguished by active / paused / completed / failed states, with rejection for terminal ones. The component filters clients-side to `state.toUpperCase() === "ACTIVE"` and never renders the other states as targets; there is no per-state rejection path because there is no per-state send path.
- **`Notifications` (F10, `543c295`).** Deliverable called for variant toasts (success / warning / error / info), explicit dismiss buttons, stacking rules, browser-notification permission flow, and "99+" badge overflow. The shipping component has **none of that**: two priorities (normal / high), click-to-dismiss (no button), no stacking limit, no `window.Notification` integration, no overflow formatter. More severely, the component is **not imported anywhere** and has no exported API to add toasts — `useState` is private, no `addToast` hook. It is a stub. `NavBadge` in the same file is functional; that's the only live piece.

Both drifts surfaced from the mechanics of writing tests, because `getByLabelText` / `getByRole` queries forced a grep of the actual component contract before a selector could be chosen. You cannot write a test for a button that isn't in the source.

Recommended pattern when drift appears:
1. Test **actual** behavior, not the presumed shape. Put a short drift note at the top of the test file and in the commit message.
2. Decide separately whether the absent feature is (a) a latent bug — users expect it and it got lost — or (b) a spec that needs harmonizing with the trimmed-down impl. Do not leave "not yet shipped" features sitting as latent TODOs in audit tables.
3. For the running app: these three drifts mean operators currently have no way to dismiss a refusal from the panel, no way to inject into a non-active workspace, and **no toast feedback at all** (the toast component exists but is never mounted and has no data plumbed in). The `Notifications` case is the most severe because the deliverable implied a running feature and what shipped is a stub — this warrants an explicit follow-up: either wire it up against highway-emitted `notification` channel events or remove the dead component and treat toasts as a future scope.

The broader signal: the AUDIT-2026-04-15 §12.3 deliverables were authored before the frontend tests existed. Writing the tests is the forcing function that realigns spec with impl. Three out of ten F-items surfaced drift in this round (F8 two, F10 one) — expect the E2E scenarios to surface more.

## 3. App-level patterns worth auditing

Direct grep-evidence from the current tree (as of `b17ae49` on `dev`).

### 3.1 Component size — mount cost and re-render radius

The four largest single components:

| File | LOC | Concerns |
|---|---|---|
| `src/surfaces/sessions/Wizard.tsx` | 802 | 6-step wizard, all steps rendered in one component tree; 17 inline `CSSProperties` constants + 2 style functions; local `useState` per step. **Measured (§13.7.2, `e870018`):** 41 tests in 6.1 s; per-file peak 287 MB. Mount cost is not actually the issue — prior prediction revised. Real issue is the inner-function-component pattern (see below). |
| `src/surfaces/profiles/ProfilesPage.tsx` | 530 | Sidebar + editor + version panel + delete modal in one component; 14 inline style constants + 1 style function; 11-field form via `useState`. |
| `src/surfaces/discovery/VerticalsTab.tsx` | 494 | Heavy nested-table renderer; no memoization on rows. |
| `src/surfaces/discovery/RolesTab.tsx` | 366 | Same pattern as Verticals. |

**Recommendation.** ProfilesPage would benefit from decomposition into its natural subcomponents (sidebar/editor/versions/delete). Wizard has a specific anti-pattern to fix first: the six step renderers (`SelectVerticalStep`, `SelectWorkflowStep`, `AssignProfilesStep`, `ContextStep`, `BudgetOverridesStep`, `ReviewLaunchStep`) are declared as **nested function components inside `Wizard`'s body** (`Wizard.tsx:408–666`). Every Wizard re-render creates fresh function references for all six, and React reconciler treats the active step's component as a new component type — unmounting and remounting the whole step subtree instead of updating it in place. Cheap fix: extract each step to module scope as a normal component that takes the shared state as props. More invasive: extract to its own file. Either way, the re-render radius drops and the mount/unmount churn disappears.

### 3.2 Functional style helpers — per-render allocation

Pattern: `const listItem = (selected: boolean): React.CSSProperties => ({ ... })`. Called in a render path, this allocates a fresh style object per item per render. Evidence:

- `src/surfaces/admin/UsersPage.tsx:102` — `badge(disabled)`
- `src/surfaces/profiles/ProfilesPage.tsx:47` — `listItem(selected)`
- `src/surfaces/sessions/Wizard.tsx:88` — `stepItem(state)`
- `src/surfaces/sessions/Wizard.tsx:151` — `card(selected)`

Impact is small per call but scales with list length. For UsersPage and ProfilesPage the lists are typically single-digit rows; for Wizard's role-assignment step with many roles per vertical it is larger. Cheap fix: replace with a `Record<Variant, CSSProperties>` lookup at module scope and index by the variant.

### 3.3 `useEffect` deps are the sharpest edge

The Wizard (`Wizard.tsx`) is the next expected source of a symmetrical bug: it has `useCallback` imports already, indicating derived functions are being built — meaning it is exactly the shape where unstable deps creep in (a `useMemo` that returns a new object when inputs are referentially-different-but-value-equal, then drives a `useEffect` that writes to state).

**Recommendation for Wizard and any new component.**
1. Default to React Query return objects as the source of truth; read `.data` lazily rather than copying.
2. When you do need derived state, use `useMemo` with primitive deps when possible (IDs, strings) rather than the full object.
3. For `useEffect([obj])` patterns, either prove the reference is stable or switch to value-compare-via-key (`useEffect([obj.id, obj.version])`).
4. `StrictMode` in dev will catch a class of these via double-invoke; keep it on.

### 3.4 Form state via `useState`

`ProfilesPage.tsx` holds 11 form fields as a single `useState` object. Every field edit re-renders the whole editor subtree. `react-hook-form` would narrow each re-render to the field being edited (uncontrolled-by-default, subscription-based updates). Low-urgency optimization, but worth packaging into the ProfilesPage split above — the editor subcomponent is the natural place to make the switch.

Likely relevant again for Wizard: step-4 context form is currently in a dedicated `ContextForm` subcomponent but still `useState`-driven. If the context schema grows, same trade applies.

## 4. Optimization roadmap — priority order

Rough effort × payoff. Update as items land.

### P1 — quick wins, defensive

1. **`useEffect` dep audit.** Walk every `useEffect` whose dep array contains an object or array. For each, prove the reference is stable under typical re-renders (React Query stable data, `useMemo` with primitive inputs, module-scope constant). File a follow-up if not. Highest-signal files: `ProfilesPage.tsx`, `Wizard.tsx`, `SettingsPage.tsx`, `AuditLogPage.tsx`, `UsersPage.tsx`, `SessionsPage.tsx`. Effort: 1–2 h. Payoff: closes the class of bug that caused §13.7.1.
2. **Module-scope style records.** Replace the four functional `CSSProperties` helpers (§3.2) with `Record<Variant, CSSProperties>` at module scope. Effort: 15 min. Payoff: negligible per render but removes a false-positive when profiling.

### P2 — structural, medium effort

3. **Decompose ProfilesPage.** Split into `ProfilesSidebar`, `ProfileEditor`, `ProfileVersionsPanel`, `DeleteProfileModal`. Mount cost of the page drops; each subcomponent becomes memoizable independently. Effort: 3–4 h. Payoff: smaller re-render radius, easier to profile, foundation for react-hook-form swap. Note: test files already aligned with this split — each sub-surface has its own `.test.tsx`.
4. **Extract Wizard step components to module scope.** Quick win ahead of full decomposition: move the six `SelectVerticalStep` / `SelectWorkflowStep` / `AssignProfilesStep` / `ContextStep` / `BudgetOverridesStep` / `ReviewLaunchStep` functions (currently declared inside `Wizard`'s body, §3.1) to module scope or a sibling `steps/` folder, passing state via props instead of closure. Stops the per-render function recreation and the mount/unmount churn when the active step changes. Effort: 1–2 h. Payoff: cheaper step transitions; as a secondary, test-time render cost drops without changing public behavior.
5. **Route-level lazy loading.** `React.lazy()` + `Suspense` for each surface in `src/App.tsx`. Initial bundle shrinks; cold-load time of `/sessions` stays cheap while the session is being launched. Effort: 1 h + eval.

### P3 — nice-to-have

6. **`react-hook-form` for large forms.** Start with the post-decomposition `ProfileEditor`. Effort: 2–3 h per form. Payoff: field-level re-render radius, field-level validation, smaller typed state.
7. **Virtualization for long lists.** Currently not needed — profile/user/session lists are dozens at most — but `wacp-console/specs/wcon-vision` anticipates tenant deployments with hundreds of sessions. Revisit when real tenant data shows up.

## 5. Already landed this session

| Change | Effect |
|---|---|
| `d63648a` — `mockReturnValue` instead of `mockImplementation` for stable-ref test | Closed the infinite render loop in `ProfilesPage.actions.test.tsx`. |
| `d63648a` — global `afterEach(cleanup)` in `test-setup.ts` | Prevents DOM accumulation across tests for every future test file. |
| `d63648a` — module-scoped `QueryClient` + `queryClient.clear()` in `afterEach` of `ProfilesPage.actions.test.tsx` | Defensive, not load-bearing post-diagnosis. Kept because it makes the QC reset intent explicit. |
| `82a4213` — `execArgv: ["--max-old-space-size=1536"]` in `vitest.config.ts` | Bounds any single vitest worker so a regression surfaces as a clean OOM inside vitest, never as a WSL crash. |
| `82a4213` — `npm run test:isolated` + `scripts/run-tests-isolated.sh` | Per-file process isolation. Trades ~30 s walltime for a hard upper bound on cross-file heap carry-over. |
| `e870018` — `Wizard.tsx` a11y label bindings (§2.4) | Second instance of the same gap after `d71c4fe`; raises the case for `eslint-plugin-jsx-a11y`. |

## 6. Watch-list — what would indicate regression

Signals to act on when they appear:

- **Single test file RSS > 500 MB** during `npm run test:isolated`. Current baseline (post-`e870018`): every file < 300 MB; **session peak 291 MB** across 17 files. A jump above 500 MB means a new component has reintroduced an unstable-ref pattern or is mounting an unexpectedly heavy subtree.
- **`npm run test:isolated` walltime > 90 s.** Current baseline: **62 s across 17 files** (per-file mean ~3.6 s). Wizard is the slowest single file at 7 s (41 tests). Any single file breaking 10 s deserves a look.
- **Vitest `transform` + `setup` + `import` totals > 3 s in the per-file header.** Indicates the module graph is growing fast (too much eagerly-loaded code) — signal for §4 item 5 (route-level splitting).
- **React DevTools Profiler showing > 16 ms commit time** for a non-initial render on any interactive surface. Indicates the re-render radius is too wide and §4 items 3 / 4 / 6 apply.
- **Memory graph in browser DevTools showing a growing `detached DOM nodes` count** while navigating between surfaces. Indicates a `useEffect` cleanup is missing in the relevant surface — search its source for `addEventListener`, `setInterval`, `new WebSocket`, `new ResizeObserver` without a matching return function.

## 7. Diagnostic protocol for the next memory-adjacent bug

Sequenced steps that would have saved several hours on `ProfilesPage.actions.test.tsx`:

1. **See the actual output first.** Run vitest with `--reporter=verbose`, redirect to a file, read it. Do not pipe through `tail` while the process may still be producing output. If `tail` returns truncated, the run has not finished.
2. **Is it still emitting test markers, or is it silent under memory pressure?** If tests are still being reported one-by-one but slowly, you have a performance problem (probably render-loop-in-one-test). If RSS is climbing while no test lines are emitted, you have a pinned event loop — almost always an async waitFor inside an infinite render.
3. **Attach an RSS monitor in parallel.** `Monitor`/`ps` / `top` — track peak, but more importantly, track the **shape** of the curve. Linear growth → leak. Sudden jump → single-test allocation spike. Plateau at the cap with no progress → render-loop / pinned event loop.
4. **Bisect by describe-block.** If a file has multiple describe blocks, run one at a time. The cost is small (file-scoped tests are fast) and it localizes the offending case in minutes.
5. **Correlate with source code.** Once the offending test is identified, look at what mocks it sets up and compare to the passing tests. `mockImplementation` returning a fresh object is a near-certain smell whenever the consumer uses the result in a dep array.
6. **Two successive `fireEvent.change` calls don't flush React state between them** (React 19, §13.7.2 evidence). If you need to test "type X, then clear to ''", inject a flush point: `fireEvent.change(input, {target:{value:"X"}}); await waitFor(() => expect(input.value).toBe("X")); fireEvent.change(input, {target:{value:""}});`. Without the `waitFor` the second event reads stale controlled-input state and the subsequent `setState` can appear to no-op. This burned a debug round in §13.7.2; now baked into the pattern library.

## 8. Backend — not yet investigated

Testing coverage for the Rust crates (`wacp-console/crates/*`) lives under §12.2 / §13.4 of the audit and has not produced runtime-performance signals in this session. The natural next pass (after §13.7 closes) is to run the T1–T10 coverage benchmarks under `cargo bench` for the hot paths — `console-core::session_monitor` (broadcast fan-out), `console-core::session_launcher` (coordinator sequence), `console-api::middleware` (per-request auth/CSRF). Any findings there should be captured back into this document under a new §9.

## 9. console-db — spec-vs-schema drift from the §13.7.5 coverage sweep

Writing the branch-coverage tests for `console-db` (audit §13.7.5, `testing.rs` harness + `queries/coverage_tests.rs`) surfaced two Rust-side drifts — the direct backend analogue of the frontend drifts recorded in §2.5. The *mechanics of writing the negative-path tests* was again the forcing function: the `NOT NULL constraint failed` panic during test authoring pointed straight at the mismatch.

### 9.1 `session_assignments.profile_id` — type says Optional, schema says NOT NULL

- `migrations/007_session_assignments.sql`: `profile_id TEXT NOT NULL`, `profile_version INTEGER NOT NULL`.
- `queries/session_assignments.rs::SessionAssignmentRow`: `profile_id: Option<String>`, `profile_version: Option<i64>`.
- `queries/session_assignments.rs::count_assigned`: `WHERE session_id = ? AND profile_id IS NOT NULL` — defensive clause for a case the schema does not allow.

Consequence today: the `IS NOT NULL` filter in `count_assigned` is dead code against the current schema. A caller that constructs a `SessionAssignmentRow` with `profile_id: None` gets a `NotNullViolation` at `INSERT` time (covered by the new `not_null_violation_when_profile_id_is_none` test), *not* a compile error or a defaulted row. The API surface lies about the field being optional.

Two possible resolutions — pick one, don't leave both:
1. **Tighten the struct.** Change `profile_id: String` and `profile_version: i64`. The schema becomes the source of truth; callers can't represent an invalid state. Requires touching every construction site.
2. **Loosen the schema.** If unassigned role slots are actually a valid transient state (e.g., a session configured mid-wizard where not every slot is filled yet), drop the NOT NULL constraint and let the struct's Optional stand. Then `count_assigned`'s defensive clause becomes load-bearing.

Either choice is defensible. What isn't defensible is keeping both: today the code pretends to support a state the database refuses to store.

### 9.2 `profiles::max_version` — NULL aggregate handling is ambiguous

```rust
let row: Option<(i64,)> = sqlx::query_as("SELECT MAX(version) FROM profiles WHERE id = ?")
    .fetch_optional(pool).await?;
Ok(row.map(|r| r.0))
```

SQLite's `MAX(...)` over an empty set returns NULL. sqlx decodes that NULL into `i64` = 0 (observed from the failing test run that expected `None` for a missing profile). So:

- `row.map(|r| r.0)` never hits the `None` branch — aggregate queries always produce one row.
- For a missing profile, the function returns `Ok(Some(0))` — indistinguishable from "profile exists with version 0" (which the schema allows but is unusual).

Impact is small — all current callers create a profile before asking for `max_version` — but the signature `Option<i64>` implies more than the implementation delivers. Either:
1. Change the inner tuple to `Option<(Option<i64>,)>` (decode NULL explicitly) so missing vs. present is distinguishable, or
2. Change the signature to `Result<i64, sqlx::Error>` with the understanding that "no rows" is encoded as 0.

I'd lean toward (1) — the caller is currently doing `unwrap_or(0) + 1` anyway, and an explicit "no versions exist yet" signal is more honest than a sentinel.

### 9.3 Perf signals observed (or not)

- **Test walltime.** 98 tests for `console-db` complete in 0.76 s on the dev box (single-threaded tokio, in-memory DB for happy paths, tempfile-backed DB for `FaultyDb`-driven BUSY tests). No hotspot visible; each test averages ~8 ms.
- **sqlx migration cost.** Each `create_test_pool()` call re-runs all 9 migrations against a fresh in-memory DB (~3 ms per test). Not worth amortizing today at current test counts, but if the suite grows past ~500 tests, a `lazy_static!` migrated template + `ATTACH DATABASE` clone pattern would cut the setup cost. Note for future.
- **`FaultyDb::hold_write_lock` — detach-before-begin.** First cut of the harness returned the `PoolConnection` from the companion pool holding a `BEGIN IMMEDIATE`; when dropped, the connection went back to the pool *with the transaction still open*, so the next test's writes against the main pool saw a phantom reserved lock. Fix was to `.detach()` the connection so Drop closes the underlying SQLite handle (which releases the lock at the OS level). Captured in `testing.rs` doc comment. This is the backend mirror of the §3.3 `useEffect`-with-unstable-deps trap: a cleanup that looks correct at the type level but leaks state because the runtime's cleanup contract is weaker than the type implies.

## 10. wacp-llm stub provider — observations from §13.7.6

Writing the deterministic `StubAdapter` (`wacp/crates/wacp-llm/src/providers/stub.rs`, audit §13.7.6) surfaced a cluster of small allocation + sharing decisions that are low-impact today but would bite if the stub became a hot-path consumer (e.g., if the runtime adopts it for a mock-mode by default, or if an integration suite drives hundreds of agents against it in a tight loop).

### 10.1 Per-call full-input serialization

`serialize_for_match(messages, tools)` is called **twice** per `complete()` invocation today — once inside `resolve_response()` to find a fixture match, and again inside `complete()` / `complete_stream()` to compute `input_tokens` (`serialized.len() / 4`). Each call walks every message, allocates a new `String`, and writes role / content / block-variant formatting into it. For a typical coordinator turn (1–5 messages, ~500 chars) this is cheap (~μs), but for long conversations with tool-result blocks the cost scales with message history.

**Fix when it matters.** Lift serialization to the caller; pass the serialized form plus its length into `resolve_response()`. One allocation per call instead of two. The current two-call shape was chosen for readability; revisit if `complete()` latency shows up in the integration-suite `tokio-console` trace.

### 10.2 Streaming events materialized eagerly

`StubAdapter::complete_stream` builds the entire `Vec<StreamEvent>` (one per character + tool calls + Usage + Done) **before** returning the `StreamHandle`. For a fixture with 10-char content and 2 tool calls the vec has ~15 events; for a fixture simulating a 1000-token response it would be ~1000+ events allocated up-front. Memory cost is bounded by the fixture size (not user-driven input), so it's a compile-time decision rather than a runtime unknown, but the pattern diverges from the real Anthropic / OpenAI providers which stream from SSE as bytes arrive.

**Fix when it matters.** Replace `futures::stream::iter(events).then(delay)` with an `async_stream::stream! { … }` that yields events lazily. Keeps the same public shape, but peak memory tracks the widest fixture entry's content instead of its full event list. Noted — not prioritized. The unit test `stream_emits_content_then_usage_then_done` would need a small adjustment (the test currently collects all events at once, which still works).

### 10.3 `StubFixtures` sharing — Arc is the right call

`StubAdapter` holds `Arc<StubFixtures>` and derives `Clone`. Multiple adapters (e.g., one per workspace in an integration test that spawns many agents) share the parsed fixture set at no memory cost beyond the original YAML-parse allocation. The factory `build_adapter()` constructs one `Arc` per call — if a future runtime consumer calls this per-request (instead of once at boot), the YAML would be re-parsed each time. Recommendation when wiring from `wacp-runtime`: construct once at runtime startup and reuse the returned `Arc<dyn LlmAdapter>` across workspaces.

### 10.4 Hash matcher laziness

`StubMatcher::matches()` computes the SHA-256 only inside the `Hash` arm; `Prefix` and `Contains` skip the digest entirely. First-match-wins ordering means a fixture file that lists a `Prefix` match first avoids the hash cost for requests that take the prefix branch. Authoring guidance for the baseline fixture (`stub_responses.yaml`): put cheap matchers first and reserve `Hash` for the scenarios that genuinely need message-exact dispatch.

### 10.5 Integration-test stability signal

The §13.7.6 I6 test `i6_stub_adapter_drives_agent_round_trip` spawns a real `wacp-runtime` child, connects `wacp-sdk::Agent` + `CoordinatorService` gRPC, runs two `complete()` turns, exercises `complete_stream()`, and tears down — finishing in **0.28 s** on the dev box with RSS staying under 50 MB for the test process (runtime child is separate). This is the low-water-mark to beat if the stub provider grows — any future change that pushes either walltime > 2 s or RSS > 100 MB is a signal to profile before shipping.

### 10.6 Watch-list additions

Append to §6:
- **`cargo test -p wacp-llm` walltime > 2 s.** Current baseline: **0.07 s for 169 lib tests + 0.01 s for 83 branch-coverage tests**. Any jump into multi-second territory is a signal that a new provider or a heavy fixture has changed the parse/construction cost — almost always recoverable by moving work out of `#[test]` setup into module-scope `lazy_static` / `LazyLock`.
- **`cargo test -p console-integration --test llm_stub_e2e` walltime > 2 s.** Current baseline: **0.28 s for 2 scenarios**. Beyond 2 s points at a slower runtime-harness spawn (runtime binary bloat, slow `/healthz` handshake) rather than the stub itself — bisect by timing `RuntimeHarness::spawn_default().await` separately.

## 11. WA3.5 / WA3.6 wiring — backend drifts surfaced

The §13.7.6b WA3.5 (checkpoint-approval gates) + WA3.6 (auto-integration) work surfaced two backend-side drifts that share a structural shape with the frontend `useEffect`-deps trap (§3.3) and the spec-vs-impl drift (§2.5): both are *latent* — the production code compiles and runs, but the first time a new code path actually exercises the surface, it fails. Tests that drove the new path (rather than re-using existing patterns) caught both.

### 11.1 Rust ↔ proto enum offset

`GateType` in `wacp-types/src/enums.rs` has 7 variants starting at discriminant 0 (`TaskApproval = 0` … `CheckpointApproval = 6`). The proto-generated `wacp_v1::GateType` has 8 variants starting at 1 (`Unspecified = 0`, `TaskApproval = 1` … `CheckpointApproval = 7`). Casting `internal_gate_type as i32` and assigning it to a proto `r#type: i32` field produces a *one-off* wire value: a `TaskApproval` from Rust serializes as `Unspecified` over the wire.

This was masked for the entire project lifetime because no production code path actually emitted a `GateEvent` to the highway stream — `GateController::open_gate` was only used in tests. WA3.5's first-ever production emission tripped it on the very first integration test (`wa3_5_provisional_checkpoint_emits_gate` failed with `left: 6, right: 7`).

**Fix landed.** New `gate_type_to_proto(GateType) -> wacp_v1::GateType` helper in `wacp/crates/wacp-runtime/src/init.rs`. The same shape exists for **every** internal-Rust ↔ proto enum pair where Rust starts at 0 and proto starts at 1 (most of them — see `primitives.proto` `*_UNSPECIFIED = 0` for the pattern). Future emit paths should use a similar helper rather than `as i32`.

**Recommendation (P1).** Audit the other internal-enum-to-proto-int casts in the runtime. Candidates: `SignalType` (already has `proto_to_signal_type` for the reverse direction; the forward direction `signal_type as i32` is used in `fan_out_event` at `init.rs:411` and works only because both enums happen to align after `Unspecified` was prepended on the proto side — `SignalType::Ready = 0` Rust + `SIGNAL_TYPE_UNSPECIFIED = 0` proto means Ready → Unspecified on the wire, which is *also wrong* but currently undetected because no consumer asserts on the discriminant). `WorkspaceState` (similar shape — `init.rs:474` casts `*to as i32`). `TaskStatus` (`init.rs:1086+` already uses an explicit match table — the right pattern). One-pass audit + a tiny clippy-style helper would prevent the next instance.

### 11.2 Cross-crate exhaustive matches on shared enums

Adding `GateType::CheckpointApproval` broke the build of `console-core::event_enricher::gate_type_string` because that function pattern-matches exhaustively on `proto::GateType`. The Rust compiler caught it at compile time — good — but only when console-core was built (cargo built wacp-types first, succeeded, then propagated the new enum down to console-core).

**The structural shape:** the proto types are shared between runtime and console via `wacp-transport::wacp_v1`. Any exhaustive match on a proto enum in the consumer code becomes a forced update site whenever the producer adds a variant. The `_ =>` wildcard avoids the build-break but at the cost of silently-wrong stringification (the new variant would render as e.g. `"unspecified"`).

**Recommendation.** Keep the exhaustive matches — they're the correct pattern. The build-break is the *desired* signal. But document the propagation expectation: "`event_enricher::gate_type_string` is intentionally exhaustive; if a new `GateType` variant lands in `wacp-types`, this match must extend in the same PR." A short comment above the match achieves this. Considered: a workspace-wide check that all `proto::*` enum matches are exhaustive — too invasive for the gain. The compiler does the right thing already.

### 11.3 Async cascade from `Coordinator::handle_event`

WA3.6 made `Coordinator::handle_event` async because the auto-integration path needs to `.await` an mpsc send to the workspace handle's coordinator_tx. This rippled to 6 call sites (2 in init.rs, 4 in tests). Each was a one-line `.await` addition — but the cascade is the kind of thing that's easy to miss when the change is described as "coordinator-only, no runtime changes" (the runtime *does* change, just minimally — every caller needs the `.await`).

**No fix needed**, but worth noting for future spec authoring: **"async means async cascading"**. Any spec that says "make method X async" should explicitly enumerate all call sites or include a grep command in the spec itself. The wiring-strategy-b §3.3.6 should be updated to mention this; WA3.6's coding spec at `wacp/impl/wa3-6-auto-integration.md` lists all six call sites in its §3 to make this concrete for any future similar change.

### 11.4 The same enum-offset trap, second instance

WA3.6 + T7.3 surfaced the same Rust↔proto enum-offset bug as §11.1, this time on `WorkspaceState`. `WorkspaceState::Closed as i32 = 7` (Rust enum starting at `Idle = 0`); `WORKSPACE_STATE_CLOSED = 8` in the proto (after `WORKSPACE_STATE_UNSPECIFIED = 0`). Casting at the `fan_out_event` boundary produced wire frames with `current = 7`, which the Console decoded as `Conflicted`. The session monitor's completion detection compares against `proto::Closed` and silently never fired.

**Fix landed (`8ce249a`).** New `workspace_state_to_proto(WorkspaceState) -> proto::WorkspaceState` helper in `wacp/crates/wacp-runtime/src/init.rs`, used in both arms (StateChanged + Terminated) of `fan_out_event` that emit `WorkspaceStateChange` frames. Mirror of the WA3.5 `gate_type_to_proto` fix.

**Recommendation upgraded to P0.** This is now the **second** time the same bug shape has bitten a different enum on the same code-path style. Other suspects with the same shape:
- `SignalType` — `fan_out_event` Signal arm at `init.rs:411` does `signal.signal_type as i32`. `SignalType::Ready = 0` Rust; `SIGNAL_TYPE_UNSPECIFIED = 0` + `SIGNAL_TYPE_READY = 1` proto. So Ready → Unspecified on the wire. Currently undetected because no Console-side consumer asserts on the discriminant of a Signal frame. Visible the moment one does.
- `TaskStatus` — `init.rs:~1086` already uses an explicit match table, the right pattern. Spot-checks confirm it's correct.
- `EnvelopeState`, `EnvelopePriority`, `EnvelopeOrigin`, `CheckpointStatus`, `Confidence`, `BaseRole`, `MergeStrategy`, `IntegrationMode`, `ConflictType`, `ResolutionStrategy`, `PortRightType`, `GateDecision`, `TrailScope`, `StorageTier`, `ErrorCategory` — all share the Rust-starts-at-0 / proto-starts-after-Unspecified pattern. Anywhere these get cast as `i32` for a wire field is suspect.

A focused one-shot pass would be: `rg "as i32" wacp/crates/wacp-runtime` and audit every result that's setting a proto field. Estimated 30–60 min. Lower-effort than waiting for the third instance to bite.

### 11.5 Discovered patterns from the un-ignore sweep

T7.7 (10 concurrent sessions) and T7.8 (slow WS consumer) surfaced two patterns worth recording:

**T7.7 — RSS measurement is too noisy for in-test assertions.** The original sketch wanted "no monitor task held >50 MB resident". From inside a Tokio test, `procfs::process::Stat::vm_rss` and friends return RSS for the entire test binary, not per-task. There's no clean way to attribute memory to one of N spawned futures without external instrumentation. Replacement signal: the monitor's `JoinHandle` resolving on terminal state. If a per-session monitor were leaking memory, it'd typically also leak the join handle (no terminal exit), so the resolution check is a reasonable proxy. Document this for future "no leak" assertions.

**T7.8 — the loopback TCP buffer absorbs single-checkpoint frame bursts before broadcast can overflow.** The naïve approach (drive a real agent → checkpoint → wait for broadcast Lagged) doesn't work because tokio broadcast capacity 4 + 6 frames generated by one cycle gets entirely absorbed into the 64 KB loopback TCP write buffer. The WS task's `socket.send().await` returns instantly per frame, so the broadcast receiver never falls behind. The deterministic path is to push directly into `SessionMonitorHandle::broadcast_tx` with N >> capacity frames of large payload. Pattern documented in T7.8's commit and worth re-using for any future "broadcast lag" tests.

### 11.6 Performance signals (none new)

- `cargo test -p wacp-coordinator`: 0.82 s for 387 tests (was 0.82 s for 378 — the 9 WA3.5/WA3.6 tests added ~zero runtime cost).
- `cargo test -p wacp-workspace`: 0.30 s for 65 tests (was 0.30 s for 60).
- `cargo test -p wacp-runtime`: 0.83 s for 109 tests (was 0.83 s for 103).
- `cargo test -p console-integration --test llm_stub_e2e`: 0.21 s for 2 scenarios (unchanged from baseline).
- `cargo test -p console-integration` (full suite, post un-ignore sweep): 12 active, 0 ignored; ≈ 2.2 s wallclock for the lifecycle + chaos + cross_session + llm_stub_e2e suites combined.
- No new RSS or wall-time regressions. The auto-integration cache (`HashMap<String, Checkpoint>`) is one entry per active workspace at most — bounded by tree size, not by request rate.

## 12. §13.7.7 D1 — Playwright E2E surfaces the first backend drifts

Same forcing-function pattern as §9 (`console-db`), §10 (`wacp-llm` stub), and §11 (WA3.5 / WA3.6): writing the new test layer turns up latent production gaps. §13.7.7 D1 (`03d0411`) wired the Playwright harness — two `webServer` entries (the new `wacp-mock-runtime` bin + the console binary served with `--frontend-path dist`) plus a smoke spec — and in the course of that wiring two real drifts showed up, plus a chunk of CI-pipeline debt large enough to warrant its own doc. None are §13.7.7 deliverables; all are logged here and/or at `impl/ci-health-2026-04-17.md`, then folded into AUDIT §13.5 at the §13.7.7 D5 closure.

### 12.1 Console binary skips the bootstrap flow — fixed in D2

`console-core::bootstrap` implements `bootstrap_if_needed` + `write_bootstrap_token` per `wcon-auth` §6, with passing unit tests. But `console/src/main.rs::Commands::Serve` never called them — the serve path ran migrations → taxonomy → gRPC pool → startup recovery → AppState → HTTP, skipping the bootstrap check. A fresh console binary launched against an empty DB therefore had no admin user, no bootstrap-token file, and no way to log in. `wcon-vision` BC6 ("no default credentials — bootstrap generates a one-time credential") was silently violated at the binary boundary; only the library-level tests covered it.

**Fixed** in the §13.7.7 D2 commit: `Commands::Serve` now calls `bootstrap_if_needed` after migrations and `write_bootstrap_token` on a `Bootstrapped` result, logging the credential-path at info. The integration console harness (`integration/src/console_harness.rs`) keeps skipping it (defensible — integration tests seed users directly via `console-db::queries::users` fixtures).

Mirror of §2.5 and §9.1: feature was specified and implemented in the library, absent at the composition point. Standard drift shape; fix is additive and non-breaking.

### 12.2 Mock runtime `/v1/verticals` response shape ≠ console's REST-client expectation — fixed in D2

Observed as a warn-level log during D1 smoke-test boot:

```
WARN wacp_console: failed to load taxonomy from runtime — starting with empty index
  error=failed to parse vertical list: error decoding response body
```

Closer read (during D2): the mock returned a 3-field summary (`{id, name, defining_constraint}`), but the console's actual REST-client expectation is a 6-field summary (`console_runtime::rest_client::VerticalSummary` — adds `task_type_count`, `workflow_count`, `tool_count`). The console's 2-step loader then fetches each full manifest via `/v1/verticals/{id}` — the mock's detail endpoint was already correct, only the list summary was wrong. The console was falling back to an empty taxonomy + continuing.

**Fixed** in D2: `console-test-support/src/mock_rest.rs::VerticalListItem` now includes the three count fields, computed from `task_types.len()`, `workflows.len()`, `tool_policies.len()` on the shared `VerticalManifest`. The console's `build_taxonomy` now loads both fixture verticals (`roles=8 tools=9 verticals=2` in the Playwright-run log) and the mock matches the real runtime's contract.

Leaves one design question for later: the real runtime's `GET /v1/verticals` is presumed to return the same 6-field summary — confirm against the live `wacp-runtime` REST server before declaring this drift fully closed. If the real runtime returns something else, perf-opt §9.x-style "pick one representation" applies.

### 12.3 Test-tooling / CI-pipeline debt — filed separately

Not a perf-opt signal; doesn't belong here. D1 surfaced three orthogonal CI failures (mold linker missing on runner image, `pnpm lint` failing pre-typecheck, 55 pre-existing `tsc` errors in test files blocking `pnpm build`) plus pre-existing `cargo fmt` drift. All five CI workflows on `main` have been red since 2026-04-15 — contradicts SEED's "CI green" line. Details, evidence (workflow run IDs), root-cause analysis, and recommended fix order at `impl/ci-health-2026-04-17.md`.

Short version for readers who stop here: **§13.7.7 D3 (Playwright CI stage) is blocked** on at least the linker fix + the `tsc`/test-file split before the stage can turn green. Those fixes are scope-outside §13.7.7 — they'll land in a dedicated cleanup commit before D3, not folded into D3.

### 12.4 Forced-change deadlock — `authenticate_cookie` rejects the change-password route itself — fixed in D2

Surfaced when writing the auth-flows E2E spec. The flow is: admin boots, bootstrap-token login succeeds → `/change-password` page → user fills form → POST `/api/auth/change-password` → **403** with "You must change your password before continuing".

Root cause: `console-core::authenticator::authenticate_cookie` returns `PasswordChangeRequired` when `user.must_change_password == true`. Used by the `Auth` extractor. The change-password route is built on `Auth`, so the very request that should clear the flag gets rejected by the authenticator. Chicken-and-egg: the only route that can un-flag a user is the one route that refuses flagged users.

This bug has shipped unexecuted for the entire forced-change path's lifetime. The library-level `bootstrap_if_needed` test covers creation + `must_change_password=true` insertion; no test exercised the HTTP round-trip of `/api/auth/change-password` under a freshly-bootstrapped identity. Frontend unit tests used mocked API responses. Only an E2E browser test that actually rides the forced-change redirect could catch it.

**Fixed** in D2:
- `authenticate_cookie` keeps its strict behaviour (everything continues to enforce the flag).
- New `authenticate_cookie_allow_pending_change` skips the flag check.
- New `AuthAllowPendingChange` axum extractor wraps the permissive authenticator, cookie-only (bearer tokens are rejected — token-auth'd clients are post-rotation by definition).
- `POST /api/auth/change-password` swaps `Auth` → `AuthAllowPendingChange`.

Mirror of §11.1 (Rust↔proto enum-offset): a surface that looked "tested enough" from unit tests but had a structural latent bug that required an integration-level caller to surface. Adds to the pattern library: **"if a library function is supposed to be idempotent under a specific state, and only one composition path reaches it in that state, test from that composition path — not just the library entry point."**

### 12.5 ProfilesPage — `Create New` click unmounts React (deferred)

Surfaced while writing the golden-path E2E spec. The test landed on `/profiles`, asserted the sidebar rendered, clicked the `Create New` button (which is the only UI affordance to open the new-profile form), and the test's post-click h2 assertion timed out. Diagnostic capture showed:

- BEFORE click: `h2 texts: ['Profiles']`, URL `/profiles`, normal render.
- AFTER click: `h2 texts: []`, URL `/profiles`, page content collapsed to ~395 bytes (just `<!DOCTYPE html><html lang="en"><head>…`). React has unmounted the entire application.

The click's handler (`ProfilesPage.tsx::handleNew`) is pure state mutation — `setSelectedId(null) + setCreating(true) + setForm(EMPTY_FORM) + setShowDelete(false) + setShowVersions(false)` — and shouldn't throw on its own. The unmount implies a render-path exception that React 19 surfaces by blanking the root. No ErrorBoundary is installed, so the exception's origin is invisible from outside; inspecting browser console logs during a `--headed` Playwright run would localise it.

**Not fixed in D2.** Scope stays on the §13.7.7 deliverable; the spec works around it by asserting only the pre-click state (Profiles h2, Create New button visible, "No profiles found." empty-state text). Unskipping the Create New → form-render path needs a bisect pass:

1. Run `pnpm dev` locally + open the profiles page + click Create New + watch the DevTools console for the uncaught exception.
2. Likely candidates: (a) one of the `useEffect` hooks at `ProfilesPage.tsx:205` firing with unstable deps (symmetrical to §3.3's anticipation); (b) a render-path cast like `profilesQuery.data as ProfileSummary[]` hitting an unexpected shape from a query that returns `{items: [...]}` vs `[...]`; (c) one of the form-field inputs at `:380-:466` reading an undefined value from `form` — none obvious from the static code, all plausible at runtime. 30–60 min investigation.

This drift is the fifth F-series–shaped signal after §2.5 F8 (RefusalPanel / InjectionBar), F10 (Notifications stub), §9 (console-db schema-type mismatch), §11 (runtime enum-offset), and the D2 auth deadlock — continuing to support the claim that **writing cross-layer tests is the forcing function for finding latent bugs** in surfaces that have never had an end-to-end user flow exercised against them.

## 13. §13.7.8 I1–I5 — integration + chaos findings

Opened up-front so each §13.7.8 I-suite has a home to file anything it surfaces. Leave subsections empty until something lands — the empty header is a drift-filing *prompt*, not a claim.

Patterns to actively look for while writing these suites (priority order per `AUDIT-2026-04-15.md` §13.7.8 plan §4):

1. **Rust-enum-as-i32 offset** (§11.1, §11.4) — two instances already caught on `GateType` + `WorkspaceState`; the §11 P0 audit recommendation stands. Most likely to trip I2 (recovery decodes `WorkspaceState` from runtime responses) and I5 (vertical manifest decode).
2. **Schema-vs-type drift** (§9.1) — `session_assignments.profile_id` `Option<String>` vs `NOT NULL`. I2 seeds DB rows directly; any column whose runtime value is never `None` while the struct says `Option<T>` is a candidate.
3. **Cleanup-that-leaks** (§3.3, §11.5) — the WS chaos suite (I4) is the whole point of this row.
4. **Spec-vs-impl drift** (§2.5, §12.1, §12.4) — features described in the audit but not wired end-to-end. I3's runtime-auth matrix already flags one (no actual api-key vs. session distinction on the runtime wire today).
5. **Async cascade from signature change** (§11.3) — not expected in this workstream but worth flagging if any shared helper's signature changes mid-suite.

### 13.1 I1 — `launch_failure_matrix.rs` (landed, no new bugs surfaced)

Ten tests across the full SubmitGoal / Decompose / Dispatch error axis plus the three rollback permutations (single-root, partial-failure tolerated, total-failure still terminates). All ten pass green in 0.35 s, which is the notable observation — nothing the suite exercises was broken.

**What it proves (regression guard).**
- Every public `reason_code()` output is shape-correct for its error kind — no cross-wiring where a Dispatch failure produces a `submit_goal`-shaped reason.
- Rollback is exactly `O(workspaces_created_so_far)` — dispatch failure on task 1 aborts [root]; on task 2 aborts [root + task_1_ws]; on last of N aborts [root + N-1 task workspaces]. Off-by-ones here would be subtle and costly; now asserted.
- `rollback_partial_failure_does_not_propagate` confirms the launcher's "abort failure is logged at warn but tolerated" contract (launcher `:391`). A future change that accidentally escalated abort-failure to return-error would be caught.
- `rollback_total_failure_does_not_hang_or_panic` wraps the launch in a 5 s `tokio::time::timeout`. The launcher does one abort per workspace (no retry loop), so a total failure finishes in O(workspaces) time. Any future change that introduces retry-with-backoff on abort failures would surface here as a timeout-triggered test failure — exactly the "make a hang fail fast" pattern §7 recommends.

**InjectableCoordinator API widening (carried back into §13.6).** The initial P0 queue was `VecDeque<Status>` — strictly fail-next-N. Two scenarios in I1 (dispatch-fails-on-task-2, dispatch-fails-on-task-3) need "pass the first K calls, then fail" — a pattern the queue couldn't express because an empty queue forwards everything and there was no way to enqueue "forward". Widened to `VecDeque<Option<Status>>` with a pair of `pass_dispatch()` / `pass_abort()` helpers. Tests express their scripts as an explicit sequence: `pass_dispatch(); pass_dispatch(); inject_dispatch(Unavailable)` means "forward twice, fail the third call".

**`GrpcPool::new()` already returns `Arc<Self>`.** Spent ~30 s re-discovering. Noted for future test authoring: `let pool = GrpcPool::new(...); pool.connect().await;` — no outer `Arc::new(...)`.

**No perf signal.** Per-test walltime is bounded by the runtime-harness spawn (~200 ms) + a single coordinator call roundtrip (~10 ms). Each test serially spawns a runtime child, so the 10-test suite walltime is dominated by child-spawn cost. Cache not relevant at this scale. If I2 + I3 later push the full `cargo test -p console-integration` past 20 s, consider sharing a runtime child across tests within a file (non-trivial — the rollback tests mutate runtime state).

### 13.2 I2 — `recovery_matrix.rs`

*Reserved for findings; empty until the suite lands.*

### 13.3 I3 — `auth_matrix.rs`

*Reserved for findings; empty until the suite lands.*

### 13.4 I4 — `ws_chaos.rs`

*Reserved for findings; empty until the suite lands.*

### 13.5 I5 — `taxonomy_reload.rs`

*Reserved for findings; empty until the suite lands.*

### 13.6 Shared infrastructure (P0, `78a7fab` + I1 follow-up)

`InjectableCoordinator` — reusable failure-injection mock `CoordinatorService`. Per-RPC `VecDeque<Option<Status>>` queues for `SubmitGoal`/`Decompose`/`Dispatch`/`AbortWorkspace`; empty queue forwards; `Some(status)` short-circuits; `None` forwards explicitly (lets tests script "forward first K, fail the K+1th"). Generalizes WA5's `failure_proxy::FailureProxy` (which only injects on `Dispatch`).

API:
- `inject_submit_goal(status)` / `inject_decompose(status)` / `inject_dispatch(status)` / `inject_abort(status)` — queue a failure.
- `pass_dispatch()` / `pass_abort()` — queue an explicit forward. Needed when a later entry in the queue is a failure and the test wants to drive the first N calls through the real runtime first. Added as part of I1 follow-up once the queue-ordering need surfaced.
- `submit_goal_count()` / `decompose_count()` / `dispatch_count()` / `abort_count()` — per-RPC assertion counters.

Two-test smoke (`tests/mock_coordinator_smoke.rs`) verifies both legs: forward path with empty queue + inject path with one `Unavailable` pushed. Wall: 0.12 s. `cargo clippy -p console-integration --all-targets -- -D warnings` clean after switching the `match` arms to `if let Some(Some(status)) = ...` per `clippy::single_match`.

Cargo.toml edit — moved `tonic`, `tokio-stream`, `wacp-transport` from `[dev-dependencies]` to `[dependencies]` (the new mock lives in `src/` so tests-only deps wouldn't satisfy it). No behaviour change for existing suites; build time unchanged.

---

*Working document. Update as optimizations land or new signals appear. Not a spec — intent is to guide attention, not fix scope.*
