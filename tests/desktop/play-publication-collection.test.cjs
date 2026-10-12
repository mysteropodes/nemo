'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');
const { spawn } = require('node:child_process');
const { once } = require('node:events');
const bootstrap = path.join(__dirname, 'play-publication-observer-bootstrap.js');
const RUN = 'da5f47f9-5681-4e71-a0b6-aa7578b6b5fa';
const CUT = '3d755db1-cdf7-44f8-8706-cc812e1845ca';

test('correlation scalar schema accepts real failure phases and rejects incompatible or private fields', async () => {
  const accepted = ['failed-validation', 'failed-device-lost', 'failed-retry-lost',
    'failed-retry-outdated', 'failed-recreate', 'failed-reconfigure'];
  const { api, p } = await create();
  const base = { event: 'host-receipt', documentSnapshotId: 'native-opacity:native-document-1:0',
    contextId: 'scene-root', hostCallOrdinal: 1, workId: 'native-work-18446744073709551615' };
  for (const status of accepted) api.record({ ...base, status });
  for (const phase of ['installing', 'release-requested', 'releasing', 'replacing', 'indeterminate']) {
    api.record({ event: 'play-state', playing: false, phase });
  }
  const before = api.inspect().dropped;
  for (const change of [{ documentSnapshotId: '' }, { documentSnapshotId: 'a\n' },
    { contextId: '/private/path' }, { hostCallOrdinal: 0 }, { hostCallOrdinal: Number.MAX_SAFE_INTEGER + 1 },
    { workId: 'native-work-18446744073709551616' }, { status: 'unknown-private-error' },
    { document: { layers: [] } }, { error: 'private exception' }]) api.record({ ...base, ...change });
  assert.equal(api.inspect().dropped - before, 9);
  await p.seal();
  assert.equal(p.deliveries.filter(d => d.action === 'record').length, 12);
  assert.equal(p.deliveries.at(-1).payload.dropped, 9);
});

test('missing session attribution forces a partial cut even when scalar delivery succeeds', async () => {
  const { api, p } = await create();
  const ticket = api.beginHost({}, { instanceId: RUN, lifecycleGeneration: 1 }, {
    documentId: 'native-document-1', contentRevision: 0, documentSnapshotId: 'snapshot-1', contextId: 'scene-root', frame: 10 });
  api.rawReceipt(ticket, { workId: 'native-work-1', viewGeneration: 1, status: 'presented' });
  await p.seal();
  assert.equal(p.deliveries.at(-1).payload.dropped, 1);
  assert.equal(api.inspect().failed, 0);
  const request = p.deliveries.find(d => d.payload.event?.event === 'host-request').payload.event;
  assert.equal(Object.hasOwn(request, 'hostGeneration'), false);
  assert.equal(Object.hasOwn(request, 'sessionOrdinal'), false);
});

function ports(mode = 'ok') {
  const deliveries = [], waiting = [];
  let listener;
  return {
    deliveries, waiting,
    invoke(action, payload) {
      if (mode === 'throw') throw new Error('/private/secret');
      if (mode === 'reject') return Promise.reject(new Error('private-token'));
      if (mode === 'pending' && action === 'record') return new Promise(resolve => waiting.push(() => {
        deliveries.push({ action, payload }); resolve({ sequence: payload.sequence });
      }));
      deliveries.push({ action, payload });
      if (action === 'record') return Promise.resolve({ sequence: payload.sequence });
      if (action === 'seal') return Promise.resolve({ ...payload, kind: 'seal', status: 'provisional', cutId: CUT });
      return Promise.resolve({ ...payload, kind: 'footer', terminalFence: true,
        status: payload.dropped || payload.failed ? 'partial' : 'complete' });
    },
    async listen(callback) { listener = callback; return () => {}; },
    async seal() { await listener({ payload: { runId: RUN } }); },
  };
}

async function create(p = ports(), limit) {
  const { createObserver } = require(bootstrap);
  const api = createObserver({ runId: RUN }, p, limit);
  await api.ready;
  return { api, p };
}

