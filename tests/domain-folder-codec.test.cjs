'use strict';
// P20 — folder metadata codec.
//
// The document half is exercised against the REAL `SM.importJSON` source, not
// a reimplementation: timeline.js cannot be loaded whole in a headless vm (it
// needs app.js, which needs Paper.js and a DOM), so the method text is sliced
// out and evaluated on its own. That is the harness tests/project-open.test.cjs
// already established for this exact problem; the stub list below is what the
// real method touches on the way through, nothing more.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const codec = require('../src/js/domain/document/folder-codec.js');
const codecSource = fs.readFileSync(path.join(root, 'src/js/domain/document/folder-codec.js'), 'utf8');
const timelineSource = fs.readFileSync(path.join(root, 'src/js/timeline.js'), 'utf8');
const tweensSource = fs.readFileSync(path.join(root, 'src/js/tweens.js'), 'utf8');

function slice(source, startMarker, endMarker, label) {
  const start = source.indexOf(startMarker);
  assert.notEqual(start, -1, `${label}: start marker not found — the source moved`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(end, -1, `${label}: end marker not found — the source moved`);
  return source.slice(start, end);
}

// Everything the real importJSON reaches for on a folder round-trip. Kept
// explicit rather than auto-stubbed so a NEW dependency shows up as a failure
// to be looked at, not as a silently swallowed noop.
function documentContext() {
  const noop = () => {};
  const toasts = [];
  const state = { layers: [], activeLayerIdx: 0, symbols: {} };
  const engine = { clearRetainedPaths: noop };
  const context = {
    console, JSON, Math, Date, Object, Array, String, Number, Boolean, RegExp,
    parseInt, parseFloat, isNaN,
    SM: { t(key) { return key; } },
    state, userLayers: [], _symbolPaperLayers: {},
    showToast(message) { toasts.push(message); },
    SMEngineBridge: engine,
    createUserLayer(name) { state.layers.push({ name: name, frames: [] }); return state.layers.length - 1; },
    nextLayerColor() { return '#888888'; },
    exitToScene: noop, loadFrame: noop, renderTimeline: noop, renderLayerList: noop,
    updateUI: noop, goToFrame: noop, renderNow: noop, setActiveLayer: noop,
    updatePlayhead: noop, refreshLayerColors: noop, saveAllLayerFrames: noop,
    activateUL: noop, drawStage: noop, renderOS: noop, renderArcs: noop,
    renderSymbolTabs: noop, syncDocFields: noop,
  };
  context.window = context;
  vm.createContext(context);
  vm.runInContext(codecSource, context, { filename: 'folder-codec.js' });
  vm.runInContext(fs.readFileSync(path.join(root, 'src/js/project-document.js'), 'utf8'), context,
    { filename: 'project-document.js' });
  const method = slice(timelineSource, '  importJSON:function(json,silent){', '\n  getState:function()', 'importJSON')
    .replace('  importJSON:', '').replace(/},\s*$/, '}');
  const importJSON = vm.runInContext(`(${method})`, context, { filename: 'timeline-importJSON.js' });
  return { context, state, toasts, importJSON };
}

// An independent fixture: two folders, both flags, one member each, plus a
// layer deliberately OUTSIDE any folder. Frames are valid-but-empty so the
// round-trip exercises folder metadata without dragging in stroke
// deserialization (desP lives in app.js and is not what P20 touches).
function folderDocument() {
  return {
    version: 13, totalFrames: 1, fps: 12,
    layers: [
      { name: 'Hero', folderId: 'fld_a', frames: [{ strokes: [] }] },
      { name: 'Prop', folderId: 'fld_b', frames: [{ strokes: [] }] },
      { name: 'Loose', frames: [{ strokes: [] }] },
    ],
    layerFolders: {
      fld_a: { name: 'Characters', collapsed: true },
      fld_b: { name: 'Props', collapsed: false },
    },
  };
}

// ---- pure codec semantics ------------------------------------------------

test('document export keeps the live map BY REFERENCE and preserves undefined', () => {
  const map = { f1: { name: 'A', collapsed: false } };
  assert.equal(codec.exportFolderMap(map), map);
  assert.equal(codec.exportFolderMap(undefined), undefined);
});

