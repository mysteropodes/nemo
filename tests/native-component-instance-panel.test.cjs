'use strict';
// #2256 Q5: the Component Instance inspector (#comp-instance-sec) on a NATIVE object project. The panel is the real native-component-instance-panel.js over
// the real component read adapter and instance intent; the engine is a model that answers the real wire contracts (the instance read and the instance.set
// command). A fake event dispatcher runs the section's CAPTURE listeners before the legacy listeners bound on the fields, like the DOM.
const test = require('node:test');
const assert = require('node:assert/strict');
const Panel = require('../src/js/adapters/native-component-instance-panel.js');
const Component = require('../src/js/adapters/native-object-component.js');

const READ = 'query.document.object.component.read', SET = 'command.document.object.component.instance.set';
const settle = async () => { for (let i = 0; i < 20; i += 1) await new Promise((resolve) => setImmediate(resolve)); };
const IDS = ['comp-instance-sec', 'comp-playmode', 'comp-singleframe-row', 'comp-singleframe', 'comp-speed', 'comp-offset', 'comp-frame-strip', 'btn-comp-preview',
  'btn-comp-enter', 'btn-comp-detach', 'btn-lip-apply'];

function node(id, tag) {
  const listeners = { capture: {}, own: {} }, kids = [];
  const el = { id, tagName: tag || 'DIV', style: {}, dataset: {}, attributes: {}, value: '', checked: false, title: '', textContent: '', children: kids,
    addEventListener(type, fn, capture) { ((capture ? listeners.capture : listeners.own)[type] = (capture ? listeners.capture : listeners.own)[type] || []).push(fn); },
    setAttribute(k, v) { this.attributes[k] = String(v); }, removeAttribute(k) { delete this.attributes[k]; },
    appendChild(k) { kids.push(k); k.parentNode = this; return k; }, listeners };
  Object.defineProperty(el, 'innerHTML', { set() { kids.splice(0); }, get: () => '' });
  return el;
}
function fire(section, target, type) {
  const event = { type, target, stopped: false, stopPropagation() { this.stopped = true; }, stopImmediatePropagation() { this.stopped = true; } };
  (section.listeners.capture[type] || []).forEach((fn) => { if (!event.stopped) fn(event); });
  if (!event.stopped) (target.listeners.own[type] || []).forEach((fn) => fn(event));
  return event;
}
const RECORD = () => ({ mode: 'once', singleFrame: 0, speed: 1, placedAt: 0, frameKeys: {} });
// The engine's resolved frame for the model: only what the tests pin (the playback kernel itself is tested in Rust).
const resolved = (rec, frame, total) => (rec.frameKeys[frame] !== undefined ? rec.frameKeys[frame] : rec.mode === 'single' ? Math.min(total - 1, rec.singleFrame)
  : Math.min(total - 1, Math.floor(Math.max(0, frame - rec.placedAt) * rec.speed)));

