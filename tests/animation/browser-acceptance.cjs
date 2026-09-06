'use strict';
// Opt-in application acceptance, not a second unit suite. See the adjacent ADR.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { chromium } = require(process.env.NEMO_PLAYWRIGHT_MODULE || 'playwright');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');
const root = path.resolve(__dirname, '../..');
const out = path.resolve(process.argv[2] || path.join(root, 'reports/r08-browser-acceptance'));
const fixturePath = path.join(__dirname, 'fixtures/curve-workflow.json');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const frames = [0, 5, 10, 15, 20];
// Independent closed-form oracle; never imports/evaluates the production kernel.
const expected = (frame, edited) => {
  const t = frame / 20, ease = t * t * (3 - 2 * t);
  return (edited ? 64 : 0) + (edited ? 64 : 128) * ease;
};
const report = { schema: 'nemo.r08.browser-acceptance.v1', status: 'running',
  head: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
  fixtureSha256: hash(fs.readFileSync(fixturePath)), sourceHashes: {}, artifacts: [], phases: [], errors: [],
  limits: ['Controlled synthetic fixture; not the R03 production corpus.',
    'Fixture supplies initial geometry and keys; value edit, undo/redo, save/open and SVG export use real DOM controls.',
    'Rust readback and Paper PNG preview use production APIs. No Tauri, native dialogs, hardware GPU or video acceptance.'] };