test('document import adopts the parsed map by reference, defaulting to {}', () => {
  const parsed = { f1: { name: 'A', collapsed: true } };
  assert.equal(codec.importFolderMap(parsed), parsed);
  assert.deepEqual(codec.importFolderMap(undefined), {});
  assert.deepEqual(codec.importFolderMap(null), {});
  assert.deepEqual(codec.importFolderMap(0), {});
});

test('undo snapshot DEEP COPIES, so later edits cannot reach into it', () => {
  const live = { f1: { name: 'Characters', collapsed: false } };
  const snap = codec.snapshotFolderMap(live);
  assert.deepEqual(snap, live);
  assert.notEqual(snap, live);
  assert.notEqual(snap.f1, live.f1);
  live.f1.name = 'Renamed';
  live.f1.collapsed = true;
  assert.equal(snap.f1.name, 'Characters', 'snapshot aliased live state');
  assert.equal(snap.f1.collapsed, false);
  assert.deepEqual(codec.snapshotFolderMap(undefined), {});
});

test('undo restore deep copies back out, and is guarded on presence', () => {
  const snap = { f1: { name: 'Characters', collapsed: true } };
  const restored = codec.restoreFolderMap(snap);
  assert.deepEqual(restored, snap);
  assert.notEqual(restored, snap);
  assert.notEqual(restored.f1, snap.f1);
  // A snapshot predating these maps carries undefined; the caller must then
  // leave the live map alone rather than clearing it.
  assert.equal(codec.hasFolderMap(undefined), false);
  assert.equal(codec.hasFolderMap({}), true);
  assert.equal(codec.hasFolderMap(snap), true);
});

test('per-layer membership: export passes through, import skips falsy ids', () => {
  assert.equal(codec.exportFolderId({ folderId: 'fld_a' }), 'fld_a');
  assert.equal(codec.exportFolderId({}), undefined);

  const target = { name: 'fresh' };
  codec.applyFolderId(target, {});
  assert.equal('folderId' in target, false, 'a missing id must not stamp undefined');
  codec.applyFolderId(target, { folderId: '' });
  assert.equal('folderId' in target, false, 'an empty id must not be adopted');
  codec.applyFolderId(target, { folderId: 'fld_a' });
  assert.equal(target.folderId, 'fld_a');
  // Pre-existing membership survives a source that carries none.
  codec.applyFolderId(target, { folderId: undefined });
  assert.equal(target.folderId, 'fld_a');
});

test('the codec module is pure: no globals, no state, no DOM', () => {
  const stripped = codecSource.split('\n').map(l => l.replace(/\/\/.*$/, '')).join('\n');
  assert.equal(/\bwindow\b|\bdocument\b|\bstate\./.test(stripped), false);
});

// ---- real document round-trip -------------------------------------------

test('two folders, names, collapsed flags and memberships survive real importJSON', () => {
  const { state, toasts, importJSON } = documentContext();
  importJSON(JSON.stringify(folderDocument()), true);

  assert.deepEqual(toasts, [], 'importJSON reported a problem');
  assert.deepEqual(state.layerFolders, {
    fld_a: { name: 'Characters', collapsed: true },
    fld_b: { name: 'Props', collapsed: false },
  });
  assert.equal(state.layers[0].folderId, 'fld_a');
  assert.equal(state.layers[1].folderId, 'fld_b');
  assert.equal(state.layers[2].folderId, undefined, 'a layer outside every folder gained one');
});

test('a legacy document with no folder metadata imports as an empty map', () => {
  const { state, toasts, importJSON } = documentContext();
  const legacy = folderDocument();
  delete legacy.layerFolders;
  legacy.layers.forEach(layer => { delete layer.folderId; });
  importJSON(JSON.stringify(legacy), true);

  assert.deepEqual(toasts, []);
  // The `{}` default is built by the codec running INSIDE the vm, so it
  // carries that realm's Object.prototype and a strict deep-equal against a
  // literal from this realm fails on prototype identity alone. Assert the
  // property that actually matters — present and empty, not undefined.
  assert.notEqual(state.layerFolders, undefined, 'missing folder map must default to {}, not undefined');
  assert.equal(typeof state.layerFolders, 'object');
  assert.deepEqual(Object.keys(state.layerFolders), []);
  assert.equal(state.layers.every(l => l.folderId === undefined), true);
});

