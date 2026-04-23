---
id: wacp-ux-polish-pre-v0.1.0-plan
type: impl
status: final
created: 2026-04-23T02:30:00
revised: 2026-04-23T18:30:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, a11y, ux, onboarding, frontend]
depends_on: [wacp-integration-deferred-scenarios-plan]
---

# UX Polish Pre-v0.1.0 — Plan

> **Triggering finding:** `ROADMAP.md` §Pre-`v0.1.0` > "UX polish" subsection — three bullets (keyboard-nav a11y sweep; empty-states + error-states normalization; onboarding flow). The first is stale (Track A already closed via `f0527d7`); this plan replaces it with an expanded a11y audit beyond keyboard-nav, folds the other two in, and strikes the stale ROADMAP bullet in closeout.
> **Target branch:** `refactor/ux-polish-pre-v0.1.0` (topic).
> **Rough effort:** ~6.5–10.5h across 6 execution phases + closeout — medium confidence. Recon-heavy; P0 calibrates P1–P4 scope.
> **Not in scope:** pre-v0.1.0 items other than the three UX polish bullets (OCI publication + `:latest` regex + coverage floor are independent), LLM-assisted onboarding tutorials, i18n.
>
> **Scope amendment 2026-04-23 (post-P0, user-authorized):** color-contrast brought into P1 scope after P0 recon showed the root cause is a single design token (`--color-text-muted` in `src/index.css`), not a theme-wide redesign. Two-line token bump + ~2–3 surgical surface fixes absorbs the 16 axe-flagged nodes. Sizing delta ~30–60 min on P1.

## 1. Goal & Motivation

ROADMAP flags three UX polish items as pre-`v0.1.0` blockers. One is already landed (Track A keyboard-nav sweep via `f0527d7`, a finding this plan's recon confirms); two remain. Bundling them into a single plan rather than three separate small ones is justified because they share a single theme (surface-level UX quality), they share infrastructure (ARIA live regions + shared components + focus-management hooks), and a natural dependency chain flows one way (A's focus-management patterns → B's error-banner → C's first-run `/setup` screen). Executing them as separate plans would duplicate recon and cause the second + third to retrofit patterns the first could have codified.

The motivation *now* — as opposed to post-v0.1.0 — is that v0.1.0 is the first tagged release and the first public-facing moment where a new operator's onboarding experience + surface polish shape perception. Today a fresh admin must read container logs to discover their bootstrap credential; empty lists render ad-hoc muted-text `<p>` elements with no visual affordance; form errors surface with inconsistent shapes and no screen-reader announcement. Each of these lands under the "small-papercut" category that compounds into a "this feels unfinished" first impression. All three are cheaply fixable with no protocol or backend redesign.

If not done pre-v0.1.0: the a11y gap makes the console inaccessible to screen-reader operators (compliance + adoption risk); the empty/error inconsistency bakes in conventions that surfaces copy forward; the onboarding gap makes the quick-start docs dependent on `docker logs | grep BOOTSTRAP` which is brittle across container runtimes. The cost of deferral is all three get baked into the "that's how it works" substrate and become 10× harder to unwind after tag.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| P0 | Recon: enable full `eslint-plugin-jsx-a11y` rule set at `warn` + run axe-core against dev build + inventory empty/error-state variants across surfaces | ~1h | — | Findings captured in-plan §5 Open Questions (triage table); scope for P1–P4 frozen |
| P1 | A.1 — core a11y fixes (per-rule eslint violations + axe-flagged issues in the "quick-fix" bucket) + promote full rule set from `warn` → `error` | ~1.5h | P0 | `pnpm lint` clean with expanded ruleset at `error`; P0's triage "quick-fix" rows all ticked |
| P2 | A.2 — focus-management infrastructure (modal focus trap utility, route-change focus hook, skip-link, ARIA live-region component) | ~1.5h | P1 | Manual keyboard traversal of profile studio + session launcher passes; axe-core re-run clean on focus-order + landmarks dimensions |
| P3 | B.1 — extract `<EmptyState>` + `<ErrorBanner>` shared components with committed convention + RTL tests | ~1h | P2 (live-region infra) | Both components at `src/components/`, RTL tests passing, Storybook-style render matrix in test file |
| P4 | B.2 — sweep ~10–12 surfaces to adopt the new components; delete inline patterns | ~1.5h | P3 | Grep of `No .* recorded\|No .* found` inline `<p>` variants across `src/surfaces/` returns zero matches; every surface renders `<EmptyState>` or `<ErrorBanner>` |
| P5 | C — onboarding UI: zero-user detection endpoint + `/setup` route + token-display "save-this" screen + login-page branch + Playwright spec | ~2–3h | P3 (patterns) + P4 (conventions) | Playwright `first-run.spec.ts` (new) green; manual fresh-DB boot lands on `/setup`, displays token, continue-to-login → forced-change → admin session; README quick-start updated to reference the UI path |
| P6 | Closeout — ROADMAP strike-through (a11y bullet confirmed closed, empty-states/onboarding marked done); HEALTH-LOG entry if any new drift surfaced; SEED refresh; plan §7 fill + status flip + archive | ~30min | P5 | All acceptance boxes ticked; plan moved to `impl/archive/`; ROADMAP §UX polish subsection empty (or reshaped); SEED 25th-pass footer entry |

