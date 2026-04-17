import { defineConfig, devices } from "@playwright/test";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import path from "node:path";

// Resolve the workspace target/ dir from this config's location so the
// webServer blocks can invoke freshly-built debug binaries without asking
// developers to hand-set a PATH entry. `../../../..` climbs:
//   frontend/ → wacp-console/ → wacp-platform/ → (repo root) → target/debug/
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const workspaceRoot = path.resolve(__dirname, "..", "..");
const runtimeBin = path.join(workspaceRoot, "target", "debug", "wacp-mock-runtime");
const consoleBin = path.join(workspaceRoot, "target", "debug", "wacp-console");
const frontendDist = path.join(__dirname, "dist");

const MOCK_AGENT = "[::1]:9190";
const MOCK_HIGHWAY = "[::1]:9191";
const MOCK_COORDINATOR = "[::1]:9192";
const MOCK_REST = "[::1]:9193";
const CONSOLE_LISTEN = "[::1]:8787";
const CONSOLE_STATE_DIR = path.join(__dirname, ".e2e-state");
const CONSOLE_DB = path.join(CONSOLE_STATE_DIR, "console.db");

// SQLite refuses to create parent directories; pre-create the per-run state
// dir before the webServer spawns. Running this at config load time keeps
// setup deterministic without needing a separate `globalSetup` file.
fs.mkdirSync(CONSOLE_STATE_DIR, { recursive: true });

export default defineConfig({
  testDir: "./e2e",
  outputDir: "./test-results",
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI
    ? [["html", { outputFolder: "playwright-report", open: "never" }], ["github"]]
    : [["html", { outputFolder: "playwright-report", open: "never" }], ["list"]],
  use: {
    baseURL: `http://${CONSOLE_LISTEN}`,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: [
    {
      command: runtimeBin,
      env: {
        WACP_MOCK_AGENT_ADDR: MOCK_AGENT,
        WACP_MOCK_HIGHWAY_ADDR: MOCK_HIGHWAY,
        WACP_MOCK_COORDINATOR_ADDR: MOCK_COORDINATOR,
        WACP_MOCK_REST_ADDR: MOCK_REST,
        RUST_LOG: "warn",
      },
      // Use the REST gateway as the readiness probe — `/v1/verticals` always
      // responds 200 with the fixture list, so Playwright knows the mock is
      // up once this URL replies.
      url: `http://${MOCK_REST}/v1/verticals`,
      reuseExistingServer: !process.env.CI,
      timeout: 15_000,
      stdout: "pipe",
      stderr: "pipe",
    },
    {
      command: [
        consoleBin,
        "serve",
        "--listen",
        CONSOLE_LISTEN,
        "--database",
        CONSOLE_DB,
        "--frontend-path",
        frontendDist,
        "--agent-address",
        MOCK_AGENT,
        "--highway-address",
        MOCK_HIGHWAY,
        "--coordinator-address",
        MOCK_COORDINATOR,
        "--rest-address",
        `http://${MOCK_REST}`,
      ]
        .map((a) => (a.includes(" ") ? `"${a}"` : a))
        .join(" "),
      env: {
        RUST_LOG: "warn",
        // Redirect the bootstrap-token write so tests can read it without
        // touching the user's XDG state dir.
        XDG_STATE_HOME: CONSOLE_STATE_DIR,
      },
      url: `http://${CONSOLE_LISTEN}/api/health`,
      reuseExistingServer: !process.env.CI,
      timeout: 30_000,
      stdout: "pipe",
      stderr: "pipe",
    },
  ],
});
