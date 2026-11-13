import { defineConfig, devices } from "@playwright/test";

// End-to-end tests for the main user flows (UC-01, UC-02, UC-03, UC-05).
// The UI runs against the real TEMISZ server and Postgres (DATABASE_URL),
// on a freshly created sample account.
const PORT = 1430;

export default defineConfig({
  testDir: "./e2e",
  timeout: 60_000,
  workers: 1,
  // CI: one retry, recorded with a trace (recording every run slowed page loads on the runners)
  retries: process.env.CI ? 1 : 0,
  fullyParallel: false,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    viewport: { width: 1440, height: 950 },
    locale: "hu-HU",
    trace: "on-first-retry",
    launchOptions: process.env.PW_CHROMIUM ? { executablePath: process.env.PW_CHROMIUM } : {},
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 950 } } }],
  webServer: {
    command: `cargo run -q -p temisz-server -- --port ${PORT} --static dist --insecure-cookies --dev --reset-sample-account`,
    url: `http://127.0.0.1:${PORT}/`,
    timeout: 300_000,
    reuseExistingServer: false,
    env: { SUPERADMIN_EMAILS: "admin@minta.temisz.hu" },
  },
});