function harness(options = {}) {
  const sent = [], toasts = [], legacy = [], nodes = new Map(), entered = [];
  IDS.forEach((id) => nodes.set(id, node(id, id === 'comp-playmode' ? 'SELECT' : id.startsWith('btn-') ? 'BUTTON' : 'DIV')));
  const section = nodes.get('comp-instance-sec');
  const rec = Object.assign(RECORD(), options.record), total = options.total || 8, keys = new Set(options.keys || []);
  let revision = 4, refresh = 0, frame = options.frame === undefined ? 3 : options.frame;
  const rows = () => [{ layerUid: 'L-a', name: 'Alpha', kind: 'draw' }, { layerUid: 'L-i', name: 'Inst', kind: 'component', componentId: 'sym_x' },
    { layerUid: 'L-j', name: 'Inst2', kind: 'component', componentId: 'sym_y' }];
  const info = () => ({ apiVersion: 2, instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: revision,
    documentSnapshotId: 'snap-' + revision, layers: rows() });
  const base = (request, ok) => ({ apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: revision, ok });
  async function dispatch(request) {
    sent.push(JSON.parse(JSON.stringify(request)));
    if (options.refuse && request.operation !== READ) return Object.assign(base(request, false), { error: { code: options.refuse, message: options.message || 'refused' } });
    if (request.operation === READ) {
      const selector = request.payload.selector;
      return Object.assign(base(request, true), { result: { atRevision: revision, documentSnapshotId: 'snap-' + revision, kind: 'instance', layerUid: selector.layerUid,
        componentId: 'sym_x', mode: rec.mode, singleFrame: rec.singleFrame, speed: rec.speed, placedAt: rec.placedAt, totalFrames: total,
        resolvedFrame: resolved(rec, selector.frame, total), explicit: rec.frameKeys[selector.frame] !== undefined } });
    }
    const patch = request.payload;
    let changed = false;
    const put = (key, value) => { if (rec[key] !== value) { rec[key] = value; changed = true; } };
    if (patch.mode !== undefined) put('mode', patch.mode);
    if (patch.singleFrame !== undefined) put('singleFrame', patch.singleFrame);
    if (patch.placedAt !== undefined) put('placedAt', patch.placedAt);
    if (patch.speed !== undefined) put('speed', patch.speed);
    if (patch.atFrame !== undefined && keys.has(patch.atFrame) && rec.frameKeys[patch.atFrame] !== patch.singleFrame) { rec.frameKeys[patch.atFrame] = patch.singleFrame; changed = true; }
    const response = Object.assign(base(request, true), { contentRevision: revision + (changed ? 1 : 0), result: { applied: changed, historyEntriesAdded: changed ? 1 : 0 } });
    if (changed) revision += 1;
    return response;
  }
  const session = { info, isActive: () => true, blocksLegacy: () => options.blocks !== false, requestId: (label) => label + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({ dispatch, async reconcile() {} }), runComponentCommand: (action) => action({ dispatch, async reconcile() {} }) };
  const root = { state: { activeLayerIdx: options.active === undefined ? 1 : options.active, currentFrame: frame, activeSymbolId: options.entered || null,
    layers: [{ name: 'stub' }, { name: 'stub', symbolId: 'x' }, { symbolId: 'x' }] }, _layerSel: options.sel, document: { getElementById: (id) => nodes.get(id) || null,
    createElement: (tag) => node('', tag.toUpperCase()), activeElement: null }, NemoNativeObjectProject: { desktop: () => session }, NemoNativeObjectComponent: Component,
    NemoNativeComponentContext: { enter: async (host, componentId, entryFrame, layerUid) => { entered.push([componentId, entryFrame, layerUid]); return true; } },
    showToast: (m) => toasts.push(m), nativeCompositingRefresh() { refresh += 1; } };
  // The legacy panel writers, bound on the fields themselves: a native project must never reach them.
  IDS.forEach((id) => { const n = nodes.get(id); ['change', 'click'].forEach((type) => n.addEventListener(type, () => legacy.push(id + ':' + type))); });
  return { root, sent, toasts, legacy, nodes, section, rec, entered, refreshes: () => refresh, revision: () => revision, sync: () => Panel.sync(root),
    setFrame(value) { frame = value; root.state.currentFrame = value; } };
}
const writes = (h) => h.sent.filter((r) => r.operation !== READ);
const reads = (h) => h.sent.filter((r) => r.operation === READ);

test('the section shows for the active instance layer and is filled from ONE pinned instance read at the playhead frame', async () => {
  const h = harness({ record: { mode: 'pingpong', speed: 2, placedAt: 1, singleFrame: 5 } });
  assert.equal(h.sync(), true, 'the native project owns the panel');
  await settle();
  assert.equal(h.section.style.display, 'block');
  assert.equal(reads(h).length, 1);
  assert.deepEqual(reads(h)[0].payload, { atRevision: 4, documentSnapshotId: 'snap-4', selector: { kind: 'instance', layerUid: 'L-i', frame: 3 } });
  assert.equal('expectedRevision' in reads(h)[0], false, 'a pinned read carries no expectedRevision');
  assert.deepEqual(['comp-playmode', 'comp-speed', 'comp-offset'].map((id) => h.nodes.get(id).value), ['pingpong', '2', '1']);
  assert.equal(h.nodes.get('comp-singleframe').value, '4', 'the Frame field shows the resolved frame, as the baseline did');
  assert.equal(h.nodes.get('comp-singleframe-row').style.display, 'none', 'the frame row waits for Single Frame mode');
  assert.deepEqual(h.legacy, []);
  assert.equal(writes(h).length, 0, 'reading writes nothing');
  h.rec.mode = 'single'; h.setFrame(6); h.sync(); await settle();
  assert.equal(h.nodes.get('comp-singleframe-row').style.display, 'flex');
  assert.equal(h.nodes.get('comp-singleframe').value, '5');
});

