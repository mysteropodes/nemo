'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');
const { after, test } = require('node:test');

const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-browser-preview-test-'));
after(() => fs.rmSync(scratch, { recursive: true, force: true, maxRetries: 3 }));
process.env.NEMO_ISOLATION_ROOT = path.join(scratch, 'runtime');
const sourceRoot = path.resolve(__dirname, '..', '..');
const repoRoot = runtimeSourceFixture(sourceRoot, path.join(scratch, 'source'));
const isolation = require('./lib/isolation.cjs');
const { browserLaunchConfig } = require('./lib/browser-runtime.cjs');
const cli = path.join(repoRoot, 'scripts/nemo/browser.cjs');
const buildCli = path.join(sourceRoot, 'scripts/nemo/build.cjs');

// Runtime identity resolves its source root from the CLI's location. Copy the
// actual runtime and served assets into a private Git root before mutating them.
function runtimeSourceFixture(sourceRoot, root) {
  const files = [
    'scripts/nemo/browser.cjs', 'scripts/nemo/lib',
    'package.json', 'src-tauri/tauri.conf.json',
    'geometry-wasm/Cargo.toml', 'vectorize-wasm/Cargo.toml', 'src-tauri/Cargo.toml',
    'src/index.html', 'src/wasm/geometry_wasm_bg.wasm',
    'src/wasm-vectorize/vectorize_wasm_bg.wasm',
  ];
  for (const file of files) {
    const target = path.join(root, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.cpSync(path.join(sourceRoot, file), target, { recursive: true });
  }
  function git(args) {
    const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr || String(result.error || 'Git fixture setup failed'));
  }
  git(['init', '-q']);
  git(['config', 'maintenance.auto', 'false']);
  git(['config', 'gc.auto', '0']);
  git(['add', '.']);
  git(['-c', 'user.name=Runtime fixture', '-c', 'user.email=runtime@example.invalid',
    '-c', 'commit.gpgsign=false', 'commit', '-qm', 'Runtime source fixture']);
  return root;
}

function exited(child) {
  if (child.exitCode != null || child.signalCode != null) return Promise.resolve({ code: child.exitCode, signal: child.signalCode });
  return new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
}

function firstLine(child) {
  return new Promise((resolve, reject) => {
    let data = ''; const timer = setTimeout(() => done(new Error('readiness timed out')), 10000);
    function done(error, value) { clearTimeout(timer); child.stdout.off('data', read); child.off('exit', early); error ? reject(error) : resolve(value); }
    function read(chunk) { data += chunk; const newline = data.indexOf('\n'); if (newline !== -1) done(null, data.slice(0, newline)); }
    function early(code) { done(new Error(`launcher exited before readiness (${code}): ${data}`)); }
    child.stdout.on('data', read); child.once('exit', early);
  });
}

async function launch(task, extra = []) {
  const child = spawn(process.execPath, [cli, 'start', '--task', task, ...extra], {
    cwd: repoRoot, env: { ...process.env }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  return { child, info: JSON.parse(await firstLine(child)) };
}

function command(args, executable = cli) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [executable, ...args], { cwd: repoRoot, env: { ...process.env }, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = ''; let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk; }); child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.once('exit', (code, signal) => resolve({ code, signal, stdout, stderr }));
  });
}

function request(url, options = {}) {
  return new Promise((resolve, reject) => {
    const req = http.request(url, options, (res) => {
      const chunks = []; res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, body: Buffer.concat(chunks) }));
    });
    req.on('error', reject); req.end();
  });
}

async function stopOwned(instance) {
  if (!instance || !isolation.pidAlive(instance.child.pid)) return;
  const result = await isolation.requestStop(instance.info.taskId, instance.info.ownerToken, { timeoutMs: 5000 });
  assert.equal(result.stopped, true); await exited(instance.child);
  assert.equal(isolation.releaseTask(instance.info.taskId, instance.info.ownerToken).released, true);
}