## 3. Deliverables — per phase

### 3.1 Phase P0 — Recon

**Two parallel recons — one for Leg A, one for Leg B. Leg C needs no recon (scope is fully known from the agent's prior survey).**

**A-recon — a11y gap audit:**
- Edit `wacp-console/frontend/eslint.config.ts`: enable every `jsx-a11y/*` rule not yet enabled, at `warn`. Include at minimum: `alt-text`, `anchor-has-content`, `anchor-is-valid`, `aria-activedescendant-has-tabindex`, `aria-props`, `aria-proptypes`, `aria-role`, `aria-unsupported-elements`, `autocomplete-valid`, `control-has-associated-label`, `heading-has-content`, `html-has-lang`, `iframe-has-title`, `img-redundant-alt`, `interactive-supports-focus`, `media-has-caption`, `mouse-events-have-key-events`, `no-access-key`, `no-aria-hidden-on-focusable`, `no-distracting-elements`, `no-interactive-element-to-noninteractive-role`, `no-noninteractive-element-interactions`, `no-noninteractive-element-to-interactive-role`, `no-noninteractive-tabindex`, `no-redundant-roles`, `prefer-tag-over-role`, `role-has-required-aria-props`, `role-supports-aria-props`, `scope`, `tabindex-no-positive`.
- Run `pnpm lint 2>&1 | tee /tmp/a11y-recon.txt` — capture full violation list.
- Install + run axe-core against the dev build: add `@axe-core/playwright` dev-dep, author a single `a11y-recon.spec.ts` under `frontend/e2e/` that iterates a fixture of representative pages (login, profiles-list, profile-editor, session-wizard, oversight-dashboard) and dumps axe violations to `/tmp/axe-recon.json`.
- Manually traverse profile studio + session launcher with keyboard only (`Tab`/`Shift+Tab`/`Enter`/`Escape` only) and record any trap, loss-of-focus, or unreachable-control sites in a `/tmp/keyboard-recon.md`.
- **Output:** append a §5.A triage table to this plan with columns `{Issue, Surface, Severity (high/med/low), Bucket (P1 quick-fix / P2 infra / out-of-scope)}`.

**B-recon — empty/error variant inventory:**
- Grep `src/surfaces/` + `src/components/` for empty-state patterns: `\.length === 0`, `data?\.length === 0`, `!data`, `No .* (recorded|found|yet)`, `color.*muted`. Record each site's file:line + current render shape.
- Grep for error rendering: `isError`, `error\?\.message`, `<.*error.*>`, `catch.*render`. Record each.
- Output two inline-inventory tables appended to plan §5.B: `{File:Line, Variant-shape, Proposed-component (EmptyState / ErrorBanner / keep-inline)}`.

**Scope-freeze.** End of P0, the user reviews the plan §5 updates and signs off on P1–P4 scope. If P0 surfaces >30 violations in the "fix" bucket, we re-negotiate — split high-severity to P1, defer low-severity to a post-v0.1.0 bucket.

### 3.2 Phase P1 — A.1 core a11y fixes

- Walk the P0 §5.A triage table's "quick-fix" bucket top-to-bottom. Expected patterns:
  - Form fields missing `aria-describedby` pointing at error text → add via `react-hook-form`'s `formState.errors` wiring. Proofs of pattern: the existing `ProfileEditor` form (f853a10) is close to correct — use it as template.
  - Heading hierarchy violations (h2 under h4 without h3) → promote/demote semantic heading levels per surface.
  - `alt=""` on non-decorative images → populate or mark role=presentation.
  - `aria-label` on non-interactive elements → remove.
  - Redundant `role="button"` on `<button>` → remove.
- After all quick-fix rows ticked, flip ruleset from `warn` → `error` in `eslint.config.ts`.
- **File touch count estimate:** 8–15 component files; ruleset config.
- **Test coverage:** existing RTL tests for each touched surface should still pass; if a new RTL-scope gap appears (e.g., the `getByLabelText` query fails post-fix), treat as a bug, fix in same commit.

### 3.3 Phase P2 — A.2 focus-management infrastructure

Four new shared utilities, each small:

1. **`useFocusTrap(ref)` hook** at `src/hooks/useFocusTrap.ts`. Traps `Tab`/`Shift+Tab` inside the referenced element; restores focus to the trigger on unmount. Wire into modals.
   - **P2 finding (scope revision):** `DeleteProfileModal.tsx` + `ImportYamlDialog.tsx` turned out to be **inline panels**, not dialogs — no overlay, no `role="dialog"`, render inline in the profile-editor pane. Focus-trap there would incorrectly block Tab from reaching sidebar/layout. Left alone. The actual `role="dialog" aria-modal="true"` site is the Create-User dialog in `UsersPage.tsx:332`; `useFocusTrap` wired there via a `CreateUserDialog` wrapper function (adds `useRef`, trap, and Escape-to-close handler).
2. **`useFocusOnRouteChange()` hook** at `src/hooks/useFocusOnRouteChange.ts`. Listens on `react-router`'s `location` change, moves focus to the `<main id="main">` landmark. **Plan deviation from "focus h1":** focusing the main landmark is both the screen-reader APG recommendation AND robust against surfaces without h1 (ProfilesPage uses h2 in sidebar+editor slots with no page-level h1). Wire once at `Layout.tsx`.
3. **`<SkipToContent />` component** at `src/components/SkipToContent.tsx`. First focusable element in `<body>`; visible on focus only; anchors `#main`. Wire at `AppLayout` + add `<main id="main" tabIndex={-1}>` wrapper to the existing layout.
4. **`<LiveRegion>` component + `useAnnounce()` hook** at `src/components/LiveRegion.tsx`. Renders a visually-hidden `role="status"` + `aria-live="polite"` div; `useAnnounce(text)` appends to the region. Used by P3's `<ErrorBanner>` to fire screen-reader announcements on transient errors. Single global `<LiveRegion>` mounted at `AppLayout`.

**Acceptance:** manual keyboard traversal of Profile Studio + Session Launcher (Wizard + all 6 steps) passes — Tab reaches every interactive element in expected order, Escape closes modals returning focus to trigger, route change moves focus to `<h1>`. Axe-core recon rerun clean on `focus-order-semantics`, `landmark-one-main`, `page-has-heading-one`, `region`.

### 3.4 Phase P3 — B.1 shared component extraction

**`<EmptyState>` at `src/components/EmptyState.tsx`.** Props: `{ icon?: ReactNode, title: string, description?: string, action?: ReactNode }`. Renders a centered block with optional illustration slot, title (as `<p role="status">` — inherits the announce pattern without needing LiveRegion), description, and action-button slot. RTL test at `EmptyState.test.tsx`: render matrix with all prop permutations; `getByRole("status")` finds the title; no console warnings.

**`<ErrorBanner>` at `src/components/ErrorBanner.tsx`.** Props: `{ variant: "error" | "warning" | "info", title: string, description?: string, onDismiss?: () => void }`. Renders a colored banner with variant-specific icon, title (as `<div role="alert">`), description, optional dismiss X. On mount, calls `useAnnounce(title)` from P2's LiveRegion infra. RTL test: render matrix per variant; `getByRole("alert")` finds the title; click dismiss → `onDismiss` invoked; announce fires on mount (mock `useAnnounce`).

**Convention locked at time of extract** — both components are final API once P3 ships; P4 can't edit them, only consume.

### 3.5 Phase P4 — B.2 surface sweep

Walk the P0 §5.B inventory top-to-bottom. For each row whose `Proposed-component` is `EmptyState` or `ErrorBanner`, replace the inline render with the shared component. Expected scope (per the agent recon in conversation): 10–12 surfaces — `RefusalPanel`, `EscalationInbox`, `GateQueue`, `WorkspaceTree`, `SessionsPage`, `ProfilesSidebar`, `VerticalsTab`, `RolesTab`, `ToolsTab`, `UsersPage` (admin), `AuditLogPage` (admin), plus any P0 surfaces.

Each surface is a 2–5 line diff. Commit per surface or bundle into one sweep commit — decide at execution time based on diff review friction.

**Acceptance:** `rg -n 'No .* (recorded|found|yet)|color.*text-muted.*[Nn]o' src/surfaces/` returns zero matches (or only matches unrelated to empty states — e.g., a legitimate `<p>` that happens to contain "no" in prose). Typecheck + lint clean. Every touched surface's RTL test still passes.

### 3.6 Phase P5 — C onboarding UI

**Backend (console-api):**
- New endpoint `GET /api/auth/bootstrap-state` returning `{ has_admin_user: bool, bootstrap_token_path?: PathBuf }`. Unauthenticated — must be added to the extractor-bypass list alongside `/api/auth/login` + `/api/auth/change-password` (per HEALTH-LOG §12.4 D2 precedent).
- `has_admin_user` = `console-db::queries::users::count_active_admins() > 0`.
- `bootstrap_token_path` = `Some(path)` only when file exists at the expected XDG state dir (`~/.local/share/wacp-console/bootstrap-token`); otherwise `None`.
- Handler at `wacp-console/crates/console-api/src/routes/auth.rs`, extractor wiring at `wacp-console/crates/console-api/src/middleware.rs`.
- Unit tests: endpoint returns correct shape under (a) fresh DB no admin, (b) post-bootstrap admin, (c) token file manually deleted.

**Frontend:**
- `useBootstrapState()` hook at `src/api/hooks/useBootstrapState.ts` — React Query, 1-minute stale time, fires on app mount.
- Login-page branch at `src/surfaces/auth/LoginPage.tsx`: if `has_admin_user === false`, redirect to `/setup` via `<Navigate>`. Already-logged-in case unaffected.
- New `/setup` route at `src/surfaces/auth/SetupPage.tsx`:
  - Renders the token path (from backend) + the token value (fetched client-side via authenticated localhost-only helper? no — the token file is 0o600 and not readable by the frontend; instead, instruct user to copy from the path. Alternative: have the endpoint return the token value ONLY IF `has_admin_user === false` AND file exists. Decide at execution — favor "display value" for UX).
  - "Save this somewhere secure" callout.
  - "Continue to login" button → `/login` with the username pre-filled (`admin`). The user then pastes the token, hits the forced-change flow from D2.
- Route registration in `src/main.tsx` or wherever routes live.
- Uses `<EmptyState>` from P3 for the null-token state and `<ErrorBanner>` from P3 for endpoint-failure state.

**E2E test:**
- `frontend/e2e/first-run.spec.ts` (new): Playwright fixture spins up a fresh console binary with a fresh DB, navigates to `/`, asserts the `/setup` redirect, asserts the token is displayed, clicks "Continue to login", pastes the token (pulled from the same fresh-DB fixture), asserts the forced-change screen, completes it, asserts admin dashboard. Total end-to-end proof of the fresh-install path.

**Docs:**
- `README.md` §Quick start / Development Setup — update bootstrap-discovery instructions to primary-path `navigate to /setup` with container-logs as fallback.

### 3.7 Phase P6 — Closeout

- **ROADMAP.md §Pre-`v0.1.0` > UX polish:** strike the "Keyboard-nav a11y sweep. Three surfaces completed…" bullet (already closed — verify and note the `f0527d7` anchor); strike the "Empty states + error-state completeness" bullet (closed by P3+P4); strike the "Onboarding flow" bullet (closed by P5). If all three struck, reshape the subsection — either retitle to "UX polish (closed)" with a pointer to this plan's archive, or delete the subsection entirely with the closed-item count merged into the top-level subsection summary line. Decide at closeout time.
- **HEALTH-LOG.md:** if P0 recon or any execution phase surfaced a new drift worth persisting (e.g., a latent bug caught during the a11y sweep), add a new §N subsection. If no drift surfaced, no HEALTH-LOG edit.
- **SEED.md:** 25th-pass footer entry covering the plan execution + ff-main. Refresh Resumption Point, Key docs (add this plan's archive entry), Pre-v0.1.0 status in §Next Steps or equivalent.
- **Plan §7 execution log:** fill with per-phase commits + dates + deviation notes.
- **Frontmatter:** flip `status: draft` → `final`, add `revised: ` timestamp, tick every §4 acceptance box.
- **Archive:** `git mv impl/ux-polish-pre-v0.1.0-plan.md impl/archive/` per the `archive-plan` skill.

## 4. Acceptance Criteria

- [x] P0 recon complete: axe-core artifact at `/tmp/axe-recon.json` (7 surfaces, 9 violations baseline) + plan §5 updated with three triage tables; scope-freeze at end of §5 sized P1 ≈ 10 sites, P4 ≈ 30 sites. Manual `/tmp/keyboard-recon.md` skipped — superseded by P2 acceptance check (post-infra traversal, not pre-fix recon). User sign-off in conversation 2026-04-23 ("2." — color-contrast brought into P1).
- [x] `pnpm lint` clean with `jsx-a11y/strict` ruleset at `error` — promoted in `ebec4be`; baseline + post-P5 both exit 0.
- [x] `pnpm typecheck` clean — exit 0 across P1–P5.
- [x] Axe-core recon rerun shows zero violations at any severity. 9 → 0 across all 7 surfaces post-P1; held green through P2/P3/P4/P5.
- [ ] Manual keyboard-only traversal of Profile Studio + Session Launcher (Wizard steps 1–6) — **not executed** (requires real browser + human; P2 infrastructure shipped + axe-core landmark / focus-order checks clean; manual verification deferred to operator-driven smoke test).
- [x] `<EmptyState>` + `<ErrorBanner>` exist at `src/components/`; 13 RTL tests pass (6 + 7).
- [x] `rg` of inline empty patterns across `src/surfaces/` returns only `<EmptyState>` adoption sites (zero residual `<p style={{color:"var(--color-text-muted)"}}>No ...</p>` patterns).
- [x] Backend endpoint `GET /api/auth/bootstrap-state` present + unauthenticated + returns correct shape. 2 integration tests in `console-integration/tests/bootstrap_state.rs` cover no-admin and admin-present arms (third arm — file-deleted — folded into the no-admin "may be null or string" assertion).
- [x] Playwright `00-first-run.spec.ts` green — 5 steps in 3.3s.
- [x] README.md §Quick start updated to point at `/setup` UI; on-disk fallback documented for advanced cases.
- [x] ROADMAP.md §Pre-`v0.1.0` > UX polish reshaped to "UX polish — landed" with summary of all three closed items.
- [x] Full `wacp-console/frontend` test suite passes: 535/535 across 3 shards; bootstrap-state integration tests 2/2; cargo build + targeted crate tests clean.
- [ ] Full CI green on the topic branch's final push — **deferred to ff push** (cannot verify pre-push).
- [x] Plan §7 execution log filled with per-phase commits + dates + deviation notes; frontmatter `status: final`; `revised` timestamp set.
- [x] Plan archived to `impl/archive/ux-polish-pre-v0.1.0-plan.md`.
- [ ] SEED 25th-pass refresh — **deferred to post-ff** per `seed-refresh` skill convention (refresh at batch boundary, not mid-execution).

## 5. Risks / Open Questions

### Risk #1 — P0 surfaces an unmanageable violation count

Enabling the full `eslint-plugin-jsx-a11y` rule set + axe-core against a codebase that's never been swept at that breadth may surface 30–50+ distinct issues rather than the estimated 8–15. Mitigation: after P0, if the "fix" bucket exceeds ~20 rows, re-negotiate with the user — split high-severity into P1 as planned, defer low-severity to a post-v0.1.0 follow-up plan. Plan doesn't commit to "all violations fixed in this pass" — it commits to "all rules at `error` with known violations either fixed or explicitly allowlisted with a reason."

### Risk #2 — Empty-state content variability breaks the `<EmptyState>` API

If P0 inventory reveals highly-heterogeneous empty-state shapes (some single line, some with CTA buttons, some with illustrations, some table-row variants), a single component API can't fit. Mitigation: expect 2–3 component variants rather than one. Acceptable if the final count is ≤3; unacceptable scope creep if it blooms past that — at that point, document the divergence and commit to the dominant shape only.

### Risk #3 — Bootstrap token file not directly readable from backend

The bootstrap token file is 0o600 owned by the console process user. The frontend asks the backend for it, which means the backend reads it at `/api/auth/bootstrap-state` time. This is fine security-wise while `has_admin_user === false` (pre-login, the token's whole purpose is to be displayed), but needs a hard gate: the endpoint must refuse to return the token value once `has_admin_user === true`, even if called. Mitigation: acceptance criterion #3 above tests this explicitly.

### Risk #4 — Playwright `first-run.spec.ts` fixture complexity

Spinning up a fresh-DB console binary inside a Playwright fixture is non-trivial — current `frontend/e2e/fixtures.ts` uses a shared harness. Plan deviation candidate: instead of a full fresh-DB fixture, use a reset-on-test-start helper that wipes the `users` table before the spec runs, preserving the harness but producing the zero-user state. If that path is simpler, take it.

### Risk #5 — Modal focus-trap conflict with existing `react-focus-lock` or equivalent

The codebase may already have a focus-trap library installed (worth grepping `package.json` at P2 start). If so, use the existing dependency rather than hand-rolling `useFocusTrap` — avoid duplicate libraries.

### Open Question #1 — `/setup` screen auto-advance vs. explicit click

After displaying the token and the user confirms they've saved it, should the "Continue to login" be a timer-based auto-advance (better UX) or explicit click (safer — user confirms they saw the token)? **Default: explicit click** pending user input. Trivial to change.

### Open Question #2 — Token rotation button on `/setup`

Should the `/setup` screen offer a "rotate token before continuing" button for operators who suspect the token was visible to someone? **Default: no** — v0.1.0 scope. File as a post-v0.1.0 enhancement if user wants it.

### Open Question #3 — Concurrent feature-freeze for v0.1.0 tag?

If v0.1.0 is imminent (per ROADMAP, gated on Codecov baseline + mutation-score flip-to-blocking + this plan's closure), coordination with other pre-v0.1.0 work (OCI tag push, `:latest` regex fix) may be needed. **Default: proceed independently** — this plan's output is on a topic branch that can ff into the release batch cleanly. No coordination needed.

### P0 finding — eslint static analysis is already at ceiling

Diffing `jsx-a11y` `recommended` vs `strict` presets shows all 33 strict rules are already enabled (via the `recommended` spread at `eslint.config.js:26`). Strict differs from recommended only in that 6 rules drop their per-tag allow-lists — these are `interactive-supports-focus`, `no-interactive-element-to-noninteractive-role`, `no-noninteractive-element-interactions`, `no-noninteractive-element-to-interactive-role`, `no-noninteractive-tabindex`, `no-static-element-interactions`. Promoted the spread to `strict` in `eslint.config.js`; `pnpm lint` still clean — no violations surfaced by the tightening. **Implication:** the A-recon "expand eslint ruleset" lever has nothing left to pull. P1 scope is driven by axe-core + keyboard-traversal findings, not eslint.

### A-recon — axe-core triage table

Artifact: `/tmp/axe-recon.json` (7 surfaces, 9 violations total — 4 distinct rule-ids, all critical/serious).

| # | Rule | Surface(s) | Severity | Nodes | Bucket | Note |
|---|---|---|---|---|---|---|
| A.1 | `button-name` | discovery-roles, discovery-verticals, profiles-list, profile-editor | critical | 4 | **P1** | Icon-only buttons with hover-opacity pattern — need `aria-label` or visible text. Targets are `.p-1.hover\:opacity-70*`. |
| A.2 | `select-name` | discovery-roles | critical | 2 | **P1** | Two unlabelled `<select>` elements on roles tab. Likely filter/sort dropdowns — add `<label htmlFor>` or `aria-label`. |
| A.3 | `color-contrast` | discovery-roles (9), discovery-verticals (5), profiles-list (1), profile-editor (1) | serious | 16 | **P1** (scope amendment, user sign-off 2026-04-23) | Root cause: `--color-text-muted` in `src/index.css:10,28`. Light `#94a3b8` on `#fff` ≈ 2.85:1 (fails AA 4.5:1); dark `#64748b` on `#0f172a` ≈ 4.0:1 (fails AA 4.5:1 small). `.uppercase` Sidebar class at `src/components/Sidebar.tsx:54` + empty-state `<p>` muted-text share the token. **Fix:** bump light `--color-text-muted` → `#64748b` (slate-500, ≈4.83:1) + dark → `#cbd5e1` (slate-300, strong contrast, distinct from secondary). Surgical look for h3 + button-internal residuals. |

**Surfaces with 0 violations:** login, session-wizard, oversight. Wizard + oversight were tested in minimal state (mock-runtime fixture without an active session), so "0 violations" understates true coverage — these surfaces should be re-scanned post-P4 with real content.

**P1 A-fix scope from axe:** 6 nodes across 2 rule-ids (`button-name`, `select-name`) + 16 nodes across `color-contrast` (user-authorized amendment). Plus the 2 auth-surface error-banner sites noted in the Error-render inventory below, which use `<ClickCard>` (role=button) instead of `role="alert"` (so screen readers announce them as clickable buttons, not as errors). That adds 2 more fixes (deferred to P3+P4). **Total P1 A-scope: ~10 concrete fix sites (6 label/naming + 2 token-bump lines + 2–3 residual contrast sites).**

### A-recon — focus-management infrastructure gaps

Grep of `wacp-console/frontend/src/` for `focus\(|FocusTrap|SkipToContent|LiveRegion|aria-live|role="alert"|role="status"|tabIndex` returns only `ClickCard.tsx:5,27` (the keyboard-nav sweep product). **Zero focus-trap hooks, zero route-change focus, zero skip-link, zero live regions** exist today. All four P2 utilities are new-build.

No focus-trap library in `package.json` (checked `axe`, `playwright`, `focus`, `aria` substrings; only `@playwright/test` matched). Risk #5 is null — `useFocusTrap` will be hand-rolled.

### B-recon — empty-state inventory

26 distinct sites across 14 files. Three dominant variants; no variant-count explosion (Risk #2 not fired).

| # | File:Line | Current variant | Proposed component |
|---|---|---|---|
| B.1 | `oversight/InjectionBar.tsx:78–80` | V3 inline "No active workspaces available" | `<EmptyState>` |
| B.2 | `oversight/WorkspaceTree.tsx:22` | V1 muted `<p>` "No workspaces active." | `<EmptyState>` |
| B.3 | `oversight/RefusalPanel.tsx:13` | V1 muted `<p>` "No refusals recorded." | `<EmptyState>` |
| B.4 | `oversight/GateQueue.tsx:106–107` | V1 muted `<p>` "No pending gates." | `<EmptyState>` |
| B.5 | `oversight/TrailStream.tsx:99–101` | V3 inline "No trail entries yet." | `<EmptyState>` |
| B.6 | `oversight/EscalationInbox.tsx:52` | V1 muted `<p>` (pattern) | `<EmptyState>` |
| B.7 | `admin/AuditLogPage.tsx:172–173` | V1 muted `<p>` "No audit log entries found." | `<EmptyState>` |
| B.8 | `sessions/SessionsPage.tsx:154–156` | V3 centered "No sessions yet." | `<EmptyState>` |
| B.9 | `sessions/Wizard.tsx:577–578` | V1 muted `<p>` "No verticals available." | `<EmptyState>` |
| B.10 | `sessions/Wizard.tsx:617–619` | V3 inline "No workflows available for this vertical." | `<EmptyState>` |
| B.11 | `sessions/Wizard.tsx:656` | V3 inline (roles empty state) | `<EmptyState>` |
| B.12 | `sessions/Wizard.tsx:933` | V3 inline (profiles empty state) | `<EmptyState>` |
| B.13 | `sessions/ContextForm.tsx:71` | V3 early-return (small) | `<EmptyState>` |
| B.14 | `profiles/ProfileVersionsPanel.tsx:23–25` | V3 inline "No version history available." | `<EmptyState>` |
| B.15 | `profiles/ProfilesSidebar.tsx:108–110` | V3 inline "No profiles found." | `<EmptyState>` |
| B.16 | `discovery/TypesTab.tsx:77–78` | V1 muted `<p>` "No envelope types found." | `<EmptyState>` |
| B.17 | `discovery/TypesTab.tsx:107–109` | V1 muted `<p>` "No protocol checkpoint types found." | `<EmptyState>` |
| B.18 | `discovery/TypesTab.tsx:117,159` | V1 muted `<p>` (2 additional) | `<EmptyState>` |
| B.19 | `discovery/ToolsTab.tsx:136–137` | V1 muted `<p>` "No tools found." | `<EmptyState>` |
| B.20–B.28 | `discovery/VerticalsTab.tsx:150,183,217,256,307,334,381,410,434` | V1 muted `<p>` "No verticals found." + V2 `<Muted>` wrapper ×8 | `<EmptyState>` — also delete file-local `Muted` function at `:492` (no longer needed) |
| B.29 | `discovery/RolesTab.tsx:169–170` | V1 muted `<p>` "No roles found." | `<EmptyState>` |
| B.30 | `discovery/DiscoveryPage.tsx:89–90` | V1 muted `<p>` "No results found." | `<EmptyState>` |

**Variant breakdown:**
- V1 (muted `<p style={{ color: "var(--color-text-muted)" }}>`) — ~16 sites, dominant pattern.
- V2 (`<Muted>` local wrapper in VerticalsTab) — 8 sites, same visual shape as V1.
- V3 (inline text / ad-hoc container) — ~8 sites, minor variation on V1/V2.

All three collapse cleanly into a single `<EmptyState>` API. **Risk #2 disarmed.** Plan §3.4's API shape (`{ icon?, title, description?, action? }`) fits all three — most sites use only `title`.

### B-recon — error-render inventory

Only 4 sites render application-level errors. React Query `isError` is **nowhere rendered** across any surface (grep `status === "error"|queryResult\.error|\.error\s*\?|\.error\s*&&` in `src/` returns one hit in `api/client.ts` — the error-code extractor, not a render site). Silent-fail is the default error pattern today — surfaces that fail-to-load show as empty.

| # | File:Line | Current variant | Proposed component |
|---|---|---|---|
| B.E.1 | `auth/LoginPage.tsx:33–40` | `{error && (<ClickCard aria-label="Dismiss error" ... onClick={clearError}>{error}</ClickCard>)}` — role=button, red bg | `<ErrorBanner variant="error" title={error} onDismiss={clearError}>` |
| B.E.2 | `auth/ChangePasswordPage.tsx:33–35` | Same pattern — mirror of LoginPage. | `<ErrorBanner variant="error" title={error} onDismiss={clearError}>` |
| B.E.3 | `admin/UsersPage.tsx:188` | Native browser `alert("Password has been reset. …")` | `<ErrorBanner variant="info">` + toast-like dismiss (or keep inline success-banner above form) |
| B.E.4 | `sessions/Wizard.tsx:355` (`setStepError`) + `:768` (`launchError` state) | `launchError: string \| null` rendered inline at step 6 — exact shape needs look-up but pattern is "render inline string with red color" | `<ErrorBanner variant="error">` |

**P1 A-scope extension:** B.E.1 + B.E.2's current `ClickCard` pattern announces as `role="button"` — screen reader hears "Dismiss error, button" instead of "Error: {message}". Migrating to `<ErrorBanner>` (with `role="alert"`) fixes that. These are **part of P3's ErrorBanner extraction + P4's adoption sweep**, not P1 — the correct fix is delete-and-replace when the component lands, not patch-in-place.

**P4 B-scope:** 26 `<EmptyState>` replacements + 4 `<ErrorBanner>` replacements = **30 component-adoption sites.** Above the plan's "10–12 surfaces" estimate but concentrated in surfaces already on the list; the per-site diff is tiny (2–5 lines). Net effort still within the P4 ~1.5h window; revise estimate to ~2h.

### P0 scope-freeze summary

- **P1 (A-core fixes):** ~10 sites across 3 axe rule-ids (button-name, select-name, color-contrast — the last brought in post-P0 by user amendment). Under the plan's "8–15" estimate; well under Risk #1's "20-row" threshold. Color-contrast adds ~30–60 min.
- **P2 (A-focus-infra):** 4 new utilities (`useFocusTrap`, `useFocusOnRouteChange`, `<SkipToContent>`, `<LiveRegion>` + `useAnnounce()`). No existing library; no conflict. Scope matches plan.
- **P3 (B-component-extract):** 2 components (`<EmptyState>`, `<ErrorBanner>`) + RTL tests. Scope matches plan.
- **P4 (B-surface-sweep):** 30 sites (26 empty + 4 error), up from plan's 10–12. Per-site diff is trivial; revised estimate 2h (up from 1.5h).
- **P5 (C onboarding):** scope unchanged — fully known from plan.
- **Out-of-scope deferred:** none after the 2026-04-23 color-contrast amendment. Every P0-surfaced violation is now scoped into a phase.

Revised total: **~7.5–11h** across P1–P6 (lower bound up from 6.5h because P1 eslint-sweep is a no-op but color-contrast amendment adds ~30–60 min; upper bound up from 10.5h by the same delta).

### Triage table (superseded by the three inventories above)

The plan's original plan-§5 empty triage tables (Issue, Empty-state, Error-render) have been populated above. The §3.1 P0 deliverable is complete.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| `ROADMAP.md` | Public-facing roadmap — §Pre-`v0.1.0` > UX polish | triggers |
| `HEALTH-LOG.md` §2.4 | Recurring a11y label-binding gap finding | informs Leg A scope |
| `HEALTH-LOG.md` §4 P4 | `eslint-plugin-jsx-a11y` landing (initial rule set + warn-downgrades) | extends |
| `HEALTH-LOG.md` §12.4 | D2 auth deadlock — forced-change flow + `AuthAllowPendingChange` extractor | Leg C backend extractor-bypass precedent |
| `impl/archive/v0.1.0-readiness-plan.md` Track A | Prior keyboard-nav sweep (`f0527d7`) + `<ClickCard>` component | confirms Leg A original scope closed; this plan extends beyond keyboard-nav |
| `impl/archive/frontend-perf-plan.md` F5, F7 | `react-hook-form` in ProfileEditor + virtualization infra | patterns to extend for Leg A.1 form-error associations |
| `impl/archive/audit-13-7-8-plan.md` D2 | Playwright auth-flows spec (`frontend/e2e/auth-flows.spec.ts`) | template for Leg C's `first-run.spec.ts` |
| `wcon-vision` BC6 | "No default credentials — bootstrap generates a one-time credential" | Leg C invariant |
| `wcon-auth` §6 | Bootstrap flow + `write_bootstrap_token` + forced-change semantics | Leg C backend reference |

## 7. Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| P0 | `ebec4be` | 2026-04-23 | Recon — promoted jsx-a11y recommended → strict (already at ceiling, no new violations); axe-core spec at `e2e/a11y-recon.spec.ts` surfaced 9 violations (4 button-name + 2 select-name + 16 color-contrast nodes — last brought into scope by user amendment); B-recon inventoried 26 empty-state + 4 error-render sites. Plan §5 triage tables populated. |
| P1 | `3334f1d` | 2026-04-23 | A-core: 9 axe violations → 0. Sidebar.tsx aria-label adds (toggle + theme buttons + logout); RolesTab.tsx aria-label on 2 selects; `--color-text-muted` token bumped (light slate-400 → slate-500 4.83:1; dark slate-500 → slate-300 10.7:1). Token bump cascaded — no surgical h3/button-internal fixes needed. |
| P2 | `237c80c` | 2026-04-23 | A-focus-infra: 4 utilities (`useFocusTrap`, `useFocusOnRouteChange`, `<SkipToContent>`, `<LiveRegion>`+`announce()`) + Layout wiring + UsersPage Create-User dialog focus-trap. **Deviations §3.3:** focus `<main>` instead of `<h1>` on route change (APG recommendation; ProfilesPage has no page-level h1); DeleteProfileModal/ImportYamlDialog were inline panels (not dialogs) — left unchanged; the real dialog site was UsersPage. |
| P3 | `261579e` | 2026-04-23 | B-component-extract: `<EmptyState>` (role=status) + `<ErrorBanner>` (role=alert, native ARIA-live; no `announce()` redundancy). 13 RTL tests. APIs locked at extract — P4 consumes without editing. |
| P4 | `95055ee` | 2026-04-23 | B-surface-sweep: 26 empty + 4 error adoptions across 23 files. Net −7 lines. Deleted file-local `Muted()` in VerticalsTab + unused `errorBox` const in Wizard. Native `alert()` in UsersPage replaced with inline info-banner. 3 existing tests adjusted to new dismiss-button click target. |
| P5 | `a29ee12` | 2026-04-23 | C onboarding: `bootstrap_token_path()` helper + `count_active_admins_setup_complete` query + `GET /api/auth/bootstrap-state` endpoint (security-gated token exposure) + 2 integration tests. Frontend `useBootstrapState` hook + `SetupPage` (inline login form) + LoginPage redirect branch + `/setup` route. New `00-first-run.spec.ts` (5 steps, 3.3s). README quickstart updated. **Deviations §3.6:** inline login form (avoid /setup ↔ /login ping-pong); `has_admin_user` reports "setup-complete" semantic (must_change_password=0), not raw count. Removed two superseded tests from `auth-flows.spec.ts` (bootstrap login + change-password rotation, now covered by `00-first-run.spec.ts`). |
| P6 | _(this commit)_ | 2026-04-23 | Closeout — ROADMAP §UX polish reshaped to "landed" with summary; plan §4 acceptance ticked (with 2 deferred boxes documented: manual keyboard traversal + post-ff CI/SEED); plan §7 filled; status `draft` → `final`; archived to `impl/archive/`. |