test('the frame strip has one cell per component frame, outlines the resolved frame and fills the explicit pick', async () => {
  const h = harness({ total: 5, record: { mode: 'single', singleFrame: 2 } }); h.sync(); await settle();
  const cells = h.nodes.get('comp-frame-strip').children;
  assert.equal(cells.length, 5);
  assert.deepEqual(cells.map((c) => c.dataset.nativeCompFrame), ['0', '1', '2', '3', '4']);
  assert.deepEqual(cells.map((c) => c.style.background), ['var(--bg)', 'var(--bg)', 'var(--accent)', 'var(--bg)', 'var(--bg)']);
  assert.equal(cells[2].style.border, '1px solid var(--accent)');
  assert.equal(cells[1].title, 'Frame 1');
  const loop = harness({ total: 5, record: { mode: 'loop', speed: 1 }, frame: 3 }); loop.sync(); await settle();
  assert.deepEqual(loop.nodes.get('comp-frame-strip').children.map((c) => c.style.background), ['var(--bg)', 'var(--bg)', 'var(--bg)', 'rgba(255,255,255,.25)', 'var(--bg)']);
});

test('the section is hidden for a non-instance layer, a multi-layer selection and an entered component, and the legacy body runs only for a legacy project', async () => {
  const flat = harness({ active: 0 }); assert.equal(flat.sync(), true); await settle();
  assert.equal(flat.section.style.display, 'none');
  assert.equal(flat.sent.length, 0, 'nothing is read for a layer that is not an instance');
  const many = harness({ sel: [1, 2] }); many.sync(); await settle();
  assert.equal(many.section.style.display, 'none');
  assert.equal(many.sent.length, 0);
  const inside = harness({ entered: 'sym_x' }); inside.sync(); await settle();
  assert.equal(inside.section.style.display, 'none');
  const legacy = harness({ blocks: false });
  assert.equal(legacy.sync(), false, 'a legacy project keeps its own panel');
  assert.equal(legacy.sent.length, 0);
});

const CASES = [
  ['comp-playmode', 'loop', { mode: 'loop' }],
  ['comp-singleframe', '5', { singleFrame: 5, atFrame: 3 }],
  ['comp-speed', '2.5', { speed: 2.5 }],
  ['comp-offset', '-4', { placedAt: -4 }],
];
for (const [id, input, patch] of CASES) {
  test(`${id}: one change is exactly ONE instance.set request naming only that member, and the legacy writer never runs`, async () => {
    const h = harness(); h.sync(); await settle();
    h.nodes.get(id).value = input;
    const event = fire(h.section, h.nodes.get(id), 'change');
    assert.equal(event.stopped, true, 'the capture listener keeps the event from the legacy field listener');
    await settle();
    assert.equal(writes(h).length, 1, 'one request');
    assert.equal(writes(h)[0].operation, SET);
    assert.deepEqual(writes(h)[0].payload, Object.assign({ command: 'object.component.instance.set', layerUid: 'L-i' }, patch));
    assert.equal(writes(h)[0].expectedRevision, 4);
    assert.deepEqual(h.legacy, [], 'no legacy listener ran');
    assert.equal(h.root.state.layers[1].symPlayMode, undefined, 'the stub row is never written');
    assert.equal(h.revision(), 5);
    assert.equal(h.refreshes(), 1, 'the canvas repaints from the confirmed revision');
    assert.equal(reads(h).length, 2, 'the panel is read again at the new revision');
  });
}