test('two real preview processes hold distinct origins and serve exact source bytes and identity', async () => {
  const a = await launch(`preview-a-${process.pid}`); const b = await launch(`preview-b-${process.pid}`);
  try {
    assert.notEqual(a.info.origin, b.info.origin); assert.notEqual(a.info.roots.root, b.info.roots.root);
    const [indexA, indexB, identityA, wasmHead] = await Promise.all([
      request(a.info.url), request(b.info.origin + '/index.html'), request(a.info.identityUrl),
      request(a.info.origin + '/wasm/geometry_wasm_bg.wasm', { method: 'HEAD' }),
    ]);
    const expected = fs.readFileSync(path.join(repoRoot, 'src', 'index.html'));
    assert.equal(indexA.status, 200); assert.deepEqual(indexA.body, expected);
    assert.equal(indexB.status, 200); assert.deepEqual(indexB.body, expected);
    assert.equal(wasmHead.status, 200); assert.equal(wasmHead.headers['content-type'], 'application/wasm'); assert.equal(wasmHead.body.length, 0);
    const served = JSON.parse(identityA.body);
    assert.equal(identityA.status, 200); assert.equal(served.healthy, true); assert.equal(served.taskId, a.info.taskId);
    assert.equal(served.source.matches, true); assert.equal(served.build.matches, true);
    assert.equal(served.browser.integrated, false); assert.equal(JSON.stringify(served).includes(a.info.ownerToken), false);
    assert.equal(served.environment.TMPDIR, a.info.roots.temp);
    assert.equal(served.environment.XDG_CACHE_HOME, a.info.roots.cache);
    assert.equal(served.environment.NEMO_REPORT_DIR, a.info.roots.reports);
    assert.equal(served.environment.NEMO_BROWSER_PROFILE, a.info.roots.browserProfile);
  } finally { await stopOwned(a); await stopOwned(b); }
});

test('owner mismatch is refused and stale process state is detected and recoverable', async () => {
  const instance = await launch(`preview-owner-${process.pid}`);
  const refused = await isolation.requestStop(instance.info.taskId, 'wrong-token', { timeoutMs: 50 });
  assert.equal(refused.stopped, false); assert.equal(isolation.pidAlive(instance.child.pid), true);
  instance.child.kill('SIGKILL'); await exited(instance.child);
  const stale = isolation.verifyHandshake(instance.info.taskId, { ownerToken: instance.info.ownerToken, checkSource: true });
  assert.equal(stale.ok, false); assert.match(stale.reason, /not running/);
  assert.equal(isolation.releaseTask(instance.info.taskId, instance.info.ownerToken).released, true);
  assert.equal(fs.existsSync(instance.info.roots.root), false);
});

test('CLI status and stop enforce ownership and remove roots', async () => {
  const instance = await launch(`preview-cli-${process.pid}`);
  const bad = await command(['stop', '--task', instance.info.taskId, '--owner', 'wrong-token']);
  assert.equal(bad.code, 1); assert.equal(JSON.parse(bad.stdout).stopped, false); assert.equal(isolation.pidAlive(instance.child.pid), true);
  const status = await command(['status', '--task', instance.info.taskId, '--owner', instance.info.ownerToken]);
  assert.equal(status.code, 0); assert.equal(JSON.parse(status.stdout).ok, true);
  const stopping = exited(instance.child);
  const stopped = await command(['stop', '--task', instance.info.taskId, '--owner', instance.info.ownerToken]);
  await stopping; const result = JSON.parse(stopped.stdout);
  assert.equal(stopped.code, 0); assert.equal(result.stopped, true); assert.equal(result.released.released, true);
  assert.equal(fs.existsSync(instance.info.roots.root), false);
});

