'use strict';

const { test, expect } = require('@playwright/test');
const { setTimeout: delay } = require('node:timers/promises');
const { startPreview, stopPreview, withPreviews } = require('./preview-lifecycle.cjs');

function fixture(testInfo) {
  const owned = [];
  const runtimeRoot = testInfo.outputPath('runtime');
  const nonce = `${process.pid}-${Date.now()}`;
  return {
    owned,
    async start(index) {
      const preview = await startPreview(`cleanup-${index}-${nonce}`, runtimeRoot);
      owned.push(preview);
      const response = await fetch(preview.info.identityUrl);
      expect(response.status).toBe(200);
      await response.arrayBuffer();
      return preview;
    },
    // Safety net runs after the assertions, so a regression cannot leak children.
    async dispose() {
      await Promise.allSettled(owned.map(async preview => {
        if (preview.child.exitCode == null && preview.child.signalCode == null) {
          await stopPreview(preview);
        }
      }));
    },
  };
}

async function expectStopped(previews) {
  for (const preview of previews) {
    await expect.poll(() => preview.child.exitCode != null || preview.child.signalCode != null).toBe(true);
    expect(() => process.kill(preview.child.pid, 0)).toThrow();
    await expect(fetch(preview.info.identityUrl, { signal: AbortSignal.timeout(1000) })).rejects.toThrow();
  }
}

test('second-start failure cleans a real preview whose readiness settles later', async ({}, testInfo) => {
  const f = fixture(testInfo);
  const failure = new Error('injected second-start failure');
  let firstServing;
  const serving = new Promise(resolve => { firstServing = resolve; });
  let used = false;
  try {
    await expect(withPreviews([
      async () => {
        const preview = await f.start(0);
        firstServing();
        // The other start rejects while this successful attempt is still pending.
        await delay(100);
        return preview;
      },
      async () => { await serving; throw failure; },
    ], async () => { used = true; })).rejects.toBe(failure);
    expect(used).toBe(false);
    expect(f.owned).toHaveLength(1);
    await expectStopped(f.owned);
  } finally { await f.dispose(); }
});

test('a failed stop does not skip the other preview stop', async ({}, testInfo) => {
  const f = fixture(testInfo);
  const attempts = [];
  try {
    await expect(withPreviews([() => f.start(0), () => f.start(1)], async () => {}, async preview => {
      attempts.push(preview.info.taskId);
      // Force the real stop CLI to reject ownership and exercise its kill fallback.
      const instance = attempts[0] === preview.info.taskId
        ? { ...preview, info: { ...preview.info, ownerToken: 'injected-invalid-owner' } }
        : preview;
      await stopPreview(instance);
    })).rejects.toThrow();
    expect(new Set(attempts)).toEqual(new Set(f.owned.map(preview => preview.info.taskId)));
    expect(attempts).toHaveLength(2);
    await expectStopped(f.owned);
  } finally { await f.dispose(); }
});

test('successful use returns its result and stops both previews', async ({}, testInfo) => {
  const f = fixture(testInfo);
  try {
    const result = await withPreviews([() => f.start(0), () => f.start(1)], async previews => {
      expect(previews).toHaveLength(2);
      expect(previews[0].info.origin).not.toBe(previews[1].info.origin);
      return 'used both previews';
    });
    expect(result).toBe('used both previews');
    await expectStopped(f.owned);
  } finally { await f.dispose(); }
});

test('body and stop failures are both reported after every cleanup attempt', async ({}, testInfo) => {
  const f = fixture(testInfo);
  const bodyFailure = new Error('injected body failure');
  const stopFailure = new Error('injected stop failure');
  const attempts = [];
  try {
    await expect(withPreviews([() => f.start(0), () => f.start(1)], async () => {
      throw bodyFailure;
    }, async preview => {
      attempts.push(preview.info.taskId);
      await stopPreview(preview);
      if (attempts[0] === preview.info.taskId) throw stopFailure;
    })).rejects.toMatchObject({ errors: [bodyFailure, stopFailure] });
    expect(attempts).toHaveLength(2);
    await expectStopped(f.owned);
  } finally { await f.dispose(); }
});