test('the baseline coercions: single frame max(0, parseInt||0), speed max(0.1, parseFloat||1), offset parseInt||0, all before they are sent', async () => {
  const h = harness(); h.sync(); await settle();
  const cases = [['comp-singleframe', '-7', { singleFrame: 0, atFrame: 3 }], ['comp-singleframe', '4.9', { singleFrame: 4, atFrame: 3 }], ['comp-singleframe', 'abc', { singleFrame: 0, atFrame: 3 }],
    ['comp-speed', '0', { speed: 1 }], ['comp-speed', '0.01', { speed: 0.1 }], ['comp-speed', 'x', { speed: 1 }], ['comp-speed', '3', { speed: 3 }],
    ['comp-offset', '2.9', { placedAt: 2 }], ['comp-offset', 'nope', { placedAt: 0 }], ['comp-offset', '99999999999', { placedAt: 2147483647 }]];
  for (const [id, raw, patch] of cases) {
    h.sent.length = 0; h.rec.singleFrame = 99; h.rec.speed = 99; h.rec.placedAt = 99;
    h.nodes.get(id).value = raw; fire(h.section, h.nodes.get(id), 'change'); await settle();
    assert.deepEqual(writes(h).map((r) => r.payload), [Object.assign({ command: 'object.component.instance.set', layerUid: 'L-i' }, patch)], `${id} ${raw}`);
  }
});

test('a click on a strip cell is ONE request: Single Frame mode on that frame, naming the playhead frame', async () => {
  const h = harness({ total: 6 }); h.sync(); await settle();
  const cell = h.nodes.get('comp-frame-strip').children[4];
  const event = fire(h.section, cell, 'click');
  assert.equal(event.stopped, true);
  await settle();
  assert.deepEqual(writes(h).map((r) => r.payload), [{ command: 'object.component.instance.set', layerUid: 'L-i', mode: 'single', singleFrame: 4, atFrame: 3 }]);
  assert.deepEqual([h.rec.mode, h.rec.singleFrame], ['single', 4]);
  assert.equal(h.nodes.get('comp-singleframe-row').style.display, 'flex', 'repainted from the stored facts');
  assert.deepEqual(h.legacy, []);
});

test('a pick on an outer keyframe is also that key\'s component frame; off a key only the layer-level frame changes', async () => {
  const onKey = harness({ keys: [3], record: { mode: 'single' } }); onKey.sync(); await settle();
  onKey.nodes.get('comp-singleframe').value = '6'; fire(onKey.section, onKey.nodes.get('comp-singleframe'), 'change'); await settle();
  assert.deepEqual(onKey.rec.frameKeys, { 3: 6 });
  const strip = onKey.nodes.get('comp-frame-strip').children;
  assert.equal(strip[6].style.background, 'var(--accent)', 'the explicit pick is filled');
  const off = harness({ keys: [], record: { mode: 'single' } }); off.sync(); await settle();
  off.nodes.get('comp-singleframe').value = '6'; fire(off.section, off.nodes.get('comp-singleframe'), 'change'); await settle();
  assert.deepEqual([off.rec.frameKeys, off.rec.singleFrame], [{}, 6]);
});

test('a scrub previews in the field and sends its single request on release: the numeric fields declare the static scrub contract of ui.js', async () => {
  const h = harness(); h.sync(); await settle();
  ['comp-singleframe', 'comp-speed', 'comp-offset'].forEach((id) => assert.equal(h.nodes.get(id).dataset.nativeOpacityScrub, 'static', id));
  assert.equal(h.nodes.get('comp-playmode').dataset.nativeOpacityScrub, undefined);
  for (const during of ['6', '7', '8', '9']) h.nodes.get('comp-offset').value = during;
  assert.equal(writes(h).length, 0, 'no request while dragging');
  fire(h.section, h.nodes.get('comp-offset'), 'change'); await settle();
  assert.deepEqual(writes(h).map((r) => r.payload.placedAt), [9], 'one request on release');
  const legacyProject = harness({ blocks: false }); Panel.sync(legacyProject.root);
  assert.equal(legacyProject.nodes.get('comp-offset').dataset.nativeOpacityScrub, undefined, 'a legacy project keeps the live scrub');
});

test('a value equal to the stored one sends its request and the owner publishes nothing: no revision, no canvas refresh work beyond the repaint', async () => {
  const h = harness({ record: { mode: 'once', speed: 1, placedAt: 0 } }); h.sync(); await settle();
  h.nodes.get('comp-speed').value = '1'; fire(h.section, h.nodes.get('comp-speed'), 'change'); await settle();
  assert.equal(h.revision(), 4);
  assert.equal(writes(h).length, 1);
});

