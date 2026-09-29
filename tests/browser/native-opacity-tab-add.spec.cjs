'use strict';

const { test, expect } = require('@playwright/test');
const path = require('node:path');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');

const fixture = path.resolve(__dirname, '../animation/fixtures/curve-workflow.json');
test.use({ channel: 'chrome' });

test('editor tab add explains native-only browser denial without changing the open project', async ({ browser }) => {
  test.setTimeout(60000);
  const runtime = await startBrowserRuntime({ taskId: `n20m-tab-add-${process.pid}-${Date.now()}`, port: 0 });
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
    const chooser = page.waitForEvent('filechooser');
    await page.locator('#start-open').click();
    await (await chooser).setFiles(fixture);
    await expect(page.locator('#start-screen')).toBeHidden();
    await expect.poll(() => page.evaluate(() => window.state.layers[0]?.layerUid)).toBe('r08_curve_layer');
    if (await page.locator('.tut-skip').isVisible()) await page.locator('.tut-skip').click();
    const snapshot = () => page.evaluate(() => ({
      layers: JSON.stringify(state.layers), undo: JSON.stringify(state.undoStack),
      redo: JSON.stringify(state.redoStack), frame: state.currentFrame,
      meta: NemoOpacityApplication.meta(), label: SMProject.getCurrentLabel(),
      tabs: document.getElementById('project-tabs-list').children.length,
      autosave: localStorage.getItem('nemo-auto'),
    }));
    const before = await snapshot();
    expect(await page.evaluate(() => typeof window.__TAURI__)).toBe('undefined');
    await page.locator('#project-tab-add').click();
    await expect(page.locator('#project-tabs-bar [role="alert"]')).toBeVisible();
    await expect(page.locator('#project-tabs-bar [role="alert"]')).toHaveText(/New project unavailable.*native admission was denied/i);
    expect(await snapshot()).toEqual(before);
    expect(pageErrors).toEqual([]);
  } finally {
    if (context) await context.close();
    await runtime.close();
  }
});