test('main-frame packaged bootstrap alone publishes the test global', () => {
  const code = fs.readFileSync(bootstrap, 'utf8');
  for (const [href, child, enabled] of [
    ['tauri://localhost/index.html', false, true],
    ['tauri://localhost', false, true],
    ['tauri://localhost/', false, true],
    ['tauri://localhost:3000/', false, false],
    ['tauri://user:secret@localhost/', false, false],
    ['http://localhost:1420', false, false],
    ['https://localhost/index.html', false, false],
    ['tauri://localhost/index.html', true, false],
    ['tauri://localhost/other.html', false, false],
  ]) {
    const p = ports();
    const w = { location: { href }, __NEMO_PLAY_OBSERVER_CONFIG__: { runId: RUN },
      __TAURI__: { core: { invoke: p.invoke }, event: { listen: async (_, cb) => p.listen(cb) } } };
    w.top = child ? {} : w;
    vm.runInNewContext(code, { window: w, URL, Promise, Set, Object, Number, JSON, TextEncoder });
    assert.equal(!!w.NemoTestPlayObserver, enabled);
    assert.equal(Object.hasOwn(w, '__NEMO_PLAY_OBSERVER_CONFIG__'), false);
  }
});

test('unconfigured or missing transport cannot pretend collection is ready', () => {
  const code = fs.readFileSync(bootstrap, 'utf8');
  for (const config of [undefined, { runId: RUN }]) {
    const w = { location: { href: 'tauri://localhost/index.html' }, __NEMO_PLAY_OBSERVER_CONFIG__: config };
    w.top = w;
    vm.runInNewContext(code, { window: w, URL, Promise, Set, Object, Number, JSON, TextEncoder });
    assert.equal(!!w.NemoTestPlayObserver, false);
  }
});

test('literal event sequence and immutable scalar pins reach the sink before footer', async () => {
  const { api, p } = await create();
  const event = { event: 'host-receipt', instanceId: RUN, documentId: 'native-document-3',
    contentRevision: 2, frame: 10, workId: 'native-work-159', viewGeneration: 159,
    lifecycleGeneration: 1, jsLifecycleGeneration: 2, hostLifecycleGeneration: 1,
    owner: 'native', sessionOrdinal: 1, status: 'presented' };
  assert.equal(api.record(event), undefined);
  event.frame = 99;
  await p.seal();
  assert.deepEqual(p.deliveries.map(d => d.action), ['record', 'record', 'seal', 'finish']);
  assert.deepEqual(p.deliveries.slice(0, 2).map(d => [d.payload.sequence, d.payload.event.event]), [[1, 'ready'], [2, 'host-receipt']]);
  assert.equal(p.deliveries[1].payload.event.frame, 10);
  assert.equal(p.deliveries[1].payload.event.workId, 'native-work-159');
  assert.deepEqual(p.deliveries[3].payload, { runId: RUN, cutId: CUT, generated: 2, dropped: 0, failed: 0, pending: 0 });
});

test('drain waits outside product flow for an outstanding diagnostic ACK', async () => {
  const p = ports('pending');
  const { createObserver } = require(bootstrap);
  const api = createObserver({ runId: RUN }, p);
  await new Promise(resolve => setImmediate(resolve));
  p.waiting.shift()();
  await api.ready;
  api.record({ event: 'play-stop', runGeneration: 2 });
  await new Promise(resolve => setImmediate(resolve));
  const finishing = p.seal();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(p.deliveries.some(d => d.action === 'seal'), false);
  p.waiting.shift()();
  await finishing;
  assert.equal(p.deliveries.at(-1).action, 'finish');
});

for (const mode of ['throw', 'reject']) {
  test(`${mode} sink never changes caller results and is reported as failure`, async () => {
    const { api, p } = await create(ports(mode));
    const calls = [];
    assert.doesNotThrow(() => { calls.push('commit'); api.record({ event: 'commit-return', frame: 2 }); calls.push('paint'); });
    assert.deepEqual(calls, ['commit', 'paint']);
    await p.seal();
    assert.equal(api.inspect().failed > 0, true);
  });
}

test('wrong ACK sequence and listener failure are never complete', async () => {
  const p = ports();
  p.invoke = async () => ({ sequence: 9 });
  const { api } = await create(p);
  await p.seal();
  assert.equal(api.inspect().failed > 0, true);
  const bad = ports(); bad.listen = async () => { throw new Error('private'); };
  const failed = await create(bad);
  assert.equal(failed.api.inspect().failed, 1);
  assert.equal(bad.deliveries.length, 0);
});

