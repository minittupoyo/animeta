import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests/ui',
  fullyParallel: true,
  use: {
    baseURL: 'http://127.0.0.1:1420',
    viewport: { width: 1280, height: 900 },
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'bun run dev --host 127.0.0.1',
    url: 'http://127.0.0.1:1420',
    reuseExistingServer: !process.env.CI,
  },
  reporter: 'list',
});
