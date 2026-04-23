// §13.7.7 D2 — auth-flows E2E.
//
// Covers: bad password, logout, re-login with rotated password. Runs
// serially because later tests depend on the admin state set up by earlier ones.
//
// **Bootstrap-credential login + forced password change** were originally
// in this file. They've been folded into `00-first-run.spec.ts` (P5.C
// onboarding plan) which exercises the same flow with the additional
// /setup-screen entry point. Removing the duplicate here avoids the
// double-consume-bootstrap conflict.
//
// Deferred to a later pass (see skipped `test.skip` at the bottom):
//   - 5-failed-attempts lockout path (requires per-test DB reset to stay
//     idempotent; defer to a follow-up fixture).
//   - API-token create + use (not a Settings-page affordance in the shipping
//     UI; needs a drift check first — see perf-opt §2.5).

import { test, expect } from "./fixtures";
import {
  ADMIN_USERNAME,
  FINAL_ADMIN_PASSWORD,
  expectLoginError,
  submitLogin,
} from "./helpers/admin";

test.describe.configure({ mode: "serial" });

test.describe("Admin logout → re-login (post-rotation)", () => {
  test("bad password shows the login error banner", async ({ page }) => {
    await page.goto("/login");
    await submitLogin(page, ADMIN_USERNAME, "definitely-not-the-password");
    // Backend responds 401 with `{error: "unauthenticated", message:
    // "Authentication required"}`; the auth store surfaces
    // `detail.message` which renders "Authentication required" in the
    // red banner above the form.
    await expectLoginError(page, "authentication required");
    await expect(page).toHaveURL(/\/login$/);
  });

  test("logout clears the session and routes back to /login", async ({ page }) => {
    // Get into the authenticated state first.
    await page.goto("/login");
    await submitLogin(page, ADMIN_USERNAME, FINAL_ADMIN_PASSWORD);
    await page.waitForURL(/\/discovery/, { timeout: 5_000 });

    // Sidebar renders a `<button title="Logout">` with an icon-only body;
    // `title` is the accessible name for icon-only buttons. Use the explicit
    // `getByTitle` locator rather than role+name to dodge a11y-algorithm
    // quirks across browsers.
    await page.getByTitle("Logout").click();

    // `api.post('/api/auth/logout')` followed by auth-store clearing user
    // sets user=null; the next page render shows the login form again. Some
    // flows also navigate; assert the URL moves off the authenticated area.
    await page.waitForURL(/\/login/, { timeout: 5_000 });
  });

  test("re-login with rotated password lands directly on /discovery", async ({ page }) => {
    await page.goto("/login");
    await submitLogin(page, ADMIN_USERNAME, FINAL_ADMIN_PASSWORD);
    // No must_change_password this time — straight to /discovery.
    await page.waitForURL(/\/discovery/, { timeout: 5_000 });
    await expect(page).toHaveURL(/\/discovery$/);
  });
});

test.describe("Deferred — documented drifts and missing fixtures", () => {
  test.skip("5 consecutive failed logins lock the account (MAX_FAILED_PER_ACCOUNT=5)", () => {
    // Rate-limit behavior exists (wacp-console/crates/console-core/src/
    // rate_limit.rs) but asserting it here requires a per-test DB reset
    // because running this at the end of the suite terminates the admin
    // account for subsequent runs under PLAYWRIGHT_REUSE_STATE. Defer
    // until a test-only DB-reset endpoint or a dedicated test-user
    // seeding step exists.
  });

  test.skip("create API token in Settings, use it to hit /api/ with Bearer auth", () => {
    // The shipping Settings page does not surface a token-creation UI;
    // `crates/console-api/src/routes/tokens.rs` exposes the routes, but
    // the frontend surface isn't present. Confirm the drift against
    // wcon-api / wcon-auth and either add the UI or remove this test.
    // See perf-opt §2.5 for the drift-resolution protocol.
  });
});