fs.mkdirSync(out, { recursive: true });
function artifact(name, bytes) {
  fs.writeFileSync(path.join(out, name), bytes);
  const entry = { name, bytes: bytes.length, sha256: hash(bytes) };
  report.artifacts.push(entry); return entry;
}
// Runs against decoded output pixels, not the document's geometry or evaluator.
async function pixels(page, dataUrl) {
  return page.evaluate(async url => {
    const img = new Image(); img.src = url; await img.decode();
    const c = document.createElement('canvas'); c.width = img.width; c.height = img.height;
    const ctx = c.getContext('2d'); ctx.drawImage(img, 0, 0);
    const rgba = ctx.getImageData(0, 0, c.width, c.height).data;
    let red = 0, other = 0, x0 = c.width, y0 = c.height, x1 = -1, y1 = -1;
    for (let i = 0; i < rgba.length; i += 4) {
      if (rgba[i] === 255 && rgba[i + 1] === 0 && rgba[i + 2] === 0 && rgba[i + 3] === 255) {
        const p = i / 4, x = p % c.width, y = Math.floor(p / c.width);
        red++; x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y);
      } else if (!(rgba[i] === 255 && rgba[i + 1] === 255 && rgba[i + 2] === 255 && rgba[i + 3] === 255)) other++;
    }
    return { width: c.width, height: c.height, red, other, bounds: [x0, y0, x1 + 1, y1 + 1] };
  }, dataUrl);
}
function assertPixels(actual, frame, edited) {
  const left = 20 + expected(frame, edited);
  assert.deepEqual(actual, { width: 320, height: 180, red: 400, other: 0, bounds: [left, 60, left + 20, 80] });
}
async function seek(page, frame) {
  await page.locator('#tl-cf').fill(String(frame + 1));
  await page.locator('#tl-cf').press('Tab');
  assert.equal(await page.evaluate(() => state.currentFrame), frame);
}
async function phase(page, name, edited) {
  console.log(`Sampling ${name}`);
  const before = await page.evaluate(() => JSON.parse(SM.exportJSON()).layers);
  const samples = [];
  for (const frame of frames) {
    await seek(page, frame);
    const value = await page.evaluate(f => SMMotion.rawValueAtFrame(state.layers[0], 'position', f)[0], frame);
    assert.ok(Math.abs(value - expected(frame, edited)) < 1e-9, `${name} frame ${frame}: ${value}`);
    const rendered = await page.evaluate(async f => {
      SMEngineBridge.beginEffectsExport();
      try { return await SMEngineBridge.renderFrameToPixelsPNG(f, 1, false); }
      finally { SMEngineBridge.endEffectsExport(); }
    }, frame);
    const preview = await page.evaluate(f => SMExport.previewFrame(f, 1, false), frame);
    const rustPixels = await pixels(page, rendered), paperPixels = await pixels(page, preview);
    assertPixels(rustPixels, frame, edited); assertPixels(paperPixels, frame, edited);
    const png = artifact(`${name}-${frame}.png`, Buffer.from(rendered.split(',')[1], 'base64'));
    samples.push({ frame, positionX: value, rustPixels, paperPixels, pngSha256: png.sha256 });
  }
  assert.deepEqual(await page.evaluate(() => JSON.parse(SM.exportJSON()).layers), before, 'rendering must preserve stored layers');
  report.phases.push({ name, edited, samples, storedLayersUnchanged: true });
}
// Paper installs browser globals that conflict with Playwright's in-page poller.
// Poll from Node so the application environment is not changed for automation.
async function until(page, predicate, timeout = 30000) {
  const deadline = Date.now() + timeout;
  while (!await page.evaluate(predicate)) {
    assert.ok(Date.now() < deadline, 'application readiness timed out');
    await new Promise(resolve => setTimeout(resolve, 250));
  }
}
async function openFile(browser, origin, filename) {
  const context = await browser.newContext({ viewport: { width: 1400, height: 1000 }, acceptDownloads: true });
  const page = await context.newPage(); page.setDefaultTimeout(30000);
  page.on('pageerror', error => report.errors.push(error.message));
  await page.goto(origin, { waitUntil: 'networkidle' });
  // First software vello shader compilation can take over a minute.
  await until(page, () => window.SMEngineBridge && SMEngineBridge.isEnabled(), 180000);
  const adapter = await page.evaluate(async () => { const a = await navigator.gpu.requestAdapter();
    return { vendor: a.info.vendor, architecture: a.info.architecture, isFallbackAdapter: a.info.isFallbackAdapter }; });
  assert.equal(adapter.architecture, 'swiftshader'); assert.equal(adapter.isFallbackAdapter, true);
  report.adapter = adapter;
  const chooser = page.waitForEvent('filechooser'); await page.locator('#start-open').click();
  await (await chooser).setFiles(filename);
  await until(page, () => state.canvasW === 320 && state.layers[0].name === 'R08 rectangle');
  await page.locator('.app-mode-btn[data-mode="motion"]').click();
  const tutorial = page.locator('.tut-close');
  if (await tutorial.isVisible()) await tutorial.click();
  await page.locator('#layer-list .lrow[data-layer="0"] .lnm').first().click();
  await page.locator('#layer-list .lrow[data-layer="0"] .larrow').first().click();
  return { page, context };
}
async function main() {
  const runtime = await startBrowserRuntime({ taskId: `r08-browser-${process.pid}-${Date.now()}`, port: 0 });
  let browser;
  try {
    const identityBefore = await fetch(runtime.origin + IDENTITY_PATH).then(r => r.json());
    assert.equal(identityBefore.healthy, true); assert.equal(identityBefore.source.current.head, report.head);
    for (const file of ['src/index.html', 'src/js/motion.js', 'src/js/animation/curve.js', 'src/wasm/geometry_wasm.js', 'src/wasm/geometry_wasm_bg.wasm']) {
      const local = fs.readFileSync(path.join(root, file));
      const response = await fetch(runtime.origin + '/' + file.slice(4)); assert.equal(response.status, 200);
      assert.equal(hash(Buffer.from(await response.arrayBuffer())), hash(local)); report.sourceHashes[file] = hash(local);
    }
    browser = await chromium.launch({ channel: 'chrome', headless: true,
      args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--use-webgpu-adapter=swiftshader', '--enable-unsafe-webgpu'] });
    report.browser = browser.version();
    let { page, context } = await openFile(browser, runtime.origin, fixturePath);
    assert.equal(await page.evaluate(() => SMMotion.evalCurvePoints === SMAnimationCurve.evalCurvePoints), true);
    await phase(page, 'before', false);
    await seek(page, 0);
    const xField = page.locator('#motion-props-body .motion-prop-row').filter({ has: page.locator('.motion-prop-name', { hasText: 'Position' }) }).locator('input.motion-val').first();
    await xField.fill('64'); await xField.press('Tab');
    assert.equal(await page.evaluate(() => state.layers[0].motion.position.keys[0].v[0]), 64);
    await phase(page, 'edited', true);
    await page.locator('#btn-ff').click(); await page.keyboard.press('Meta+z');
    await phase(page, 'undo', false);
    await page.locator('#btn-ff').click(); await page.keyboard.press('Meta+Shift+z');
    await phase(page, 'redo', true);
    const layersBeforeSave = await page.evaluate(() => JSON.parse(SM.exportJSON()).layers);
    await page.locator('#btn-ff').click();
    const saving = page.waitForEvent('download'); await page.keyboard.press('Meta+s');
    const download = await saving; const savedPath = path.join(out, 'edited-project.json'); await download.saveAs(savedPath);
    const bytes = fs.readFileSync(savedPath), saved = JSON.parse(bytes);
    artifact('edited-project.json', bytes);
    assert.deepEqual(saved.layers, layersBeforeSave);
    report.save = { suggestedFilename: download.suggestedFilename(), storedLayersEqual: true, keys: saved.layers[0].motion.position.keys };
    await context.close();
    ({ page, context } = await openFile(browser, runtime.origin, savedPath));
    assert.deepEqual(await page.evaluate(() => JSON.parse(SM.exportJSON()).layers), saved.layers);
    report.reopen = { freshBrowserContext: true, autosaveNotShared: true, storedLayersEqual: true };
    await phase(page, 'reopened', true);
    report.svg = [];
    for (const frame of frames) {
      await seek(page, frame);
      // Work-area range has no numeric controls. This selects the export frame only.
      await page.evaluate(f => { state.waIn = f; state.waOut = f; }, frame);
      await page.locator('#app-menu-btn').click(); await page.locator('#ctx-export').click();
      await page.locator('#exp-format').selectOption('svg'); await page.locator('#exp-range').selectOption('wa');
      const exporting = page.waitForEvent('download'); await page.locator('#exp-run').click();
      const svgDownload = await exporting; const svgPath = path.join(out, `export-${frame}.svg`); await svgDownload.saveAs(svgPath);
      const svg = fs.readFileSync(svgPath), measured = await pixels(page, 'data:image/svg+xml;base64,' + svg.toString('base64'));
      assertPixels(measured, frame, true);
      report.svg.push({ frame, pixels: measured, ...artifact(`export-${frame}.svg`, svg) });
      if (await page.locator('#export-close').isVisible()) await page.locator('#export-close').click();
    }
    assert.deepEqual(await page.evaluate(() => JSON.parse(SM.exportJSON()).layers), saved.layers, 'SVG export must preserve layers');
    await seek(page, 5);
    await page.locator('#btn-ff').focus(); await page.keyboard.press('Meta+Alt+0');
    const canvas = await page.locator('#rust-canvas').boundingBox();
    const viewPoints = await page.evaluate(() => [new Point(0, 0), new Point(320, 180), new Point(280, 140)]
      .map(p => { const v = view.projectToView(p); return { x: v.x, y: v.y }; }));
    await page.mouse.click(canvas.x + viewPoints[2].x, canvas.y + viewPoints[2].y);
    await page.mouse.move(10, 10);
    await page.screenshot({ path: path.join(out, 'reopened-frame-5.png') });
    artifact('reopened-frame-5.png', fs.readFileSync(path.join(out, 'reopened-frame-5.png')));
    report.pngSequenceBrowser = await page.evaluate(() => SMExport.exportPNGSequence({ start: 0, end: 20 }));
    assert.equal(report.pngSequenceBrowser.ok, false);
    const identityAfter = await fetch(runtime.origin + IDENTITY_PATH).then(r => r.json());
    assert.equal(identityAfter.healthy, true);
    report.identity = { healthyBefore: true, healthyAfter: true, sourceMatches: identityAfter.source.matches, buildMatches: identityAfter.build.matches };
    assert.deepEqual(report.errors, []); report.status = 'pass';
  } finally {
    if (browser) await browser.close(); await runtime.close();
    fs.writeFileSync(path.join(out, 'receipt.json'), JSON.stringify(report, null, 2) + '\n');
  }
}
main().then(() => console.log(JSON.stringify({ status: report.status, phases: report.phases.length, exports: report.svg.length, out }))).catch(error => {
  report.status = 'fail'; report.failure = error.stack; fs.writeFileSync(path.join(out, 'receipt.json'), JSON.stringify(report, null, 2) + '\n');
  console.error(error); process.exitCode = 1;
});
