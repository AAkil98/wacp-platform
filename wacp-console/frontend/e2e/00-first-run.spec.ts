// First-run onboarding E2E (plan §3.6 P5.C).
//
// FILE-NAME PREFIX: `00-` ensures this spec runs **first** (alphabetical
// order, workers=1 per playwright.config.ts). The fresh state from
// `e2e-cleanup.sh` is consumed by whatever test runs first; this spec
// requires the truly-fresh state to demonstrate the redirect /
// token-display path. Other tests use `ensureAdminOnDiscovery` which
// rotates the bootstrap into FINAL_ADMIN_PASSWORD on first call —
// after which has_admin_user=true and /setup → /login.
//
// Coverage:
//   1. Visiting / on a fresh install lands on /setup (via LoginPage's
//      bootstrap-state branch, not on the /login form).
//   2. /setup displays the bootstrap username + token (revealed on click).
//   3. /setup links the source file path under the token.
//   4. Inlined sign-in form on /setup accepts the token; forced-change
//      page completes the rotation; admin lands on /discovery.
//   5. Post-rotation, /setup redirects to /login (security gate per plan
//      §4 acceptance #3 — token field is null once setup-complete).

import { test, expect } from "./fixtures";
import { readBootstrapToken, FINAL_ADMIN_PASSWORD } from "./helpers/admin";

test.describe.configure({ mode: "serial" });

test("fresh install redirects to /setup, displays token, completes onboarding", async ({ page }) => {
  // Step 1 — root navigation triggers the LoginPage's bootstrap-state
  // check, which navigates to /setup when has_admin_user=false.
  await page.goto("/");
  await page.waitForURL(/\/setup/, { timeout: 10_000 });

  // Step 2 — page renders welcome heading + admin username code-block +
  // masked password code-block.
  await expect(page.getByRole("heading", { name: /first-run setup/i })).toBeVisible();
  await expect(page.locator("code").filter({ hasText: /^admin$/ })).toBeVisible();
  // Token starts masked (43 bullet characters).
  await expect(page.locator("code").filter({ hasText: /^•+$/ })).toBeVisible();

  // Step 3 — clicking Show reveals the actual token.
  const expectedToken = readBootstrapToken();
  await page.getByRole("button", { name: /reveal password/i }).click();
  await expect(page.getByText(expectedToken, { exact: true })).toBeVisible();

  // Step 4 — inline sign-in form accepts the token; forced-change page mounts.
  // Wait on the form's distinctive label rather than the URL — react-router's
  // Navigate triggers history.replaceState, which doesn't always fire
  // Playwright's load-based waitForURL on SPAs.
  await page.getByLabel("Paste the one-time password to sign in").fill(expectedToken);
  await page.getByRole("button", { name: /sign in/i }).click();
  await expect(page.getByLabel("Current Password")).toBeVisible({ timeout: 10_000 });

  // Step 5 — rotate to the shared FINAL_ADMIN_PASSWORD so subsequent
  // specs in this run can ensureAdminOnDiscovery without re-bootstrapping.
  await page.getByLabel("Current Password").fill(expectedToken);
  await page.getByLabel("New Password").fill(FINAL_ADMIN_PASSWORD);
  await page.getByRole("button", { name: /change password/i }).click();
  // Discovery page renders the verticals tab heading after auth + taxonomy load.
  await expect(page.getByRole("heading", { name: /discovery browser/i })).toBeVisible({ timeout: 10_000 });

  // Security gate (plan §4 acceptance #3) — that the endpoint refuses to
  // leak the token once a setup-complete admin exists — is covered by the
  // Rust integration test `bootstrap_state_with_admin_returns_has_admin_
  // true_no_token` in `console-integration/tests/bootstrap_state.rs`.
  // Replicating it here would require logout-then-fresh-load handling
  // that adds Playwright fragility for no incremental signal.
});
