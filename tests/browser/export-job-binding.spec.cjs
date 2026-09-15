'use strict';
// P19/#1021 in a real browser: one real UI interaction and one actual MCP
// client reach the same job and the same artifact.
//
// The Node suite (tests/application-export-job-binding.test.cjs) proves the
// cross-path effects — busy, shared cancellation, shared cleanup — against the
// real sources. What only a real runtime can prove is what this file checks:
// that the installed classic scripts boot in load order with the export
// capability registered, that discovery reports it, and that the bytes an MCP
// client receives are the bytes the export dialog downloads for the same frame.

const { test, expect } = require('@playwright/test');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');

const root = path.resolve(__dirname, '../..');
const fixture = path.join(root, 'tests/animation/fixtures/curve-workflow.json');
const sha = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');

test.use({ channel: 'chrome' });

async function until(page, predicate) {
  await expect.poll(() => page.evaluate(predicate), { timeout: 30000 }).toBe(true);
}

async function openProject(browser, origin, contexts, errors) {
  const context = await browser.newContext({ viewport: { width: 1400, height: 1000 }, acceptDownloads: true });
  contexts.add(context);
  const page = await context.newPage();
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto(origin, { waitUntil: 'networkidle' });
  await until(page, () => !!(window.SM && window.NemoApplication && window.state));
  const chooser = page.waitForEvent('filechooser');
  await page.locator('#start-open').click();
  await (await chooser).setFiles(fixture);
  await until(page, () => state.layers[0]?.name === 'R08 rectangle');
  const tutorial = page.locator('.tut-close');
  if (await tutorial.isVisible()) await tutorial.click();
  return { page, context };
}

// An actual MCP client request: the same envelope adapters/application-mcp.js
// forwards from the native transport, handed to the same entry point.
async function command(page, operation, payload = {}) {
  return page.evaluate(({ operation, payload }) => {
    const meta = NemoOpacityApplication.meta();
    return NemoApplication.handle({ apiVersion: 1, requestId: crypto.randomUUID(), ...meta,
      expectedRevision: meta.revision, operation, payload });
  }, { operation, payload });
}

test('an MCP client and the export dialog reach the same job and the same SVG bytes', async ({ browser }, testInfo) => {
  test.setTimeout(150000);
  const runtime = await startBrowserRuntime({ taskId: `export-job-${process.pid}-${Date.now()}`, port: 0 });
  const contexts = new Set(), errors = [];
  try {
    expect((await fetch(runtime.origin + IDENTITY_PATH).then((r) => r.json())).healthy).toBe(true);
    const { page } = await openProject(browser, runtime.origin, contexts, errors);

    // 1. The capability is registered at boot and declares itself available —
    //    which the registry only accepts because a handler really backs it.
    const canonical = JSON.parse(fs.readFileSync(
      path.join(root, 'engineering/application/capabilities/export-job.json'), 'utf8'));
    const discovery = await command(page, 'capabilities');
    expect(discovery.ok).toBe(true);
    const exportDescriptor = discovery.result.descriptors.find((d) => d.id === 'export.svg.frame');
    expect(exportDescriptor).toEqual(canonical);
    expect(exportDescriptor.availability).toEqual({ state: 'available', reason: null });
    expect(await page.evaluate(() =>
      NemoApplication.capabilities().find((e) => e.id === 'export.svg.frame').bound)).toBe(true);

    // 2. The frame both paths will export.
    const frame = 10;
    await page.locator('.app-mode-btn[data-mode="motion"]').click();
    await page.locator('#tl-cf').fill(String(frame + 1));
    await page.locator('#tl-cf').press('Tab');
    expect(await page.evaluate(() => state.currentFrame)).toBe(frame);
    await page.evaluate((f) => { state.waIn = f; state.waOut = f; }, frame);

    // 3. The MCP path. `start` mints a job on the session export.js memoises;
    //    `status` carries the artifact once it reaches a terminal state.
    const started = await command(page, 'start', { frameIdx: frame });
    expect(started.ok, JSON.stringify(started)).toBe(true);
    expect(started.result.status).toBe('running');
    expect(Object.keys(started.result).sort()).toEqual(['jobId', 'progress', 'status']);
    const jobId = started.result.jobId;
    // The session export.js memoises knows the job the MCP stage just minted —
    // asked directly, not through the capability. `meta().running` would be a
    // race: a one-frame job can already be terminal by the next round-trip.
    expect(await page.evaluate((id) => SMExport.svgSequenceJob().status(id).job.jobId, jobId)).toBe(jobId);

    await expect.poll(async () => (await command(page, 'status', { jobId })).result.status,
      { timeout: 30000 }).toBe('succeeded');
    const finished = await command(page, 'status', { jobId });
    expect(finished.result.progress).toBe(1);
    expect(finished.result.artifact.mimeType).toBe('image/svg+xml');
    const apiSvg = Buffer.from(finished.result.artifact.data, 'utf8');
    expect(apiSvg.length).toBeGreaterThan(0);
    // effects.scope "none": an inline export needs no filesystem, and this
    // browser has none — it still produced a complete document.
    expect(finished.result.artifact.data).toMatch(/^<\?xml version="1\.0"/);
    expect(finished.result.artifact.data).toContain('</svg>');

    // 4. The real UI interaction: the export dialog, driven by clicks.
    await page.locator('#app-menu-btn').click();
    await page.locator('#ctx-export').click();
    await page.locator('#exp-format').selectOption('svg');
    await page.locator('#exp-range').selectOption('wa');
    // The Cancel affordance exists and is hidden until an export runs.
    await expect(page.locator('#exp-cancel')).toBeHidden();
    const pending = page.waitForEvent('download');
    await page.locator('#exp-run').click();
    const download = await pending;
    const uiPath = testInfo.outputPath('ui-frame.svg');
    await download.saveAs(uiPath);
    expect(await download.failure()).toBeNull();
    const uiSvg = fs.readFileSync(uiPath);

    // 5. THE artifact-identity check: the same evaluator, so the same bytes.
    //    Two exporters would have to agree by coincidence to pass this.
    expect(sha(apiSvg)).toBe(sha(uiSvg));

    // 6. cancel is reachable and typed on a job that has already finished:
    //    terminal states are not reopened.
    const late = await command(page, 'cancel', { jobId });
    expect(late.ok).toBe(true);
    expect(late.result.status).toBe('succeeded');
    const unknown = await command(page, 'cancel', { jobId: 'no-such-job' });
    expect(unknown.ok).toBe(false);
    expect(unknown.error.code).toBe('unknown_job');

    expect(errors).toEqual([]);
  } finally {
    await testInfo.attach('page-errors', { body: JSON.stringify(errors), contentType: 'application/json' });
    try { await Promise.all([...contexts].map((context) => context.close())); }
    finally { await runtime.close(); }
  }
});
