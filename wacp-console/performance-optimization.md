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

## 3. App-level patterns worth auditing

Direct grep-evidence from the current tree (as of `b17ae49` on `dev`).

### 3.1 Component size — mount cost and re-render radius

The four largest single components:

| File | LOC | Concerns |
|---|---|---|
| `src/surfaces/sessions/Wizard.tsx` | 802 | 6-step wizard, all steps rendered in one component tree; 17 inline `CSSProperties` constants + 2 style functions; local `useState` per step. The next test build-out (`AUDIT-2026-04-15.md` §13.7.2) will exercise this — expect measurable mount cost. |
| `src/surfaces/profiles/ProfilesPage.tsx` | 530 | Sidebar + editor + version panel + delete modal in one component; 14 inline style constants + 1 style function; 11-field form via `useState`. |
| `src/surfaces/discovery/VerticalsTab.tsx` | 494 | Heavy nested-table renderer; no memoization on rows. |
| `src/surfaces/discovery/RolesTab.tsx` | 366 | Same pattern as Verticals. |

**Recommendation.** Wizard and ProfilesPage would each benefit from being decomposed into the natural inner components (step-per-file for Wizard; sidebar/editor/versions/delete for ProfilesPage). The reasons are identical for tests and production: smaller mount radius, cheaper re-renders, easier to profile in DevTools, and memoization boundaries become available.

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
4. **Decompose Wizard.** Six steps as six files, composed by a thin `Wizard` shell. Effort: 4–6 h. Payoff: identical reasoning to ProfilesPage; also makes §13.7.2 test file a natural one-file-per-step layout.
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

## 6. Watch-list — what would indicate regression

Signals to act on when they appear:

- **Single test file RSS > 500 MB** during `npm run test:isolated`. Current baseline (post-`d63648a`): every file < 400 MB; session peak 323 MB. A jump above 500 MB means a new component (likely Wizard test files from §13.7.2) has reintroduced an unstable-ref pattern or is mounting an unexpectedly heavy subtree.
- **`npm run test:isolated` walltime > 90 s.** Current baseline: 54 s. Per-file mean ~3.4 s. Any file breaking 10 s deserves a look.
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

## 8. Backend — not yet investigated

Testing coverage for the Rust crates (`wacp-console/crates/*`) lives under §12.2 / §13.4 of the audit and has not produced runtime-performance signals in this session. The natural next pass (after §13.7 closes) is to run the T1–T10 coverage benchmarks under `cargo bench` for the hot paths — `console-core::session_monitor` (broadcast fan-out), `console-core::session_launcher` (coordinator sequence), `console-api::middleware` (per-request auth/CSRF). Any findings there should be captured back into this document under a new §9.

---

*Working document. Update as optimizations land or new signals appear. Not a spec — intent is to guide attention, not fix scope.*
