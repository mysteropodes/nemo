'use strict';

const { test, expect } = require('@playwright/test');
const { startPreview, withPreviews } = require('./preview-lifecycle.cjs');

test('two isolated previews serve the current app and retain distinct browser state', async ({ browser }, testInfo) => {
  const runtimeRoot = testInfo.outputPath('runtime');
  const nonce = `${process.pid}-${Date.now()}`;
  await withPreviews([
    () => startPreview(`playwright-a-${nonce}`, runtimeRoot),
    () => startPreview(`playwright-b-${nonce}`, runtimeRoot),
  ], async previews => {
    const contexts = [];
    try {
      expect(previews[0].info.origin).not.toBe(previews[1].info.origin);
      expect(previews[0].info.roots.browserProfile).not.toBe(previews[1].info.roots.browserProfile);

      for (let index = 0; index < previews.length; index++) {
        const preview = previews[index];
        const identityResponse = await fetch(preview.info.identityUrl);
        expect(identityResponse.status).toBe(200);
        const identity = await identityResponse.json();
        expect(identity.healthy).toBe(true);
        expect(identity.taskId).toBe(preview.info.taskId);
        expect(identity.source.matches).toBe(true);
        expect(identity.build.matches).toBe(true);

        const context = await browser.newContext();
        contexts.push(context);
        const page = await context.newPage();
        const response = await page.goto(preview.info.url, { waitUntil: 'domcontentloaded' });
        expect(response.status()).toBe(200);
        await expect(page.locator('#app')).toBeAttached();
        await expect(page.locator('#start-screen')).toBeAttached();
        await page.evaluate(value => localStorage.setItem('nemo-r06-browser-state', value), `task-${index}`);
        await page.reload({ waitUntil: 'domcontentloaded' });
        expect(await page.evaluate(() => localStorage.getItem('nemo-r06-browser-state'))).toBe(`task-${index}`);
      }

      expect(await contexts[0].pages()[0].evaluate(() => localStorage.getItem('nemo-r06-browser-state'))).toBe('task-0');
      expect(await contexts[1].pages()[0].evaluate(() => localStorage.getItem('nemo-r06-browser-state'))).toBe('task-1');
    } finally {
      await Promise.allSettled(contexts.map(context => context.close()));
    }
  });
});
