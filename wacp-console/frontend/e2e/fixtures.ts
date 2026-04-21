// Playwright test fixture with opt-in V8 coverage collection.
//
// When `E2E_COVERAGE=1` is set, each test starts JS coverage via Chrome
// DevTools Protocol before the test body runs and stops/dumps it after.
// Raw V8 coverage lands in `coverage/playwright-raw/${testId}.json` in the
// `{result: [...]}` shape that `c8 report` consumes.
//
// Tests import `{ test, expect }` from here instead of `@playwright/test`
// directly so the instrumentation is transparent — zero changes to the
// test body.
//
// No-op when E2E_COVERAGE is unset: the base Playwright test is returned
// unmodified.

// Playwright's fixture callback is named `use` by convention — nothing to do
// with React's `use` hook. Disable the lint for this file; we don't use any
// React primitives here.
/* eslint-disable react-hooks/rules-of-hooks */

import { test as base, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import fs from "node:fs/promises";
import path from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const COVERAGE_DIR = path.resolve(__dirname, "..", "coverage", "playwright-raw");

const ENABLED = process.env.E2E_COVERAGE === "1";

export const test = base.extend({
  page: async ({ page }, use, testInfo) => {
    if (ENABLED) {
      await page.coverage.startJSCoverage({ resetOnNavigation: false });
    }
    await use(page);
    if (ENABLED) {
      const coverage = await page.coverage.stopJSCoverage();
      await fs.mkdir(COVERAGE_DIR, { recursive: true });
      const filename = `${testInfo.testId}.json`;
      await fs.writeFile(
        path.join(COVERAGE_DIR, filename),
        JSON.stringify({ result: coverage }),
      );
    }
  },
});

export { expect };
