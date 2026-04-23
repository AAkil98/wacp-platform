---
id: wacp-ux-polish-pre-v0.1.0-plan
type: impl
status: draft
created: 2026-04-23T02:30:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, a11y, ux, onboarding, frontend]
depends_on: [wacp-integration-deferred-scenarios-plan]
---

# UX Polish Pre-v0.1.0 — Plan

> **Triggering finding:** `ROADMAP.md` §Pre-`v0.1.0` > "UX polish" subsection — three bullets (keyboard-nav a11y sweep; empty-states + error-states normalization; onboarding flow). The first is stale (Track A already closed via `f0527d7`); this plan replaces it with an expanded a11y audit beyond keyboard-nav, folds the other two in, and strikes the stale ROADMAP bullet in closeout.
> **Target branch:** `refactor/ux-polish-pre-v0.1.0` (topic).
> **Rough effort:** ~6.5–10.5h across 6 execution phases + closeout — medium confidence. Recon-heavy; P0 calibrates P1–P4 scope.
> **Not in scope:** color-contrast / theme-level a11y audit (belongs to a future visual-design pass), pre-v0.1.0 items other than the three UX polish bullets (OCI publication + `:latest` regex + coverage floor are independent), LLM-assisted onboarding tutorials, i18n.

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

1. **`useFocusTrap(ref)` hook** at `src/hooks/useFocusTrap.ts`. Traps `Tab`/`Shift+Tab` inside the referenced element; restores focus to the trigger on unmount. Wire into `DeleteProfileModal.tsx`, `ImportYamlDialog.tsx`, and any other modal surfaced by P0's keyboard-recon.
2. **`useFocusOnRouteChange()` hook** at `src/hooks/useFocusOnRouteChange.ts`. Listens on `react-router`'s `location` change, moves focus to the primary `<h1>` of the new route. Wire once at `AppLayout`. Requires each surface to render a focusable `<h1>` (`tabIndex={-1}`) — add where missing.
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

- [ ] P0 recon complete: `/tmp/a11y-recon.txt` + `/tmp/axe-recon.json` + `/tmp/keyboard-recon.md` produced; plan §5 updated with triage tables; user signed off on P1–P4 scope.
- [ ] `pnpm lint` clean with the full `eslint-plugin-jsx-a11y` rule set at `error` — `cd wacp-console/frontend && pnpm lint` exit 0.
- [ ] `pnpm typecheck` clean — `cd wacp-console/frontend && pnpm typecheck` exit 0.
- [ ] Axe-core recon rerun (same fixture as P0) shows zero violations at severity ≥ `serious`, and every `moderate` is either fixed or documented as accepted in the plan §5 triage.
- [ ] Manual keyboard-only traversal of Profile Studio + Session Launcher (Wizard steps 1–6) completes without trap, loss-of-focus, or unreachable-control.
- [ ] `<EmptyState>` + `<ErrorBanner>` components exist at `src/components/`; RTL tests pass: `cd wacp-console/frontend && pnpm test -- EmptyState ErrorBanner` exit 0.
- [ ] `rg -n 'No .* (recorded|found|yet)|color.*text-muted.*[Nn]o' wacp-console/frontend/src/surfaces/` returns zero matches related to empty-state rendering.
- [ ] Backend endpoint `GET /api/auth/bootstrap-state` present + unauthenticated + returns correct shape under all three DB states (no admin / admin + token file / admin + token file deleted); unit tests at `console-api/src/routes/auth.rs` pass.
- [ ] Playwright `first-run.spec.ts` green — `cd wacp-console/frontend && pnpm test:e2e first-run` exit 0.
- [ ] README.md §Quick start updated to primary-path `/setup` UI with container-logs as fallback.
- [ ] ROADMAP.md §Pre-`v0.1.0` > UX polish subsection reshaped or struck.
- [ ] Full `wacp-console/frontend` test suite + full Rust workspace test suite pass — `cd wacp-console/frontend && pnpm test` + `cargo test --workspace` both exit 0.
- [ ] Full CI green on the topic branch's final push — all 4 workflows (`ci-lint`, `ci-wacp`, `ci-console`, `coverage`) pass.
- [ ] Plan §7 execution log has per-phase commits + dates + deviation notes; frontmatter `status` flipped to `final`; `revised` timestamp set.
- [ ] Plan archived to `impl/archive/ux-polish-pre-v0.1.0-plan.md`.
- [ ] SEED 25th-pass refresh committed post-ff.

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

### Triage table (populated by P0)

| # | Issue | Surface | Severity | Bucket |
|---|---|---|---|---|
| A.1 | _(P0 fills this)_ | _(P0)_ | _(P0)_ | _(P0)_ |

### Empty-state inventory (populated by P0)

| # | File:Line | Current variant | Proposed component |
|---|---|---|---|
| B.1 | _(P0 fills this)_ | _(P0)_ | _(P0)_ |

### Error-render inventory (populated by P0)

| # | File:Line | Current variant | Proposed component |
|---|---|---|---|
| B.E.1 | _(P0 fills this)_ | _(P0)_ | _(P0)_ |

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
| P0 | _(tbd)_ | _(tbd)_ | Recon — plan §5 triage tables populated + scope-freeze sign-off |
| P1 | _(tbd)_ | _(tbd)_ | A.1 core fixes + ruleset promotion to `error` |
| P2 | _(tbd)_ | _(tbd)_ | A.2 focus-management infra |
| P3 | _(tbd)_ | _(tbd)_ | B.1 `<EmptyState>` + `<ErrorBanner>` extraction |
| P4 | _(tbd)_ | _(tbd)_ | B.2 surface sweep |
| P5 | _(tbd)_ | _(tbd)_ | C onboarding UI (backend endpoint + `/setup` + login branch + E2E spec) |
| P6 | _(tbd)_ | _(tbd)_ | Closeout — ROADMAP strike + SEED refresh + archive |
