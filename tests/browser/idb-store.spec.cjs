'use strict';
const { test, expect } = require('@playwright/test');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');
const root = path.resolve(__dirname, '../..');
const bytes = fs.readFileSync(path.join(root, 'tests/animation/fixtures/curve-workflow.json'), 'utf8');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
test.use({ channel: 'chrome' });

test('production IndexedDB facade persists opaque autosave across reload, removes it, and preserves Resume denial', async ({ browser }, testInfo) => {
  test.setTimeout(60000);
  const runtime = await startBrowserRuntime({ taskId: `p34-idb-${process.pid}-${Date.now()}`, port: 0 });
  let context;
  try {
    const identity = await fetch(runtime.origin + IDENTITY_PATH).then(response => response.json());
    expect(identity.healthy).toBe(true);
    expect(identity.source.startup.dirty).toBe(false);
    const shipped = await fetch(runtime.origin + '/js/idb-store.js').then(response => response.text());
    expect(sha(shipped)).toBe(sha(fs.readFileSync(path.join(root, 'src/js/idb-store.js'))));
    context = await browser.newContext(); // fresh storage; never touches a user's profile
    const page = await context.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(runtime.origin, { waitUntil: 'networkidle' });
    await expect.poll(() => page.evaluate(() => !!window.SMIdb && !!window.SMProject)).toBe(true);
    expect(await page.evaluate(() => SMIdb.get('nemo-auto'))).toBeUndefined();
    await expect(page.locator('#start-resume')).toBeHidden();
    expect(await page.evaluate(() => typeof window.__TAURI__)).toBe('undefined');
    expect(await page.evaluate(value => SMIdb.set('nemo-auto', value), bytes)).toBeUndefined();
    expect(await page.evaluate(() => localStorage.getItem('nemo-auto'))).toBeNull();
    await page.reload({ waitUntil: 'networkidle' });
    expect(await page.evaluate(() => SMIdb.get('nemo-auto'))).toBe(bytes);
    await expect(page.locator('#start-resume')).toBeVisible();
    // Exercise the real consumer; storage presence does not admit legacy editing.
    const snapshot = () => page.evaluate(() => ({ layers: JSON.stringify(state.layers),
      undo: JSON.stringify(state.undoStack), redo: JSON.stringify(state.redoStack), frame: state.currentFrame }));
    const before = await snapshot();
    await page.locator('#start-resume').click();
    await expect(page.locator('#start-screen')).toBeVisible();
    expect(await snapshot()).toEqual(before);
    expect(await page.evaluate(() => SMIdb.get('nemo-auto'))).toBe(bytes);
    expect(await page.evaluate(() => SMIdb.remove('nemo-auto'))).toBeUndefined();
    expect(await page.evaluate(() => SMIdb.get('nemo-auto'))).toBeUndefined();
    await page.reload({ waitUntil: 'networkidle' });
    await expect(page.locator('#start-resume')).toBeHidden();
    expect(await page.evaluate(() => SMIdb.get('nemo-auto'))).toBeUndefined();
    expect(errors).toEqual([]);
    await testInfo.attach('p34-browser-storage', { contentType: 'application/json', body: Buffer.from(JSON.stringify({
      sourceSha: identity.source.startup.head, browserVersion: browser.version(), adapterSha256: sha(shipped),
      fixtureSha256: sha(bytes), roundTripAfterReload: true, removedAfterReload: true,
      resume: 'unavailable; preserved document', tauri: 'not present; no installed-native acceptance', pageErrors: errors,
    }, null, 2)) });
  } finally {
    try { if (context) await context.close(); } finally { await runtime.close(); }
  }
});

test('unavailable IndexedDB rejects and the production missing-storage startup stays usable', async ({ browser }) => {
  test.setTimeout(60000);
  const runtime = await startBrowserRuntime({ taskId: `p34-unavailable-${process.pid}-${Date.now()}`, port: 0 });
  let context;
  try {
    context = await browser.newContext();
    // Environment control before production bootstrap, not a substitute facade.
    await context.addInitScript(() => Object.defineProperty(window, 'indexedDB', { value: undefined }));
    const page = await context.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(runtime.origin, { waitUntil: 'networkidle' });
    await expect(page.locator('#start-open')).toBeVisible();
    await expect(page.locator('#start-resume')).toBeHidden();
    const messages = await page.evaluate(async () => {
      const results = [];
      for (const operation of [() => SMIdb.get('nemo-auto'), () => SMIdb.set('nemo-auto', 'control'), () => SMIdb.remove('nemo-auto')]) {
        try { await operation(); results.push('unexpected success'); } catch (error) { results.push(error.message); }
      }
      return results;
    });
    expect(messages).toEqual(Array(3).fill('indexedDB unavailable'));
    expect(errors).toEqual([]);
  } finally {
    try { if (context) await context.close(); } finally { await runtime.close(); }
  }
});