test('unknown nested folder metadata is preserved verbatim, not dropped or redefined', () => {
  const { state, importJSON } = documentContext();
  const doc = folderDocument();
  doc.layerFolders.fld_a.futureField = { tint: '#abcdef', nested: [1, 2] };
  importJSON(JSON.stringify(doc), true);
  assert.deepEqual(state.layerFolders.fld_a.futureField, { tint: '#abcdef', nested: [1, 2] });
});

// ---- real undo snapshot/restore -----------------------------------------

function undoContext(liveFolders) {
  const noop = () => {};
  const state = {
    layers: [], activeLayerIdx: 0, totalFrames: 1, currentFrame: 0, waOut: 0,
    layerFolders: liveFolders, layerLinkGroups: {}, cameraKeys: [], imageMeshes: {},
    motionArcs: {}, tweenEasing: {}, tweenOverrides: {}, trackRoles: {},
  };
  const context = {
    console, JSON, Math, Date, Object, Array, String, Number, Boolean,
    state, _cloneLayersForUndo(layers) { return layers.slice(); },
  };
  context.window = context;
  vm.createContext(context);
  vm.runInContext(codecSource, context, { filename: 'folder-codec.js' });
  const snapshotSrc = slice(tweensSource, 'function layersSnapshotNow(){', '\nfunction ', 'layersSnapshotNow');
  vm.runInContext(snapshotSrc, context, { filename: 'tweens-layersSnapshotNow.js' });
  return { context, state };
}

test('real layersSnapshotNow deep copies folder metadata out of live state', () => {
  const live = { fld_a: { name: 'Characters', collapsed: false } };
  const { context, state } = undoContext(live);
  const snapshot = context.layersSnapshotNow();

  assert.deepEqual(snapshot.layerFolders, live);
  assert.notEqual(snapshot.layerFolders, live, 'snapshot aliased the live map');
  state.layerFolders.fld_a.name = 'Renamed after the snapshot';
  assert.equal(snapshot.layerFolders.fld_a.name, 'Characters',
    'renaming a folder reached into an existing undo snapshot');
});

test('real layersSnapshotNow tolerates a missing live folder map', () => {
  const { context } = undoContext(undefined);
  assert.deepEqual(context.layersSnapshotNow().layerFolders, {});
});

// ---- routing guard -------------------------------------------------------

test('the real call sites route through the codec rather than re-inlining', () => {
  // Each of these is the exact shape the codec replaced. If one comes back,
  // the extraction has silently regressed even though behaviour still passes.
  assert.match(timelineSource, /layerFolders:NemoFolderCodec\.exportFolderMap\(state\.layerFolders\)/);
  assert.match(timelineSource, /state\.layerFolders=NemoFolderCodec\.importFolderMap\(d\.layerFolders\)/);
  assert.match(timelineSource, /folderId:NemoFolderCodec\.exportFolderId\(l\)/);
  assert.match(timelineSource, /NemoFolderCodec\.applyFolderId\(state\.layers\[idx\],ld\)/);
  assert.match(tweensSource, /layerFolders:NemoFolderCodec\.snapshotFolderMap\(state\.layerFolders\)/);
  assert.match(tweensSource, /NemoFolderCodec\.hasFolderMap\(s\.layerFolders\)/);
  assert.match(tweensSource, /NemoFolderCodec\.restoreFolderMap\(s\.layerFolders\)/);

  assert.equal(/layerFolders:JSON\.parse\(JSON\.stringify\(state\.layerFolders/.test(tweensSource), false);
  assert.equal(/state\.layerFolders=d\.layerFolders\|\|\{\}/.test(timelineSource), false);
  assert.equal(/if\(ld\.folderId\)state\.layers\[idx\]\.folderId=ld\.folderId/.test(timelineSource), false);
});

test('index.html loads the codec before both of its consumers', () => {
  const html = fs.readFileSync(path.join(root, 'src/index.html'), 'utf8');
  const codecAt = html.indexOf('js/domain/document/folder-codec.js');
  const tweensAt = html.indexOf('src="js/tweens.js"');
  const timelineAt = html.indexOf('src="js/timeline.js"');
  assert.notEqual(codecAt, -1, 'folder-codec.js is not referenced by index.html');
  assert.ok(codecAt < tweensAt, 'codec must load before tweens.js');
  assert.ok(codecAt < timelineAt, 'codec must load before timeline.js');
});
