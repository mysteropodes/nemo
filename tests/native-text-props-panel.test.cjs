'use strict';
// #2372 insp-text-props: the Text properties section (tp-*) of ONE selected native text block on a native object project. The values come from the
// native text inspect read, and each change is ONE object.text.edit (one revision, one history entry) through the REAL tp-* listeners of timeline.js
// (run in a vm over a fake document): typed values on change, a scrub once at release, a refusal is a toast, never a throw, and the legacy vector-text
// path is unchanged off a native project.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Panel = require('../src/js/adapters/native-text-props-panel.js');
const Reader = require('../src/js/adapters/native-text-read.js');
const Writer = require('../src/js/adapters/native-object-text-layout.js');
const Contract = require('../src/js/adapters/native-object-item-contract.js');
const Paper = require('./helpers/native-import-paper.cjs');
const overlayStub = require('./helpers/native-selection-overlay-stub.cjs');
const { fixture, block, target } = require('./native-text-tool-support.cjs');

const settle = async () => { for (let i = 0; i < 24; i++) await new Promise((done) => setTimeout(done, 0)); };
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const FRAME = 3, TID = 'native-text:1:0';
const STORED = () => ({ ...block('Hello native'), color: '#11223344', italic: false, wordSpacing: 1, lineHeightMult: 1.5, textCase: 'upper', align: 'center' });

function harness(over, fixtureOptions) {
  const f = fixture(fixtureOptions), toasts = [];
  f.seed(target(TID, FRAME), STORED());
  const info = () => ({ ...f.session.info(), canvasW: 400, canvasH: 400, layers: [{ layerUid: 'L0' }] });
  const kind = { role: 'text' };
  const item = () => ({ stableTarget: target(TID, FRAME), closed: true, count: 4, role: kind.role, filled: true, bounds: [100, 100, 300, 200], matrix: [1, 0, 0, 1, 0, 0],
    segments: [seg(100, 100), seg(300, 100), seg(300, 200), seg(100, 200)], style: { fill: '#112233', width: 0, opacity: 1 } });
  const session = { ...f.session, info, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...info(), frame: FRAME } }),
    async readPresentedStrokes(_selector, _pin, wanted) { const { segments, ...rest } = item(); return { items: [wanted(rest) ? { ...rest, segments } : rest] }; } };
  const paper = Paper.load();
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: FRAME, activeLayerIdx: 0, layers: [{ layerUid: 'L0' }], selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    document: { getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 400, height: 400 }) }) },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }) } },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true }, NemoNativeObjectItemContract: Contract,
    NemoNativeTextRead: Reader, NemoNativeObjectTextLayout: Writer, NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: overlayStub(Intent), NemoNativeTextPropsPanel: Panel };
  Object.assign(root, over);
  const edits = () => f.owner.log.received.filter((q) => q.payload && q.payload.command === 'object.text.edit');
  return { f, root, toasts, kind, edits, stored: () => f.owner.items.get(TID),
    select: async () => { root.selectedPaths = [await Intent.pick(root, 200, 150)]; root.NemoNativeSelectOverlay.show(root); return root.selectedPaths[0]; } };
}

