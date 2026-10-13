'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, 'desktop/startup-observer-bootstrap.js'), 'utf8');
const { createStartupProbe } = require('./desktop/startup-observer-bootstrap.js');
const RUN = '12345678-1234-1234-1234-123456789abc';
const CUT = 'abcdef12-1234-1234-1234-123456789abc';
function realm(initial = 'loading', delivery = () => true) {
  function target() {
    const handlers = new Map();
    return { handlers, addEventListener(n, f) { if (!handlers.has(n)) handlers.set(n, new Set()); handlers.get(n).add(f); },
      removeEventListener(n, f) { handlers.get(n)?.delete(f); },
      dispatch(n, event = { isTrusted: true }) { for (const f of [...(handlers.get(n) || [])]) f(event); } };
  }
  const doc = Object.assign(target(), { readyState: initial });
  const win = Object.assign(target(), { document: doc }); win.top = win;
  const node = { hidden: false, parentElement: null,
    getBoundingClientRect() { return { width: 180, height: 48 }; } };
  doc.getElementById = id => { assert.equal(id, 'start-open'); return node; };
  const messages = [], frames = new Map(), cancelled = []; let next = 0;
  const ports = { window: win, document: doc,
    emit(e) { messages.push(e); return delivery(e); },
    getComputedStyle() { return { display: 'block', visibility: 'visible', opacity: '1' }; },
    requestAnimationFrame(f) { frames.set(++next, f); return next; },
    cancelAnimationFrame(id) { cancelled.push(id); frames.delete(id); } };
  const config = { enabled: true, runId: RUN, loadGeneration: 7 };
  function tick() { const first = frames.entries().next().value; if (first) { frames.delete(first[0]); first[1](); } }
  return { doc, win, node, ports, config, messages, frames, cancelled, tick,
    start() { return createStartupProbe(config, ports); } };
}
function stages(r) { return r.messages.map(e => e.stage); }

test('source expression installs no browser global or starts an ordinary page probe', () => {
  const context = vm.createContext({});
  const before = Object.getOwnPropertyDescriptors(context);
  const api = vm.runInContext(source, context);
  assert.equal(typeof api.createStartupProbe, 'function');
  assert.deepEqual(Object.getOwnPropertyDescriptors(context), before);
});

test('actual readiness, control and two owned rAF acknowledgements remain distinct', async () => {
  const r = realm(), probe = r.start();
  r.doc.readyState = 'interactive'; r.doc.dispatch('DOMContentLoaded');
  r.doc.readyState = 'complete'; r.win.dispatch('load');
  r.tick(); r.tick(); r.tick(); await probe.drained();
  assert.deepEqual(stages(r), ['early-script', 'dom-ready', 'startup-control', 'window-load', 'raf-ack', 'raf-ack']);
  assert.equal(r.messages[2].visibility, 'visible');
  assert.deepEqual(r.messages.filter(e => e.stage === 'raf-ack').map(e => e.ordinal), [1, 2]);
  assert.equal(r.frames.size, 0); assert.equal(probe.state().visiblePaint, 'not-observed');
  assert.deepEqual(r.messages.map(e => e.sequence), [1, 2, 3, 4, 5, 6]);
  for (const e of r.messages) { assert.equal(e.runId, RUN); assert.equal(e.loadGeneration, 7); assert(Object.isFrozen(e)); }
  probe.dispose();
});

test('already-ready document is labeled as observation, never fabricated browser events', async () => {
  for (const ready of ['interactive', 'complete']) {
    const r = realm(ready), probe = r.start(); r.tick(); r.tick(); await probe.drained();
    assert.equal(r.messages[1].stage, 'ready-state-observed');
    assert.equal(r.messages[1].readyState, ready);
    assert(!stages(r).includes('dom-ready')); assert(!stages(r).includes('window-load'));
    assert.equal(probe.state().domEvent, false); assert.equal(probe.state().loadEvent, false);
    probe.dispose();
  }
  const r = realm(), probe = r.start(); await probe.drained();
  assert.deepEqual(stages(r), ['early-script']); assert.equal(probe.state().contentReady, false); probe.dispose();
});

