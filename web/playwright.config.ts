import { defineConfig } from '@playwright/test'

// E2E tests run against a running stack (default: `docker compose up` on :8080).
// Locally they use an installed browser (PW_CHANNEL=msedge or chrome); CI will install Chromium (M6).
export default defineConfig({
  testDir: './e2e',
  timeout: 30_000,
  fullyParallel: false,
  reporter: 'list',
  use: {
    baseURL: process.env.E2E_BASE_URL ?? 'http://localhost:8080',
    channel: process.env.PW_CHANNEL,
    trace: 'retain-on-failure',
  },
})
