'use strict';
const { test, expect } = require('@playwright/test');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');
const mediaProject = require('../fixtures/media/project.json');
const componentProject = require('../fixtures/components/project.json');

test.use({ channel: 'chrome' });

test('production Media and Transplant consumers retain rows and selection across real folder clicks', async ({ browser }) => {
  test.setTimeout(90000);
  const runtime = await startBrowserRuntime({ taskId: `asset-tree-${process.pid}-${Date.now()}`, port: 0 });
  let context;
  const pageErrors = [];
  try {
    const identity = await fetch(runtime.origin + IDENTITY_PATH).then(response => response.json());
    expect(identity.healthy).toBe(true);
    context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
    await context.addInitScript(() => localStorage.setItem('nemo-tut-feedback-shown', '1'));
    const page = await context.newPage();
    page.on('pageerror', error => pageErrors.push(error.message));
    await page.goto(runtime.origin, { waitUntil: 'networkidle' });
    await expect.poll(() => page.evaluate(() => !!(window.SMAssetTree && window.SMMediaLibrary && window.state && window.SMProject))).toBe(true);
    expect(await page.evaluate(() => Object.hasOwn(window, 'createAssetTree'))).toBe(false);
    await page.evaluate(() => document.getElementById('start-screen').classList.add('hid'));
    if (await page.locator('#assets-sec > .pbdy').evaluate(element => element.classList.contains('hid'))) {
      await page.locator('#assets-sec > .phdr').click();
    }

    // Controlled presentation inputs, then the unmodified production render().
    // The click oracle starts after setup, so it tests no document/history write.
    await page.evaluate(entries => {
      state.symbols = { p35_component: { name: 'P35 Component' } };
      state.mediaLibrary = entries.concat([
        { id: 'p35_video', name: 'P35 Video', kind: 'video', status: 'loading' },
        { id: 'p35_audio', name: 'P35 Audio', kind: 'audio', status: 'loading' },
      ]);
      window.__p35GetTabs = SMProject.getOpenTabs;
      SMProject.getOpenTabs = () => [{ id: 'p35_tab', name: 'P35 Composition' }];
      SMMediaLibrary.reload();
    }, mediaProject.mediaLibrary);
    const snapshot = () => page.evaluate(() => JSON.stringify({
      layers: state.layers, symbols: state.symbols, mediaLibrary: state.mediaLibrary,
      undo: state.undoStack, redo: state.redoStack, activeLayerIdx: state.activeLayerIdx,
    }));
    const beforeMediaClick = await snapshot();
    const mediaFolders = page.locator('#media-grid > .asset-folder');
    await expect(mediaFolders).toHaveCount(5);
    await expect(mediaFolders.nth(0).locator('.asset-folder-body .bp-item')).toContainText('P35 Component');
    await expect(mediaFolders.nth(1).locator('.asset-folder-body .bp-item')).toContainText('P35 Composition');
    await expect(mediaFolders.nth(2).locator('.asset-folder-body .media-row, .asset-folder-body .media-tile')).toHaveCount(mediaProject.mediaLibrary.length);
    await expect(mediaFolders.nth(3).locator('.asset-folder-body')).toContainText('P35 Video');
    await expect(mediaFolders.nth(4).locator('.asset-folder-body')).toContainText('P35 Audio');
    const mediaBody = mediaFolders.nth(2).locator('.asset-folder-body');
    const mediaText = await mediaBody.textContent();
    await mediaFolders.nth(2).locator('.asset-folder-hdr').click();
    await expect(mediaBody).toHaveClass(/collapsed/);
    await mediaFolders.nth(2).locator('.asset-folder-hdr').click();
    await expect(mediaBody).not.toHaveClass(/collapsed/);
    expect(await mediaBody.textContent()).toBe(mediaText);
    expect(await snapshot()).toBe(beforeMediaClick);

    const foreign = {
      layers: [componentProject.layers[0], mediaProject.layers[0]],
      mediaLibrary: mediaProject.mediaLibrary,
    };
    const beforeTransplant = await snapshot();
    await page.locator('#btn-transplant-open').click();
    const chooser = page.waitForEvent('filechooser');
    await page.locator('#tp-pick').click();
    await (await chooser).setFiles({ name: 'p35-foreign.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(foreign)) });
    await expect(page.locator('#tp-layer-list .asset-folder')).toHaveCount(2);
    await expect(page.locator('#tp-media-list .asset-folder')).toHaveCount(1);
    const layerCheckbox = page.locator('#tp-layer-list .asset-folder').first().locator('input[type="checkbox"]');
    const mediaCheckbox = page.locator('#tp-media-list .asset-folder').first().locator('input[type="checkbox"]').first();
    await layerCheckbox.check();
    await mediaCheckbox.check();
    for (const selector of ['#tp-layer-list', '#tp-media-list']) {
      const folder = page.locator(`${selector} .asset-folder`).first();
      await folder.locator('.asset-folder-hdr').click();
      await expect(folder.locator('.asset-folder-body')).toHaveClass(/collapsed/);
      await folder.locator('.asset-folder-hdr').click();
      await expect(folder.locator('.asset-folder-body')).not.toHaveClass(/collapsed/);
    }
    await expect(layerCheckbox).toBeChecked();
    await expect(mediaCheckbox).toBeChecked();
    expect(await snapshot()).toBe(beforeTransplant);
    expect(pageErrors).toEqual([]);
    expect((await fetch(runtime.origin + IDENTITY_PATH).then(response => response.json())).healthy).toBe(true);
    await page.evaluate(() => { SMProject.getOpenTabs = window.__p35GetTabs; delete window.__p35GetTabs; });
  } finally {
    if (context) await context.close();
    await runtime.close();
  }
});
