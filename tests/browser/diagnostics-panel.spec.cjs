'use strict';

const { test, expect } = require('@playwright/test');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');

test.use({ channel: 'chrome' });

test('browser compatibility Diagnostics panel reports reproduction unavailable without changing the document', async ({ browser }, testInfo) => {
  test.setTimeout(60000);
  const runtime = await startBrowserRuntime({ taskId: `diagnostics-panel-${process.pid}-${Date.now()}`, port: 0 });
  let context;
  const pageErrors = [];
  const downloads = [];
  try {
    const identity = await fetch(runtime.origin + IDENTITY_PATH).then((response) => response.json());
    expect(identity.healthy).toBe(true);
    context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
    await context.addInitScript(() => localStorage.setItem('nemo-tut-feedback-shown', '1'));
    const page = await context.newPage();
    page.on('pageerror', (error) => pageErrors.push(error.message));
    page.on('download', (download) => downloads.push(download));
    await page.goto(runtime.origin, { waitUntil: 'networkidle' });
    await expect.poll(() => page.evaluate(() => !!(window.SM && window.SMLabs && window.NemoApplication && window.state))).toBe(true);
    // The separated native baseline cannot create a writable browser
    // project. Dismiss only the landing overlay so the real Settings UI is
    // reachable; the document and history remain as booted.
    await page.evaluate(() => document.getElementById('start-screen').classList.add('hid'));

    const snapshot = () => page.evaluate(() => ({
      layers: JSON.stringify(state.layers),
      currentFrame: state.currentFrame,
      totalFrames: state.totalFrames,
      undo: JSON.stringify(state.undoStack),
      redo: JSON.stringify(state.redoStack),
      identity: NemoOpacityApplication.meta(),
    }));
    const before = await snapshot();

    const settings = page.locator('#settings-modal');
    const diagnostics = page.locator('#labs-diagnostics');
    const openLabs = async () => {
      await page.locator('#project-tabs-settings').click();
      await expect(settings).toBeVisible();
      await settings.locator('#settings-tabs .settings-tab[data-tab="labs"]').click();
      await expect(settings.locator('.settings-pane[data-pane="labs"]')).toBeVisible();
    };
    const checkbox = () => settings.locator('#labs-tools-list > div')
      .filter({ hasText: /diagnostics/i }).locator('input[type="checkbox"]');

    await expect(diagnostics).toHaveCount(0);
    await openLabs();
    await expect(checkbox()).toHaveCount(1);
    await expect(checkbox()).not.toBeChecked();
    await checkbox().check();
    await expect(checkbox()).toBeChecked();
    await settings.locator('#settings-close').click();
    await expect(diagnostics).toBeVisible();

    const refresh = diagnostics.locator('[data-diag-refresh]');
    await expect(refresh).toBeVisible();
    // This browser has no native opacity writer. The source-stage compatibility
    // inspector shows its empty v1 trace; T08B owns explicit native unavailability.
    await expect(diagnostics.locator('table')).toHaveCount(0);
    const emptyStatus = diagnostics.locator('[data-diag-empty]');
    await expect(emptyStatus).toBeVisible();
    const emptyLabel = await page.evaluate(() => SM.t('labsDiagnosticsEmpty'));
    await expect(emptyStatus).toHaveText(emptyLabel);
    await diagnostics.locator('[data-diag-report]').click();
    await expect(diagnostics.locator('[data-diag-report-status]')).toContainText('Reproduction unavailable');
    await expect(diagnostics.locator('[data-diag-report-status]')).toContainText('native synthetic recording');
    await testInfo.attach('source-stage-report-unavailable', { body: await diagnostics.screenshot(), contentType: 'image/png' });
    expect(await snapshot()).toEqual(before);
    await refresh.click();
    await expect(emptyStatus).toHaveText(emptyLabel);
    await expect(diagnostics.locator('[data-diag-report-status]')).toBeEmpty();
    await expect(diagnostics.locator('table')).toHaveCount(0);

    await openLabs();
    await checkbox().uncheck();
    await expect(checkbox()).not.toBeChecked();
    await expect(diagnostics).toHaveCount(0);
    await checkbox().check();
    await settings.locator('#settings-close').click();
    await expect(diagnostics).toBeVisible();
    await diagnostics.locator('[data-diag-refresh]').click();
    expect(await snapshot()).toEqual(before);
    expect(downloads).toEqual([]);
    expect(pageErrors).toEqual([]);
  } finally {
    if (context) await context.close();
    await runtime.close();
  }
});
