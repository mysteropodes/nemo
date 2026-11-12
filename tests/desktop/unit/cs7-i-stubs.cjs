'use strict';
// Disposable stand-ins for the component-edit-draw and legacy-import scenarios: a tiny document model that answers
// the read-only Nemo MCP queries and a toast the accessibility read can see. No app, helper or installed run is
// involved. Faults are injected by name to prove the oracles fail them.
const fs = require('node:fs');
const path = require('node:path');
const { encodeRGBA, canvas, fillRect } = require('../../fixtures/lib/png.cjs');
const { World, identity, clone, INSTANCE, stubReader } = require('./cs7-h-stubs.cjs');
const legacy = require('../cs7-scenarios/legacy-import.cjs');

const COMPONENT = 'native-component:1:0';
const norm = r => { for (let i = 1; i < r.k.length; i++) if (r.k[i] === 'held') r.c[i] = r.c[i - 1]; return r; };
const mk = (keys, n = 12) => {
  const k = Array(n).fill('held'), c = Array(n).fill(false);
  for (const [i, has] of keys) { k[i] = 'key'; c[i] = has; }
  return norm({ k, c });
};

// ---- component drawing ---------------------------------------------------------------------------------------
const DRAW_FRAMES = { 'comp-draw': 8, 'comp-pen': 7, 'comp-line': 6, 'comp-rect': 5, 'comp-ellipse': 4, 'comp-fillbrush': 3, 'comp-bitmap': 2 };
const DRAW_TOOLS = { 'comp-draw': 'draw', 'comp-pen': 'pen', 'comp-line': 'line', 'comp-rect': 'rect', 'comp-ellipse': 'ellipse', 'comp-fillbrush': 'fillbrush', 'comp-bitmap': 'draw',
  'motion-draw': 'draw', 'motion-pen': 'pen', 'motion-shape': 'rect' };
const MOTION_FRAMES = { 'motion-draw': 10, 'motion-pen': 11, 'motion-shape': 9 };
const TOOL_TITLES = { draw: 'Brush (B)', pen: 'Pen — click to place points, drag for curves (P)', fillbrush: 'Fill brush — paint a freehand filled shape, pressure-sensitive (N)',
  rect: 'Rectangle (R)', line: 'Line (U in Animation 2D — Motion mode reserves U to reveal animated properties)', ellipse: 'Ellipse (L)', select: 'Selection / Transform (V)' };
const CANVAS_FRAME = { x: 100, y: 80, w: 500, h: 300 };