test('invalid opt-in, identity, subframe and inaccessible frame fail without emission', () => {
  for (const config of [{}, { enabled: false }, { runId: 'private-secret' },
    { loadGeneration: 0 }, { loadGeneration: 1.5 }, { loadGeneration: 2147483648 }]) {
    const r = realm(); Object.assign(r.config, config);
    if (!Object.keys(config).length) for (const key of Object.keys(r.config)) delete r.config[key];
    assert.throws(() => r.start(), /startup probe unavailable/); assert.equal(r.messages.length, 0);
  }
  for (const variant of ['subframe', 'throw', 'document', 'port']) {
    const r = realm();
    if (variant === 'subframe') r.win.top = {};
    if (variant === 'throw') Object.defineProperty(r.win, 'top', { get() { throw Error('private-frame'); } });
    if (variant === 'document') r.win.document = {};
    if (variant === 'port') delete r.ports.emit;
    assert.throws(() => r.start(), /startup probe unavailable/); assert.equal(r.messages.length, 0);
  }
});

test('caller identity mutation cannot retarget events or probe state', async () => {
  const r = realm(), probe = r.start(); r.config.runId = 'private-secret'; r.config.loadGeneration = 99;
  r.doc.readyState = 'interactive'; r.doc.dispatch('DOMContentLoaded'); await probe.drained();
  assert(r.messages.every(e => e.runId === RUN && e.loadGeneration === 7));
  assert(Object.isFrozen(probe)); assert(Object.isFrozen(probe.state())); probe.dispose();
  const deceptive = realm(); let reads = 0;
  Object.defineProperty(deceptive.config, 'runId', { get() { return reads++ ? 'secret-retarget' : RUN; } });
  const captured = deceptive.start(); await captured.drained();
  assert.equal(reads, 1); assert.equal(deceptive.messages[0].runId, RUN); captured.dispose();
});

test('synthetic, duplicate and out-of-order readiness never claims trusted readiness', async () => {
  for (const variant of ['synthetic', 'loading', 'load-first', 'duplicate']) {
    const r = realm(), probe = r.start();
    if (variant === 'synthetic') { r.doc.readyState = 'interactive'; r.doc.dispatch('DOMContentLoaded', { isTrusted: false }); }
    if (variant === 'loading') r.doc.dispatch('DOMContentLoaded');
    if (variant === 'load-first') { r.doc.readyState = 'complete'; r.win.dispatch('load'); }
    if (variant === 'duplicate') { r.doc.readyState = 'interactive'; r.doc.dispatch('DOMContentLoaded'); r.doc.dispatch('DOMContentLoaded'); }
    await probe.drained(); assert.equal(probe.state().refused, true);
    if (variant !== 'duplicate') assert.equal(probe.state().domEvent, false);
    assert.equal(probe.state().loadEvent, false); probe.dispose();
  }
});

test('private error and rejection values become only fixed categories', async () => {
  const r = realm(), probe = r.start();
  r.win.dispatch('error', { isTrusted: true, error: { name: 'SyntaxError', message: 'secret-message', stack: 'secret-stack' } });
  r.win.dispatch('unhandledrejection', { isTrusted: true, reason: 'secret-reason' });
  const bad = {}; Object.defineProperty(bad, 'name', { get() { throw Error('secret-getter'); } });
  r.win.dispatch('error', { isTrusted: true, error: bad }); await probe.drained();
  assert.deepEqual(r.messages.slice(1).map(e => e.category), ['SyntaxError', 'unknown', 'unknown']);
  let reads = 0; const deceptive = {};
  Object.defineProperty(deceptive, 'name', { get() { return reads++ ? 'secret-name' : 'TypeError'; } });
  r.win.dispatch('error', { isTrusted: true, error: deceptive }); await probe.drained();
  assert.equal(reads, 1); assert.equal(r.messages.at(-1).category, 'TypeError');
  assert(!JSON.stringify(r.messages).includes('secret')); probe.dispose();
});

test('visibility inspection is allowlisted and bounded, missing or unknown never defaults visible', async () => {
  for (const variant of ['missing', 'hidden', 'zero', 'nan', 'throw', 'ancestor', 'deep', 'style']) {
    const r = realm('interactive');
    if (variant === 'missing') r.doc.getElementById = () => null;
    if (variant === 'hidden') r.node.hidden = true;
    if (variant === 'zero') r.node.getBoundingClientRect = () => ({ width: 0, height: 0 });
    if (variant === 'nan') r.node.getBoundingClientRect = () => ({ width: NaN, height: Infinity });
    if (variant === 'throw') r.doc.getElementById = () => { throw Error('secret-dom'); };
    if (variant === 'ancestor') r.node.parentElement = { hidden: true, parentElement: null };
    if (variant === 'deep') { let node = r.node; for (let i = 0; i < 10; i++) node = node.parentElement = { parentElement: null }; }
    if (variant === 'style') r.ports.getComputedStyle = () => ({});
    const probe = r.start(); await probe.drained();
    const expected = variant === 'missing' ? 'missing' : ['hidden', 'zero', 'ancestor'].includes(variant) ? 'hidden' : 'unknown';
    assert.equal(r.messages.find(e => e.stage === 'startup-control').visibility, expected);
    assert(!JSON.stringify(r.messages).includes('secret')); probe.dispose();
  }
});

