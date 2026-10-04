'use strict';
const { test, expect } = require('@playwright/test');
const path = require('node:path');
const { startBrowserRuntime } = require('../../scripts/nemo/lib/browser-runtime.cjs');

const fixture = path.resolve(__dirname, '../animation/fixtures/curve-workflow.json');
test.use({ channel: 'chrome' });

test('real loaded playback advances and wraps on a controlled animation clock', async ({ browser }) => {
  test.setTimeout(90000);
  const runtime = await startBrowserRuntime({ taskId: `p24-playback-${process.pid}-${Date.now()}`, port: 0 });
  const context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
  try {
    const page = await context.newPage();
    const pageErrors = [];
    page.on('pageerror', error => pageErrors.push(error.message));
    await page.goto(runtime.origin, { waitUntil: 'networkidle' });
    await expect.poll(() => page.evaluate(() => !!(window.SM && window.state && window.NemoPlaybackStep)),
      { timeout: 30000 }).toBe(true);
    const chooser = page.waitForEvent('filechooser');
    await page.locator('#start-open').click();
    await (await chooser).setFiles(fixture);
    await expect.poll(() => page.evaluate(() => state.layers[0]?.name), { timeout: 30000 }).toBe('R08 rectangle');

    const observed = await page.evaluate(() => {
      const originalRaf = window.requestAnimationFrame;
      const originalCancel = window.cancelAnimationFrame;
      const originalNow = Object.getOwnPropertyDescriptor(performance, 'now');
      const scripts = Array.from(document.scripts, script => new URL(script.src || location.href).pathname);
      const playbackIndex = scripts.indexOf('/js/domain/animation/playback-step.js');
      const queue = [];
      window.requestAnimationFrame = callback => { queue.push(callback); return queue.length; };
      window.cancelAnimationFrame = () => {};
      Object.defineProperty(performance, 'now', { configurable: true, value: () => 1000 });
      state.currentFrame = 3;
      state.waIn = 0;
      state.waOut = 4;
      state.fps = 10;
      state.loopPlayback = true;
      state.pingPongPlayback = false;
      state.playing = false;
      try {
        startPlay();
        const first = queue.shift();
        first(1150);
        const forward = state.currentFrame;
        const second = queue.shift();
        second(1250);
        return { forward, wrapped: state.currentFrame, direction: state.playDir,
          playing: state.playing, loadedModule: typeof NemoPlaybackStep.advance === 'function',
          productionOrder: playbackIndex >= 0 && scripts[playbackIndex + 1] === '/js/timeline.js' };
      } finally {
        stopPlay();
        window.requestAnimationFrame = originalRaf;
        window.cancelAnimationFrame = originalCancel;
        if (originalNow) Object.defineProperty(performance, 'now', originalNow);
        else delete performance.now;
      }
    });
    expect(observed).toEqual({ forward: 4, wrapped: 0, direction: 1,
      playing: true, loadedModule: true, productionOrder: true });
    expect(pageErrors).toEqual([]);
  } finally {
    await context.close();
    await runtime.close();
  }
});
