'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, 'desktop/startup-observer-bootstrap.js'), 'utf8');
const { createStartupProbe } = require('./desktop/startup-observer-bootstrap.js');
const RUN = '12345678-1234-1234-1234-123456789abc';
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