test('transport loss, error overflow and late/disposed work remain bounded and honest', async () => {
  for (const delivery of [() => false, () => { throw Error('secret-transport'); }, () => Promise.reject(Error('secret-async'))]) {
    const r = realm('interactive', delivery), probe = r.start(); await probe.drained();
    assert.equal(probe.state().sent, 0); assert(probe.state().failed > 0); assert.equal(probe.state().refused, true); probe.dispose();
  }
  const r = realm(), probe = r.start();
  for (let i = 0; i < 100; i++) r.win.dispatch('error', { isTrusted: true, error: Error('private') });
  await probe.drained(); assert(r.messages.length <= 15); assert(probe.state().dropped > 0);
  for (const e of r.messages) assert(Buffer.byteLength(JSON.stringify(e)) <= 1024);
  probe.dispose();
  const pending = realm('interactive'), active = pending.start(); await active.drained();
  const callback = [...pending.frames.values()][0]; active.dispose(); callback(); await active.drained();
  assert.equal(pending.frames.size, 0); assert.equal(pending.cancelled.length, 1); assert(active.state().late > 0);
  assert([...pending.win.handlers.values(), ...pending.doc.handlers.values()].every(set => set.size === 0));
});

test('throwing event getters and duplicate frame delivery cannot fabricate probe progress', async () => {
  const r = realm(), probe = r.start();
  const event = {}; Object.defineProperty(event, 'isTrusted', { get() { throw Error('private-trust'); } });
  assert.doesNotThrow(() => r.doc.dispatch('DOMContentLoaded', event)); await probe.drained();
  assert.equal(probe.state().refused, true); assert.equal(probe.state().domEvent, false);
  assert(!JSON.stringify(r.messages).includes('private-trust')); probe.dispose();
  const duplicate = realm('interactive'), active = duplicate.start();
  const callback = [...duplicate.frames.values()][0]; duplicate.tick(); callback(); duplicate.tick();
  await active.drained(); assert.equal(active.state().refused, true); assert.equal(active.state().rafAcks, 1);
  assert.equal(duplicate.messages.filter(e => e.stage === 'raf-ack').length, 1); active.dispose();
});

test('probe changes no document/control values, global descriptors or product actions', async () => {
  const r = realm('interactive');
  const beforeWindow = Object.getOwnPropertyDescriptors(r.win), beforeNode = Object.getOwnPropertyDescriptors(r.node);
  r.win.click = r.win.focus = r.win.reload = () => { throw Error('product action'); };
  const before = Object.getOwnPropertyDescriptors(r.win);
  const probe = r.start(); r.tick(); r.tick(); await probe.drained(); probe.dispose();
  assert.deepEqual(Object.getOwnPropertyDescriptors(r.win), before);
  assert.deepEqual(Object.getOwnPropertyDescriptors(r.node), beforeNode);
  assert.equal(beforeWindow.document.value, r.doc);
});

test('cut detaches observations but drains every pre-cut queued ACK in order', async () => {
  const releases = [], r = realm('interactive', () => new Promise(resolve => releases.push(resolve)));
  const probe = r.start(), escaped = [...r.frames.values()][0];
  assert.equal(probe.state().pending, 3);
  await Promise.resolve(); assert.equal(r.messages.length, 1);
  const result = probe.beginCut(CUT);
  assert.equal(probe.beginCut(CUT), result);
  assert.equal(probe.cutState().status, 'draining');
  assert.equal(probe.cutState().pending, 3);
  assert.equal(r.frames.size, 0); assert.equal(r.cancelled.length, 1);
  assert([...r.win.handlers.values(), ...r.doc.handlers.values()].every(set => set.size === 0));
  r.win.dispatch('error'); r.doc.dispatch('DOMContentLoaded');
  let settled = false; result.then(() => { settled = true; });
  for (let i = 0; i < 3; i++) {
    assert.equal(releases.length, i + 1); releases[i](true);
    // Each async emit and the serial .then require their own microtask turn.
    for (let j = 0; j < 6; j++) await Promise.resolve();
    assert.equal(probe.state().pending, 2 - i);
  }
  const footer = await result;
  assert.equal(settled, true); assert.equal(footer.status, 'drained');
  assert.equal(footer.generated, 3); assert.equal(footer.sent, 3);
  assert.equal(footer.pending, 0); assert.equal(footer.failed, 0);
  assert.equal(footer.dropped, 0); assert.equal(footer.late, 0);
  assert.equal(footer.runId, RUN); assert.equal(footer.loadGeneration, 7);
  assert.equal(footer.cutId, CUT); assert(Object.isFrozen(footer));
  assert.deepEqual(stages(r), ['early-script', 'ready-state-observed', 'startup-control']);
  assert.equal(probe.cutState().visiblePaint, 'not-observed');
  // A settled snapshot cannot erase subsequently observed late callbacks.
  escaped(); assert.equal(probe.cutState().late, 1);
  assert.equal(probe.cutState().status, 'partial'); assert.equal(footer.late, 0);
  assert.equal(probe.beginCut(CUT), result); assert.equal(probe.state().generated, 3);
  probe.dispose();
});

