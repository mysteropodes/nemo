'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { createRequire } = require('node:module');
const { createWorkflow, prepare } = require('../cs7-scenarios/play-resume.cjs');
const { retained, read } = require('../cs7-play-evidence.cjs');
const { spawn } = require('node:child_process'), { once } = require('node:events');
// Reuse the original oracle's explicitly synthetic model, without registering
// its tests or treating its pixels/receipts as installed evidence.
const source = path.resolve(__dirname, '../../play-publication-oracle.test.cjs');
const text = fs.readFileSync(source, 'utf8'), end = text.indexOf('\ntest(');
assert.ok(end > 0);
const model = require('node:vm').compileFunction(text.slice(0, end) + '\nreturn model;', ['require', '__dirname'], { filename: source })(createRequire(source), path.dirname(source));
function fixture(t, alter) {
  const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-play-model-')));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const e = model(), calls = [], owner = { identity: { sourceSha: e.sourceCommit,
    executableSha256: e.executableSha256, sessionId: e.taskId, pid: e.pid }, windowId: e.windowId, runId: e.runId };
  for (const v of e.visuals) { v.file = 'frame-' + v.frame + '.png'; fs.writeFileSync(path.join(directory, v.file), v.png); }
  const session = { playContext: async () => structuredClone(owner), retainedPlayEvidence: () => e,
    requestEvidence: async r => {
      calls.push(r.phase);
      const v = e.visuals.find(v => r.phase === 'frame-' + v.frame);
      const raw = { observation: 'Synthetic model only', productFailure: false,
        ...(v ? { visual: Object.fromEntries(Object.entries(v).filter(([k]) => k !== 'png')) } : {}) };
      return alter ? alter(raw, r) : raw;
    } };
  return { directory, e, owner, session, calls,
    workflow: createWorkflow({ fixture: '/model-only/no-app-launch', reportDirectory: directory }),
    row: { id: 'play-resume', issue: 1573, checks: ['visible-recovery', 'ordered-telemetry', 'visible-frames'].map(id => ({ id })) } };
}
test('Play keeps original phases and defers the unchanged trace/pixel oracle until cleanup', async t => {
  const f = fixture(t), result = await f.workflow.run(f);
  assert.deepEqual(f.calls, ['open', 'play-minimize', 'restore-play-stop', 'frame-0', 'frame-10', 'frame-20']);
  assert.deepEqual(result.checks.map(c => c.status), ['unrun', 'unrun', 'unrun']);
  assert.deepEqual((await result.finalize()).map(c => c.status), ['pass', 'pass', 'pass']);
  await assert.rejects(f.workflow.run(f), /cannot restart/);
});
test('a product failure preserves unrun telemetry/pixels and does not request another phase', async t => {
  const f = fixture(t, (raw, r) => ({ ...raw, productFailure: r.phase === 'play-minimize' }));
  const result = await f.workflow.run(f);
  assert.equal(result.safeToContinue, false); assert.equal(result.finalize, undefined);
  assert.deepEqual(f.calls, ['open', 'play-minimize']);
  assert.deepEqual(result.checks.map(c => c.status), ['fail', 'unrun', 'unrun']);
});
test('missing testimony, foreign owner and outside/symlinked witnesses are refused', async t => {
  for (const mode of ['missing', 'foreign', 'outside', 'symlink']) await t.test(mode, async t => {
    const f = fixture(t, (raw, r) => {
      if (mode === 'missing') delete raw.observation;
      if (raw.visual && mode === 'outside') raw.visual.file = '../outside.png';
      if (raw.visual && mode === 'symlink') raw.visual.file = 'alias.png';
      if (mode === 'foreign' && r.phase === 'open') f.owner.identity.pid++;
      return raw;
    });
    fs.symlinkSync(path.join(f.directory, 'frame-0.png'), path.join(f.directory, 'alias.png'));
    await assert.rejects(f.workflow.run(f));
    assert.ok(f.calls.length < 6);
  });
});
test('original oracle refuses absent cleanup, corrupted pixels, missing trace and fabricated publication links', async t => {
  for (const mode of ['cleanup', 'pixels', 'trace', 'publication']) await t.test(mode, async t => {
    const f = fixture(t, raw => {
      if (mode === 'publication' && raw.visual) raw.visual.workId = 'native-work-999';
      return raw;
    });
    if (mode === 'pixels') fs.writeFileSync(path.join(f.directory, 'frame-0.png'), Buffer.from('invalid'));
    const result = await f.workflow.run(f);
    if (mode === 'cleanup') f.e.cleanup.slotsReleased = false;
    if (mode === 'trace') f.e.stderr = Buffer.alloc(0);
    assert.deepEqual((await result.finalize()).map(c => c.status), ['fail', 'fail', 'fail']);
  });
});
test('original fixture cannot be substituted at preparation', () => {
  assert.throws(() => prepare({ config: { playObserver: true, play: { fixture: source } } }), /fixture/);
});
test('retained evidence requires normal owner exit, released slots and the exact terminal copy', async t => {
  const f = fixture(t), child = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { stdio: 'ignore' });
  await once(child, 'spawn');
  t.after(() => { if (child.exitCode === null && child.signalCode === null) child.kill(); });
  const identity = { ...f.owner.identity, pid: child.pid };
  const copy = { ...f.e.collection, pid: child.pid };
  const instance = { info: { pid: child.pid }, stopped: true,
    snapshot: { app: { playObserver: { runId: f.e.runId } }, runtime: { roots: { root: path.join(f.directory, 'removed-task') } } },
    stopReceipt: { stopped: true, processTree: { stopped: true }, released: { released: true, slots: { retained: [] } }, retainedData: false },
    playObserverCollection: copy };
  fs.writeFileSync(path.join(f.directory, 'play-observer.json'), JSON.stringify(copy));
  fs.writeFileSync(path.join(f.directory, 'native-app.stderr.log'), f.e.stderr);
  const args = { instance, identity, windowId: 81, directory: f.directory, cleaned: true, unsigned: true };
  assert.throws(() => retained(args), /cleanup/);
  const closed = once(child, 'close'); child.kill(); await closed;
  assert.equal(retained(args).cleanup.slotsReleased, true);
  assert.throws(() => retained({ ...args, cleaned: false }), /cleanup/);
  instance.stopReceipt.released.slots.retained.push({ protected: true });
  assert.throws(() => retained(args), /cleanup/); instance.stopReceipt.released.slots.retained.pop();
  fs.writeFileSync(path.join(f.directory, 'play-observer.json'), '{}');
  assert.throws(() => retained(args), /manifest changed/);
  fs.writeFileSync(path.join(f.directory, 'play-observer.json'), JSON.stringify(copy));
  fs.appendFileSync(path.join(f.directory, 'native-app.stderr.log'), 'late\n');
  assert.throws(() => retained(args), /trace changed/);
  fs.symlinkSync(path.join(f.directory, 'native-app.stderr.log'), path.join(f.directory, 'alias.log'));
  assert.throws(() => read(path.join(f.directory, 'alias.log')), /canonical/);
});
