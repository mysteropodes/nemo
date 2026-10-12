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
  const viewport = r.load(path.join(ROOT, 'src/js/application/native-opacity-viewport.js')).create(h.state.lifecycle,
    change.surface ? { surface: change.surface(r) } : {}, () => false, () => true);
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
  return { r, h, state, input, paints, audio, rafs, controller, requests, cancels, viewport,
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
    assert.equal(s.r.api.inspect().dropped, 0, 'actual receipt-derived rejection is now attributed');
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
  assert.equal(s.r.api.inspect().dropped, 0);
});
test('actual native revision drift cannot publish the old receipt or invent a current correlation', async () => {
  const s = await setup('on', { hostGate:r => vm.runInContext('new Promise(resolve => { globalThis.finishHost = resolve; })', r.context) });
  const old = s.navigate(10); await drain(); const revised = s.h.externalOpacity(40);
  s.r.context.finishHost(); assert.equal(await old, false); await revised; await drain();
  assert.equal(s.h.state.identity.contentRevision, 1); assert.equal(s.state.currentFrame, 0);
  assert.equal(accepted(s.r.events()), false); assert.equal(s.r.api.inspect().productPendingNavigation, 0);
});

test('actual viewport deferred failure joins its own raw call and does not borrow rollback restoration', async () => {
  const s = await setup('on', { collector:true, receipt:r => vm.runInContext('(receipt => ({...receipt,workId:"native-work-"+receipt.viewGeneration,status:"deferred-occluded"}))', r.context) });
  assert.equal(await s.navigate(10), false); await drain();
  const events = s.r.events(), failure = events.find(e => e.event === 'failure'), terminal = events.find(e => e.event === 'navigate-return');
  assert.ok(failure); assert.equal(failure.hostCallOrdinal, 1); assert.equal(failure.frame, 10);
  assert.equal(failure.workId, 'native-work-1'); assert.equal(failure.status, 'deferred-occluded');
  assert.equal(terminal.hostCallOrdinal, 1); assert.equal(terminal.result, false);
  assert.ok(events.some(e => e.event === 'host-receipt' && e.hostCallOrdinal === 2 && e.frame === 0));
  assert.equal(accepted(events), false); assert.equal(s.r.api.inspect().dropped, 0);
  await s.seal(); assert.equal(s.footer().status, 'complete');
});

