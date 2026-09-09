'use strict';

const { test, expect } = require('@playwright/test');
const { startPreview, withPreviews } = require('./preview-lifecycle.cjs');

// Deliberately fails (T03/#1052). Excluded from the normal suite by
// playwright.config.cjs's grepInvert — this proves the trace/HTML/JUnit
// pipeline actually captures a real failure, it is not itself a regression
// check. Run it on its own from repo root:
//   NEMO_BROWSER_NEGATIVE_CONTROL=1 node_modules/.bin/playwright test tests/browser
// Expect: failing exit code, a failing entry in playwright-report/junit.xml,
// an HTML report showing the failure, and a retained trace/screenshot/video
// for this test only. Reuses the same isolated preview harness as the real
// specs rather than a synthetic page, so the capture is proven against the
// actual app under real isolation.
test('harness negative control: intentional assertion failure @negative-control', async ({ browser }, testInfo) => {
  const runtimeRoot = testInfo.outputPath('runtime');
  const taskId = `playwright-negative-control-${process.pid}-${Date.now()}`;
  await withPreviews([() => startPreview(taskId, runtimeRoot)], async ([preview]) => {
    const context = await browser.newContext();
    try {
      const page = await context.newPage();
      await page.goto(preview.info.url, { waitUntil: 'domcontentloaded' });
      await expect(page.locator('#this-element-does-not-exist-by-design')).toBeAttached({ timeout: 2000 });
    } finally {
      await context.close();
    }
  });
});
