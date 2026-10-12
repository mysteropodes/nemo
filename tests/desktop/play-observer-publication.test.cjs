'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createRequire } = require('node:module');
const { extractFunction } = require('../fixtures/lib/sandbox.cjs');
const ROOT = path.resolve(__dirname, '../..');
// Reuse only the existing disposable SAME-REALM loader/host fixture, not its tests.
const helperPath = path.join(__dirname, 'play-observer-correlation.test.cjs');
const helper = fs.readFileSync(helperPath, 'utf8');
const end = helper.indexOf("test('actual lifecycle/raw"); assert.ok(end > 0);
const { realm } = new Function('require', '__dirname', helper.slice(0, end) + '\nreturn {realm};')(createRequire(helperPath), __dirname);
async function setup(mode = 'on', change = {}) {
  const r = realm(mode); await r.api.ready;
  let seal, footer;
  if (change.collector) {
    const deliveries = [], runId = 'da5f47f9-5681-4e71-a0b6-aa7578b6b5fa';
    const bootstrap = r.load(path.join(__dirname, 'play-publication-observer-bootstrap.js'));
    r.api = bootstrap.createObserver({ runId }, {
      async listen(callback) { seal = () => callback({ payload: { runId } }); return () => {}; },
      async invoke(action, payload) {
        deliveries.push({ action, payload });
        if (action === 'record') return { sequence: payload.sequence };
        if (action === 'seal') return { ...payload,kind:'seal',status:'provisional',cutId:'3d755db1-cdf7-44f8-8706-cc812e1845ca' };
        if (action === 'finish') { footer = { status: payload.dropped || payload.failed ? 'partial' : 'complete', ...payload }; return footer; }
        return {};
      }
    });
    r.window.NemoTestPlayObserver = r.api; await r.api.ready;
    r.events = () => deliveries.filter(d => d.action === 'record').map(d => JSON.parse(JSON.stringify(d.payload.event)));
  }
  const options = vm.runInContext(`({ previewReceipt(receipt) { receipt.workId = 'native-work-' + receipt.viewGeneration; return receipt; } })`, r.context);
  if (change.hostGate) options.previewGate = change.hostGate(r);
  if (change.receipt) options.previewReceipt = change.receipt(r);
  const h = r.helpers.nativeHarness(r.helpers.staticSource(), options); await h.controller.activate(h.prepared);
  const viewport = r.load(path.join(ROOT, 'src/js/application/native-opacity-viewport.js')).create(h.state.lifecycle, {}, () => false, () => true);
  const controller = { blocksLegacy: () => true, isActive: h.state.lifecycle.isActive,
    identity: h.state.lifecycle.identity, presentPreview: frame => viewport.presentPreview(frame) };
  r.window.NemoNativeOpacityCutover = controller; r.window._curFrame = 0;
  const state = r.context.state = vm.runInContext('({playing:false,playDir:1,currentFrame:0,totalFrames:21,fps:10,waIn:0,waOut:20,loopPlayback:false,pingPongPlayback:false})', r.context);
  const input = { value: 1 }, paints = [], audio = [], rafs = new Map(), requests = [], cancels = []; let now = 0, sequence = 0;
  if (change.input) { let held = 1; Object.defineProperty(input, 'value', { get: () => held, set: value => { held = change.input(value); } }); }
  r.context.document = { getElementById: id => id === 'btn-play' ? { innerHTML: '', classList: { add() {}, remove() {} } } : input };
  r.context.updateUI = flag => { paints.push([state.currentFrame, flag]);
    if (change.windowFrame) r.window._curFrame = 99;
    if (change.paint && state.currentFrame === 10) throw Error('paint failed'); };
  r.context.showToast = () => {};
  r.window.SMAudio = r.context.SMAudio = { onPlayStart: frame => audio.push(['start', frame]), onPlayStop: () => audio.push(['stop']), onLoop() {} };
  r.context.NemoNativeOpacityMotionSurface = r.load(path.join(ROOT, 'src/js/adapters/native-opacity-motion-surface.js'));
  r.context.performance = { now: () => now };
  r.window.requestAnimationFrame = r.context.requestAnimationFrame = fn => { rafs.set(++sequence, fn); requests.push(sequence); return sequence; };
  r.window.cancelAnimationFrame = r.context.cancelAnimationFrame = id => { cancels.push(id); rafs.delete(id); };
  r.context.clearInterval = () => {};
  const app = fs.readFileSync(path.join(ROOT, 'src/js/app.js'), 'utf8');
  const timeline = fs.readFileSync(path.join(ROOT, 'src/js/timeline.js'), 'utf8');
  vm.runInContext('var _nativeFrameNavigator=null, nativePlayScheduler=null, playInt=null, playRaf=null;\n' +
    extractFunction(app, 'goToFrame') + ['startNativePlay', 'advancePlayFrame', 'stopPlay'].map(name => extractFunction(timeline, name)).join('\n'), r.context);
  return { r, h, state, input, paints, audio, rafs, controller, requests, cancels,
    seal: () => seal(), footer: () => footer,
    navigate: frame => r.context.goToFrame(frame), start: () => r.context.startNativePlay(controller), stop: () => r.context.stopPlay(),
    tick(ms) { now += ms; const fn = rafs.values().next().value; assert.equal(typeof fn, 'function'); rafs.clear(); fn(now); } };
}
async function drain() { await new Promise(resolve => setImmediate(resolve)); }
test('real navigator and app assignment/paint/input produce a complete receipt-bound publication', async () => {
  const s = await setup(); assert.equal(await s.navigate(10), true); await drain();
  assert.equal(s.state.currentFrame, 10); assert.equal(s.r.window._curFrame, 10); assert.equal(Number(s.input.value), 11);
  const events = s.r.events();
  assert.ok(events.some(e => e.event === 'ui-publication' && e.currentFrame === 10 && e.frame === 10));
  assert.ok(events.some(e => e.event === 'sync-return' && e.currentFrame === 10));
  assert.ok(events.some(e => e.event === 'navigate-return' && e.result === true));
  assert.equal(s.r.api.inspect().productPendingNavigation, 0);
});

