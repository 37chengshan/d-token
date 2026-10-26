import { defineConfig, devices } from "@playwright/test";

const baseURL = process.env.E2E_BASE_URL ?? "http://127.0.0.1:5180";
const chromiumExecutablePath = process.env.E2E_CHROMIUM_EXECUTABLE_PATH;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  timeout: 45_000,
  expect: {
    timeout: 7_000,
  },
  reporter: [
    ["list"],
    ["html", { outputFolder: "../../../support/artifacts/playwright/report", open: "never" }],
  ],
  outputDir: "../../../support/artifacts/playwright/test-results",
  use: {
    baseURL,
    locale: "zh-CN",
    timezoneId: "Asia/Shanghai",
    reducedMotion: "reduce",
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: process.env.E2E_DISABLE_VIDEO ? "off" : "retain-on-failure",
    actionTimeout: 10_000,
    navigationTimeout: 30_000,
  },
  projects: [
    {
      name: "desktop-chromium",
      use: {
        ...devices["Desktop Chrome"],
        launchOptions: chromiumExecutablePath
          ? { executablePath: chromiumExecutablePath }
          : undefined,
        viewport: { width: 1180, height: 780 },
      },
    },
    {
      name: "macos-webkit",
      use: {
        ...devices["Desktop Safari"],
        viewport: { width: 1180, height: 780 },
      },
    },
  ],
  webServer: process.env.E2E_BASE_URL
    ? undefined
    : {
        command: "npm run build && npm run preview -- --host 127.0.0.1 --port 5180 --strictPort",
        url: baseURL,
        reuseExistingServer: !process.env.CI,
        timeout: 120_000,
      },
});
