'use strict';

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

// Start attempts must all settle before teardown: a sibling may become ready
// after another start rejects. Keep every successful instance inside this scope.
async function withPreviews(starters, use, stop = stopPreview) {
  const previews = [];
  const errors = [];
  let value;
  try {
    const starts = await Promise.allSettled(starters.map(start => Promise.resolve().then(start)));
    for (const result of starts) {
      if (result.status === 'fulfilled') previews.push(result.value);
      else errors.push(result.reason);
    }
    if (errors.length === 0) value = await use(previews);
  } catch (error) {
    errors.push(error);
  } finally {
    const stops = await Promise.allSettled(previews.reverse().map(preview =>
      Promise.resolve().then(() => stop(preview))));
    for (const result of stops) {
      if (result.status === 'rejected') errors.push(result.reason);
    }
  }
  if (errors.length === 1) throw errors[0];
  if (errors.length > 1) throw new AggregateError(errors, 'preview setup, use or cleanup failed');
  return value;
}

module.exports = { startPreview, stopPreview, withPreviews };
