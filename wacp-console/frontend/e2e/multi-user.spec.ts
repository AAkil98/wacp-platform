// §13.7.7 D2 — multi-user role matrix (stubbed).
//
// Audit scope: admin/operator/viewer can see / edit / launch / approve only
// what their role permits. Stubbed because the D2 scope is "two specs
// unskipped"; this one depends on fixtures that don't exist yet.

import { test } from "./fixtures";

test.skip("admin can edit users; operator cannot; viewer cannot", () => {
  // To unskip this test, the harness needs:
  //   1. A seed step that inserts three fixture users (admin/operator/viewer)
  //      with known passwords directly into the console DB. `console-db`
  //      exposes `users::insert_user`; a test-only seeder either runs
  //      inline from the spec via a tiny admin-only `POST /api/admin/users`
  //      affordance or — cleaner — a separate `wacp-console seed-users`
  //      CLI subcommand that the webServer runs once at startup.
  //   2. Three independent Playwright `browser.newContext()` calls so each
  //      user's session cookies are isolated. See
  //      https://playwright.dev/docs/auth for the storageState pattern.
  //   3. A role-aware assertion library: each action (navigate to
  //      /admin/users, click Create on profiles, click Launch on sessions,
  //      click Approve on an open gate) should succeed for the authorized
  //      role and surface a 403 / disabled-button / missing-nav-link state
  //      for the unauthorized role. The surface currently uses `AdminGuard`
  //      (hard redirect) for admin-only routes but leaner affordances for
  //      operator-vs-viewer; audit the surface before writing assertions.
  //
  // Anchor: `wcon-auth` §5 (role matrix), `wcon-api` §4 (authorization per
  // endpoint), `wcon-ui` §3 (role-gated UI).
});

test.skip("operator can launch a session; viewer cannot", () => {
  // See the note above — same fixture + context isolation requirements.
  // Additionally blocked on the mock runtime being able to serve a full
  // `SubmitGoal → Decompose → Dispatch` exchange (same dependency as
  // `golden-path.spec.ts`'s deferred block).
});

test.skip("viewer can read sessions + gates but cannot approve", () => {
  // See the notes above.
});