// The real tp-* code of timeline.js (the native glue, textPropsRoot .. the listener IIFE), run over a fake document that keeps each element's listeners.
function panel(h, calls, extra) {
  const src = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'timeline.js'), 'utf8');
  const from = src.indexOf('function textPropsRoot(){'), to = src.indexOf('// Effects stack panel (2026-07 rewrite)');
  assert.ok(from > 0 && to > from);
  const els = {}, groups = { '.tp-align-btn': ['left', 'center', 'right'], '.tp-case-btn': ['none', 'upper', 'lower', 'capitalize'], '.tp-width-btn': ['auto', 'fixed'] };
  const cls = () => { const set = new Set(); return { add: (c) => set.add(c), toggle: (c, on) => { const want = on === undefined ? !set.has(c) : !!on; if (want) set.add(c); else set.delete(c); return want; }, contains: (c) => set.has(c) }; };
  const base = (id) => ({ id, value: '', checked: true, style: {}, dataset: {}, listeners: {}, classList: cls(), disabled: false, addEventListener(type, fn) { (this.listeners[type] = this.listeners[type] || []).push(fn); } });
  const el = (id) => els[id] || (els[id] = base(id));
  const btns = {};
  Object.keys(groups).forEach((sel) => { btns[sel] = groups[sel].map((v) => Object.assign(base(sel + v), { dataset: { val: v } })); });
  btns['.tp-style-btn'] = ['bold', 'italic', 'underline', 'strike'].map((v) => Object.assign(base('style' + v), { dataset: { flag: v } }));
  const fire = (id, type, value) => { const e = typeof id === 'string' ? el(id) : id; if (value !== undefined) e.value = String(value); (e.listeners[type] || []).forEach((fn) => fn.call(e)); };
  const click = (sel, v) => { const b = btns[sel].find((x) => (x.dataset.val || x.dataset.flag) === v); (b.listeners.click || []).forEach((fn) => fn.call(b)); return b; };
  const ctx = vm.createContext(Object.assign({ document: { getElementById: el, querySelectorAll: (s) => btns[s] || [], querySelector: (s) => { const m = /^\.tp-style-btn\[data-flag="(\w+)"\]$/.exec(s); return m ? btns['.tp-style-btn'].find((b) => b.dataset.flag === m[1]) : null; }, activeElement: null }, window: h.root, state: h.root.state,
    SM: { t: (k) => k }, showToast: h.root.showToast, console, Point: h.root.Point, pushUndo() { calls.push('legacy:pushUndo'); throw new Error('a native project refuses the history checkpoint'); },
    saveActiveLayerFrame() { calls.push('legacy:save'); }, updateUI() {} }, extra || {}));
  Object.defineProperty(ctx, 'selectedPaths', { get: () => h.root.selectedPaths, configurable: true });
  vm.runInContext(src.slice(from, to), ctx);
  const update = () => vm.runInContext('updateTextPropsPanel()', ctx);
  const on = (sel) => btns[sel].filter((b) => b.classList.contains('ac')).map((b) => b.dataset.val || b.dataset.flag);
  return { el, fire, click, update, on, visible: (id) => el(id).style.display !== 'none', ctx };
}

test('the section shows the selected native text block\'s stored settings (the inspect read), and nothing is written by showing them', async () => {
  const h = harness(), calls = [], ui = panel(h, calls);
  await h.select(); ui.update(); await settle();
  assert.equal(ui.visible('text-props-sec'), true);
  assert.equal(ui.el('tp-content').value, 'Hello native');
  assert.equal(ui.el('tp-font').value, 'Roboto');
  assert.equal(ui.el('tp-size').value, 48);
  assert.equal(ui.el('tp-color').value, '#112233');
  assert.equal(ui.el('tp-color').dataset.hex8, '#11223344', 'the alpha travels on dataset.hex8 (CLAUDE.md section 2)');
  assert.deepEqual(ui.on('.tp-align-btn'), ['center']);
  assert.deepEqual(ui.on('.tp-style-btn'), ['bold'], 'only the stored flags are active');
  assert.deepEqual(ui.on('.tp-case-btn'), ['upper']);
  assert.deepEqual([ui.el('tp-letter-spacing').value, ui.el('tp-line-height').value, ui.el('tp-word-spacing').value], [2, 1.5, 1]);
  assert.deepEqual(ui.on('.tp-width-btn'), ['auto']);
  assert.equal(ui.el('tp-fixed-width').style.display, 'none');
  assert.deepEqual(h.edits(), []);
  assert.deepEqual(calls, []);
});

test('the numeric fields scrub on the static path (one change at release) while a native text block is selected, and not otherwise', async () => {
  const h = harness(), calls = [], ui = panel(h, calls);
  ui.update();
  assert.equal(ui.el('tp-size').dataset.nativeOpacityScrub, undefined);
  await h.select(); ui.update();
  ['tp-size', 'tp-letter-spacing', 'tp-line-height', 'tp-word-spacing', 'tp-fixed-width'].forEach((id) => assert.equal(ui.el(id).dataset.nativeOpacityScrub, 'static', id));
  h.root.selectedPaths = []; ui.update();
  assert.equal(ui.el('tp-size').dataset.nativeOpacityScrub, undefined);
});