test('unknown fields, secrets, nested values, tuple coercion and accessors are dropped', async () => {
  const { api, p } = await create();
  for (const event of [
    { event: 'play-stop', path: '/private/project' }, { event: 'play-stop', error: 'token-secret' },
    { event: 'host-receipt', workId: 159 }, { event: 'host-receipt', documentId: '/private/file' },
    { event: 'navigate-return', result: {} }, { event: 'commit-return', frame: -1 },
    { event: 'host-receipt', contentRevision: '2' }, { event: 'host-receipt', frame: Infinity },
    { event: 'host-receipt', status: 'private-message' }, { event: 'unknown-event' },
    { event: 'host-receipt', owner: 'paper' },
    Object.defineProperty({ event: 'play-stop' }, 'frame', { enumerable: true, get() { throw new Error('secret'); } }),
  ]) assert.doesNotThrow(() => api.record(event));
  await p.seal();
  assert.equal(p.deliveries.filter(d => d.action === 'record').length, 1);
  assert.equal(p.deliveries.at(-1).payload.dropped, 12);
  assert.doesNotMatch(JSON.stringify(p.deliveries), /private|secret/);
});

test('full and late collectors stay bounded and report incomplete tails', async () => {
  const { api, p } = await create(ports(), 3);
  for (let i = 0; i < 30; i++) api.record({ event: 'play-step', frame: i });
  await p.seal();
  assert.equal(p.deliveries.filter(d => d.action === 'record').length, 3);
  assert.equal(p.deliveries.at(-1).payload.dropped, 28);
  const closedCounters = api.inspect();
  api.record({ event: 'ui-publication', frame: 20 });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(p.deliveries.at(-1).action, 'finish');
  assert.equal(api.inspect().phase, 'retired');
  assert.equal(api.inspect().outsideInterval, 1);
  assert.equal(api.inspect().generated, closedCounters.generated);
});

function fixture(t) {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-play-collection-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const reports = path.join(root, 'task', 'reports'); fs.mkdirSync(reports, { recursive: true });
  const destination = path.join(root, 'retained');
  // A disposable metadata producer, not Nemo or an installed-acceptance run.
  const producer = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'ignore' });
  const instance = { task: 'task-one', validated: true, snapshot: { taskId: 'task-one', source: { matches: true }, build: { matches: true },
    app: { pid: producer.pid, taskId: 'task-one', isolated: true, windows: ['main'],
      playObserver: { runId: RUN, schema: 'nemo.play-observer/1' } }, runtime: { roots: { root: path.join(root, 'task'), reports } } } };
  const frames = [
    { kind: 'header', schema: 'nemo.play-observer/1', runId: RUN, taskId: 'task-one', pid: producer.pid, window: 'main' },
    { kind: 'event', runId: RUN, sequence: 1, event: { event: 'ready' } },
    { kind: 'event', runId: RUN, sequence: 2, event: { event: 'play-stop', runGeneration: 1 } },
    { kind: 'seal', runId: RUN, cutId: CUT, status: 'provisional', generated: 2, accepted: 2, dropped: 0, failed: 0, rejected: 0 },
    { kind: 'footer', runId: RUN, cutId: CUT, terminalFence: true, pending: 0, status: 'complete', generated: 2, accepted: 2, dropped: 0, failed: 0, rejected: 0 },
  ];
  const log = path.join(reports, 'native-app.stderr.log');
  const write = values => fs.writeFileSync(log, '/private/other-native-trace\n' + values.map(v => 'NEMO_PLAY_OBSERVER ' + JSON.stringify(v)).join('\n') + '\n');
  async function stopped() {
    if (producer.exitCode === null && producer.signalCode === null) {
      const closed = once(producer, 'close'); producer.kill('SIGTERM'); await closed;
    }
    instance.stopped = true;
  }
  t.after(async () => {
    await stopped();
    require('./native-harness.cjs').finalizePlayObserver(instance);
  });
  return { root, reports, destination, instance, frames, log, write, stopped };
}