async function rawMetadata(s, frame) {
  await s.controller.presentPreview(frame);
  const value = s.h.state.previews.at(-1);
  s.r.context.requestForRaw = value;
  return vm.runInContext('(() => { var x=JSON.parse(JSON.stringify(requestForRaw));delete x.apiVersion;delete x.instanceId;return x; })()', s.r.context);
}
test('raw rejection preserves the ACTUAL object and never decorates, serializes or matches its message', async () => {
  const s = await setup(); const meta = await rawMetadata(s, 10), raw = s.h.state.previewConsumers[0];
  const error = vm.runInContext('new Error("private host details")', s.r.context), keys = Reflect.ownKeys(error);
  const rejected = raw.present(s.h.state.identity.instanceId, 1, meta, () => Promise.reject(error), () => {});
  await assert.rejects(rejected, actual => actual === error);
  const context = s.r.api.errorContext(error); assert.ok(Object.isFrozen(context));
  assert.equal(context.frame, 10); assert.equal(context.sessionOrdinal, 1); assert.equal(context.hostGeneration, 1);
  assert.equal(context.hostCallOrdinal, 2); assert.equal(Object.hasOwn(context, 'workId'), false);
  assert.deepEqual(Reflect.ownKeys(error), keys);
  assert.equal(error.message, 'private host details'); assert.equal(error.name, 'Error');
  const unrelated = vm.runInContext('new Error("private host details")', s.r.context);
  assert.equal(s.r.api.errorContext(unrelated), undefined);
  const ticket = s.r.api.navigationRequest(99, 10, s.h.state.lifecycle.identity());
  s.r.api.navigationFailure(ticket, unrelated); s.r.api.navigationReturn(ticket, false); await drain();
  const terminal = s.r.events().find(e => e.event === 'navigate-return' && e.intent === 99);
  assert.equal(Object.hasOwn(terminal, 'hostCallOrdinal'), false); assert.ok(s.r.api.inspect().dropped > 0);
  assert.equal(JSON.stringify(s.r.events()).includes('private host details'), false);
});
test('distinct overlapping raw rejected objects keep their OWN snapshot/frame/host call', async () => {
  const s = await setup(), first = await rawMetadata(s, 10), next = await rawMetadata(s, 20), raw = s.h.state.previewConsumers[0];
  const a = vm.runInContext('new Error("same private text")', s.r.context), b = vm.runInContext('new Error("same private text")', s.r.context);
  const outputs = await Promise.allSettled([
    raw.present(s.h.state.identity.instanceId, 1, first, () => Promise.reject(a), () => {}),
    raw.present(s.h.state.identity.instanceId, 1, next, () => Promise.reject(b), () => {})]);
  assert.strictEqual(outputs[0].reason, a); assert.strictEqual(outputs[1].reason, b);
  const ca = s.r.api.errorContext(a), cb = s.r.api.errorContext(b);
  assert.equal(ca.frame, 10); assert.equal(cb.frame, 20); assert.equal(ca.hostCallOrdinal, 3); assert.equal(cb.hostCallOrdinal, 4);
  assert.equal(ca.sessionOrdinal, cb.sessionOrdinal); assert.equal(ca.documentSnapshotId, cb.documentSnapshotId);
});
test('the SAME rejected object reused across real calls becomes ambiguous, never latest-context attribution', async () => {
  const s = await setup(), a = await rawMetadata(s, 10), b = await rawMetadata(s, 20), raw = s.h.state.previewConsumers[0];
  const error = vm.runInContext('new Error("shared rejection")', s.r.context);
  await assert.rejects(raw.present(s.h.state.identity.instanceId, 1, a, () => Promise.reject(error), () => {}), e => e === error);
  assert.equal(s.r.api.errorContext(error).frame, 10);
  await assert.rejects(raw.present(s.h.state.identity.instanceId, 1, b, () => Promise.reject(error), () => {}), e => e === error);
  assert.equal(s.r.api.errorContext(error), null); assert.ok(s.r.api.inspect().dropped > 0);
});
test('primitive and unmapped rejection stays PARTIAL with no private text or invented host/work pins', async () => {
  const s = await setup(), meta = await rawMetadata(s, 10), raw = s.h.state.previewConsumers[0];
  await assert.rejects(raw.present(s.h.state.identity.instanceId, 1, meta, () => Promise.reject('private primitive'), () => {}), e => e === 'private primitive');
  assert.equal(s.r.api.errorContext('private primitive'), undefined); assert.ok(s.r.api.inspect().dropped > 0);
  await drain(); assert.equal(JSON.stringify(s.r.events()).includes('private primitive'), false);
});
test('old actual rejected context stays immutable after release/re-entry and cannot attach to the new owner', async () => {
  const s = await setup(), meta = await rawMetadata(s, 10), raw = s.h.state.previewConsumers[0];
  const error = vm.runInContext('new Error("retained old error")', s.r.context);
  await assert.rejects(raw.present(s.h.state.identity.instanceId, 1, meta, () => Promise.reject(error), () => {}), e => e === error);
  const old = s.r.api.errorContext(error);
  await s.h.controller.releaseCurrent('error-reentry'); await s.h.controller.activate(s.h.prepared);
  assert.strictEqual(s.r.api.errorContext(error), old); assert.equal(old.hostGeneration, 1); assert.equal(old.sessionOrdinal, 1);
  const ticket = s.r.api.navigationRequest(2, 10, s.h.state.lifecycle.identity());
  s.r.api.navigationFailure(ticket, error); s.r.api.navigationReturn(ticket, false); await drain();
  const row = s.r.events().find(e => e.event === 'navigate-return' && e.intent === 2);
  assert.equal(row.documentId, 'native-document-2'); assert.equal(Object.hasOwn(row, 'hostCallOrdinal'), false);
  assert.ok(s.r.api.inspect().dropped > 0);
});
test('receipt-derived failure oracle rejects independent call/session/snapshot/work substitutions', async () => {
  const s = await setup('on', { receipt:r => vm.runInContext('(receipt => ({...receipt,workId:"native-work-"+receipt.viewGeneration,status:"deferred-timeout"}))', r.context) });
  await s.navigate(10); await drain(); const rows = s.r.events();
  const joinedFailure = events => {
    const failed = events.find(e => e.event === 'failure' && e.frame === 10);
    return !!failed && events.slice(0, events.indexOf(failed)).some(e => e.event === 'host-receipt' &&
      ['instanceId','documentId','contentRevision','documentSnapshotId','frame','sessionOrdinal','hostGeneration','hostCallOrdinal','workId','viewGeneration','status'].every(k => e[k] === failed[k]));
  };
  assert.equal(joinedFailure(rows), true);
  for (const key of ['hostCallOrdinal','sessionOrdinal','documentSnapshotId','workId']) {
    const altered = structuredClone(rows), failed = altered.find(e => e.event === 'failure');
    failed[key] = typeof failed[key] === 'number' ? failed[key] + 9 : 'unrelated'; assert.equal(joinedFailure(altered), false, key);
  }
});