test('cut retains failed ACKs and real pending until an unsettled transport finishes', async () => {
  let release;
  const r = realm('interactive', event => event.sequence === 1
    ? new Promise(resolve => { release = resolve; }) : Promise.reject(Error('private-failure')));
  const probe = r.start(); await Promise.resolve();
  const result = probe.beginCut(CUT);
  for (let i = 0; i < 10; i++) await Promise.resolve();
  assert.equal(probe.cutState().pending, 3); assert.equal(probe.cutState().status, 'draining');
  release(false); const footer = await result;
  assert.equal(footer.pending, 0); assert.equal(footer.failed, 3);
  assert.equal(footer.sent, 0); assert.equal(footer.status, 'partial');
  assert(!JSON.stringify(footer).includes('private-failure')); probe.dispose();
});

test('cut identity is immutable and invalid, different or disposed requests cannot reopen', async () => {
  const r = realm(), probe = r.start();
  assert.equal(probe.cutState(), null);
  for (const value of [null, {}, 3, CUT.toUpperCase(), 'private-cut'])
    assert.throws(() => probe.beginCut(value), /startup cut unavailable/);
  const result = probe.beginCut(CUT);
  r.config.runId = 'private-retarget'; r.config.loadGeneration = 90;
  assert.throws(() => probe.beginCut(RUN), /startup cut unavailable/);
  const footer = await result;
  assert.equal(footer.runId, RUN); assert.equal(footer.loadGeneration, 7);
  assert.equal(footer.cutId, CUT); assert.equal(footer.status, 'drained');
  probe.dispose(); assert.throws(() => probe.beginCut(CUT), /startup cut unavailable/);
  const disposed = realm('interactive'), other = disposed.start(); other.dispose();
  assert.throws(() => other.beginCut(CUT), /startup cut unavailable/);
  await other.drained(); assert.equal(other.state().pending, 0);
  assert.equal(other.state().late, 3); assert.equal(other.state().sent, 0);
  const deceptive = realm(); let reads = 0;
  Object.defineProperty(deceptive.config, 'runId', { get() { return reads++ ? 'private-retarget' : RUN; } });
  const captured = deceptive.start(), pinned = await captured.beginCut(CUT);
  assert.equal(reads, 1); assert.equal(pinned.runId, RUN); captured.dispose();
});

test('dispose during a cut preserves retirement semantics and cannot claim a clean drain', async () => {
  let release;
  const r = realm('interactive', () => new Promise(resolve => { release = resolve; }));
  const probe = r.start(); await Promise.resolve();
  const result = probe.beginCut(CUT); probe.dispose();
  assert.equal(probe.state().pending, 3); release(true);
  const footer = await result;
  assert.equal(footer.sent, 1); assert.equal(footer.late, 2);
  assert.equal(footer.pending, 0); assert.equal(footer.status, 'partial');
  assert.throws(() => probe.beginCut(CUT), /startup cut unavailable/);
});

test('cut refuses false clean results after overflow, late listeners or detach failure', async () => {
  const r = realm(), probe = r.start(), error = [...r.win.handlers.get('error')][0];
  for (let i = 0; i < 20; i++) r.win.dispatch('error');
  const result = probe.beginCut(CUT); error({ isTrusted: true });
  const footer = await result;
  assert.equal(footer.generated, 15); assert.equal(footer.sent, 15);
  assert.equal(footer.dropped, 1); assert.equal(footer.late, 1);
  assert.equal(footer.pending, 0); assert.equal(footer.status, 'partial'); probe.dispose();
  const broken = realm('interactive'), active = broken.start();
  broken.ports.cancelAnimationFrame = () => { throw Error('private-cancel'); };
  const failed = await active.beginCut(CUT);
  assert.equal(failed.failed, 1); assert.equal(failed.status, 'partial');
  assert(!JSON.stringify(failed).includes('private')); active.dispose();
});