test('source change fails closed without invalidating a build in the original checkout', async () => {
  const child = spawn(process.execPath, [buildCli, 'start', '--task', `preview-peer-build-${process.pid}`,
    '--command', process.execPath, '--', '-e', 'process.exit(0)'], {
    cwd: sourceRoot, env: { ...process.env }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  const peer = { child, info: JSON.parse(await firstLine(child)) };
  let instance;
  const marker = path.join(repoRoot, `.browser-preview-source-change-${process.pid}`);
  try {
    instance = await launch(`preview-source-${process.pid}`);
    fs.writeFileSync(marker, 'change');
    const response = await request(instance.info.identityUrl); const body = JSON.parse(response.body);
    assert.equal(response.status, 409); assert.equal(body.healthy, false); assert.equal(body.source.matches, false);
    assert.equal((await request(instance.info.url)).status, 409);
    // Keep the mutation present while the real source-checkout build checks its
    // owner/source/build handshake: sharing repoRoot reproduced a status-1 race.
    const status = await command(['status', '--task', peer.info.taskId, '--owner', peer.info.ownerToken], buildCli);
    const build = JSON.parse(status.stdout);
    assert.equal(status.code, 0, build.reason);
    assert.equal(build.ok, true);
    assert.equal(build.source.matches, true);
    assert.equal(build.build.matches, true);
  } finally {
    fs.rmSync(marker, { force: true });
    try { await stopOwned(instance); } finally { await stopOwned(peer); }
  }
});

test('static server rejects writes, traversal and escaping symlinks', async () => {
  const link = path.join(repoRoot, 'src', `.browser-preview-escape-${process.pid}`);
  fs.symlinkSync(path.join(repoRoot, 'package.json'), link);
  const instance = await launch(`preview-path-${process.pid}`);
  try {
    assert.equal((await request(instance.info.origin + '/package.json')).status, 404);
    assert.equal((await request(instance.info.origin + '/%2e%2e/package.json')).status, 404);
    assert.equal((await request(instance.info.origin + '/' + path.basename(link))).status, 403);
    const post = await request(instance.info.url, { method: 'POST' });
    assert.equal(post.status, 405); assert.equal(post.headers.allow, 'GET, HEAD');
  } finally { await stopOwned(instance); fs.rmSync(link, { force: true }); }
});

test('startup bind failure leaves no launcher record or task roots', async () => {
  const blocker = http.createServer(); await new Promise((resolve) => blocker.listen(0, '127.0.0.1', resolve));
  const task = `preview-fail-${process.pid}`;
  const failed = await command(['start', '--task', task, '--port', String(blocker.address().port)]);
  await new Promise((resolve) => blocker.close(resolve));
  assert.equal(failed.code, 1); assert.match(failed.stderr, /EADDRINUSE/);
  assert.equal(isolation.readLauncher(task), null); assert.equal(fs.existsSync(isolation.taskRoot(task)), false);
});

test('duplicate startup preserves the first owner and browser profiles require explicit launch', async () => {
  const task = `preview-duplicate-${process.pid}`; const first = await launch(task);
  try {
    const duplicate = await command(['start', '--task', task]);
    assert.equal(duplicate.code, 1); assert.match(duplicate.stderr, /already registered/);
    assert.equal(isolation.readLauncher(task).ownerToken, first.info.ownerToken);
    assert.equal((await request(first.info.identityUrl)).status, 200);
    const config = browserLaunchConfig(first.info.roots, first.info.url, { browser: process.execPath, headless: true });
    assert.ok(config.args.includes(`--user-data-dir=${first.info.roots.browserProfile}`));
    assert.ok(config.args.includes(`--disk-cache-dir=${path.join(first.info.roots.cache, 'browser')}`));
    assert.ok(config.args.includes('--headless=new'));
  } finally { await stopOwned(first); }
});


test('an exited requested browser is reported and shutdown waits for a stubborn browser', {
  skip: process.platform === 'win32' ? 'POSIX signal and executable fixtures require a Unix host' : false,
}, async () => {
  const failedBrowser = path.join(scratch, 'browser-exits');
  fs.writeFileSync(failedBrowser, '#!/bin/sh\nexit 23\n', { mode: 0o700 });
  const failed = await launch(`preview-browser-exits-${process.pid}`, ['--browser', failedBrowser]);
  try {
    let body;
    for (let i = 0; i < 100; i++) {
      body = JSON.parse((await request(failed.info.identityUrl)).body);
      if (body.browser.error) break;
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
    assert.equal(body.browser.integrated, false);
    assert.equal(body.browser.active, false);
    assert.match(body.browser.error, /code 23/);
  } finally { await stopOwned(failed); }
  const stubborn = path.join(scratch, 'browser-stubborn');
  const ready = path.join(scratch, 'stubborn-ready');
  fs.writeFileSync(stubborn, `#!/usr/bin/env node\nprocess.on('SIGTERM', () => {});\nrequire('node:fs').writeFileSync(${JSON.stringify(ready)}, 'ready');\nsetInterval(() => {}, 1000);\n`, { mode: 0o700 });
  const instance = await launch(`preview-browser-stubborn-${process.pid}`, ['--browser', stubborn]);
  try {
    for (let i = 0; i < 100 && !fs.existsSync(ready); i++) await new Promise((resolve) => setTimeout(resolve, 10));
    assert.equal(fs.existsSync(ready), true);
    const pid = instance.info.browser.pid;
    assert.equal(isolation.pidAlive(pid), true);
    await stopOwned(instance);
    assert.equal(isolation.pidAlive(pid), false);
    assert.equal(fs.existsSync(instance.info.roots.root), false);
  } finally { if (isolation.pidAlive(instance.child.pid)) await stopOwned(instance); }
});
