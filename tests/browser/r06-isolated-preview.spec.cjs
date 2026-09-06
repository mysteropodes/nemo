'use strict';

const { test, expect } = require('@playwright/test');
const { spawn } = require('node:child_process');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '../..');
const CLI = path.join(ROOT, 'scripts/nemo/browser.cjs');

function startPreview(taskId, runtimeRoot) {
  const env = { ...process.env, NEMO_ISOLATION_ROOT: runtimeRoot };
  const child = spawn(process.execPath, [CLI, 'start', '--task', taskId], {
    cwd: ROOT,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return new Promise((resolve, reject) => {
    let stdout = '';
    let stderr = '';
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error(`preview ${taskId} did not report readiness; stderr: ${stderr}`));
    }, 15_000);
    child.stderr.on('data', chunk => { stderr += chunk; });
    child.stdout.on('data', chunk => {
      stdout += chunk;
      const newline = stdout.indexOf('\n');
      if (newline < 0) return;
      clearTimeout(timer);
      try {
        resolve({ child, env, info: JSON.parse(stdout.slice(0, newline)) });
      } catch (error) {
        child.kill('SIGKILL');
        reject(new Error(`preview ${taskId} returned malformed readiness: ${error.message}`));
      }
    });
    child.once('error', error => {
      clearTimeout(timer);
      reject(error);
    });
    child.once('exit', code => {
      if (!stdout.includes('\n')) {
        clearTimeout(timer);
        reject(new Error(`preview ${taskId} exited ${code} before readiness; stderr: ${stderr}`));
      }
    });
  });
}

function stopPreview(instance) {
  const stopped = spawn(process.execPath, [
    CLI, 'stop', '--task', instance.info.taskId, '--owner', instance.info.ownerToken,
    '--timeout-ms', '10000',
  ], { cwd: ROOT, env: instance.env, stdio: ['ignore', 'pipe', 'pipe'] });
  return new Promise((resolve, reject) => {
    let stdout = '';
    let stderr = '';
    const timer = setTimeout(() => {
      stopped.kill('SIGKILL');
      if (instance.child.exitCode == null) instance.child.kill('SIGKILL');
      reject(new Error(`preview stop timed out; stderr: ${stderr}`));
    }, 15_000);
    stopped.stdout.on('data', chunk => { stdout += chunk; });
    stopped.stderr.on('data', chunk => { stderr += chunk; });
    stopped.once('error', reject);
    stopped.once('exit', code => {
      clearTimeout(timer);
      if (code !== 0) {
        if (instance.child.exitCode == null) instance.child.kill('SIGKILL');
        reject(new Error(stderr || stdout || `preview stop exited ${code}`));
      } else resolve();
    });
  });
}

test('two isolated previews serve the current app and retain distinct browser state', async ({ browser }, testInfo) => {
  const runtimeRoot = testInfo.outputPath('runtime');
  const nonce = `${process.pid}-${Date.now()}`;
  const previews = await Promise.all([
    startPreview(`playwright-a-${nonce}`, runtimeRoot),
    startPreview(`playwright-b-${nonce}`, runtimeRoot),
  ]);
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
    for (const preview of previews.reverse()) await stopPreview(preview);
  }
});
