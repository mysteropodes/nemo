'use strict';

const { test, expect } = require('@playwright/test');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');

const root = path.resolve(__dirname, '../..');
const fixture = path.join(root, 'tests/animation/fixtures/curve-workflow.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const fixtureSha = 'dceb05d13576a4dda0eb1a1a9d8c0184e8617e9a3a2662150ee54f4badedf08d';
test.use({ channel: 'chrome' });

// This is the production browser, with no Tauri object, transport mock or injected
// native owner. An unavailable native capability must not select a second writer.
test('native opacity is unavailable in the browser and cannot fall back to a legacy write', async ({ browser }, testInfo) => {
  test.setTimeout(90000);
  expect(sha(fs.readFileSync(fixture))).toBe(fixtureSha);
  const descriptorPath = 'engineering/application/capabilities-v2/native-opacity.json';
  const descriptorBytes = fs.readFileSync(path.join(root, descriptorPath));
  const descriptor = JSON.parse(descriptorBytes);
  expect(descriptor.id).toBe('native.opacity');
  expect(descriptor.authority.fallback).toBe('none');
  const runtime = await startBrowserRuntime({ taskId: `n21-browser-${process.pid}-${Date.now()}`, port: 0 });
  let context;
  const errors = [];
  try {
    const identity = await fetch(runtime.origin + IDENTITY_PATH).then(response => response.json());
    expect(identity.healthy).toBe(true);
    expect(identity.source.startup.head).toMatch(/^[a-f0-9]{40}$/);
    expect(identity.source.matches).toBe(true);
    expect(identity.source.startup.dirty, 'BLOCKED: commit the candidate before browser acceptance').toBe(false);
    const sourceHashes = { [descriptorPath]: sha(descriptorBytes) };
    for (const file of ['src/index.html', 'src/js/adapters/native-opacity-legacy-surface.js',
      'src/js/bootstrap/native-opacity-application.js',
      'src/js/bootstrap/opacity-application.js', 'src/js/adapters/application-mcp.js']) {
      const response = await fetch(runtime.origin + '/' + file.slice(4));
      expect(response.status).toBe(200);
      sourceHashes[file] = sha(fs.readFileSync(path.join(root, file)));
      expect(sha(Buffer.from(await response.arrayBuffer()))).toBe(sourceHashes[file]);
    }
    context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
    const page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(runtime.origin, { waitUntil: 'networkidle' });
    await expect.poll(() => page.evaluate(() => !!(window.SM && window.NemoApplication && window.state)),
      { timeout: 30000 }).toBe(true);
    // Retain observations only: never replace a production handler or host.
    await page.evaluate(() => { window.__n21ObservedOwners = {
      handle: NemoApplication.handle, legacy: NemoOpacityApplication.legacy }; });
    const chooser = page.waitForEvent('filechooser');
    await page.locator('#start-open').click();
    await (await chooser).setFiles(fixture);
    await expect.poll(() => page.evaluate(() => window.state.layers[0]?.layerUid),
      { timeout: 30000 }).toBe('r08_curve_layer');

    const observed = await page.evaluate(async () => {
      const owner = () => ({ tauri: typeof window.__TAURI__,
        cutover: typeof window.NemoNativeOpacityCutover,
        project: typeof window.NemoNativeOpacityProject,
        admission: typeof window.NemoNativeOpacityLegacyAdmission });
      const meta = NemoOpacityApplication.meta();
      const request = { apiVersion: 2, requestId: 'n21-browser-no-fallback',
        instanceId: meta.instanceId, documentId: meta.documentId,
        expectedRevision: meta.revision, operation: 'command.document.apply',
        payload: { command: 'layer.opacity.set', stableTarget: { layerUid: 'r08_curve_layer' }, value: 7 } };
      // exportJSON flushes Paper frames and is now correctly denied by the
      // browser write guard. Observe the live document without invoking it.
      const snapshot = () => ({
        layers: state.layers.map(layer => ({ layerUid: layer.layerUid,
          motion: JSON.stringify(layer.motion ?? null),
          motionStatic: JSON.stringify(layer.motionStatic ?? null) })),
        undoCount: state.undoStack.length, redoCount: state.redoStack.length,
        meta: NemoOpacityApplication.meta(),
      });
      const before = snapshot();
      const initialRead = await NemoApplication.handle({ apiVersion: 1, requestId: 'n21-browser-initial-read',
        ...meta, operation: 'property.get',
        payload: { layerId: 'r08_curve_layer', property: 'opacity' } });
      const refused = await NemoApplication.handle(request);
      const after = snapshot();
      const discovery = await NemoApplication.handle({ apiVersion: 1, requestId: 'n21-browser-discovery',
        ...meta, operation: 'capabilities', payload: {} });
      const legacy = await NemoApplication.handle({ apiVersion: 1, requestId: 'n21-browser-legacy-control',
        ...meta, expectedRevision: meta.revision, operation: 'property.set',
        payload: { layerId: 'r08_curve_layer', property: 'opacity', value: 26 } });
      const afterLegacy = snapshot();
      const current = NemoOpacityApplication.meta();
      const read = await NemoApplication.handle({ apiVersion: 1, requestId: 'n21-browser-read',
        ...current, operation: 'property.get',
        payload: { layerId: 'r08_curve_layer', property: 'opacity' } });
      const handlers = { handleType: typeof NemoApplication.handle,
        legacyType: typeof NemoOpacityApplication.legacy,
        handleUnchanged: NemoApplication.handle === window.__n21ObservedOwners.handle,
        legacyUnchanged: NemoOpacityApplication.legacy === window.__n21ObservedOwners.legacy,
        handleSource: NemoApplication.handle.toString(), legacySource: NemoOpacityApplication.legacy.toString() };
      delete window.__n21ObservedOwners;
      return { before, after, afterLegacy, initialRead, refused, discovery, legacy, read, owner: owner(), handlers,
        userAgent: navigator.userAgent };
    });
    await testInfo.attach('native-opacity-browser-observation', { contentType: 'application/json',
      body: Buffer.from(JSON.stringify({ sourceSha: identity.source.startup.head,
        browserVersion: browser.version(), nativeRequest: observed.refused,
        legacyWrite: observed.legacy,
        beforeOpacity: observed.initialRead.result?.value ?? null,
        afterOpacity: observed.read.result?.value ?? null,
        beforeRevision: observed.before.meta.revision,
        afterRevision: observed.afterLegacy.meta.revision }, null, 2)) });
    expect(observed.owner).toEqual({ tauri: 'undefined', cutover: 'undefined',
      project: 'undefined', admission: 'undefined' });
    expect(observed.refused.ok).toBe(false);
    expect(observed.refused.error.code).toBe('invalid_request');
    expect(observed.after).toEqual(observed.before);
    expect(observed.discovery.ok).toBe(true);
    expect(observed.discovery.result.descriptors.map(value => value.id)).not.toContain('native.opacity');
    expect(observed.legacy.ok).toBe(false);
    expect(observed.legacy.error.code).toBe('unavailable');
    expect(observed.afterLegacy).toEqual(observed.before);
    expect(observed.initialRead.ok).toBe(true);
    expect(observed.initialRead.result.value).toBe(100);
    expect(observed.read.ok).toBe(true);
    expect(observed.read.result.value).toBe(100);
    expect(observed.handlers).toMatchObject({ handleType: 'function', legacyType: 'function',
      handleUnchanged: true, legacyUnchanged: true });
    expect(errors).toEqual([]);
    expect((await fetch(runtime.origin + IDENTITY_PATH).then(response => response.json())).healthy).toBe(true);
    await testInfo.attach('native-opacity-browser-result', { contentType: 'application/json',
      body: Buffer.from(JSON.stringify({ schema: 'nemo.n21-browser/1', result: 'pass',
        sourceSha: identity.source.startup.head, sourceDirty: identity.source.startup.dirty,
        browserVersion: browser.version(), userAgent: observed.userAgent, pageErrors: errors, fixtureSha, sourceHashes,
        nativeAvailability: 'unavailable', nativeRequestError: observed.refused.error.code,
        rejectedRequestPreservedDocument: true, legacyWriteError: observed.legacy.error.code,
        legacyRead: observed.read,
        owner: observed.owner, handlers: { ...observed.handlers,
          handleSource: sha(observed.handlers.handleSource), legacySource: sha(observed.handlers.legacySource) },
        limitations: ['Browser capability boundary only; no installed-native acceptance.'] }, null, 2)) });
  } finally {
    await testInfo.attach('page-errors', { contentType: 'application/json', body: Buffer.from(JSON.stringify(errors)) });
    try { if (context) await context.close(); } finally { await runtime.close(); }
  }
});
