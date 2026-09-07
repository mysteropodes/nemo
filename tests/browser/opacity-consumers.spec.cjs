'use strict';

const { test, expect } = require('@playwright/test');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');
const root = path.resolve(__dirname, '../..');
const fixture = path.join(root, 'tests/animation/fixtures/curve-workflow.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

// Exercise the installed classic scripts and Paper SVG exporter in a real browser.
// This is browser document/export acceptance, not native or WebGPU pixel acceptance.
test.use({ channel: 'chrome' });

// Paper installs browser globals that conflict with Playwright's in-page poller.
async function until(page, predicate) {
  await expect.poll(() => page.evaluate(predicate), { timeout: 30000 }).toBe(true);
}

async function openProject(browser, origin, filename, contexts, errors) {
  const context = await browser.newContext({ viewport: { width: 1400, height: 1000 }, acceptDownloads: true });
  contexts.add(context);
  const page = await context.newPage();
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(origin, { waitUntil: 'networkidle' });
  try {
    await until(page, () => !!(window.SM && window.NemoApplication && window.state));
  } catch (error) {
    const readiness = await page.evaluate(() => ({ document: !!window.SM,
      application: !!window.NemoApplication, state: !!window.state }));
    throw new Error(`Application bootstrap did not become ready: ${JSON.stringify(readiness)}; page errors: ${errors.join('; ')}`, { cause: error });
  }
  const chooser = page.waitForEvent('filechooser');
  await page.locator('#start-open').click();
  await (await chooser).setFiles(filename);
  await until(page, () => state.layers[0]?.name === 'R08 rectangle');
  await page.locator('.app-mode-btn[data-mode="motion"]').click();
  const tutorial = page.locator('.tut-close');
  if (await tutorial.isVisible()) await tutorial.click();
  return { page, context };
}

async function command(page, operation, payload = {}) {
  return page.evaluate(({ operation, payload }) => {
    const meta = NemoOpacityApplication.meta();
    return NemoApplication.handle({ apiVersion: 1, requestId: crypto.randomUUID(), ...meta,
      expectedRevision: meta.revision, operation, payload });
  }, { operation, payload });
}

async function save(page, filename) {
  await page.locator('#btn-ff').click();
  const pending = page.waitForEvent('download');
  await page.keyboard.press(process.platform === 'darwin' ? 'Meta+s' : 'Control+s');
  const download = await pending;
  await download.saveAs(filename);
  expect(await download.failure()).toBeNull();
  return JSON.parse(fs.readFileSync(filename, 'utf8'));
}

async function exportedPixels(page, filename, frame) {
  await page.locator('#tl-cf').fill(String(frame + 1));
  await page.locator('#tl-cf').press('Tab');
  expect(await page.evaluate(() => state.currentFrame)).toBe(frame);
  // The work area has no numeric DOM controls; select only the export frame.
  await page.evaluate(f => { state.waIn = f; state.waOut = f; }, frame);
  await page.locator('#app-menu-btn').click();
  await page.locator('#ctx-export').click();
  await page.locator('#exp-format').selectOption('svg');
  await page.locator('#exp-range').selectOption('wa');
  const pending = page.waitForEvent('download');
  await page.locator('#exp-run').click();
  const download = await pending;
  await download.saveAs(filename);
  expect(await download.failure()).toBeNull();
  const bytes = fs.readFileSync(filename);
  const pixels = await page.evaluate(async url => {
    const image = new Image(); image.src = url; await image.decode();
    const canvas = document.createElement('canvas'); canvas.width = image.width; canvas.height = image.height;
    const ctx = canvas.getContext('2d'); ctx.fillStyle = '#fff'; ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(image, 0, 0);
    const rgba = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
    const colors = {}, bounds = [canvas.width, canvas.height, -1, -1];
    let count = 0;
    for (let i = 0; i < rgba.length; i += 4) {
      if (rgba[i] === 255 && rgba[i + 1] === 255 && rgba[i + 2] === 255 && rgba[i + 3] === 255) continue;
      const x = (i / 4) % canvas.width, y = Math.floor(i / 4 / canvas.width);
      bounds[0] = Math.min(bounds[0], x); bounds[1] = Math.min(bounds[1], y);
      bounds[2] = Math.max(bounds[2], x + 1); bounds[3] = Math.max(bounds[3], y + 1);
      colors[Array.from(rgba.slice(i, i + 4)).join(',')] = true; count++;
    }
    return { width: canvas.width, height: canvas.height, colors: Object.keys(colors), count, bounds };
  }, 'data:image/svg+xml;base64,' + bytes.toString('base64'));
  if (await page.locator('#export-close').isVisible()) await page.locator('#export-close').click();
  return { ...pixels, sha256: sha(bytes) };
}

test('opacity commands survive real save/reopen and preserve stored layers during keyed SVG export', async ({ browser }, testInfo) => {
  test.setTimeout(150000);
  const runtime = await startBrowserRuntime({ taskId: `opacity-consumers-${process.pid}-${Date.now()}`, port: 0 });
  const contexts = new Set(), errors = [], samples = [];
  try {
    const identity = await fetch(runtime.origin + IDENTITY_PATH).then(r => r.json());
    expect(identity.healthy).toBe(true);
    const sourceHashes = {};
    for (const file of ['src/index.html', 'src/js/application/opacity-application.js', 'src/js/bootstrap/opacity-application.js',
      'src/js/domain/animation/opacity.js', 'src/js/motion.js', 'src/js/project.js', 'src/js/export.js']) {
      const response = await fetch(runtime.origin + '/' + file.slice(4));
      expect(response.status).toBe(200);
      sourceHashes[file] = sha(fs.readFileSync(path.join(root, file)));
      expect(sha(Buffer.from(await response.arrayBuffer()))).toBe(sourceHashes[file]);
    }
    let { page, context } = await openProject(browser, runtime.origin, fixture, contexts, errors);
    const layerId = await page.evaluate(() => state.layers[0].layerUid);
    const payload = { layerId, property: 'opacity' };
    expect((await command(page, 'property.set', { ...payload, value: 25 })).ok).toBe(true);
    const staticPath = testInfo.outputPath('static.json'), staticSaved = await save(page, staticPath);
    expect(staticSaved.layers[0].layerUid).toBe(layerId);
    expect(staticSaved.layers[0].motionStatic.opacity).toEqual([25]);
    await context.close(); contexts.delete(context);
    ({ page, context } = await openProject(browser, runtime.origin, staticPath, contexts, errors));
    expect((await command(page, 'property.get', payload)).result.value).toBe(25);
    expect(await page.evaluate(() => JSON.parse(SM.exportJSON()).layers)).toEqual(staticSaved.layers);
    expect((await command(page, 'property.set', { ...payload, value: 40 })).ok).toBe(true);
    expect((await command(page, 'property.set', { ...payload, value: 60 })).ok).toBe(true);
    expect((await command(page, 'history.undo')).ok).toBe(true);
    const historyBeforeImport = await page.evaluate(() =>
      [state.undoStack, state.undoLabels, state.redoStack, state.redoLabels].map(stack => stack.length));
    expect(historyBeforeImport.every(count => count > 0)).toBe(true);
    const oldRequest = await page.evaluate(({ layerId }) => {
      const meta = NemoOpacityApplication.meta();
      return { apiVersion: 1, requestId: 'before-document-replacement', ...meta, expectedRevision: meta.revision,
        operation: 'property.set', payload: { layerId, property: 'opacity', value: 99 } };
    }, { layerId });
    // Real importer must rotate document identity even inside the same app instance.
    expect(await page.evaluate(json => SM.importJSON(json), JSON.stringify(staticSaved))).toBe(true);
    const replacement = await page.evaluate(request => NemoApplication.handle(request), oldRequest);
    expect(replacement.ok, 'the pre-import request must not mutate the replacement document').toBe(false);
    expect(replacement.error.code).toBe('wrong_document');
    expect(await page.evaluate(() =>
      [state.undoStack, state.undoLabels, state.redoStack, state.redoLabels].map(stack => stack.length)))
      .toEqual([0, 0, 0, 0]);
    for (const operation of ['history.undo', 'history.redo']) {
      const refused = await command(page, operation);
      expect(refused.ok).toBe(false);
      expect(refused.error.code).toBe('history_unavailable');
    }
    expect(await page.evaluate(() => JSON.parse(SM.exportJSON()).layers)).toEqual(staticSaved.layers);
    expect((await command(page, 'property.get', payload)).result.value).toBe(25);
    expect((await command(page, 'property.key.set', { ...payload, frame: 0, value: 20 })).ok).toBe(true);
    expect((await command(page, 'property.key.set', { ...payload, frame: 20, value: 80 })).ok).toBe(true);
    const savedPath = testInfo.outputPath('keyed.json'), saved = await save(page, savedPath);
    expect(saved.layers[0].motion.opacity.keys.map(k => [k.frame, k.v])).toEqual([[0, [20]], [20, [80]]]);
    await context.close(); contexts.delete(context);
    ({ page, context } = await openProject(browser, runtime.origin, savedPath, contexts, errors));
    expect(await page.evaluate(() => JSON.parse(SM.exportJSON()).layers)).toEqual(saved.layers);
    expect(await page.evaluate(() => state.layers[0].layerUid)).toBe(layerId);
    // Independent fixture oracle: smoothstep midpoint is exactly half; the red
    // 20x20 square translates 0/64/128px at frames 0/10/20, over a white backdrop.
    for (const [frame, opacity, left] of [[0, 20, 20], [10, 50, 84], [20, 80, 148]]) {
      expect((await command(page, 'property.get', { ...payload, frame })).result.value).toBeCloseTo(opacity, 8);
      const sample = await exportedPixels(page, testInfo.outputPath(`opacity-${frame}.svg`), frame);
      expect([sample.width, sample.height, sample.count]).toEqual([320, 180, 400]);
      expect(sample.bounds).toEqual([left, 60, left + 20, 80]);
      for (const color of sample.colors) {
        const [r, g, b, a] = color.split(',').map(Number);
        expect([r, a]).toEqual([255, 255]);
        expect(Math.abs(g - 255 * (1 - opacity / 100))).toBeLessThanOrEqual(1);
        expect(b).toBe(g);
      }
      samples.push({ frame, opacity, ...sample });
      expect(await page.evaluate(() => JSON.parse(SM.exportJSON()).layers)).toEqual(saved.layers);
    }
    expect(errors).toEqual([]);
    expect((await fetch(runtime.origin + IDENTITY_PATH).then(r => r.json())).healthy).toBe(true);
    await testInfo.attach('opacity-consumers', { body: JSON.stringify({ sourceHashes, layerId, samples,
      fixtureSha256: sha(fs.readFileSync(fixture)), staticSaved: true, freshContextReopened: true,
      storedLayersUnchanged: true, oldDocumentRejected: true, oldHistoryRejected: true }), contentType: 'application/json' });
  } finally {
    await testInfo.attach('page-errors', { body: JSON.stringify(errors), contentType: 'application/json' });
    try { await Promise.all([...contexts].map(context => context.close())); }
    finally { await runtime.close(); }
  }
});
