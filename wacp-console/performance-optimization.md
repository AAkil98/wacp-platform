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

---

*Working document. Update as optimizations land or new signals appear. Not a spec — intent is to guide attention, not fix scope.*