class DrawWorld extends World {
  constructor(reportDirectory, faults = {}) {
    super(reportDirectory, faults);
    this.state = {
      scene: { anchor: mk([[0, true]]), 'comp-inst': mk([[0, true]]), plain: mk([[0, false]]), bake: mk([[0, true], [1, false]]) },
      component: { 'comp-layer': mk([[0, true], [1, false]]) },
      kinds: { anchor: 'drawing', 'comp-inst': 'instance', plain: 'drawing', bake: 'instance' },
    };
    this.undoStack = [1, 2, 3].map(() => clone(this.state));
    this.tool = 'select'; this.mode = 'anim2d'; this.above = false; this.playhead = 0; this.bitmapFrames = new Set();
  }
  growScene() { this.state.scene.anchor.c[1] = !this.state.scene.anchor.c[1]; }
  draw(ctx, uid, frame, phase) {
    const f = this.faults, times = f.double === phase ? 2 : 1;
    if (f.noop === phase) return;
    this.undoStack.push(clone(this.state)); this.redoStack = [];
    for (let t = 0; t < times; t++) {
      const r = this.state[ctx][uid];
      if (f.originKey && ctx === 'scene') { r.k[0] = 'key'; r.c[0] = true; norm(r); }
      else if (f.noOwnKey === phase) r.c[frame] = true; else { r.k[frame] = 'key'; r.c[frame] = true; norm(r); }
      this.rev++;
    }
    if (f.leak === phase) this.growScene();
  }
  png(name, image) { fs.mkdirSync(path.dirname(path.join(this.dir, name)), { recursive: true }); fs.writeFileSync(path.join(this.dir, name), encodeRGBA(image.width, image.height, image.rgba)); }
  // The window is 810x710 points and the capture 810x710 pixels, so a frame is also its pixel rectangle.
  capture(name) {
    const f = this.faults, image = canvas(810, 710, [200, 200, 200, 255]);
    if (!f.blankCapture) fillRect(image, 150, 100, 30, 30, [0, 0, 0, 255]);
    if (this.state.component['comp-layer'].c[8] && !f.noInstance) fillRect(image, 250, 200, 100, 10, [10, 10, 10, 255]);
    if (f.noise && this.state.component['comp-layer'].c[8]) fillRect(image, 100, 80, 500, 300, [0, 0, 0, 255]);
    if (f.chrome && this.state.component['comp-layer'].c[8]) fillRect(image, 10, 10, 300, 30, [0, 0, 0, 255]);   // UI outside the canvas changed
    this.png(name, image);
  }
  nodes() {
    const f = this.faults, withClasses = (classes, on) => (f.noClasses ? {} : { classes: on ? [...classes, 'active'] : classes });
    const out = [{ role: 'AXGroup', labels: [], identifiers: { resolved: 'drawing-canvas' }, frame: { ...CANVAS_FRAME } }];
    for (const [tool, title] of Object.entries(TOOL_TITLES)) out.push({ role: 'AXButton', labels: [title], identifiers: { resolved: '' }, ...withClasses(['tool-btn'], this.tool === tool) });
    out.push({ role: 'AXButton', labels: ['Animation 2D'], identifiers: { resolved: '' }, ...withClasses(['app-mode-btn'], this.mode === 'anim2d') },
      { role: 'AXButton', labels: ['Motion — keyframes de propriétés (position/rotation/échelle/opacité)'], identifiers: { resolved: '' }, ...withClasses(['app-mode-btn'], this.mode === 'motion') },
      { role: 'AXButton', labels: ['Above — on top of existing fills'], identifiers: { resolved: '' }, ...withClasses(['icon-btn'], this.above) },
      { role: 'AXStaticText', labels: [f.noPlayhead ? '' : String(this.playhead + 1)], identifiers: { resolved: f.noPlayhead ? '' : 'info-frame' } });
    return out;
  }
  query(operation, payload) {
    if (operation !== 'query.document.object.brush.bitmap') return super.query(operation, payload);
    const snapshot = 'native-object:' + this.documentId + ':' + this.rev;
    if (this.faults.bitmapRefused) return { request: { operation, payload }, response: { apiVersion: 2, instanceId: INSTANCE, documentId: this.documentId, contentRevision: this.rev, ok: false, error: { code: 'unavailable' } } };
    const here = payload.contextId === COMPONENT && this.bitmapFrames.has(payload.frame) && this.state.component['comp-layer'].c[payload.frame] && !this.faults.noBitmapRecord;
    const entries = here && payload.offset === 0 ? [{ sourceObjectIndex: 0, layerUid: 'comp-layer', strokeId: 's1', brushId: 'b1', ink: { version: 1, width: 8, height: 8, bytes: 256 } }] : [];
    return this.envelope(operation, payload, { atRevision: this.rev, documentSnapshotId: snapshot, contextId: payload.contextId, frame: payload.frame, offset: payload.offset,
      nextOffset: null, totalEntries: here ? 1 : 0, entries });
  }
  // The native object document a Save writes: layers with their stored key frames, the component's own layers beside them.
  document() {
    const layer = (uid, rows) => ({ layerUid: uid, name: uid, timing: { keys: rows.k.flatMap((kind, i) => (kind === 'key' ? [{ frame: i }] : [])) } });
    return { format: 'nemo.native-object-document', formatVersion: 1, totalFrames: 12, layers: Object.entries(this.state.scene).map(([uid, rows]) => layer(this.faults.savedLayersWrong ? uid + '-x' : uid, rows)),
      components: [{ id: COMPONENT, layers: Object.entries(this.state.component).map(([uid, rows]) => layer(uid, rows)) }], objects: [] };
  }
  async operator(request) {
    const phase = request.phase, f = this.faults, comp = { component: { context: COMPONENT, layerUid: 'comp-layer' } };
    this.phase = phase;
    if (f.failureAt === phase) return { observation: 'the app showed an error', productFailure: true };
    if (f.malformedAt === phase) return { observation: 'looks right', productFailure: false };
    if (f.verdictAt === phase) return { ...this.evidence('scene'), pass: true };
    if (phase === 'window-placement') return { observation: 'placed', productFailure: false };
    if (phase === 'scene-setup') return this.evidence('scene', { layers: { anchor: 'anchor', component: 'comp-inst', plain: 'plain', bake: 'bake' } });
    if (phase === 'motion-enter' || phase === 'motion-exit') {
      this.mode = phase === 'motion-enter' ? (f.modeNotSwitched ? 'anim2d' : 'motion') : 'anim2d';
      if (phase === 'motion-exit') this.playhead = 8;
      if (f.modeEdits === phase) this.draw('scene', 'plain', 5, 'modeEdits');
      return this.evidence('scene');
    }
    if (MOTION_FRAMES[phase] !== undefined) {
      this.draw('scene', 'plain', MOTION_FRAMES[phase], phase);
      this.tool = f.wrongTool === phase ? 'select' : DRAW_TOOLS[phase]; this.playhead = MOTION_FRAMES[phase];
      return this.evidence('scene', { selection: { layerUid: 'plain', frame: f.wrongFrame === phase ? 0 : MOTION_FRAMES[phase] } });
    }
    if (phase === 'bitmap-baseline' || phase === 'bitmap-export') {
      this.playhead = f.wrongExportFrame === phase ? 5 : 2;
      const image = canvas(600, 400, [255, 255, 255, 255]);
      fillRect(image, 20, 20, 40, 10, [90, 90, 90, 255]);
      if (phase === 'bitmap-export' && !f.noBitmapInk) fillRect(image, 100, 100, 200, 40, [20, 20, 20, 255]);
      if (phase === 'bitmap-export' && f.inkEverywhere) fillRect(image, 0, 0, 600, 400, [20, 20, 20, 255]);
      const file = phase === 'bitmap-baseline' ? 'exports/bitmap-baseline.png' : 'exports/bitmap-brush.png';
      if (!(f.noExport === phase)) this.png(file, image);
      return { observation: 'exported', productFailure: false, exportedFile: file };
    }
    if (phase === 'component-enter' || phase === 'component-reenter') return this.evidence('component', comp);
    if (DRAW_FRAMES[phase] !== undefined) {
      this.draw('component', 'comp-layer', DRAW_FRAMES[phase], phase);
      this.tool = f.wrongTool === phase ? 'select' : DRAW_TOOLS[phase]; this.playhead = DRAW_FRAMES[phase];
      if (phase === 'comp-fillbrush') this.above = !f.aboveOff;
      if (phase === 'comp-bitmap' && !f.notBitmap) this.bitmapFrames.add(2);
      return this.evidence('component', { selection: { layerUid: 'comp-layer', frame: DRAW_FRAMES[phase] } });
    }
    if (phase === 'comp-undo') { this.step('undo'); return this.evidence('component'); }
    if (phase === 'comp-redo') { this.step('redo'); return this.evidence('component'); }
    if (phase === 'component-exit') { this.playhead = f.exitFrame ?? 8; return this.evidence('scene'); }
    if (phase === 'bake-break') {
      this.undoStack.push(clone(this.state)); this.redoStack = [];
      if (f.bakeNoKind !== true) this.state.kinds.bake = 'drawing';
      if (f.bakeWrongFrames) this.state.scene.bake.c[1] = true;
      this.rev++;
      return this.evidence('scene');
    }
    if (phase === 'bake-undo') { this.step('undo'); return this.evidence('scene'); }
    if (phase === 'bake-redo') { this.step('redo'); return this.evidence('scene'); }
    if (phase === 'draw-save') {
      fs.mkdirSync(path.join(this.dir, 'saved'), { recursive: true });
      this.fileBytes = JSON.stringify(this.document());
      if (!f.noWrite) fs.writeFileSync(path.join(this.dir, 'saved/component-draw.json'), this.fileBytes);
      return this.evidence('component', { ...comp, savedFile: 'saved/component-draw.json' });
    }
    if (phase === 'draw-reopen') {
      this.documentId = f.sameDocumentOnReopen ? this.documentId : 'doc-2'; this.undoStack = []; this.redoStack = []; this.rev = 1;
      if (f.touchFileOnOpen) fs.appendFileSync(path.join(this.dir, 'saved/component-draw.json'), ' ');
      if (f.lostDrawOnReopen) this.state.component['comp-layer'].c[2] = false;
      if (f.staleHistory) this.undoStack = [1];
      return this.evidence('component', { ...comp, savedFile: 'saved/component-draw.json' });
    }
    throw new Error('unknown phase ' + phase);
  }
}
function drawSession({ world, directory = world.dir, reader = stubReader({ directory, target: () => world, tamper: world.tamper && ((p, pair) => world.tamper(p, pair)) }) }) {
  return {
    read: fn => reader.read(fn),
    start: async () => identity, identity: async () => ({ ...identity }), cleanup: async () => {},
    runtimeContext: async () => ({ identity: { ...identity }, snapshot: {}, window: { windowId: 7 } }),
    requestEvidence: async request => world.operator(request),
    input: async (command, option) => {
      if (command === 'capture') { world.capture(option); return { status: 'ok', capture: option }; }
      if (command === 'observe') return { status: 'ok', nodes: world.nodes(), windowSize: { w: 810, h: 710 } };
      throw new Error('unexpected helper command ' + command);
    },
  };
}

