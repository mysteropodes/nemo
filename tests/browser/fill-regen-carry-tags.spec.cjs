'use strict';

const { test, expect } = require('@playwright/test');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');

// A regenerated paint-bucket fill must inherit the identity/ownership tags of
// the fill it replaces (FILL_REGEN_CARRY_TAGS, tools.js). The case that
// motivated this: a bucket fill grouped with its own walls under a combine
// mode fell OUT of its group the first time the group was moved, because
// fillRegenerateLinked re-traced the region into a brand-new Path and wrote
// only the fill* tracking tags onto it.
//
// This has to be a browser spec, not a tests/*.test.cjs one: the defect only
// exists once Paper.js, the real tools and a real pointer drag are in play.
test.use({ channel: 'chrome' });

// Paper installs browser globals that conflict with Playwright's in-page
// poller (`this.setItem is not a function`) — poll from outside the page.
async function until(page, predicate) {
  await expect.poll(() => page.evaluate(predicate), { timeout: 30000 }).toBe(true);
}

// Screen pixels depend on the project size and the fitted zoom, so anchor
// every gesture in DOCUMENT coordinates and convert through Paper's own view
// transform — a hardcoded client point lands somewhere else the moment the
// canvas or viewport changes.
async function at(page, x, y) {
  return page.evaluate(([px, py]) => {
    const p = view.projectToView(new paper.Point(px, py));
    const r = document.querySelector('#drawing-canvas').getBoundingClientRect();
    return { x: r.left + p.x, y: r.top + p.y };
  }, [x, y]);
}

async function tool(page, name) {
  await page.click(`button.tool-btn[data-tool="${name}"]`);
  expect(await page.evaluate(() => state.tool)).toBe(name);
}

// A real drag, with intermediate moves — a bare down+up does not run the
// move handlers this defect lives in.
async function drag(page, from, to, steps = 12) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  for (let i = 1; i <= steps; i++) {
    await page.mouse.move(from.x + (to.x - from.x) * i / steps,
                          from.y + (to.y - from.y) * i / steps);
    await page.waitForTimeout(8);
  }
  await page.mouse.up();
  await page.waitForTimeout(60);
}

const bucketFill = () => {
  const layer = userLayers[state.activeLayerIdx];
  const fill = layer.children.filter(c => c.data && c.data.fillSeed)[0];
  if (!fill) return null;
  return {
    strokeId: fill.data.strokeId || null,
    groupId: fill.data.groupId || null,
    fillWalls: fill.data.fillWalls || null,
    fillGapPx: fill.data.fillGapPx,
    fillSeed: fill.data.fillSeed.slice(),
    fillGradient: !!fill.data.fillGradient,
    ownerId: fill.data.ownerId || null,
    paintOrder: fill.data.paintOrder || null,
    ownerName: fill.data.ownerName || null,
    ownerColor: fill.data.ownerColor || null,
    effects: fill.data.effects || null,
    elemHidden: !!fill.data.elemHidden,
    visible: fill.visible,
  };
};