test('durable copy has the exact footer and hashes before disposable-root cleanup', async t => {
  const { collectPlayObserver, finalizePlayObserver } = require('./native-harness.cjs');
  const f = fixture(t); f.write(f.frames);
  const result = await collectPlayObserver(f.instance, { directory: f.destination, timeout: 100 });
  assert.equal(result.status, 'partial');
  assert.equal(result.phase, 'before-owner-stop');
  const bytes = fs.readFileSync(f.log);
  fs.rmSync(path.join(f.root, 'task'), { recursive: true });
  await f.stopped();
  assert.equal(finalizePlayObserver(f.instance).status, 'complete');
  assert.deepEqual(fs.readFileSync(path.join(f.destination, 'native-app.stderr.log')), bytes);
  const receipt = JSON.parse(fs.readFileSync(path.join(f.destination, 'play-observer.json')));
  assert.equal(receipt.stderrSha256, require('./native-harness.cjs').hash(path.join(f.destination, 'native-app.stderr.log')));
  assert.equal(receipt.footer.accepted, 2);
});

for (const failure of ['missing-footer', 'gap', 'duplicate', 'overflow', 'late', 'wrong-run']) {
  test(`${failure} copies useful evidence as partial instead of accepting`, async t => {
    const { collectPlayObserver } = require('./native-harness.cjs');
    const f = fixture(t);
    if (failure === 'missing-footer') f.frames.pop();
    if (failure === 'gap') f.frames[2].sequence = 3;
    if (failure === 'duplicate') f.frames.splice(2, 0, f.frames[1]);
    if (failure === 'overflow') f.frames.at(-1).dropped = 1;
    if (failure === 'late') f.frames.push({ kind: 'late', runId: RUN });
    if (failure === 'wrong-run') f.frames.at(-1).runId = 'wrong';
    f.write(f.frames);
    const result = await collectPlayObserver(f.instance, { directory: f.destination, timeout: 50 });
    assert.equal(result.status, 'partial');
    await f.stopped();
    assert.equal(require('./native-harness.cjs').finalizePlayObserver(f.instance).status, 'partial');
    assert.equal(fs.existsSync(path.join(f.destination, 'native-app.stderr.log')), true);
  });
}

test('collection refuses symlinked report/marker and an unvalidated or mismatched task', async t => {
  const { collectPlayObserver } = require('./native-harness.cjs');
  const f = fixture(t); f.write(f.frames);
  fs.symlinkSync(f.log, path.join(f.reports, 'play-observer-seal.json'));
  await assert.rejects(collectPlayObserver(f.instance, { directory: f.destination, timeout: 50 }), /observer/);
  fs.unlinkSync(path.join(f.reports, 'play-observer-seal.json'));
  f.instance.snapshot.taskId = 'other';
  await assert.rejects(collectPlayObserver(f.instance, { directory: f.destination, timeout: 50 }), /observer/);
});

test('late marker after initial copy and before process exit invalidates the terminal receipt', async t => {
  const { collectPlayObserver, finalizePlayObserver } = require('./native-harness.cjs');
  const f = fixture(t); f.write(f.frames);
  await collectPlayObserver(f.instance, { directory: f.destination, timeout: 100 });
  fs.appendFileSync(f.log, 'NEMO_PLAY_OBSERVER ' + JSON.stringify({ kind: 'late', runId: RUN }) + '\n');
  fs.rmSync(path.join(f.root, 'task'), { recursive: true });
  await f.stopped();
  const result = finalizePlayObserver(f.instance);
  assert.equal(result.status, 'partial');
  assert.equal(result.phase, 'after-owner-stop');
  assert.match(fs.readFileSync(path.join(f.destination, 'native-app.stderr.log'), 'utf8'), /"kind":"late"/);
});

test('an open producer or changed preserved copy cannot be called complete', async t => {
  const { collectPlayObserver, finalizePlayObserver } = require('./native-harness.cjs');
  const f = fixture(t); f.write(f.frames);
  await collectPlayObserver(f.instance, { directory: f.destination, timeout: 100 });
  assert.equal(finalizePlayObserver(f.instance).status, 'partial');
  assert.equal(f.instance.playObserverHandle, undefined);
  const g = fixture(t); g.write(g.frames);
  await collectPlayObserver(g.instance, { directory: g.destination, timeout: 100 });
  fs.writeFileSync(path.join(g.destination, 'native-app.stderr.log'), 'changed-owned-copy');
  await g.stopped();
  assert.equal(finalizePlayObserver(g.instance).status, 'partial');
  assert.equal(fs.readFileSync(path.join(g.destination, 'native-app.stderr.log'), 'utf8'), 'changed-owned-copy');
});

