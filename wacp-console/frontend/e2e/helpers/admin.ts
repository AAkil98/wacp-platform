// Admin bootstrap + login helpers shared across E2E specs.
//
// The console writes its first-launch bootstrap credential to
// `$XDG_STATE_HOME/wacp-console/bootstrap-token`. `playwright.config.ts`
// redirects XDG_STATE_HOME into `.e2e-state/` so reading the token from the
// test side is deterministic.
//
// After the first login, the admin is forced through `/change-password`. We
// rotate to `FINAL_ADMIN_PASSWORD` — a known 24-char value — so subsequent
// specs can log in without re-reading the bootstrap token.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, type Page } from "@playwright/test";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const STATE_DIR = path.resolve(__dirname, "..", "..", ".e2e-state");
const BOOTSTRAP_TOKEN_PATH = path.join(STATE_DIR, "wacp-console", "bootstrap-token");

export const ADMIN_USERNAME = "admin";
export const FINAL_ADMIN_PASSWORD = "E2E-admin-password-4567";

/** Read the one-time bootstrap credential the console wrote on first launch. */
export function readBootstrapToken(): string {
  return fs.readFileSync(BOOTSTRAP_TOKEN_PATH, "utf8").trim();
}

/** Fill the login form and click Sign In. Does not wait for navigation. */
export async function submitLogin(page: Page, username: string, password: string): Promise<void> {
  await page.getByLabel("Username").fill(username);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: /sign in/i }).click();
}

/**
 * Land on `/discovery` as an authenticated admin, handling three possible
 * starting states:
 *   1. Admin has already rotated to FINAL_ADMIN_PASSWORD — log in directly.
 *   2. Admin is fresh from bootstrap — log in with token, rotate password.
 *   3. Something else — throw with both attempts' URLs for diagnosis.
 *
 * Each login attempt waits on the `/api/auth/login` response so we don't
 * false-positive-match the still-on-`/login` URL that briefly precedes the
 * SPA's react-router redirect.
 */
export async function ensureAdminOnDiscovery(page: Page): Promise<void> {
  await page.goto("/login");

  // Attempt 1: final password. Race the login response with the submit so we
  // know the backend either accepted (200) or refused (401) before deciding
  // the next step. A 401 leaves us on /login with an error banner.
  const login1 = page.waitForResponse(
    (resp) => resp.url().endsWith("/api/auth/login") && resp.request().method() === "POST",
  );
  await submitLogin(page, ADMIN_USERNAME, FINAL_ADMIN_PASSWORD);
  const login1Resp = await login1;

  if (login1Resp.ok()) {
    // Wait for the SPA to react and navigate away from /login.
    await page.waitForURL((url) => !url.pathname.endsWith("/login"), { timeout: 5_000 });
    if (page.url().includes("/discovery")) return;
    if (page.url().includes("/change-password")) {
      // Unexpected — the backend accepted final password but flagged pending
      // change. Shouldn't happen once an admin has rotated, but handle the
      // case rather than hanging.
    }
  }

  // Attempt 2: bootstrap token. Expect redirect to /change-password because
  // the admin's must_change_password flag is true on first login.
  const token = readBootstrapToken();
  await page.goto("/login");
  const login2 = page.waitForResponse(
    (resp) => resp.url().endsWith("/api/auth/login") && resp.request().method() === "POST",
  );
  await submitLogin(page, ADMIN_USERNAME, token);
  const login2Resp = await login2;

  if (!login2Resp.ok()) {
    throw new Error(
      `ensureAdminOnDiscovery: bootstrap login returned ${login2Resp.status()} — ` +
        `admin password is neither FINAL nor bootstrap-token. Nuke .e2e-state and retry.`,
    );
  }

  await page.waitForURL((url) => !url.pathname.endsWith("/login"), { timeout: 5_000 });

  if (page.url().includes("/change-password")) {
    await page.getByLabel("Current Password").fill(token);
    await page.getByLabel("New Password").fill(FINAL_ADMIN_PASSWORD);
    await page.getByRole("button", { name: /change password/i }).click();
    await page.waitForURL(/\/discovery/, { timeout: 5_000 });
    return;
  }

  if (page.url().includes("/discovery")) return;

  throw new Error(`ensureAdminOnDiscovery: unexpected URL ${page.url()}`);
}

/**
 * Assert the login page's error banner shows the given text (case-insensitive
 * substring). The banner is the red `<div>` at the top of `<form>`.
 */
export async function expectLoginError(page: Page, substring: string): Promise<void> {
  const banner = page.locator("div").filter({ hasText: new RegExp(substring, "i") }).first();
  await expect(banner).toBeVisible({ timeout: 5_000 });
}
