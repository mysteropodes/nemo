'use strict';

const { test, expect } = require('@playwright/test');
const path = require('node:path');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');
const root = path.resolve(__dirname, '../..');
const fixture = path.join(root, 'tests/animation/fixtures/curve-workflow.json');

// opacity-consumers.spec.cjs exercises the opacity feature entirely through
// NemoApplication.handle('property.set', ...), never through the real Motion
// panel widget a person actually drags. This covers that gap: a genuine
// pointer-scrub gesture (CLAUDE.md §10) on the layer's own Opacity field.
test.use({ channel: 'chrome' });

async function until(page, predicate) {
  await expect.poll(() => page.evaluate(predicate), { timeout: 30000 }).toBe(true);
}

async function openProject(browser, origin, filename, contexts, errors) {
  const context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
  contexts.add(context);
  const page = await context.newPage();
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(origin, { waitUntil: 'networkidle' });
  await until(page, () => !!(window.SM && window.NemoApplication && window.state));
  const chooser = page.waitForEvent('filechooser');
  await page.locator('#start-open').click();
  await (await chooser).setFiles(filename);
  await until(page, () => state.layers[0]?.name === 'R08 rectangle');
  await page.locator('.app-mode-btn[data-mode="motion"]').click();
  const tutorial = page.locator('.tut-close');
  if (await tutorial.isVisible()) await tutorial.click();
  return page;
}

async function command(page, operation, payload = {}) {
  return page.evaluate(({ operation, payload }) => {
    const meta = NemoOpacityApplication.meta();
    return NemoApplication.handle({ apiVersion: 1, requestId: crypto.randomUUID(), ...meta,
      expectedRevision: meta.revision, operation, payload });
  }, { operation, payload });
}

// motion.js tags each property row with the holder/prop it renders
// (renderTransformProps: `pr._smHolder = holder; pr._smProp = prop;`) rather
// than a DOM id, so the row is located the same way the app itself does.
async function locateOpacityField(page) {
  return page.evaluate(() => {
    const holder = state.layers[0];
    const row = Array.from(document.querySelectorAll('#layer-list .motion-prop-row'))
      .find(el => el._smHolder === holder && el._smProp === 'opacity');
    const input = row && row.querySelector('input.scrub');
    if (!input) return null;
    // The Motion panel is a short fixed-height scroller (CLAUDE.md §11) —
    // Opacity is near the bottom of the base property list, off-screen
    // until scrolled into view, so its real click/drag point is off-page.
    input.scrollIntoView({ block: 'center' });
    const rect = input.getBoundingClientRect();
    return { value: parseFloat(input.value), x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  });
}

test('dragging the real Motion Opacity field edits the layer in one undoable step', async ({ browser }) => {
  test.setTimeout(60000);
  const runtime = await startBrowserRuntime({ taskId: `opacity-ui-consumers-${process.pid}-${Date.now()}`, port: 0 });
  const contexts = new Set(), errors = [];
  try {
    const identity = await fetch(runtime.origin + IDENTITY_PATH).then(r => r.json());
    expect(identity.healthy).toBe(true);
    const page = await openProject(browser, runtime.origin, fixture, contexts, errors);
    const layerId = await page.evaluate(() => state.layers[0].layerUid);
    const payload = { layerId, property: 'opacity' };

    // Selecting a layer row and opening its Transform twirl are two
    // deliberately separate gestures (motion.js, 2026-07-27 "selecting no
    // longer expands") — both are required before the Opacity field exists.
    await page.locator('#layer-list .lrow[data-layer="0"]').click();
    await page.locator('#layer-list .lrow[data-layer="0"] .larrow').click();

    const before = await locateOpacityField(page);
    expect(before).not.toBeNull();
    expect(before.value).toBe(100); // PROP_DEFAULT.opacity; fixture sets no motionStatic/motion for it
    const historyBefore = await page.evaluate(() => [state.undoStack.length, state.redoStack.length]);

    // ui.js's scrub math: raw = startVal + Math.round(dx/4) * step, step=1
    // for this field (no data-step set) — dx chosen as an exact multiple of
    // 4 so the drag lands precisely on `target`, not merely near it.
    const target = 55;
    const dx = (target - before.value) * 4;
    await page.mouse.move(before.x, before.y);
    await page.mouse.down();
    await page.mouse.move(before.x + dx * 0.3, before.y);
    await page.mouse.move(before.x + dx, before.y);
    await page.mouse.up();

    const after = await locateOpacityField(page);
    expect(after.value).toBe(target);
    expect((await command(page, 'property.get', payload)).result.value).toBe(target);

    const historyAfter = await page.evaluate(() => [state.undoStack.length, state.redoStack.length]);
    expect(historyAfter).toEqual([historyBefore[0] + 1, historyBefore[1]]);

    expect((await command(page, 'history.undo')).ok).toBe(true);
    expect((await command(page, 'property.get', payload)).result.value).toBe(before.value);
    expect((await command(page, 'history.redo')).ok).toBe(true);
    expect((await command(page, 'property.get', payload)).result.value).toBe(target);

    expect(errors).toEqual([]);
    expect((await fetch(runtime.origin + IDENTITY_PATH).then(r => r.json())).healthy).toBe(true);
  } finally {
    await Promise.all([...contexts].map(context => context.close()));
    await runtime.close();
  }
});
