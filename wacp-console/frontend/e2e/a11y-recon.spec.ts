// P0 recon spec for the UX-polish-pre-v0.1.0 plan.
//
// Traverses representative surfaces (login, discovery/roles, profiles-list,
// profile-editor, session-wizard, oversight-dashboard) and runs
// @axe-core/playwright against each. Violations are collected into a single
// artifact at /tmp/axe-recon.json for the plan's §5.A triage table.
//
// Not a regression test — the spec always passes regardless of violations.
// It's a one-shot survey. Removed post-P0 when the plan closes.
//
// Invocation: pnpm test:e2e a11y-recon
// Artifact:   /tmp/axe-recon.json

import AxeBuilder from "@axe-core/playwright";
import fs from "node:fs/promises";
import path from "node:path";
import { test, expect } from "./fixtures";
import { ensureAdminOnDiscovery } from "./helpers/admin";

const OUT_PATH = "/tmp/axe-recon.json";

type SurfaceRun = {
  surface: string;
  url: string;
  violations: Array<{
    id: string;
    impact: string | null | undefined;
    nodes: number;
    help: string;
    helpUrl: string;
    targets: string[];
  }>;
};

const collected: SurfaceRun[] = [];

async function scan(page: import("@playwright/test").Page, surface: string) {
  const result = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
    .analyze();
  const flat = result.violations.map((v) => ({
    id: v.id,
    impact: v.impact,
    nodes: v.nodes.length,
    help: v.help,
    helpUrl: v.helpUrl,
    targets: v.nodes.flatMap((n) => n.target.map(String)).slice(0, 5),
  }));
  collected.push({ surface, url: page.url(), violations: flat });
  // Informational only — do not fail the spec on violations.
  console.log(`[axe-recon] ${surface}: ${flat.length} violations`);
}

test.describe.configure({ mode: "serial" });

test.describe("Axe-core recon — P0 baseline", () => {
  test("login page — unauthenticated scan", async ({ page }) => {
    await page.goto("/login");
    await expect(page.getByLabel("Username")).toBeVisible();
    await scan(page, "login");
  });

  test("discovery (roles tab) — authenticated landing", async ({ page }) => {
    await ensureAdminOnDiscovery(page);
    await scan(page, "discovery-roles");
  });

  test("discovery (verticals tab)", async ({ page }) => {
    await ensureAdminOnDiscovery(page);
    await page.getByRole("button", { name: /^verticals$/i }).click();
    await page.waitForTimeout(500);
    await scan(page, "discovery-verticals");
  });

  test("profiles list", async ({ page }) => {
    await ensureAdminOnDiscovery(page);
    await page.getByRole("link", { name: /profiles/i }).click();
    await page.waitForURL(/\/profiles/, { timeout: 5_000 });
    await scan(page, "profiles-list");
  });

  test("profile editor — create-new form mounted", async ({ page }) => {
    await ensureAdminOnDiscovery(page);
    await page.getByRole("link", { name: /profiles/i }).click();
    await page.waitForURL(/\/profiles/, { timeout: 5_000 });
    await page.getByRole("button", { name: /create new/i }).click();
    await expect(page.getByRole("heading", { level: 2, name: /new profile/i })).toBeVisible();
    await scan(page, "profile-editor");
  });

  test("session wizard — step 1 vertical pick", async ({ page }) => {
    await ensureAdminOnDiscovery(page);
    await page.getByRole("link", { name: /sessions/i }).click();
    await page.waitForURL(/\/sessions/, { timeout: 5_000 });
    // "Launch New" or similar affordance; fall back to direct URL if missing.
    const launchBtn = page.getByRole("button", { name: /launch|new/i }).first();
    if (await launchBtn.isVisible().catch(() => false)) {
      await launchBtn.click();
    } else {
      await page.goto("/sessions/new");
    }
    await page.waitForTimeout(500);
    await scan(page, "session-wizard");
  });

  test("oversight dashboard", async ({ page }) => {
    await ensureAdminOnDiscovery(page);
    // Oversight link may only appear if a session is active — navigate
    // directly. Scan whatever renders.
    await page.goto("/oversight");
    await page.waitForTimeout(500);
    await scan(page, "oversight");
  });

  test.afterAll(async () => {
    await fs.mkdir(path.dirname(OUT_PATH), { recursive: true }).catch(() => {});
    await fs.writeFile(OUT_PATH, JSON.stringify(collected, null, 2));
    const total = collected.reduce((n, s) => n + s.violations.length, 0);
    console.log(`[axe-recon] wrote ${OUT_PATH} — ${collected.length} surfaces, ${total} violations`);
  });
});