test('each change is exactly one object.text.edit carrying only that member, never through the history checkpoint or a vector rebuild', async () => {
  const h = harness(), calls = [], ui = panel(h, calls);
  await h.select(); ui.update(); await settle();
  const step = async (act, expected) => {
    const before = h.edits().length, revision = h.f.session.info().contentRevision;
    act(); await settle();
    assert.equal(h.edits().length, before + 1, 'one change is one native edit');
    const q = h.edits()[before];
    assert.deepEqual(q.payload.patch, expected);
    assert.deepEqual(q.payload.stableTarget, target(TID, FRAME));
    assert.equal(q.expectedRevision, revision, 'pinned to the presented revision');
    Contract.payload(Contract.ITEM, q.payload);
    assert.equal(h.f.session.info().contentRevision, revision + 1, 'one revision');
    ui.update(); await settle();
  };
  await step(() => ui.fire('tp-size', 'change', 60), { size: 60 });
  assert.equal(ui.el('tp-size').value, 60, 'the panel shows what the engine stored');
  await step(() => ui.fire('tp-font', 'change', 'DejaVuSerif'), { font: { kind: 'bundled', fontKey: 'DejaVuSerif-Regular' } });
  await step(() => ui.fire('tp-color', 'change', '#aabbcc'), { color: '#aabbcc44' });
  await step(() => ui.click('.tp-align-btn', 'right'), { align: 'right' });
  await step(() => ui.click('.tp-style-btn', 'italic'), { italic: true });
  await step(() => ui.click('.tp-style-btn', 'bold'), { bold: false });
  await step(() => ui.click('.tp-case-btn', 'capitalize'), { textCase: 'capitalize' });
  await step(() => ui.fire('tp-letter-spacing', 'change', 3.5), { letterSpacing: 3.5 });
  await step(() => ui.fire('tp-line-height', 'change', 2), { lineHeightMult: 2 });
  await step(() => ui.fire('tp-word-spacing', 'change', -1), { wordSpacing: -1 });
  await step(() => ui.click('.tp-width-btn', 'fixed'), { fixedWidth: 300 });
  await step(() => ui.fire('tp-fixed-width', 'change', 220), { fixedWidth: 220 });
  await step(() => ui.click('.tp-width-btn', 'auto'), { fixedWidth: null });
  const content = ui.el('tp-content');
  content.value = 'Changed text';
  await step(() => ui.fire('tp-content', 'blur'), { text: 'Changed text' });
  assert.equal(h.stored().text, 'Changed text');
  assert.equal(h.stored().font.fontKey, 'DejaVuSerif-Regular');
  assert.deepEqual(calls, []);
  assert.equal(h.toasts.length, 0);
});

test('typing alone writes nothing; a value equal to the stored one is no edit; a clamped value is one edit', async () => {
  const h = harness(), calls = [], ui = panel(h, calls);
  await h.select(); ui.update(); await settle();
  ui.fire('tp-size', 'input', 12); ui.fire('tp-size', 'input', 1200);
  await settle();
  assert.deepEqual(h.edits(), [], 'no input listener: only change commits');
  ui.fire('tp-size', 'change', 48);
  ui.fire('tp-letter-spacing', 'change', 2);
  await settle();
  assert.deepEqual(h.edits(), [], 'unchanged values send nothing');
  ui.fire('tp-size', 'change', 5000);
  await settle();
  assert.deepEqual(h.edits().map((q) => q.payload.patch), [{ size: 800 }]);
});

test('two quick changes are serialised: both land, in order, one entry each', async () => {
  const h = harness(), calls = [], ui = panel(h, calls);
  await h.select(); ui.update(); await settle();
  ui.fire('tp-size', 'change', 30);
  ui.click('.tp-style-btn', 'underline');
  await settle();
  assert.deepEqual(h.edits().map((q) => q.payload.patch), [{ size: 30 }, { underline: true }]);
  assert.equal(h.stored().size, 30);
  assert.equal(h.stored().underline, true);
});

