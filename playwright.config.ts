import { defineConfig, devices } from "@playwright/test";

// End-to-end tests for the main user flows (UC-01, UC-02, UC-03, UC-05).
// The UI runs against the real Rust core through the lunda-devserver bridge
// (same commands as the desktop shell), on a fresh sample database.
const PORT = 1430;

export default defineConfig({
  testDir: "./e2e",
  timeout: 60_000,
  workers: 1,
  fullyParallel: false,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    viewport: { width: 1440, height: 950 },
    locale: "hu-HU",
    trace: "retain-on-failure",
    launchOptions: process.env.PW_CHROMIUM ? { executablePath: process.env.PW_CHROMIUM } : {},
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 950 } } }],
  webServer: {
    command: `cargo run -q -p lunda-devserver -- --port ${PORT} --data target/e2e-data --reset --sample --static dist`,
    url: `http://127.0.0.1:${PORT}/`,
    timeout: 300_000,
    reuseExistingServer: false,
  },
});
