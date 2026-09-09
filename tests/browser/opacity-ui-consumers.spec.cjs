'use strict';

const { test, expect } = require('@playwright/test');
const path = require('node:path');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');
const root = path.resolve(__dirname, '../..');
const fixture = path.join(root, 'tests/animation/fixtures/curve-workflow.json');

// opacity-consumers.spec.cjs (this directory) drives the LAYER opacity
// feature through NemoApplication.handle() end-to-end (save/reopen/keyed SVG
// export). It never touches the real Motion "Opacity" property row a human
// actually drags. This file is that missing real-cursor-gesture path: it
// drives ui.js's generic scrub mechanism (CLAUDE.md §10/§11) with an actual
// Playwright pointer drag on the live <input class="pi scrub motion-val">
// field, and checks the single-undo-entry contract that mechanism promises
// (window._scrubLiveActive brackets the whole gesture to exactly one
// pushUndo). The command API is used only as an independent read-side
// oracle, never to drive the gesture itself.
test.use({ channel: 'chrome' });

async function until(page, predicate) {
  await expect.poll(() => page.evaluate(predicate), { timeout: 30000 }).toBe(true);
}

async function command(page, operation, payload = {}) {
  return page.evaluate(({ operation, payload }) => {
    const meta = NemoOpacityApplication.meta();
    return NemoApplication.handle({ apiVersion: 1, requestId: crypto.randomUUID(), ...meta,
      expectedRevision: meta.revision, operation, payload });
  }, { operation, payload });
}

test('a real pointer-drag on the Motion Opacity row commits one value and exactly one undo entry', async ({ browser }) => {
  test.setTimeout(60000);
  const runtime = await startBrowserRuntime({ taskId: `opacity-ui-consumers-${process.pid}-${Date.now()}`, port: 0 });
  const errors = [];
  let context;
  try {
    const identity = await fetch(runtime.origin + IDENTITY_PATH).then(r => r.json());
    expect(identity.healthy).toBe(true);
    context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
    const page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(runtime.origin, { waitUntil: 'networkidle' });
    await until(page, () => !!(window.SM && window.NemoApplication && window.state));
    const chooser = page.waitForEvent('filechooser');
    await page.locator('#start-open').click();
    await (await chooser).setFiles(fixture);
    await until(page, () => state.layers[0]?.name === 'R08 rectangle');
    await page.locator('.app-mode-btn[data-mode="motion"]').click();
    const tutorial = page.locator('.tut-close');
    if (await tutorial.isVisible()) await tutorial.click();

    const layerId = await page.evaluate(() => state.layers[0].layerUid);
    const payload = { layerId, property: 'opacity' };

    // Select the layer for real, the way a person would: click its row in
    // the Motion layer list. This is what populates #motion-props-body
    // (renderMotionPropsPanel reads _layerSel/state.activeLayerIdx), not a
    // direct state write.
    await page.locator('#layer-list .lnm', { hasText: 'R08 rectangle' }).click();
    const opacityRow = page.locator('#motion-props-body .motion-prop-row')
      .filter({ has: page.locator('.motion-prop-name', { hasText: /^Opacity$/ }) });
    const field = opacityRow.locator('.motion-val');
    await expect(field).toBeVisible();

    const startVal = parseFloat(await field.inputValue());
    const sanity = await command(page, 'property.get', payload);
    expect(sanity.ok).toBe(true);
    expect(sanity.result.value).toBe(startVal);
    const undoBefore = await page.evaluate(() => state.undoStack.length);

    const box = await field.boundingBox();
    expect(box).not.toBeNull();
    const startX = box.x + box.width / 2, y = box.y + box.height / 2;
    // Independent oracle: ui.js's scrub formula (CLAUDE.md §10/§11) is
    // raw = startVal + round(dx/4)*step; this field has no data-step, so
    // step defaults to 1 and an 80px drag is exactly -20, computed here from
    // the documented mechanism, not from anything this test just wrote.
    const dx = -80;
    const expected = startVal + Math.round(dx / 4);

    await page.mouse.move(startX, y);
    await page.mouse.down();
    await page.mouse.move(startX + dx, y);
    await page.mouse.up();

    await expect(field).toHaveValue(String(expected));
    const afterDrag = await command(page, 'property.get', payload);
    expect(afterDrag.ok).toBe(true);
    expect(afterDrag.result.value).toBe(expected);

    // One gesture, one undo entry (not one per pointermove/change tick), and
    // redo is cleared by the fresh entry like any other edit.
    const [undoAfter, redoAfter] = await page.evaluate(() => [state.undoStack.length, state.redoStack.length]);
    expect(undoAfter).toBe(undoBefore + 1);
    expect(redoAfter).toBe(0);

    expect((await command(page, 'history.undo')).ok).toBe(true);
    const afterUndo = await command(page, 'property.get', payload);
    expect(afterUndo.result.value).toBe(startVal);

    expect((await command(page, 'history.redo')).ok).toBe(true);
    const afterRedo = await command(page, 'property.get', payload);
    expect(afterRedo.result.value).toBe(expected);

    expect(errors).toEqual([]);
    expect((await fetch(runtime.origin + IDENTITY_PATH).then(r => r.json())).healthy).toBe(true);
  } finally {
    if (context) await context.close();
    await runtime.close();
  }
});
