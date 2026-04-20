---
id: wacp-frontend-perf
type: impl
status: draft
created: 2026-04-20T04:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, frontend, refactor, performance, react]
depends_on: [HEALTH-LOG-3, HEALTH-LOG-4]
---

# Frontend performance + component decomposition

> **Triggering findings:** HEALTH-LOG.md §3 (app-level patterns worth auditing) + §4 (optimization roadmap) — "fix everything" scope per user 2026-04-20. Closes the full frontend-perf cluster: useEffect dep audit, module-scope style records, Wizard step extraction, ProfilesPage decomposition, react-hook-form swap, route-level lazy loading, virtualization, and the `eslint-plugin-jsx-a11y` preventive.
> **Target branch:** `refactor/frontend-perf` from `dev`. Multi-commit, ff to dev when phase-green.
> **Rough effort:** ~12–17 h across 8 phases. P1 quick wins first (clean baseline for P2), then structural work in dependency order, parallelizable tail.
> **Not in scope:** backend perf (HEALTH-LOG §8 "not yet investigated"); §9 console-db drift (separate workstream); LLM stub observations §10 (deferred with explicit rationale in-file); WA3.5/3.6 async cascade §11.3 (backend, unrelated).

## 1. Goal & Motivation

HEALTH-LOG §3.1 identified two component-size issues that should have been tackled during the stabilization work but were deferred: `Wizard.tsx` declares six step renderers inside its own function body (causing mount/unmount churn on step transitions), and `ProfilesPage.tsx` is a 538-line monolith that every form-field edit re-renders end-to-end. §3.2 / §3.3 / §3.4 layered on the per-render-allocation + `useEffect` dep + form-state patterns. §4 packaged them as a priority-ordered roadmap (P1/P2/P3) but nothing has landed yet — the doc is the tier-1 `impl/` checklist but the phased execution was waiting on a `v0.1.0`-eligible tree.

With every AUDIT §13.7 package + closeout-plan P1–P5 + Bucket-B refactor (including the two follow-ups) now on `main` at `0767f45` as of 2026-04-20 ff, the frontend is the last structural cluster blocking a clean v0.1.0 posture. This plan executes the full §3+§4 agenda in one sequenced branch, with phase-level commits so future bisects land at a specific improvement rather than an 800-line diff.

**If not done:** the next new component copies the nested-fn-step pattern (HEALTH-LOG §6 watchlist would trip but only after shipping), the "recurring a11y gap" pattern keeps surfacing per RTL suite (two instances already: `d71c4fe`, `e870018`), and the react-hook-form migration drifts past v0.1.0 into the post-launch window where form-spec changes compound.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| **F1** | `useEffect` dep audit — walk 6 files, prove stable-ref or rewrite to value-compare deps | ~1–2 h | — | no `useEffect` whose dep array contains a non-memoized object/array in the 6 files; RTL suites still green |
| **F2** | Module-scope style records — swap 4 functional `CSSProperties` helpers for `Record<Variant, CSSProperties>` | ~15 min | — (parallel to F1) | 0 call sites matching `const \w+ = \(.*: .*\): React\.CSSProperties => \(` in the 4 target files; per-render allocation smell gone |
| **F3** | Wizard step extraction — 6 nested `*Step` fn components → module-scope (or sibling file), state passed via props | ~1–2 h | F1 landed (shared `Wizard.tsx` file) | `Wizard.tsx` has no `function *Step()` declarations inside the main component body; 41 Wizard RTL tests pass; E2E unchanged |
| **F4** | `ProfilesPage` decomposition — `ProfilesSidebar` + `ProfileEditor` + `ProfileVersionsPanel` + `DeleteProfileModal` siblings | ~3–4 h | F1 landed (shared file); F2 preferred first | `ProfilesPage.tsx` < 200 lines (container only); each new subcomponent ≤ 250 lines; 49 ProfilesPage RTL tests reshaped but pass count unchanged; golden-path E2E still green |
| **F5** | `react-hook-form` in `ProfileEditor` — field-level re-render radius | ~2–3 h | F4 landed | `ProfileEditor` uses `useForm` + `register`; field edits no longer re-render sibling fields (RTL mount-count assertion) |
| **F6** | Route-level lazy loading — `React.lazy()` + `Suspense` per surface in `App.tsx` | ~1 h + eval | F1–F4 landed (otherwise extracted modules churn lazy boundaries) | initial bundle ≥ 20% smaller; `dist/assets/*.js` emits per-route chunks; cold-load of non-initial surface < 200 ms (local) |
| **F7** | Virtualization — `react-virtuoso` for profile/user/session list views | ~2–4 h | F4 landed (ProfilesSidebar is the natural home for the profile-list virtualization) | scroll render time < 16 ms at 1000 rows (React Profiler); a11y `role="list"` + keyboard nav preserved; tests updated for virtualized containers |
| **F8** | `eslint-plugin-jsx-a11y` — fail-fast on the recurring a11y label-binding gap (§2.4) | ~30 min | — (parallel to any phase) | plugin installed + `recommended` ruleset in `eslint.config.*`; `pnpm lint` exits 0; no new failures after F3–F7 code churn |