function accepted(events) {
  const finals = events.filter(e => e.event === 'navigate-return' && e.result === true);
  if (!finals.length) return false;
  const keys = ['instanceId','documentId','contentRevision','documentSnapshotId','contextId','frame',
    'jsLifecycleGeneration','hostGeneration','sessionOrdinal','hostCallOrdinal'];
  return finals.every(final => {
    if (keys.some(k => final[k] === undefined)) return false;
    const before = events.slice(0, events.indexOf(final));
    const pinned = e => keys.every(k => e[k] === final[k]) && e.workId === final.workId && e.viewGeneration === final.viewGeneration;
    const raw = before.find(e => e.event === 'host-receipt' && e.status === 'presented' && pinned(e));
    const returned = before.find(e => e.event === 'presented-return' && pinned(e));
    let last = raw ? events.indexOf(raw) : -1;
    if (last < 0 || !returned || events.indexOf(returned) <= last) return false;
    last = events.indexOf(returned);
    for (const kind of ['commit-enter','ui-publication','commit-return','paint-enter','paint-return','sync-enter','sync-return']) {
      const e = before.find(e => e.event === kind && e.intent === final.intent && pinned(e));
      if (!e || events.indexOf(e) <= last) return false;
      if (kind === 'ui-publication' && e.currentFrame !== final.frame) return false;
      if (kind === 'sync-return' && (e.currentFrame !== final.frame || e.result !== true)) return false;
      last = events.indexOf(e);
    }
    return !before.some(e => e.event === 'rollback' && e.intent === final.intent);
  });
}
test('actual complete chain rejects omitted/reordered/wrong-value/session/work/intent and failed mutants', async () => {
  const s = await setup(); await s.navigate(10); await drain(); const original = s.r.events();
  assert.equal(accepted(original), true);
  for (const kind of ['host-receipt','ui-publication','paint-return','sync-return']) {
    assert.equal(accepted(original.filter(e => e.event !== kind)), false, kind);
  }
  for (const [kind, key, value] of [['ui-publication','currentFrame',9],['sync-return','currentFrame',9],
    ['sync-return','result',false],['commit-enter','sessionOrdinal',99],['commit-return','workId','native-work-999'],
    ['paint-return','intent',99],['host-receipt','status','deferred-occluded']]) {
    const events = structuredClone(original); events.find(e => e.event === kind)[key] = value;
    assert.equal(accepted(events), false, kind + key);
  }
  const swapped = structuredClone(original), a = swapped.findIndex(e => e.event === 'paint-return'), b = swapped.findIndex(e => e.event === 'sync-return');
  [swapped[a],swapped[b]] = [swapped[b],swapped[a]]; assert.equal(accepted(swapped), false);
});
for (const change of [{ input: () => 999 }, { windowFrame: true }]) test('actual mismatched displayed/frame value makes the collector partial without correcting product state', async () => {
  const s = await setup('on', change); assert.equal(await s.navigate(10), true); await drain();
  assert.equal(accepted(s.r.events()), false); assert.ok(s.r.api.inspect().dropped > 0);
});
for (const status of ['deferred-occluded','deferred-timeout','stale-discarded','failed-device-lost']) {
  test(`actual ${status} result cannot become a successful publication`, async () => {
    const s = await setup('on', { receipt: r => vm.runInContext(`(receipt => ({...receipt,workId:'native-work-'+receipt.viewGeneration,status:${JSON.stringify(status)}}))`, r.context) });
    assert.equal(await s.navigate(10), false); await drain();
    assert.equal(accepted(s.r.events()), false); assert.equal(s.r.api.inspect().productPendingNavigation, 0);
    assert.ok(s.r.api.inspect().dropped > 0, 'unreturned host-failure context stays partial');
    assert.equal(s.state.currentFrame, 0);
  });
}
test('actual paint failure restores prior UI and cannot be credited from its transient commit', async () => {
  const s = await setup('on', { paint: true }); assert.equal(await s.navigate(10), false); await drain();
  assert.equal(s.state.currentFrame, 0); assert.equal(s.r.window._curFrame, 0);
  assert.ok(s.r.events().some(e => e.event === 'rollback' && e.currentFrame === 0));
  assert.equal(accepted(s.r.events()), false); assert.equal(s.r.api.inspect().productPendingNavigation, 0);
});
test('newer actual navigator intent supersedes queued work and settles both requests', async () => {
  const s = await setup(); assert.deepEqual(await Promise.all([s.navigate(10),s.navigate(20)]), [false,true]); await drain();
  assert.equal(s.h.state.previews.length, 1); assert.equal(s.state.currentFrame, 20);
  assert.equal(s.r.api.inspect().productPendingNavigation, 0);
  assert.ok(s.r.events().some(e => e.event === 'navigate-superseded' && e.intent === 1));
  assert.equal(accepted(s.r.events()), true);
});
async function summary(mode, change = {}) {
  const s = await setup(mode, change); s.start(); s.tick(1000); await drain(); await drain();
  s.stop(); await drain(); await drain();
  return JSON.parse(JSON.stringify({ frames:s.h.state.previews.map(p=>p.frame),state:s.state.currentFrame,window:s.r.window._curFrame,
    input:s.input.value,playing:s.state.playing,paints:s.paints,audio:s.audio,requests:s.requests,cancels:s.cancels }));
}
for (const change of [{},{ paint:true }]) test('absent/throwing/full/rejecting telemetry preserves original controls, RAF/audio and native calls', async () => {
  const baseline = await summary('off', change);
  for (const mode of ['on','throwing','full','reject']) assert.deepEqual(await summary(mode, change), baseline, mode);
});
test('actual Stop restore remains pending after the button is idle, then becomes genuinely quiescent', async () => {
  const s = await setup('on', { hostGate: r => vm.runInContext('new Promise(resolve => { globalThis.finishHost = resolve; })', r.context) });
  s.start(); s.tick(100); await drain(); s.stop();
  assert.equal(s.state.playing, false);
  assert.ok(s.r.api.inspect().productPendingNavigation > 0); assert.ok(s.r.api.inspect().productPendingPlayback > 0);
  s.r.context.finishHost(); await drain(); await drain();
  assert.equal(s.r.api.inspect().productPendingNavigation, 0); assert.equal(s.r.api.inspect().productPendingPlayback, 0);
  assert.equal(s.state.currentFrame, 0); assert.equal(s.r.window._curFrame, 0);
  assert.deepEqual(Array.from(s.h.state.previews,p=>p.frame), [1,0]);
  assert.equal(s.audio.filter(x => x[0] === 'stop').length, 1);
});
test('real premature seal after Stop refuses complete collection while its unawaited restore is pending', async () => {
  const s = await setup('on', { collector:true, hostGate: r => vm.runInContext('new Promise(resolve => { globalThis.finishHost = resolve; })', r.context) });
  s.start(); s.tick(100); await drain(); s.stop();
  await s.seal(); assert.equal(s.footer().status, 'partial');
  assert.ok(s.footer().dropped > 0);
  s.r.context.finishHost(); await drain(); await drain();
  assert.ok(s.r.api.inspect().outsideInterval > 0); assert.equal(s.footer().status, 'partial');
});
test('actual stopped/restored product work can seal a complete bounded interval', async () => {
  const s = await setup('on', { collector:true });
  s.start(); s.tick(100); await drain(); await drain(); s.stop(); await drain(); await drain();
  assert.equal(s.r.api.inspect().productPendingNavigation, 0);
  assert.equal(s.r.api.inspect().productPendingPlayback, 0); assert.equal(s.r.api.inspect().productPlaying, false);
  assert.equal(accepted(s.r.events()), true);
  await s.seal(); assert.equal(s.footer().status, 'complete');
  assert.equal(s.footer().dropped, 0); assert.equal(s.footer().failed, 0);
});
test('actual owner release during a held host call prevents any later UI publication', async () => {
  const s = await setup('on', { hostGate:r => vm.runInContext('new Promise(resolve => { globalThis.finishHost = resolve; })', r.context) });
  const old = s.navigate(10); await drain(); const released = s.h.controller.releaseCurrent('publication-close');
  s.r.context.finishHost(); assert.equal(await old, false); await released; await drain();
  assert.equal(s.h.controller.status(), 'closed'); assert.equal(s.state.currentFrame, 0);
  assert.equal(accepted(s.r.events()), false); assert.equal(s.r.api.inspect().productPendingNavigation, 0);
  assert.ok(s.r.api.inspect().dropped > 0);
});
test('actual native revision drift cannot publish the old receipt or invent a current correlation', async () => {
  const s = await setup('on', { hostGate:r => vm.runInContext('new Promise(resolve => { globalThis.finishHost = resolve; })', r.context) });
  const old = s.navigate(10); await drain(); const revised = s.h.externalOpacity(40);
  s.r.context.finishHost(); assert.equal(await old, false); await revised; await drain();
  assert.equal(s.h.state.identity.contentRevision, 1); assert.equal(s.state.currentFrame, 0);
  assert.equal(accepted(s.r.events()), false); assert.equal(s.r.api.inspect().productPendingNavigation, 0);
});
