'use strict';

const { defineConfig } = require('@playwright/test');

module.exports = defineConfig({
  testDir: './tests/browser',
  outputDir: './reports/playwright',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: 'line',
  use: {
    baseURL: 'http://127.0.0.1:1420',
    browserName: 'chromium',
    headless: true,
    viewport: { width: 1440, height: 1000 },
  },
  webServer: {
    command: 'python3 scripts/dev_server.py 1420 -d src',
    url: 'http://127.0.0.1:1420',
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