// ---- legacy import -------------------------------------------------------------------------------------------
const runsOf = rows => {
  const out = []; let start = 0;
  for (let i = 1; i <= rows.k.length; i++) {
    if (i === rows.k.length || rows.k[i] !== rows.k[start] || rows.c[i] !== rows.c[start] || rows.k[i] === 'key') { out.push([start, i - start, rows.k[start], start, rows.c[start], false]); start = i; }
  }
  return out;
};
class LegacyWorld {
  constructor(reportDirectory, faults = {}) {
    this.dir = reportDirectory; this.faults = faults; this.toast = ''; this.count = 0; this.rev = 1; this.dialog = false;
    this.doc = null; this.undoStack = []; this.redoStack = []; this.savedFrames = null;
  }
  newDocument(json, kinds = {}) {
    this.count++; this.rev = 1; this.undoStack = []; this.redoStack = [];
    const symbols = Object.values(json.symbols || {});
    const names = json.layers.map(l => (typeof l === 'string' ? l : l.name));
    this.doc = { id: 'doc-' + this.count, frames: json.totalFrames, names, kinds, comps: symbols.map(s => s.totalFrames),
      cells: { k: Array.from({ length: json.totalFrames }, (_, i) => (i === 0 || i === (json.totalFrames >> 1) ? 'key' : 'held')), c: Array(json.totalFrames).fill(true) } };
    if (this.faults.wrongLayers) this.doc.names = this.doc.names.slice(1);
  }
  open(file) {
    const json = JSON.parse(fs.readFileSync(file, 'utf8')), base = path.basename(file, '.json');
    const field = ['exprGlobals', 'trackRoles'].find(key => json[key] && (typeof json[key] !== 'object' || Object.keys(json[key]).length));
    if (field) {
      this.toast = this.faults.refusalNoPath ? REFUSED + ': legacy_unsupported_field' : REFUSED + ': legacy_unsupported_field: ' + field + ' has no native owner';
      if (this.faults.refusalReplacesDoc) this.newDocument({ totalFrames: 1, layers: [{ name: 'x' }] });
      return;
    }
    this.newDocument(json, this.faults.wrongKinds ? {} : legacy.expectedOf(json).kinds);
    this.toast = this.faults.wrongNotice ? 'Opened: ' + base : 'Opened: ' + base + legacy.OPENED + (this.faults.lossyNotice ? ' 2 item(s) were not carried over (first: layers[3].x).' : '');
  }
  envelope(operation, payload, result) {
    return { request: { operation, payload }, response: { apiVersion: 2, instanceId: INSTANCE, documentId: this.doc.id, contentRevision: this.rev, ok: true, result } };
  }
  reads() {
    const snapshot = 'native-object:' + this.doc.id + ':' + this.rev, d = this.doc;
    const contexts = [{ contextId: 'scene-root', contextKind: 'scene', name: null, totalFrames: d.frames, layerCount: d.names.length },
      ...d.comps.map((n, i) => ({ contextId: 'native-component:' + i + ':0', contextKind: 'component', name: 'C' + i, totalFrames: n, layerCount: 1 }))];
    const pin = { atRevision: this.rev, documentSnapshotId: snapshot };
    return {
      history: this.envelope('query.document.object.history', pin, { ...pin, undoDepth: this.undoStack.length, redoDepth: this.redoStack.length, offset: 0, entries: [] }),
      contexts: this.envelope('query.document.object.component.read', { ...pin, selector: { kind: 'contexts', cursor: 0, limit: 16 } },
        { ...pin, kind: 'contexts', contextCount: contexts.length, cursor: 0, items: contexts, nextCursor: null }),
      layers: this.envelope('query.document.object.component.read', { ...pin, selector: { kind: 'layers', contextId: 'scene-root', cursor: 0, limit: 16 } },
        { ...pin, kind: 'layers', contextId: 'scene-root', layerCount: d.names.length, cursor: 0, items: d.names.map((name, i) => ({ layerUid: 'l' + i, name, layerKind: d.kinds[name] || 'drawing' })), nextCursor: null }),
    };
  }
  evidence(extra = {}) { return { observation: 'did it', productFailure: false, ...extra }; }
  status() { return { available: !!this.doc, documentId: this.doc?.id, contentRevision: this.rev }; }
  query(operation, payload) {
    const all = this.reads(), snapshot = 'native-object:' + this.doc.id + ':' + this.rev;
    if (operation === 'query.document.object') return this.envelope(operation, payload, { atRevision: this.rev, documentSnapshotId: snapshot, contextId: 'scene-root', frame: 0 });
    if (operation === 'query.document.object.history') return all.history;
    if (operation === 'query.document.object.component.read') return payload.selector.kind === 'contexts' ? all.contexts : all.layers;
    if (operation === 'query.document.object.layer.timing') {
      return this.envelope(operation, payload, { kind: 'layerCells', atRevision: this.rev, documentSnapshotId: snapshot, contextId: payload.contextId, layerUid: payload.layerUid, frame: 0,
        cells: { runs: runsOf(this.doc.cells), nextFrame: null } });
    }
    throw new Error('unexpected query ' + operation);
  }
  // The native object document a Save writes: this document's layers by uid and name, with the frame count.
  write(frames) {
    const file = path.join(this.dir, 'saved/legacy-animation.json');
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, JSON.stringify({ format: this.faults.saveLegacyFormat ? undefined : 'nemo.native-object-document', formatVersion: 1, totalFrames: frames,
      layers: this.doc.names.map((name, i) => ({ layerUid: this.faults.savedLayersWrong ? 'z' + i : 'l' + i, name })), objects: [], comps: this.doc.comps, kinds: this.doc.kinds }));
  }
  push() { this.undoStack.push(clone(this.doc)); this.redoStack = []; this.rev++; }
  async operator(request) {
    const phase = request.phase, f = this.faults, file = request.details.legacyFile;
    this.phase = phase;
    if (f.failureAt === phase) return { observation: 'the app showed an error', productFailure: true };
    if (f.malformedAt === phase) return { observation: 'looks right', productFailure: false };
    if (f.verdictAt === phase) return { observation: 'did it', productFailure: false, pass: true };
    if (phase === 'window-placement') return { observation: 'placed', productFailure: false };
    if (phase === 'open-animation' || phase === 'open-media') { this.open(file); return this.evidence(); }
    if (phase === 'save-first-press') {
      if (f.silentSave) { this.toast = 'Saved: animation'; return this.evidence({ nameDialogOpened: false }); }
      this.dialog = true;
      return this.evidence({ nameDialogOpened: true });
    }
    if (phase === 'save-first-name') {
      this.dialog = false; this.write(this.doc.frames); this.toast = 'Saved: legacy-animation';
      if (f.overwriteOriginal) fs.appendFileSync(path.join(this.dir, 'legacy-source/animation.json'), ' ');
      return this.evidence({ savedFile: legacy.SAVED });
    }
    if (phase === 'save-original-refused') {
      this.toast = f.silentRefusal ? this.toast : f.savedOverOriginal ? 'Saved: animation' : legacy.REFUSED_SAVE;
      return { observation: 'refused', productFailure: false };
    }
    if (phase === 'legacy-edit') {
      this.push(); if (!f.editInvisible) this.doc.frames++;
      if (!f.editInvisible && !f.editTotalOnly) { this.doc.cells.k.splice(1, 0, 'held'); this.doc.cells.c.splice(1, 0, true); } else if (f.editTotalOnly) { this.doc.cells.k.push('held'); this.doc.cells.c.push(true); }
      return this.evidence();
    }
    if (phase === 'legacy-undo') {
      this.redoStack.push(clone(this.doc)); this.doc = this.undoStack.pop(); this.rev++;
      if (f.badUndo) this.doc.frames++;
      return this.evidence();
    }
    if (phase === 'legacy-redo') { this.undoStack.push(clone(this.doc)); this.doc = this.redoStack.pop(); this.rev++; return this.evidence(); }
    if (phase === 'save-again') {
      if (f.askAgain) { this.dialog = true; return this.evidence({ nameDialogOpened: true }); }
      if (f.askAgainHidden) return this.evidence({ nameDialogOpened: false, savedFile: legacy.SAVED, ...(this.dialog = true, {}) });
      this.write(f.saveNoEdit ? this.doc.frames - 1 : this.doc.frames); return this.evidence({ nameDialogOpened: false, savedFile: legacy.SAVED });
    }
    if (phase === 'reopen-saved') {
      const saved = JSON.parse(fs.readFileSync(path.join(this.dir, 'saved/legacy-animation.json'), 'utf8'));
      if (f.touchSaved) fs.appendFileSync(path.join(this.dir, 'saved/legacy-animation.json'), ' ');
      this.newDocument({ totalFrames: saved.totalFrames, layers: saved.layers, symbols: Object.fromEntries(saved.comps.map((n, i) => [i, { totalFrames: n }])) }, saved.kinds);
      if (f.staleHistory) this.undoStack = [clone(this.doc)];
      if (f.lostEditOnReopen) this.doc.frames--;
      return this.evidence({ savedFile: legacy.SAVED });
    }
    if (phase === 'refuse-expr' || phase === 'refuse-roles') { this.open(file); return this.evidence(); }
    throw new Error('unknown phase ' + phase);
  }
}
const REFUSED = legacy.REFUSED_OPEN;
function legacySession({ world, directory = world.dir, reader = stubReader({ directory, target: () => world, tamper: world.tamper && ((p, pair) => world.tamper(p, pair)) }) }) {
  return {
    read: fn => reader.read(fn),
    start: async () => identity, identity: async () => ({ ...identity }), cleanup: async () => {},
    runtimeContext: async () => ({ identity: { ...identity }, snapshot: {}, window: { windowId: 7 } }),
    requestEvidence: async request => world.operator(request),
    input: async command => {
      if (command !== 'observe') throw new Error('unexpected helper command ' + command);
      if (world.faults.toastHidden) return { status: 'ok', nodes: [] };
      const nodes = [{ role: 'AXStaticText', labels: [], value: world.toast }];
      if (world.dialog && !world.faults.dialogHidden) nodes.push({ role: 'AXSheet', labels: [], identifiers: { resolved: '' } }, { role: 'AXButton', labels: ['Cancel'] }, { role: 'AXButton', labels: ['Save'] });
      return { status: 'ok', nodes };
    },
  };
}

module.exports = { DrawWorld, drawSession, LegacyWorld, legacySession, COMPONENT, CANVAS_FRAME };