test('reads are asked once per (revision, layer, frame): a repaint that changed none of them asks nothing, and playback ticks coalesce', async () => {
  const h = harness(); h.sync(); await settle();
  h.sync(); h.sync(); await settle();
  assert.equal(reads(h).length, 1);
  h.sent.length = 0;
  for (const frame of [4, 5, 6, 7]) { h.setFrame(frame); h.sync(); }
  await settle();
  assert.ok(reads(h).length <= 2, 'a burst of frame changes reads at most the first and the last: ' + reads(h).length);
  assert.equal(reads(h)[reads(h).length - 1].payload.selector.frame, 7, 'the last read is the playhead where it stopped');
});

test('"Edit component" enters the real context of the row\'s component, not the stub symbolId', async () => {
  const h = harness(); h.sync(); await settle();
  const event = fire(h.section, h.nodes.get('btn-comp-enter'), 'click'); await settle();
  assert.equal(event.stopped, true);
  assert.deepEqual(h.entered, [['sym_x', null, 'L-i']]);
  assert.deepEqual(h.legacy, [], 'the legacy enterSymbol(stub) handler never ran');
  assert.equal(writes(h).length, 0);
  const second = harness({ active: 2 }); second.sync(); await settle();
  fire(second.section, second.nodes.get('btn-comp-enter'), 'click'); await settle();
  assert.deepEqual(second.entered, [['sym_y', null, 'L-j']], 'each instance enters its own component');
});

test('a refusal toasts the owner reason, changes nothing and shows the stored values again', async () => {
  const h = harness({ refuse: 'not_found', message: 'Layer is not a component instance.' }); h.sync(); await settle();
  h.nodes.get('comp-speed').value = '3'; fire(h.section, h.nodes.get('comp-speed'), 'change'); await settle();
  assert.deepEqual(h.toasts, ['Component playback refused: Layer is not a component instance.']);
  assert.equal(h.nodes.get('comp-speed').value, '1', 'the field is repainted from the stored record');
  assert.equal(h.revision(), 4);
  assert.equal(h.refreshes(), 0);
  assert.deepEqual(h.legacy, []);
});

test('the controls with no native owner say so instead of acting on a stub row, and are marked unavailable', async () => {
  const h = harness(); h.sync(); await settle();
  for (const id of ['btn-comp-preview', 'btn-comp-detach', 'btn-lip-apply']) {
    assert.equal(h.nodes.get(id).dataset.nativeUnavailable, '1');
    assert.equal(h.nodes.get(id).attributes['aria-disabled'], 'true');
    fire(h.section, h.nodes.get(id), 'click');
  }
  await settle();
  assert.deepEqual(h.toasts, ['The component preview window is not available on this project yet.', 'Breaking a component apart is not available on this project yet.',
    'Lip-sync analysis is not available on this project yet.']);
  assert.equal(writes(h).length, 0);
  assert.deepEqual(h.legacy, []);
});

test('a legacy project is not intercepted: its field listeners still run and no native request is sent', async () => {
  const h = harness({ blocks: false }); Panel.sync(h.root);
  h.nodes.get('comp-speed').value = '3';
  const event = fire(h.section, h.nodes.get('comp-speed'), 'change');
  assert.equal(event.stopped, false);
  assert.deepEqual(h.legacy, ['comp-speed:change']);
  assert.equal(h.sent.length, 0);
});

test('the panel and its intent never call the legacy component writers', async () => {
  const h = harness(); const calls = [];
  ['setSymbolPlayMode', 'setSymbolSingleFrame', 'setSymbolSpeed', 'setSymbolPlacedAt', 'enterSymbol', 'resolveSymbolFrameIdx', 'renderCompFrameStrip', 'pushUndoLayers', 'loadFrame']
    .forEach((name) => { h.root[name] = () => calls.push(name); });
  h.root.SM = new Proxy({}, { get: (_, name) => () => calls.push('SM.' + String(name)) });
  h.sync(); await settle();
  h.nodes.get('comp-speed').value = '3'; fire(h.section, h.nodes.get('comp-speed'), 'change'); await settle();
  h.nodes.get('comp-playmode').value = 'single'; fire(h.section, h.nodes.get('comp-playmode'), 'change'); await settle();
  fire(h.section, h.nodes.get('comp-frame-strip').children[2], 'click'); await settle();
  fire(h.section, h.nodes.get('btn-comp-enter'), 'click'); await settle();
  assert.deepEqual(calls, []);
});