for (const swapped of ['native-app.stderr.log', 'play-observer.json']) {
  test(`terminal copy refuses a swapped ${swapped} symlink without modifying its target`, async t => {
    const { collectPlayObserver, finalizePlayObserver } = require('./native-harness.cjs');
    const f = fixture(t); f.write(f.frames);
    await collectPlayObserver(f.instance, { directory: f.destination, timeout: 100 });
    const target = path.join(f.root, 'unrelated'); fs.writeFileSync(target, 'keep-this');
    fs.unlinkSync(path.join(f.destination, swapped)); fs.symlinkSync(target, path.join(f.destination, swapped));
    await f.stopped();
    assert.equal(finalizePlayObserver(f.instance).status, 'partial');
    assert.equal(fs.readFileSync(target, 'utf8'), 'keep-this');
    assert.equal(f.instance.playObserverHandle, undefined);
  });
}

test('a refused owner-stop closes the held diagnostic fd and persists a partial receipt', async t => {
  const { createController, collectPlayObserver } = require('./native-harness.cjs');
  const f = fixture(t); f.write(f.frames);
  const cli = path.join(f.root, 'refused.cjs');
  fs.writeFileSync(cli, "process.stdout.write(JSON.stringify({stopped:false})+'\\n')");
  const controller = createController({ root: f.root, app: 'unused', cli, timeout: 200 });
  const owned = { ...f.instance, info: { ownerToken: 'unit-owner', pid: f.instance.snapshot.app.pid },
    process: { finished: false } };
  await collectPlayObserver(owned, { directory: f.destination, timeout: 100 });
  const fd = owned.playObserverHandle.fd;
  await assert.rejects(controller.stop(owned), /refused or incomplete/);
  assert.throws(() => fs.fstatSync(fd), { code: 'EBADF' });
  assert.equal(owned.playObserverHandle, undefined);
  assert.equal(owned.playObserverCollection.status, 'partial');
  assert.equal(JSON.parse(fs.readFileSync(path.join(f.destination, 'play-observer.json'))).status, 'partial');
});

test('post-cut late IPC must settle and be counted before a terminal seal is sent', async () => {
  const p = ports(), original = p.invoke;
  let rejectLate;
  p.invoke = (action, payload) => action === 'late'
    ? new Promise((_, reject) => { rejectLate = reject; }) : original(action, payload);
  const { api } = await create(p);
  const sealing = p.seal();
  api.record({ event: 'ui-publication', frame: 20 });
  await new Promise(resolve => setImmediate(resolve));
  // Independent effect oracle: no Native footer while a late delivery is unacknowledged.
  assert.equal(p.deliveries.some(d => d.action === 'seal'), false);
  rejectLate(new Error('lost diagnostic tail'));
  await sealing;
  const footer = p.deliveries.at(-1).payload;
  assert.equal(footer.generated, 2);
  assert.equal(footer.dropped, 1);
  assert.equal(footer.failed, 1);
  assert.equal(api.inspect().late, 1);
});

test('retention rejects a symlink ancestor into task storage before creating artifacts', async t => {
  const { collectPlayObserver } = require('./native-harness.cjs');
  const f = fixture(t); f.write(f.frames);
  const alias = path.join(f.root, 'outside-alias');
  fs.symlinkSync(path.join(f.root, 'task'), alias);
  await assert.rejects(collectPlayObserver(f.instance,
    { directory: path.join(alias, 'retained'), timeout: 100 }), /observer/);
  assert.equal(fs.existsSync(path.join(f.root, 'task', 'retained')), false);
});

