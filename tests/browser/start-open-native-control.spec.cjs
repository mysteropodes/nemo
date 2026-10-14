'use strict';
// Desktop Open-card event semantics in the shipped classic-script load order.
// Chromium is a focused keyboard regression aid, not macOS AX or R24 parity.
const { test, expect } = require('@playwright/test');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');

test.use({ channel: 'chrome' });

test('Open Project invokes one chooser by click, Enter, or Space, and no chooser while disabled', async ({ browser }) => {
  test.setTimeout(60000);
  const runtime = await startBrowserRuntime({ taskId: `start-open-${process.pid}-${Date.now()}`, port: 0 });
  const context = await browser.newContext({ viewport: { width: 1200, height: 850 } });
  try {
    expect((await fetch(runtime.origin + IDENTITY_PATH).then((r) => r.json())).healthy).toBe(true);
    const page = await context.newPage();
    await page.goto(runtime.origin, { waitUntil: 'networkidle' });
    await expect.poll(() => page.evaluate(() => !!(window.SM && window.state && window.SMProject))).toBe(true);
    const button = page.locator('#start-open');
    await expect(button).toBeVisible();
    await expect(button).toHaveAttribute('type', 'button');
    await expect(button).toHaveAccessibleName(/Open Project/i);
    let chooserCount = 0;
    page.on('filechooser', () => { chooserCount += 1; });

    for (const activation of ['click', 'Enter', 'Space']) {
      const before = chooserCount;
      const chooser = page.waitForEvent('filechooser');
      if (activation === 'click') await button.click();
      else { await button.focus(); await button.press(activation); }
      await (await chooser).setFiles([]);
      await page.waitForTimeout(100);
      expect(chooserCount, `${activation} must open exactly one chooser`).toBe(before + 1);
      expect(await page.evaluate(() => state.spaceDown)).toBe(false);
    }

    await button.focus();
    const before = chooserCount;
    await button.press('q');
    await page.waitForTimeout(100);
    expect(chooserCount, 'an unrelated key must not open the chooser').toBe(before);
    await button.evaluate((element) => { element.disabled = true; });
    await button.press('Enter');
    await button.press('Space');
    await page.waitForTimeout(100);
    expect(chooserCount, 'disabled native button must not open the chooser').toBe(before);
  } finally {
    try { await context.close(); }
    finally { await runtime.close(); }
  }
});
