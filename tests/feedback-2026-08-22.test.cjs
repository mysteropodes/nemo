const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const read = (file) => fs.readFileSync(path.join(root, file), 'utf8');

test('About Nemo targets the About settings tab', () => {
  const source = read('src/js/timeline.js');
  const menuStart = source.indexOf("label:tt('menuAboutNemo')");
  assert.notEqual(menuStart, -1);
  const action = source.slice(menuStart, menuStart + 300);
  assert.match(action, /data-tab="about"/);
  assert.doesNotMatch(action, /data-tab="updates"/);
});

test('effect picker exposes a focused name search with keyboard apply', () => {
  const source = read('src/js/effects-panel.js');
  assert.match(source, /search\.type\s*=\s*'search'/);
  assert.match(source, /search\.focus\(\)/);
  assert.match(source, /if\s*\(e\.key\s*===\s*'Enter'/);
  assert.match(source, /fxSearchPlaceholder/);
  assert.match(read('src/js/i18n.js'), /fxSearchEmpty/);
});

test('pressure ribbons never receive a cosmetic outline', () => {
  const source = read('src/js/app.js');
  const start = source.indexOf('function applyBrushKeyline(');
  const end = source.indexOf('function serP(', start);
  assert.notEqual(start, -1);
  assert.notEqual(end, -1);
  const sandbox = {};
  vm.runInNewContext(`${source.slice(start, end)}\nthis.apply = applyBrushKeyline;`, sandbox);
  const ribbon = { data: { isVectorBrush: true }, strokeColor: '#111' };
  sandbox.apply(ribbon);
  assert.equal(ribbon.strokeColor, null);
});

test('layer gap reorder preserves objects, active layer, and visual bottom drop', () => {
  const source = read('src/js/app.js');
  const start = source.indexOf('function reorderLayersAtGap(');
  const end = source.indexOf('function drawStage(', start);
  assert.notEqual(start, -1);
  assert.notEqual(end, -1);
  const layers = ['A', 'B', 'C'].map((name) => ({ name }));
  const userLayers = layers.map((layer) => ({ layer, insertBelow() {} }));
  const state = { layers, activeLayerIdx: 2, currentFrame: 0 };
  const sandbox = {
    state, userLayers, arcLayer: {},
    saveAllLayerFrames() {}, pushUndoLayers() {}, activateUL() {}, loadFrame() {},
    renderOS() {}, renderArcs() {}, updateUI() {}, showToast() {},
    SM: { t(value) { return value; } }, Math, Array,
  };
  vm.runInNewContext(`${source.slice(start, end)}\nthis.reorder = reorderLayersAtGap;`, sandbox);
  // State order is the inverse of the visible layer list. Dropping visible
  // top layer C below the visible bottom row therefore targets array gap 0.
  sandbox.reorder([2], 0);
  assert.deepEqual(state.layers.map((layer) => layer.name), ['C', 'A', 'B']);
  assert.equal(state.layers[state.activeLayerIdx].name, 'C');
  assert.equal(userLayers[0].layer.name, 'C');

  const timeline = read('src/js/timeline.js');
  assert.match(timeline, /dropGap=parseInt\(row\.dataset\.layer\)\+\(after\?0:1\)/);
  assert.match(timeline, /addEventListener\('mousedown',beginLayerReorder\)/);
  assert.match(timeline, /addEventListener\('pointerdown',beginLayerReorder\)/);
  assert.match(timeline, /addEventListener\('mouseup',finishLayerReorder\)/);
  assert.match(timeline, /addEventListener\('pointerup',finishLayerReorder\)/);
  assert.match(read('src/css/style.css'), /\.layer-drop-indicator\s*\{/);
});

test('layer gap reorder moves the selection with the layer, not the array slot', () => {
  // 2026-09-01, Cyril: "si on déplace l'order d'un calque select celui ci
  // ne reste pas select, c'est le premier calque qui est select à chaque
  // fois" — _layerSel/_layerSelAnchor are raw ARRAY INDICES, and the
  // splice inside reorderLayersAtGap moves layers between indices, so an
  // untouched index just landed on whatever layer happened to be there
  // afterward instead of following the one the user actually selected.
  const source = read('src/js/app.js');
  const start = source.indexOf('function reorderLayersAtGap(');
  const end = source.indexOf('function drawStage(', start);
  assert.notEqual(start, -1);
  assert.notEqual(end, -1);
  const layers = ['A', 'B', 'C', 'D'].map((name) => ({ name }));
  const userLayers = layers.map((layer) => ({ layer, insertBelow() {} }));
  const state = { layers, activeLayerIdx: 1, currentFrame: 0 };
  let syncCalls = 0;
  const sandbox = {
    state, userLayers, arcLayer: {},
    // B (index 1) and D (index 3) selected, anchored on B — same shape a
    // real Shift/Ctrl-click multi-select leaves behind.
    _layerSel: [1, 3], _layerSelAnchor: 1,
    saveAllLayerFrames() {}, pushUndoLayers() {}, activateUL() {}, loadFrame() {},
    renderOS() {}, renderArcs() {}, updateUI() {}, showToast() {},
    SM: { t(value) { return value; } }, Math, Array,
    window: { SMMotion: { syncBarSelToLayerSel() { syncCalls++; } } },
  };
  vm.runInNewContext(`${source.slice(start, end)}\nthis.reorder = reorderLayersAtGap;`, sandbox);
  // Move A (index 0, unselected) to the very end — B, C, D each shift down
  // one slot, so a naive index-only selection would now point at C and
  // nothing instead of following B and D.
  sandbox.reorder([0], 4);
  assert.deepEqual(state.layers.map((l) => l.name), ['B', 'C', 'D', 'A']);
  const selectedNames = sandbox._layerSel.map((i) => state.layers[i].name).sort();
  assert.deepEqual(selectedNames, ['B', 'D']);
  assert.equal(state.layers[sandbox._layerSelAnchor].name, 'B');
  assert.equal(syncCalls, 1);
});

test('multi-layer selection has one union overlay and three transforms in both modes', () => {
  const source = read('src/js/motion.js');
  assert.match(source, /function multiLayerBox\(\)/);
  assert.match(source, /if \(ml\) return multiLayerOverlay\(ml\)/);
  for (const mode of ['multiLayerMove', 'multiLayerScale', 'multiLayerRotate']) {
    assert.ok((source.match(new RegExp(mode, 'g')) || []).length >= 2, `${mode} must be started and dragged`);
  }
  const select = read('src/js/select-bridge.js');
  assert.match(select, /function multiLayerSelectionBox\(\)/);
  assert.match(select, /getMultiLayerBox:\s*multiLayerSelectionBox/);
  assert.match(select, /mode\s*=\s*'layer-multi-'\s*\+\s*mlHit\.type/);
  for (const mode of ['layer-multi-move', 'layer-multi-scale', 'layer-multi-rotate']) {
    assert.ok((select.match(new RegExp(mode, 'g')) || []).length >= 1, `${mode} must be handled during drag`);
  }
  assert.match(read('src/js/engine-bridge.js'), /SMSelectBridge\.getMultiLayerBox/);
});

test('selected in/out handles retain a bright edge and blue halo', () => {
  const css = read('src/css/style.css');
  const start = css.indexOf('.layer-inout-handle.sel,.layer-inout-handle.sel.hot');
  assert.notEqual(start, -1);
  const rule = css.slice(start, start + 420);
  assert.match(rule, /rgba\(255,255,255/);
  assert.match(rule, /rgba\(74,158,255/);
});

test('Motion P1 filters share one row decision across list and grid', () => {
  const source = read('src/js/motion.js');
  assert.match(source, /function motionPropMatchesView\(/);
  assert.match(source, /function layerMatchesMotionView\(/);
  assert.ok((source.match(/if \(!layerMatchesMotionView\(ld\)\) return;/g) || []).length >= 2,
    'both Motion render passes must filter the same layer set');
  assert.match(source, /!motionPropMatchesView\(holder, prop\)/);
  assert.match(source, /nemo-motion-filter/);
  assert.match(source, /nemo-motion-columns/);
});

test('Motion P1 keeps fast compact values in the timeline and full inspector controls', () => {
  const css = read('src/css/style.css');
  assert.match(css, /#layer-list \.pi\.motion-val\s*\{width:28px/);
  assert.match(css, /#motion-header-tools/);
  assert.match(css, /data-motion-columns/);
  assert.doesNotMatch(css, /#layer-list \.motion-prop-row \.motion-fields\s*,/);
  assert.doesNotMatch(css, /#motion-props-body \.motion-fields\s*\{\s*display:none/);
  assert.match(css, /#frame-grid \.motion-group-row\{height:22px/);
});

test('Motion key drags provide snapping guides and scroll-aware deltas', () => {
  const source = read('src/js/motion.js');
  assert.match(source, /function collectMotionSnapCandidates\(/);
  assert.match(source, /function snapMotionFrame\(/);
  assert.match(source, /function autoScrollMotionDrag\(/);
  assert.match(source, /function stopMotionAutoScroll\(/);
  assert.match(source, /onDragMove\(_motionAutoScrollEvent\)/);
  assert.match(source, /motionDragScrollLeft\(\) - \(drag\.startScrollLeft \|\| 0\)/);
  assert.match(source, /clearMotionSnapGuide\(\)/);
  assert.match(read('src/css/style.css'), /\.motion-snap-guide\s*\{/);
});

test('layer order drag works from both timeline halves with one outline ghost', () => {
  const timeline = read('src/js/timeline.js');
  const motion = read('src/js/motion.js');
  const layerInout = read('src/js/layer-inout.js');
  const css = read('src/css/style.css');
  // Animation 2D's frame-grid still installs the sticky grip (it has no
  // in/out bar of its own to arm reordering from) — Motion (below) does not,
  // 2026-09-01, so a layer's inPoint sitting at frame 0 can never again put
  // that grip on top of the in-handle in Motion specifically.
  assert.match(timeline, /function installLayerReorderGrip\(/);
  assert.match(timeline, /window\.installLayerReorderGrip=installLayerReorderGrip/);
  assert.match(timeline, /\.lrow\[data-layer\],#frame-grid \.frow\[data-layer\]/);
  assert.match(timeline, /className='layer-drag-ghost'/);
  assert.doesNotMatch(motion, /installLayerReorderGrip\(spacer, li\)/);
  assert.match(layerInout, /armLayerReorder\(e, li, 'grid', row\)/);
  assert.match(css, /\.layer-reorder-grip\{/);
  assert.match(css, /\.layer-drag-ghost\{[^}]*background:transparent/);
});

test('Motion properties select whole tracks and support additive/range selection', () => {
  const source = read('src/js/motion.js');
  assert.match(source, /var _motionPropSel = \[\]/);
  assert.match(source, /function selectMotionProperty\(/);
  assert.match(source, /function keysForPropertySelection\(/);
  assert.match(source, /e\.metaKey \|\| e\.ctrlKey/);
  assert.match(source, /e && e\.shiftKey && _motionPropAnchor/);
  assert.match(source, /decorateMotionPropertyRow\(pr, holder, prop, pnm\)/);
  assert.match(read('src/css/style.css'), /\.motion-prop-row\.prop-selected\{/);
});

test('selected Motion keys accept grouped value edits without creating playhead keys', () => {
  const source = read('src/js/motion.js');
  assert.match(source, /function selectedKeysForEditableProperty\(/);
  assert.match(source, /function selectedDimensionDisplay\(/);
  assert.match(source, /function setSelectedKeyDimension\(/);
  assert.match(source, /function offsetSelectedKeyDimension\(/);
  assert.match(source, /relative:\s*relative/);
  assert.match(source, /offsetSelectedKeyDimension\(holder, prop, dim, edit\.delta\)/);
  assert.match(source, /if \(!changed\) \{/);
  assert.match(source, /placeholder = '—'/);
});

test('Motion key icons distinguish hold, linear and smoothed interpolation', () => {
  const source = read('src/js/motion.js');
  const css = read('src/css/style.css');
  assert.match(source, /function keyInterpolationKind\(/);
  assert.match(source, /return 'linear'/);
  assert.match(source, /return 'smooth'/);
  assert.match(source, /s\.key\.hold = false/);
  assert.match(source, /'motion-key ' \+ interpKind/);
  assert.match(css, /\.motion-key\.linear\{/);
  assert.match(css, /\.motion-key\.hold\{/);
  assert.match(css, /\.motion-key\.smooth\{/);
  assert.match(css, /\.motion-key\.linear\.sel\{transform:rotate\(45deg\) scale\(1\.14\)/);
  assert.match(css, /\.motion-key\.sel\{[^}]*background:var\(--accent-hover\)[^}]*border-color:#fff[^}]*box-shadow:none/);
  assert.match(source, /setKeySel\(\[\{ holder: ld, prop: prop, key: key \}\]\);[\s\S]{0,700}renderTimeline\(\);/,
    'a plain key click must repaint its selected silhouette immediately');
});

test('selected Motion keys expose independent flat incoming and outgoing ease boxes', () => {
  const source = read('src/js/motion.js');
  const css = read('src/css/style.css');
  assert.match(source, /function easeInPercent\(/);
  assert.match(source, /function setSegmentInfluence\(/);
  assert.match(source, /function setSelectedEaseInfluence\(/);
  assert.match(source, /buildKeyEaseBox\(ld, prop, k, 'in'\)/);
  assert.match(source, /buildKeyEaseBox\(ld, prop, k, 'out'\)/);
  assert.match(source, /idx > 0 \? track\.keys\[idx - 1\] : null/);
  assert.match(css, /\.motion-key-ease-box\.in\{right:/);
  assert.match(css, /\.motion-key-ease-box\.out\{left:/);
  assert.match(css, /#playhead\{[^}]*background:transparent/);
  assert.match(css, /\.fc\.cur\{background:transparent/);
});

test('Motion keeps the value-change connector between keyframes interactive', () => {
  const source = read('src/js/motion.js');
  const css = read('src/css/style.css');
  assert.match(source, /var changed = a\.v\.some/);
  assert.match(source, /rect\.setAttribute\('class', 'motion-key-connect'/);
  assert.match(source, /window\._motionConnectDrag = \{/);
  assert.match(source, /retime: e\.altKey/);
  assert.match(source, /rowEl\.appendChild\(svg\)/);
  assert.match(css, /\.motion-key-connect\{/);
});