test('canonical-equivalent platform aliases retain artifacts physically outside task storage', async t => {
  const { collectPlayObserver, finalizePlayObserver } = require('./native-harness.cjs');
  const f = fixture(t); f.write(f.frames);
  // On macOS this uses /var while fixture.root is its /private/var equivalent.
  const aliasDirectory = path.join(os.tmpdir(), path.basename(f.root), 'safe-retained');
  await collectPlayObserver(f.instance, { directory: aliasDirectory, timeout: 100 });
  assert.equal(f.instance.playObserverHandle.directory, path.join(f.root, 'safe-retained'));
  fs.rmSync(path.join(f.root, 'task'), { recursive: true });
  await f.stopped();
  assert.equal(finalizePlayObserver(f.instance).status, 'complete');
  assert.equal(fs.existsSync(path.join(f.root, 'safe-retained', 'play-observer.json')), true);
});

test('seal in flight remains provisional and failed late delivery invalidates the final host fence', async () => {
  const p = ports(), original = p.invoke;
  let sealPayload, resolveSeal, rejectLate;
  p.invoke = (action, payload) => {
    if (action === 'seal') return new Promise(resolve => { sealPayload = payload; resolveSeal = resolve; });
    if (action === 'late') return new Promise((_, reject) => { rejectLate = reject; });
    return original(action, payload);
  };
  const { api } = await create(p);
  const sealing = p.seal();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(sealPayload, { runId: RUN, generated: 1, dropped: 0, failed: 0 });
  api.record({ event: 'ui-publication', frame: 20 });
  await new Promise(resolve => setImmediate(resolve));
  resolveSeal({ ...sealPayload, kind: 'seal', status: 'provisional', cutId: CUT });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(p.deliveries.some(d => d.action === 'finish'), false);
  rejectLate(new Error('late IPC lost'));
  await sealing;
  const terminal = p.deliveries.find(d => d.action === 'finish');
  assert.ok(terminal, 'an attempted terminal fence must carry the final failure counters');
  assert.deepEqual(terminal.payload, { runId: RUN, cutId: CUT, generated: 2, dropped: 1, failed: 1, pending: 0 });
});

for (const absent of ['final-footer', 'terminalFence', 'cutId', 'pending']) {
  test(`provisional or missing ${absent} never establishes complete collection`, async t => {
    const { collectPlayObserver, finalizePlayObserver } = require('./native-harness.cjs');
    const f = fixture(t);
    if (absent === 'final-footer') f.frames.pop();
    else delete f.frames.at(-1)[absent];
    f.write(f.frames);
    await collectPlayObserver(f.instance, { directory: f.destination, timeout: 50 });
    await f.stopped();
    assert.equal(finalizePlayObserver(f.instance).status, 'partial');
  });
}

for (const mode of ['old-single-footer', 'wrong-cut', 'pending-tail', 'dirty-first-seal']) {
  test(`${mode} cannot pass a two-phase terminal proof`, async t => {
    const { collectPlayObserver, finalizePlayObserver } = require('./native-harness.cjs');
    const f = fixture(t);
    if (mode === 'old-single-footer') {
      f.frames.splice(3, 1);
      delete f.frames.at(-1).cutId; delete f.frames.at(-1).terminalFence; delete f.frames.at(-1).pending;
    }
    if (mode === 'wrong-cut') f.frames.at(-1).cutId = RUN;
    if (mode === 'pending-tail') f.frames.at(-1).pending = 1;
    if (mode === 'dirty-first-seal') f.frames[3].failed = 1;
    f.write(f.frames);
    await collectPlayObserver(f.instance, { directory: f.destination, timeout: 50 });
    await f.stopped();
    assert.equal(finalizePlayObserver(f.instance).status, 'partial');
  });
}

test('a missing or mismatched provisional ACK cannot dispatch the final fence', async () => {
  for (const change of [v => { v.runId = CUT; }, v => { v.cutId = 'unissued'; },
    v => { v.status = 'complete'; }, v => { v.generated++; }]) {
    const p = ports(), original = p.invoke;
    p.invoke = async (action, payload) => {
      const value = await original(action, payload);
      if (action === 'seal') change(value);
      return value;
    };
    const { api } = await create(p); await p.seal();
    assert.equal(p.deliveries.some(d => d.action === 'finish'), false);
    assert.equal(api.inspect().failed, 1);
    assert.equal(api.inspect().phase, 'sealed');
  }
});
