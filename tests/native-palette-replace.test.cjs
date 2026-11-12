'use strict';
// #2372 PAL-REPLACE: the Palette's "Replace in layer" on a native object project. Baseline 1065e9d8 palette-panel.js: arm Replace, click the SOURCE swatch, click the
// TARGET swatch; the active layer at the current frame (scope "frame") has every matching fill / stroke replaced and a toast says how many ("no match" otherwise).
// Natively that is ONE restyle batch (NemoNativeSelectionStyle.recolourLayer, the Selected Colors recolour over the whole layer); "layer" and "selection" span frames,
// have no engine command yet and are disabled and refused. No Paper writer, no pushUndo, no palette edit.
const test = require('node:test'), assert = require('node:assert/strict');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Style = require('../src/js/adapters/native-selection-style.js');
const Steps = require('../src/js/adapters/native-selection-steps.js');
const Paper = require('./helpers/native-import-paper.cjs');
const overlayStub = require('./helpers/native-selection-overlay-stub.cjs');
const { fixture } = require('./helpers/native-palette-fixture.cjs');

const NO_SCOPE = 'Replacing across frames is not available yet';
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (id, x0, y0, style, extra) => ({ stableTarget: { contextId: 'scene-root', layerUid: 'drawing', frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x0 + 20, y0 + 20], matrix: [1, 0, 0, 1, 0, 0],
  segments: [seg(x0, y0), seg(x0 + 20, y0), seg(x0 + 20, y0 + 20), seg(x0, y0 + 20)], style: { width: 4, opacity: 1, ...style }, ...extra });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 8; i++) await tick(); };

// The palette controller over a stubbed Style: what the three clicks ask for.
function unit(result) {
  const asked = [];
  const h = fixture({ metadata: { palettes: [{ id: 'p', name: 'P', colors: ['#ff0000', '#0000ff', '#00ff00'] }], activePaletteIdx: 0 },
    extend(root) { root.NemoNativeSelectionStyle = { recolourLayer: async (r, from, to) => { asked.push([typeof r.document, from, to]); return typeof result === 'function' ? result() : result; } }; } });
  const scope = h.elements['palette-replace-scope'];
  scope.value = 'frame'; scope.options = ['frame', 'layer', 'selection'].map((value) => ({ value, disabled: false, title: '' }));
  return { h, asked, scope };
}

test('Replace is enabled on a shown palette and the two cross-frame scopes are disabled with a title', async () => {
  const { h, scope } = unit({ status: 'nomatch', count: 0 }); await h.ready();
  assert.equal(h.elements['btn-palette-replace'].disabled, false);
  assert.equal(scope.disabled, false);
  assert.deepEqual(scope.options.map((o) => o.disabled), [false, true, true]);
  assert.deepEqual(scope.options.filter((o) => o.disabled).map((o) => o.title), [NO_SCOPE, NO_SCOPE]);
});

test('armed 3-click flow (button, source, target) asks for ONE recolour of the layer and toasts the count; no palette edit', async () => {
  const { h, asked } = unit({ status: 'applied', count: 3 }); await h.ready();
  h.elements['btn-palette-replace'].fire('click');
  assert.equal(h.notices.at(-1), 'hsPaletteReplaceHint', 'arming says what to click');
  h.swatches()[0].fire('click');
  assert.match(h.swatches()[0].className, /armed/, 'the source swatch shows as armed');
  assert.equal(asked.length, 0, 'the source click only picks');
  h.swatches()[1].fire('click'); await h.settled(); await settle();
  assert.deepEqual(asked, [['object', '#ff0000', '#0000ff']]);
  assert.equal(h.notices.at(-1), '3 colour(s) replaced');
  assert.doesNotMatch(h.swatches()[0].className, /armed/, 'the flow disarms after the target');
  assert.equal(h.edits().length, 0, 'the palette itself is untouched');
});

test('no match toasts "no match" and still disarms', async () => {
  const { h, asked } = unit({ status: 'nomatch', count: 0 }); await h.ready();
  h.elements['btn-palette-replace'].fire('click'); h.swatches()[2].fire('click'); h.swatches()[1].fire('click'); await settle();
  assert.equal(asked.length, 1);
  assert.equal(h.notices.at(-1), 'No match in the chosen scope');
  assert.doesNotMatch(h.swatches()[2].className, /armed/);
});

test('the swatch menu arms with that swatch as the source; the button and Escape cancel', async () => {
  const { h, asked } = unit({ status: 'applied', count: 1 }); await h.ready();
  h.swatches()[1].fire('contextmenu');
  h.menus.at(-1).find((x) => (x.label || '').startsWith('Remplacer')).action();
  assert.match(h.swatches()[1].className, /armed/);
  h.root.document.fire('keydown', { key: 'Escape' });
  assert.doesNotMatch(h.swatches()[1].className, /armed/);
  h.elements['btn-palette-replace'].fire('click'); h.elements['btn-palette-replace'].fire('click');
  h.swatches()[0].fire('contextmenu'); h.menus.at(-1).find((x) => (x.label || '').startsWith('Remplacer')).action();
  h.swatches()[2].fire('click'); await settle();
  assert.deepEqual(asked, [['object', '#ff0000', '#00ff00']]);
});

