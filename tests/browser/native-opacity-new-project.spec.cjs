'use strict';

const { test, expect } = require('@playwright/test');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');

test.use({ channel: 'chrome' });

test('browser New Project explains native denial without changing the document', async ({ browser }) => {
  test.setTimeout(60000);
  const runtime = await startBrowserRuntime({ taskId: `n20l-new-project-${process.pid}-${Date.now()}`, port: 0 });
  let context;
  const pageErrors = [];
  try {
    const identity = await fetch(runtime.origin + IDENTITY_PATH).then(response => response.json());
    expect(identity.healthy).toBe(true);
    context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
    const page = await context.newPage();
    page.on('pageerror', error => pageErrors.push(error.message));
    await page.goto(runtime.origin, { waitUntil: 'networkidle' });
    await expect.poll(() => page.evaluate(() => !!(window.SMProject && window.NemoOpacityApplication && window.state))).toBe(true);
    const snapshot = () => page.evaluate(() => ({
      layers: JSON.stringify(state.layers), undo: JSON.stringify(state.undoStack),
      redo: JSON.stringify(state.redoStack), frame: state.currentFrame,
      meta: NemoOpacityApplication.meta(), label: SMProject.getCurrentLabel(),
      tabs: document.getElementById('project-tabs-list').children.length,
      autosave: localStorage.getItem('nemo-auto'),
    }));
    const before = await snapshot();
    expect(await page.evaluate(() => n20AllowLegacyWrite('new-project'))).toBe(false);

    await page.locator('#start-new').click();
    const form = page.locator('#start-newpanel');
    await expect(form).toBeVisible();
    await page.locator('#np-name').fill('Unadmitted browser project');
    await page.locator('#np-preset').selectOption('1280x720');
    await page.locator('#np-fps').selectOption('30');
    await page.locator('#np-create').click();

    await expect(form.locator('[role="alert"]')).toHaveText(/New project unavailable.*native admission was denied/i);
    await expect(form).toBeVisible();
    await expect(page.locator('#start-screen')).toBeVisible();
    await expect(page.locator('#np-name')).toHaveValue('Unadmitted browser project');
    await expect(page.locator('#np-preset')).toHaveValue('1280x720');
    await expect(page.locator('#np-fps')).toHaveValue('30');
    expect(await snapshot()).toEqual(before);
    expect(pageErrors).toEqual([]);
  } finally {
    if (context) await context.close();
    await runtime.close();
  }
});