test('a refusal is a toast and writes nothing: a font with no native form, a blank text, an invalid number, a stale selection, a refused command', async () => {
  const h = harness(), calls = [], ui = panel(h, calls);
  await h.select(); ui.update(); await settle();
  const toasts = () => h.toasts.length;
  ui.fire('tp-font', 'change', 'Google:Poppins'); await settle();
  assert.equal(toasts(), 1);
  ui.el('tp-content').value = '   '; ui.fire('tp-content', 'blur'); await settle();
  assert.equal(toasts(), 2);
  ui.fire('tp-size', 'change', 'abc'); await settle();
  assert.equal(toasts(), 3);
  assert.deepEqual(h.edits(), []);
  // The document moves on under the selection: the edit is dropped, not applied to a different revision.
  h.f.state.revision += 1;
  ui.fire('tp-size', 'change', 64); await settle();
  assert.equal(toasts(), 4);
  assert.deepEqual(h.edits(), []);
  assert.deepEqual(calls, []);
});

test('a command the owner refuses is a toast, writes nothing and leaves the stored values on the panel', async () => {
  const h = harness(undefined, { refuse: (q) => (q.payload && q.payload.command === 'object.text.edit' ? { code: 'invalid_request', message: 'refused' } : null) }), ui = panel(h, []);
  await h.select(); ui.update(); await settle();
  ui.fire('tp-size', 'change', 64); await settle();
  assert.equal(h.toasts.length, 1);
  assert.equal(h.stored().size, 48);
  ui.update(); await settle();
  assert.equal(ui.el('tp-size').value, 48, 'the panel shows the stored size again');
});

test('a multi-selection, a non-text item and an empty selection hide the section and edit nothing', async () => {
  const h = harness(), calls = [], ui = panel(h, calls);
  const p = await h.select(); ui.update(); await settle();
  assert.equal(ui.visible('text-props-sec'), true);
  h.root.selectedPaths = [p, p]; ui.update();
  assert.equal(ui.visible('text-props-sec'), false, 'two selected items');
  h.root.selectedPaths = []; ui.update();
  assert.equal(ui.visible('text-props-sec'), false);
  h.kind.role = 'plain';
  await h.select(); ui.update();
  assert.equal(ui.visible('text-props-sec'), false, 'a plain stroke is not text');
  ui.fire('tp-size', 'change', 70); await settle();
  assert.deepEqual(h.edits(), []);
  h.kind.role = 'text';
  await h.select(); h.root.state.tool = 'direct'; ui.update();
  assert.equal(ui.visible('text-props-sec'), false, 'only the Select tool shows it');
});

test('off a native project the legacy vector-text path is unchanged (same listeners, the vector rebuild, no native command)', async () => {
  const h = harness(), calls = [];
  const legacy = { data: { isVectorText: true, groupId: 'g1', isTextRoot: true, text: 'Legacy', vectorFont: 'DejaVuSans-Bold', size: 40, color: '#336699', align: 'right', letterSpacing: 2, lineHeightMult: 1.4, wordSpacing: 3, anchorTopLeft: { x: 5, y: 6 } }, parent: {} };
  h.root.SMVectorText = { vectorTextGroupMembers: () => [legacy], buildVectorTextGroup: () => Promise.resolve({ paths: [legacy] }) };
  const ui = panel(h, calls);
  h.root.selectedPaths = [legacy];
  ui.update();
  assert.equal(ui.visible('text-props-sec'), true);
  assert.equal(ui.el('tp-content').value, 'Legacy');
  assert.equal(ui.el('tp-font').value, 'DejaVuSans');
  assert.equal(ui.el('tp-size').value, 40);
  assert.equal(ui.el('tp-fixed-width').style.display, 'none');
  assert.equal(ui.el('tp-size').dataset.nativeOpacityScrub, undefined, 'the legacy scrub path is untouched');
  ui.el('tp-size').value = '44';
  // The legacy handler snapshots history first (which a native project refuses): reaching it proves the legacy path ran.
  assert.throws(() => ui.fire('tp-size', 'change'), /history checkpoint/);
  assert.deepEqual(calls, ['legacy:pushUndo']);
  assert.deepEqual(h.edits(), []);
});