test('a regenerated bucket fill keeps its group membership and identity', async ({ browser }) => {
  const runtime = await startBrowserRuntime({ taskId: `fill-regen-carry-${process.pid}-${Date.now()}`, port: 0 });
  const contexts = new Set();
  const errors = [];
  try {
    expect((await fetch(runtime.origin + IDENTITY_PATH).then(r => r.json())).healthy).toBe(true);
    const context = await browser.newContext({ viewport: { width: 1500, height: 1000 } });
    contexts.add(context);
    const page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(runtime.origin, { waitUntil: 'networkidle' });
    await until(page, () => !!(window.SM && window.state && window.userLayers));
    // New Project raises its own canvas-size dialog, which must be confirmed
    // before the start screen goes away.
    await page.locator('#start-new').click();
    await page.locator('#np-create').click();
    for (const sel of ['.tut-close', '#start-close']) {
      const el = page.locator(sel);
      if (await el.count() && await el.first().isVisible().catch(() => false)) await el.first().click().catch(() => {});
    }
    // The start screen intercepts canvas pointer events while it is up; wait
    // for it to actually stop being hittable rather than for a style value.
    await expect(page.locator('#start-screen')).toBeHidden({ timeout: 30000 });

    // Two overlapping rectangles, then a bucket fill in the overlap: the fill
    // is wall-bound to BOTH rectangles, so moving either one regenerates it.
    await tool(page, 'rect');
    await drag(page, await at(page, 400, 300), await at(page, 1000, 700));
    await drag(page, await at(page, 800, 500), await at(page, 1400, 900));
    await tool(page, 'fill');
    const seed = await at(page, 900, 600); // inside the overlap of the two rects
    await page.mouse.click(seed.x, seed.y);
    await until(page, () => userLayers[state.activeLayerIdx].children.some(c => c.data && c.data.fillSeed));

    // Stamp the tags a real fill carries but that regeneration does not
    // re-derive. groupId is stamped rather than produced through SMGroup on
    // purpose: grouping makes the walls and the fill move RIGIDLY together,
    // and select-bridge then unlinks fill regen deliberately
    // (fsUnlinkFillRegen, documented) — so the grouped whole-group move is
    // not the gesture that regenerates. The gesture that does is "a
    // neighbouring wall moves while the fill is not itself selected".
    const before = await page.evaluate(() => {
      const fill = userLayers[state.activeLayerIdx].children.filter(c => c.data && c.data.fillSeed)[0];
      fill.data.groupId = 'carry-tag-group';
      fill.data.fillGradient = { stops: [['#ff0000ff', 0], ['#0000ffff', 1]], origin: [0, 0], destination: [10, 10] };
      fill.data.ownerId = 'carry-tag-witness';
      fill.data.paintOrder = 'strokeFirst';
      fill.data.ownerName = 'carry-tag-owner';
      fill.data.ownerColor = '#abcdef';
      fill.data.effects = [{ type: 'carry-tag-effect' }];
      fill.data.elemHidden = true;
      fill.visible = false;
      return {
        strokeId: fill.data.strokeId || null, groupId: fill.data.groupId,
        fillSeed: fill.data.fillSeed.slice(), fillWalls: (fill.data.fillWalls || []).slice(),
        fillGapPx: fill.data.fillGapPx,
      };
    });
    expect(before.strokeId).not.toBeNull();
    expect(before.fillWalls).toHaveLength(2);

    // Select ONLY the second rectangle — one of the fill's two walls — and
    // move it. The fill is not in the selection, so it is not unlinked; it is
    // re-traced against the wall's new position.
    await tool(page, 'select');
    await page.mouse.click((await at(page, 1300, 850)).x, (await at(page, 1300, 850)).y);
    await until(page, () => selectedPaths.length === 1 && !(selectedPaths[0].data || {}).fillSeed);
    await drag(page, await at(page, 1300, 850), await at(page, 1340, 870));

    const after = await page.evaluate(bucketFill);
    expect(after, 'the bucket fill still exists after the wall moved').not.toBeNull();
    // Proof a regeneration actually ran, rather than the fill being left
    // untouched (which would make the rest of this test vacuous): the seed is
    // re-anchored to the new region's own interior point.
    expect(after.fillSeed).not.toEqual(before.fillSeed);
    // The regression: identity and the non-re-derived tags come along.
    expect(after.strokeId).toBe(before.strokeId);
    expect(after.groupId).toBe('carry-tag-group');
    expect(after.fillGradient).toBe(true);
    expect(after.ownerId).toBe('carry-tag-witness');
    expect(after.paintOrder).toBe('strokeFirst');
    expect(after.ownerName).toBe('carry-tag-owner');
    expect(after.ownerColor).toBe('#abcdef');
    expect(after.effects).toEqual([{ type: 'carry-tag-effect' }]);
    // elemHidden is the one carried tag that also drives a live property, so
    // assert BOTH halves: carrying the tag without visible=false would show a
    // hidden element again (data says hidden, screen says otherwise).
    expect(after.elemHidden).toBe(true);
    expect(after.visible).toBe(false);
    // The fill* tags were already correct before this fix — assert them too so
    // a future change to the carry-over cannot quietly break them either.
    expect(after.fillWalls).toEqual(before.fillWalls);
    expect(after.fillGapPx).toBe(before.fillGapPx);

    // And it must be PERSISTED that way: the live item being right while the
    // saved frame disagrees is the half of this bug that survives a reload
    // (CLAUDE.md §1 — saveActiveLayerFrame is the critical consumer).
    const saved = await page.evaluate(() => {
      const ld = state.layers[state.activeLayerIdx];
      const frame = ld.frames[state.currentFrame];
      return (frame && frame.strokes || []).filter(s => s.fillSeed)
        .map(s => ({ groupId: s.groupId || null, strokeId: s.strokeId || null }));
    });
    expect(saved).toHaveLength(1);
    expect(saved[0].groupId).toBe('carry-tag-group');
    expect(saved[0].strokeId).toBe(before.strokeId);

    expect(errors).toEqual([]);
  } finally {
    for (const context of contexts) await context.close().catch(() => {});
    await runtime.close();
  }
});