## 3. Deliverables — per phase

### 3.1 F1 — `useEffect` dep audit

Per HEALTH-LOG §3.3 and §4 item 1. Target files:

- `wacp-console/frontend/src/surfaces/profiles/ProfilesPage.tsx`
- `wacp-console/frontend/src/surfaces/sessions/Wizard.tsx`
- `wacp-console/frontend/src/surfaces/settings/SettingsPage.tsx`
- `wacp-console/frontend/src/surfaces/admin/AuditLogPage.tsx`
- `wacp-console/frontend/src/surfaces/admin/UsersPage.tsx`
- `wacp-console/frontend/src/surfaces/sessions/SessionsPage.tsx`

**Per-file procedure:**
1. Grep the file for `useEffect(` occurrences.
2. For each, classify the dep array: pure-primitive (string/number/bool → OK), React-Query-return (`.data` object — check if it's handed in or dereferenced), derived object via `useMemo` (check memo deps), module-scope constant (OK), bare object/array reference.
3. For every bare object/array entry: rewrite to value-compare-via-key (`[obj.id, obj.version]`) or confirm stability from React-Query / `useQueryClient` guarantees.
4. Run the surface's RTL test to confirm no behavior change.

**Commit strategy:** one commit per file (`fix(frontend): §3.3 useEffect dep audit — X.tsx`). If a file has zero offenders, skip the commit.

### 3.2 F2 — Module-scope style records

Per HEALTH-LOG §3.2 and §4 item 2. Target call sites:

| File | Line | Helper signature |
|---|---|---|
| `src/surfaces/admin/UsersPage.tsx` | 102 | `badge(disabled: boolean)` |
| `src/surfaces/profiles/ProfilesPage.tsx` | 47 | `listItem(selected: boolean)` |
| `src/surfaces/sessions/Wizard.tsx` | 88 | `stepItem(state: "active" \| ...)` |
| `src/surfaces/sessions/Wizard.tsx` | 151 | `card(selected: boolean)` |

**Pattern:**

```tsx
// Before
const listItem = (selected: boolean): React.CSSProperties => ({
  padding: "12px 16px",
  background: selected ? "var(--color-accent)" : "transparent",
  // ...
});

// After
const LIST_ITEM_STYLE: Record<"selected" | "unselected", React.CSSProperties> = {
  selected:   { padding: "12px 16px", background: "var(--color-accent)", /* ... */ },
  unselected: { padding: "12px 16px", background: "transparent",         /* ... */ },
};
// Callsite: style={LIST_ITEM_STYLE[selected ? "selected" : "unselected"]}
```

**Commit strategy:** single commit across all 4 sites (`fix(frontend): §3.2 module-scope style records`).

### 3.3 F3 — Wizard step extraction

Per HEALTH-LOG §3.1 and §4 item 4. Current offenders (`Wizard.tsx:408, 438, 470, 501, 514, 581`):

- `SelectVerticalStep`
- `SelectWorkflowStep`
- `AssignProfilesStep`
- `ContextStep`
- `BudgetOverridesStep`
- `ReviewLaunchStep`

**Mechanics:** each is currently a function declaration inside `Wizard`'s body that closes over Wizard's state (`state`, `dispatch`, hook return values). Extract each to module scope (in the same file or a sibling `Wizard/steps/*.tsx`), lift the closed-over values to explicit `props`, update the `case` arms in the step dispatcher at `:398-403` to pass those props.

**Decision:** keep step components in the same `Wizard.tsx` for this pass (sibling `steps/` folder would trigger broader import reshuffle + `.claude/skills/blast-radius` review). Module-scope in the same file gets the fix cheaply; a follow-up can shard the file later if navigability warrants.

**Commit strategy:** one commit per step (6 commits) OR one batched commit if the refactor is truly mechanical and the review surface is small. Prefer batched — reviewers bisecting into this later want "extracted all 6 steps" as one anchor, not six interleaved renames.

### 3.4 F4 — `ProfilesPage` decomposition

Per HEALTH-LOG §3.1 and §4 item 3. Target split:

- `ProfilesPage.tsx` (container, ~150 lines after split) — routing, top-level state, sidebar + editor layout
- `ProfilesSidebar.tsx` — search + list + Create New button
- `ProfileEditor.tsx` — form (11 fields today, F5 lands `react-hook-form` here next)
- `ProfileVersionsPanel.tsx` — version history + rollback
- `DeleteProfileModal.tsx` — delete confirmation

**Tests are already partitioned this way.** `ProfilesPage.sidebar.test.tsx`, `ProfilesPage.edit.test.tsx`, `ProfilesPage.create.test.tsx`, `ProfilesPage.actions.test.tsx`, `ProfilesPage.import-versions.test.tsx` map naturally onto the new subcomponents — each test file currently stubs all hooks and asserts a slice of behavior, so after the split each test can import + render the slice directly. Test-helpers in `ProfilesPage.test-helpers.tsx` stay shared.

**Mechanics:**
1. Extract each subcomponent with explicit props.
2. Move slice of state down where it naturally belongs (e.g., `showDelete` → `DeleteProfileModal`'s open/close ownership).
3. Container keeps `selectedId` + `creating` as the only cross-subcomponent state.
4. Each sub-test's `vi.mock('../../api/hooks/index', …)` stays the same; `render(<ProfilesSidebar …/>)` replaces `render(<ProfilesPage/>)` where the test only exercises the sidebar slice.

**Commit strategy:** one commit per subcomponent (4 commits), plus a final "delete old inline code, verify tests still green" commit. Bisectable in case one sub-extraction introduces a subtle state-ownership bug.

### 3.5 F5 — `react-hook-form` in `ProfileEditor`

Per HEALTH-LOG §3.4 and §4 item 6. Post-F4 dependency — `ProfileEditor` is the target.

**Mechanics:**
1. `pnpm add react-hook-form` in `wacp-console/frontend/`.
2. Replace `useState(EMPTY_FORM)` + `updateField` with `useForm<ProfileForm>({ defaultValues: EMPTY_FORM })`.
3. Every `<input value={form.x} onChange={(e) => updateField("x", ...)} />` becomes `<input {...register("x")} />`.
4. Radio groups (autonomy + visibility) use `<Controller>` since they render as group components.
5. Submit handler wraps the existing `createMut.mutate` / `updateMut.mutate`.

**Verification:** RTL test asserting that editing "Name" does not re-render the "Description" subtree (use `React.Profiler` wrapper or count `onRender` calls).

**Commit strategy:** single commit (`refactor(frontend): §3.4 react-hook-form in ProfileEditor`).

### 3.6 F6 — Route-level lazy loading

Per HEALTH-LOG §4 item 5. Target: `wacp-console/frontend/src/App.tsx` surfaces.

**Mechanics:**

```tsx
// Before
import { ProfilesPage } from "./surfaces/profiles/ProfilesPage";

// After
const ProfilesPage = lazy(() => import("./surfaces/profiles/ProfilesPage").then(m => ({ default: m.ProfilesPage })));
```

Wrap the `<Routes>` block in `<Suspense fallback={<PageLoadingSpinner/>}>`. Apply to every surface: discovery/profiles/sessions/oversight/settings/admin.

**Verification:** `pnpm build` output shows per-route chunks; bundle-analyzer confirms initial chunk shrinks by ≥ 20%; `pnpm test:e2e` still green.

**Commit strategy:** single commit.

### 3.7 F7 — Virtualization

Per HEALTH-LOG §4 item 7. **Scope note:** doc says "currently not needed — profile/user/session lists are dozens at most — revisit when real tenant data shows up." User 2026-04-20 directive was to fold it in anyway for future-proof posture before v0.1.0.

**Target lists:**
- Profile sidebar (post-F4 `ProfilesSidebar`)
- User list (`UsersPage`)
- Session list (`SessionsPage`)

**Library choice:** `react-virtuoso` — zero-config for variable-height items, built-in keyboard navigation, screen-reader support via `role="list"` / `role="listitem"` + ARIA live regions. Alternative `react-window` is faster at fixed-height but requires manual ARIA wiring.

**Mechanics:**
1. `pnpm add react-virtuoso`.
2. Replace `{items.map((item) => <Row key={item.id} … />)}` with `<Virtuoso data={items} itemContent={(_, item) => <Row … />} />`.
3. Preserve current list container styles + empty-state + loading-state branches.
4. Update RTL tests: `getAllByRole("listitem")` still works; `getByText(itemN)` may need scroll-into-view (Virtuoso renders off-screen items lazily). Tests for "list has N items" should assert against the data length, not the DOM node count.

**Verification:** React Profiler on a 1000-row synthetic dataset shows commit time < 16 ms per interaction; keyboard tab + arrow-key navigation still works; screen reader announces row content.

**Commit strategy:** one commit per list surface (3 commits).

### 3.8 F8 — `eslint-plugin-jsx-a11y`

Per HEALTH-LOG §2.4 (recurring pattern signal). Preventive, parallel to any phase.

**Mechanics:**
1. `pnpm add -D eslint-plugin-jsx-a11y`.
2. Add `"plugin:jsx-a11y/recommended"` to `extends` in `eslint.config.*`.
3. Run `pnpm lint` and fix whatever surfaces. Known prior instances (`d71c4fe` + `e870018`) already fixed, so the plugin should come up clean — but its existence prevents the *next* instance from shipping.

**Commit strategy:** single commit (`chore(frontend): §2.4 add eslint-plugin-jsx-a11y recommended ruleset`).

## 4. Acceptance Criteria

### F1
- [ ] No `useEffect` in the 6 target files has a dep array entry that is an unmemoized object/array reference.
- [ ] Per-file RTL test pass count unchanged vs pre-branch baseline.
- [ ] Golden-path E2E spec still green.

### F2
- [ ] 4 functional `CSSProperties` helpers replaced with module-scope records.
- [ ] `rg 'const \w+ = \(.*: .*\): React\.CSSProperties => \(' wacp-console/frontend/src/surfaces/` returns zero matches.
- [ ] Per-file RTL test pass count unchanged.

### F3
- [ ] `Wizard.tsx` has zero `function *Step(` declarations inside the body of the main `Wizard` component.
- [ ] 41 Wizard RTL tests pass (pre-branch baseline).
- [ ] E2E specs still green.

### F4
- [ ] `ProfilesPage.tsx` < 200 lines; 4 new subcomponent files exist.
- [ ] Each subcomponent ≤ 250 lines.
- [ ] 49 ProfilesPage RTL tests pass (count unchanged; reshaped to target subcomponents where applicable).
- [ ] Golden-path E2E Create-New → form-render path still green.
- [ ] `.file-size-allowlist` TS section unchanged or shrunk.

### F5
- [ ] `ProfileEditor` uses `useForm` + `register` / `Controller`.
- [ ] RTL mount-count assertion: editing "Name" does not re-render "Description" subtree.
- [ ] ProfileEditor RTL tests pass.
- [ ] `react-hook-form` appears in frontend `package.json` dependencies.

### F6
- [ ] `App.tsx` uses `lazy()` + `Suspense` for every non-initial surface.
- [ ] `pnpm build` emits per-route chunks (e.g., `ProfilesPage.<hash>.js`).
- [ ] Initial chunk (`index-*.js`) ≥ 20% smaller than pre-branch baseline.
- [ ] `pnpm test:e2e` all specs pass.

### F7
- [ ] 3 list surfaces use `<Virtuoso>` (or equivalent).
- [ ] React Profiler on 1000-row synthetic dataset: commit time < 16 ms.
- [ ] Keyboard navigation preserved (manual check + RTL + E2E).
- [ ] Tests for list-length assert against data length, not DOM nodes.

### F8
- [ ] `eslint-plugin-jsx-a11y` in frontend `devDependencies`.
- [ ] ESLint config extends `jsx-a11y/recommended`.
- [ ] `pnpm lint` exits 0 on the branch tip.

**All eight phases ticked → plan eligible for `archive-plan` skill.**

## 5. Risks / Open Questions

- **F1 scope creep.** The useEffect audit may surface latent bugs beyond the 6 named files. Budget one "found and fixed beyond scope" commit; anything larger defers to a follow-up plan.
- **F3 — commit granularity.** Single batched commit vs six per-step commits. Defaulting to batched ("extracted all 6 steps"); flip if review surface demands granular bisect.
- **F4 — test reshape cost.** The 49 ProfilesPage RTL tests are currently one-file-per-sub-feature. If the reshape to target subcomponents reveals bugs in the tests (shared state that was hiding the coupling), budget another ~1 h on top of the 3–4 h estimate.
- **F5 — `react-hook-form` and radio groups.** The autonomy + visibility radio groups currently render as custom `role="radiogroup"` containers. `<Controller>` handles this but requires a wrapper component; if that turns ugly the fallback is to keep radio state in parent `useState` and hybridize with `useForm` for the text fields.
- **F6 — lazy boundary noise.** If F3/F4 are still in flight when F6 lands, the lazy chunks will shift. Preferred ordering keeps F6 after F3+F4 so chunk boundaries stabilize.
- **F7 — Virtuoso a11y.** Virtuoso's ARIA wiring is good but not bulletproof for screen readers on < 100 rows (some readers interrupt the user's current position when rows re-mount during scrolling). Mitigation: only enable Virtuoso when `items.length > 50`; fall back to plain `.map` below that threshold.
- **F8 — retroactive failures.** The plugin may flag issues beyond the known 2 instances. Budget one additional commit for the sweep; scope out if the sweep would drag the branch past 20 commits.
- **Branch sprawl.** 8 phases × 2–4 commits avg = 16–32 commits on one branch. Acceptable per git-strategy §5.2 (ff to dev preserves anchors) but watch the reviewer-budget tradeoff. Cut point: if the branch exceeds 25 commits or 3 days, split F6/F7/F8 to a sibling `refactor/frontend-perf-2` branch.
- **TanStack Query compatibility.** None of the phases touch the query layer directly, but F5's `react-hook-form` integration must not bypass the mutation handlers. Manual test: create a profile, reload — the new profile appears in the list.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| HEALTH-LOG-3 | §3 App-level patterns worth auditing (platform root) | content source of truth for F1–F5 |
| HEALTH-LOG-4 | §4 Optimization roadmap — priority order | phase ordering + effort estimates |
| HEALTH-LOG-2.4 | §2.4 A11y label-binding gap recurring pattern | F8 trigger |
| HEALTH-LOG-6 | §6 Watch-list — regression signals | post-landing validation targets |
| wacp-git-strategy | Git Strategy (`impl/git-strategy.md`) | branch naming, commit convention, ff ceremony |
| wcon-vision | Product Vision (`wacp-console/specs/`) | F7 virtualization tenant-scale justification |

## 7. Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| (plan scaffold) | _(this commit)_ | 2026-04-20 | direct-to-dev before branch creation per git-strategy §3.1 |
| F1 useEffect audit | — | — | 6 files, one commit each or skip if clean |
| F2 style records | — | — | single commit across 4 call sites |
| F3 Wizard step extraction | — | — | batched by default |
| F4 ProfilesPage decomposition | — | — | 5 commits (4 subcomponents + cleanup) |
| F5 react-hook-form ProfileEditor | — | — | |
| F6 route-level lazy loading | — | — | after F3+F4 so chunk boundaries settle |
| F7 virtualization | — | — | 3 commits (one per list surface) |
| F8 eslint-plugin-jsx-a11y | — | — | parallel to any phase |

## 8. Sequencing notes

**Preferred order:** F1 → F2 → F3 → F4 → F5 → F6 → F7 → F8.

- **F1 + F2 first.** Clean baseline; the next phases touch the same files.
- **F3 before F4.** Wizard extraction is cheap (~1–2 h) and lands a visible win before the bigger F4 effort. Also decouples review surface: F3 lands alone, F4 lands alone.
- **F4 before F5.** `react-hook-form` only makes sense against the decomposed `ProfileEditor`.
- **F4 before F6.** Lazy-chunk boundaries should settle post-decomposition.
- **F4 before F7.** The ProfilesSidebar virtualization is cleaner against the split.
- **F8 parallel.** Orthogonal; land whenever review bandwidth permits.

**Alternative parallel ordering if multi-session:** F1 + F8 as session-1 (~2 h, independent); F2 + F3 as session-2 (~2 h, shared file); F4 + F5 as session-3 (~5–7 h, dependency chain); F6 + F7 as session-4 (~3–5 h, lazy-chunk-sensitive).

---

*Scaffolded 2026-04-20 by Claude Opus 4.7 (1M context) at user's "we are fixing everything" directive, post-closeout-plan archive. Consolidates HEALTH-LOG §3 + §4 + §2.4 recurring a11y signal + §4.7 virtualization (explicitly folded in per user 2026-04-20) into a single sequenced branch plan.*