test('scope layer / selection refuses with a toast and sends no command, at arming and at the target click', async () => {
  for (const value of ['layer', 'selection']) {
    const { h, asked, scope } = unit({ status: 'applied', count: 1 }); await h.ready();
    scope.value = value;
    h.elements['btn-palette-replace'].fire('click');
    assert.equal(h.notices.at(-1), NO_SCOPE);
    // chosen after arming
    scope.value = 'frame'; h.elements['btn-palette-replace'].fire('click'); h.swatches()[0].fire('click'); scope.value = value;
    h.swatches()[1].fire('click'); await settle();
    assert.equal(h.notices.at(-1), NO_SCOPE);
    assert.equal(asked.length, 0); assert.equal(h.edits().length, 0);
  }
});

// ---- driven: the real recolour over real presented records --------------------------------------------------------------------------------------------------
function driven(items) {
  const paper = Paper.load(), batches = [], legacy = [], store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const h = fixture({ metadata: { palettes: [{ id: 'p', name: 'P', colors: ['#f00', '#00f', '#abcdef'] }], activePaletteIdx: 0 },
    extend(root) {
      Object.assign(root, { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} },
        NemoSelectCanvasIntent: Intent, NemoNativeSelectionSteps: Steps, NemoNativeSelectionStyle: Style, NemoNativeSelectOverlay: overlayStub(Intent) });
      root.pushUndo = () => { legacy.push('pushUndo'); throw new Error('legacy history'); };
      root.NemoNativeObjectLabs = { desktop: () => ({ batchStrokes: async (layerUid, frame, steps, options) => {
        batches.push({ layerUid, frame, options, steps: JSON.parse(JSON.stringify(steps)) });
        steps.forEach((s) => { const it = store.find((i) => i.stableTarget.strokeId === s.restyle.stableTarget.strokeId); it.style = { ...it.style, ...s.restyle.style }; });
        return { status: 'applied' };
      } }) };
    } });
  Object.assign(h.root.state, { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0 });
  const session = h.root.NemoNativeObjectProject.desktop(), shown = session.readPresentedObjectSession;
  session.blocksLegacy = () => true;
  session.readPresentedObjectSession = (refused) => { const r = shown(refused); return r && { ...r, presentation: { ...r.presentation, frame: 0 } }; };
  session.readPresentedStrokes = async (_s, _p, wanted) => ({ items: store.map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) });
  return { h, batches, legacy, store, style: (id) => store.find((i) => i.stableTarget.strokeId === id).style };
}
const pickTargets = async (h) => { await h.ready(); h.elements['btn-palette-replace'].fire('click'); h.swatches()[0].fire('click'); h.swatches()[1].fire('click'); await h.settled(); await settle(); };

test('driven: three #f00 shapes become #00f in ONE restyle batch (one undo step); matching strokes too, other colours, masks, images and linked fills untouched', async () => {
  const d = driven([
    quad('A', 10, 10, { fill: '#ff0000' }), quad('B', 40, 10, { fill: '#ff0000', stroke: '#112233' }), quad('C', 70, 10, { fill: '#778899', stroke: '#FF0000' }),
    quad('D', 100, 10, { fill: '#ff0000' }, { role: 'mask' }), quad('E', 130, 10, { opacity: 1 }, { role: 'raster' }),
    quad('F', 160, 10, { fill: '#ff0000' }, { role: 'fill', owner: 'R' }), quad('G', 190, 10, { fill: '#00ff00' })]);
  await pickTargets(d.h);
  assert.equal(d.batches.length, 1, 'one batch, one revision, one history entry');
  assert.deepEqual(d.legacy, []);
  assert.deepEqual(d.batches[0].options, { skipUnavailable: true });
  assert.equal(d.batches[0].layerUid, 'drawing'); assert.equal(d.batches[0].frame, 0);
  assert.deepEqual(d.batches[0].steps.map((s) => [s.restyle.stableTarget.strokeId, s.restyle.style]), [
    ['A', { fill: '#0000ff' }], ['B', { fill: '#0000ff' }], ['C', { stroke: '#0000ff' }]]);
  assert.equal(d.style('B').stroke, '#112233'); assert.equal(d.style('D').fill, '#ff0000'); assert.equal(d.style('F').fill, '#ff0000');
  assert.equal(d.h.notices.at(-1), '3 colour(s) replaced');
  assert.equal(d.h.edits().length, 0);
});

test('driven: a colour nothing wears sends no batch and says no match; a locked layer is refused with a notice', async () => {
  const none = driven([quad('A', 10, 10, { fill: '#112233' })]);
  await pickTargets(none.h);
  assert.equal(none.batches.length, 0); assert.equal(none.h.notices.at(-1), 'No match in the chosen scope');
  const locked = driven([quad('A', 10, 10, { fill: '#ff0000' })]);
  await locked.h.ready(); locked.h.root.NemoNativeComponentContext = { contextId: () => 'scene-root', layers: () => [{ layerUid: 'drawing', visible: true, locked: true }] };
  await pickTargets(locked.h);
  assert.equal(locked.batches.length, 0); assert.match(locked.h.notices.join('|'), /locked or hidden/);
});