test('a real Error already owned by another call cannot replace an associated publication context', async () => {
  const s = await setup(), meta = await rawMetadata(s, 10), raw = s.h.state.previewConsumers[0];
  const receipt = await s.controller.presentPreview(10), ticket = s.r.api.navigationRequest(9, 10, s.h.state.lifecycle.identity());
  const publication = s.r.api.beginPublication(ticket, receipt); s.r.api.endPublication(publication);
  const expected = s.r.api.receiptContext(receipt), error = vm.runInContext('new Error("another operation")', s.r.context);
  await assert.rejects(raw.present(s.h.state.identity.instanceId, 1, meta, () => Promise.reject(error), () => {}), e => e === error);
  assert.notEqual(s.r.api.errorContext(error).hostCallOrdinal, expected.hostCallOrdinal);
  s.r.api.associateNavigationError(ticket, error); s.r.api.navigationFailure(ticket, error); s.r.api.navigationReturn(ticket, false);
  assert.equal(s.r.api.errorContext(error), null); assert.ok(s.r.api.inspect().dropped > 0);
  await drain(); const row = s.r.events().find(e => e.event === 'navigate-return' && e.intent === 9);
  assert.equal(row.hostCallOrdinal, expected.hostCallOrdinal);
});

test('a resize rejection in the NEXT viewport iteration never borrows its earlier stale receipt', async () => {
  let resizeError;
  const s = await setup('on', {
    surface:r => { resizeError = vm.runInContext('new Error("private resize details")', r.context);
      return { defer() { return 1; }, cancel() {}, resize() { return Promise.reject(resizeError); } }; },
    receipt:r => vm.runInContext('(receipt => { receipt.workId="native-work-"+receipt.viewGeneration; scheduleResize(); return {...receipt,status:"stale-discarded"}; })', r.context)
  });
  s.r.context.scheduleResize = () => s.viewport.resizeViewport();
  assert.equal(await s.navigate(10), false); await drain();
  assert.equal(s.h.state.previews.length, 1);
  assert.equal(s.r.api.errorContext(resizeError), undefined); assert.ok(s.r.api.inspect().dropped > 0);
  const row = s.r.events().find(e => e.event === 'failure');
  assert.equal(Object.hasOwn(row, 'hostCallOrdinal'), false);
  assert.equal(JSON.stringify(s.r.events()).includes('private resize details'), false);
});
test('invalid raw scalar slots never retain private objects in an error context', async () => {
  const s = await setup('on', { receipt:r => vm.runInContext('(receipt => ({...receipt,workId:"native-work-1",status:{secret:"private payload"}}))', r.context) });
  assert.equal(await s.navigate(10), false); await drain();
  assert.ok(s.r.api.inspect().dropped > 0);
  assert.equal(JSON.stringify(s.r.events()).includes('private payload'), false);
  const row = s.r.events().find(e => e.event === 'failure');
  assert.equal(Object.hasOwn(row, 'status'), false);
});

test('a current UI stage with a missing publication association cannot join a cached older error', async () => {
  const s = await setup(), meta = await rawMetadata(s, 10), raw = s.h.state.previewConsumers[0];
  const old = vm.runInContext('new Error("cached paint failure")', s.r.context);
  await assert.rejects(raw.present(s.h.state.identity.instanceId, 1, meta, () => Promise.reject(old), () => {}), e => e === old);
  const prior = s.r.api.errorContext(old); assert.equal(prior.hostCallOrdinal, 2);
  s.r.window.NemoTestPlayObserver = new Proxy(s.r.api, { get(target, key) {
    return key === 'beginPublication' ? () => undefined : target[key];
  } });
  s.r.context.updateUI = () => { throw old; };
  assert.equal(await s.navigate(10), false); await drain();
  const failure = s.r.events().find(e => e.event === 'failure');
  assert.equal(Object.hasOwn(failure, 'hostCallOrdinal'), false, 'identity equality cannot substitute the older call');
  assert.ok(s.r.api.inspect().dropped > 0);
});
