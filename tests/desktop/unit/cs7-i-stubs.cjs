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
const MOTION_FRAMES = { 'motion-draw': 10, 'motion-pen': 11, 'motion-shape': 9 };
const RECTS = { canvas: { x: 100, y: 80, w: 500, h: 300 }, drawZone: { x: 200, y: 150, w: 300, h: 150 }, outside: { x: 110, y: 350, w: 60, h: 20 } };

class DrawWorld extends World {
  constructor(reportDirectory, faults = {}) {
    super(reportDirectory, faults);
    this.state = {
      scene: { anchor: mk([[0, true]]), 'comp-inst': mk([[0, true]]), plain: mk([[0, false]]), bake: mk([[0, true], [1, false]]) },
      component: { 'comp-layer': mk([[0, true], [1, false]]) },
      kinds: { anchor: 'drawing', 'comp-inst': 'instance', plain: 'drawing', bake: 'instance' },
    };
    this.undoStack = [1, 2, 3].map(() => clone(this.state));
  }
  growScene() { this.state.scene.anchor.c[1] = !this.state.scene.anchor.c[1]; }
  draw(ctx, uid, frame, phase) {
    const f = this.faults, times = f.double === phase ? 2 : 1;
    if (f.noop === phase) return;
    this.undoStack.push(clone(this.state)); this.redoStack = [];
    for (let t = 0; t < times; t++) {
      const r = this.state[ctx][uid];
      if (f.noOwnKey === phase) r.c[frame] = true; else { r.k[frame] = 'key'; r.c[frame] = true; norm(r); }
      this.rev++;
    }
    if (f.leak === phase) this.growScene();
  }
  png(name, image) { fs.mkdirSync(path.dirname(path.join(this.dir, name)), { recursive: true }); fs.writeFileSync(path.join(this.dir, name), encodeRGBA(image.width, image.height, image.rgba)); }
  capture(name) {
    const f = this.faults, image = canvas(810, 710, [200, 200, 200, 255]);
    if (!f.blankCapture) fillRect(image, 150, 100, 30, 30, [0, 0, 0, 255]);
    if (this.state.component['comp-layer'].c[8] && !f.noInstance) fillRect(image, 250, 200, 100, 10, [10, 10, 10, 255]);
    if (f.noise && this.state.component['comp-layer'].c[8]) fillRect(image, 112, 352, 50, 10, [0, 0, 0, 255]);
    this.png(name, image);
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
      if (f.modeEdits === phase) this.draw('scene', 'plain', 5, 'modeEdits');
      return this.evidence('scene');
    }
    if (MOTION_FRAMES[phase] !== undefined) {
      this.draw('scene', 'plain', MOTION_FRAMES[phase], phase);
      return this.evidence('scene', { selection: { layerUid: 'plain', frame: f.wrongFrame === phase ? 0 : MOTION_FRAMES[phase] } });
    }
    if (phase === 'instance-before') return { observation: 'measured', productFailure: false, rects: RECTS };
    if (phase === 'component-enter' || phase === 'component-reenter') return this.evidence('component', comp);
    if (DRAW_FRAMES[phase] !== undefined) {
      this.draw('component', 'comp-layer', DRAW_FRAMES[phase], phase);
      return this.evidence('component', { selection: { layerUid: 'comp-layer', frame: DRAW_FRAMES[phase] } });
    }
    if (phase === 'comp-undo') { this.step('undo'); return this.evidence('component'); }
    if (phase === 'comp-redo') { this.step('redo'); return this.evidence('component'); }
    if (phase === 'component-exit') return this.evidence('scene');
    if (phase === 'bitmap-export') {
      const image = canvas(600, 400, [255, 255, 255, 255]);
      if (!f.noBitmapInk) fillRect(image, 100, 100, 200, 40, [20, 20, 20, 255]);
      this.png('exports/bitmap-brush.png', image);
      return { observation: 'exported', productFailure: false, exportedFile: 'exports/bitmap-brush.png',
        rects: { stroke: { x: 90, y: 90, w: 230, h: 70 }, background: { x: 400, y: 300, w: 100, h: 50 } } };
    }
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
      this.fileBytes = JSON.stringify({ format: 'nemo.native-project-document', totalFrames: 12, layers: Object.keys(this.state.scene).map(uid => ({ uid })) });
      fs.writeFileSync(path.join(this.dir, 'saved/component-draw.json'), this.fileBytes);
      return this.evidence('component', { ...comp, savedFile: 'saved/component-draw.json' });
    }
    if (phase === 'draw-reopen') {
      this.documentId = 'doc-2'; this.undoStack = []; this.redoStack = []; this.rev = 1;
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
      throw new Error('unexpected helper command ' + command);
    },
  };
}

// ---- legacy import -------------------------------------------------------------------------------------------
class LegacyWorld {
  constructor(reportDirectory, faults = {}) {
    this.dir = reportDirectory; this.faults = faults; this.toast = ''; this.count = 0; this.rev = 1;
    this.doc = null; this.undoStack = []; this.redoStack = []; this.savedFrames = null;
  }
  newDocument(json) {
    this.count++; this.rev = 1; this.undoStack = []; this.redoStack = [];
    const symbols = Object.values(json.symbols || {});
    this.doc = { id: 'doc-' + this.count, frames: json.totalFrames, names: json.layers.map(l => l.name), comps: symbols.map(s => s.totalFrames) };
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
    this.newDocument(json);
    this.toast = this.faults.wrongNotice ? 'Opened: ' + base : 'Opened: ' + base + legacy.OPENED;
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
        { ...pin, kind: 'layers', contextId: 'scene-root', layerCount: d.names.length, cursor: 0, items: d.names.map((name, i) => ({ layerUid: 'l' + i, name, layerKind: 'drawing' })), nextCursor: null }),
    };
  }
  evidence(extra = {}) { return { observation: 'did it', productFailure: false, ...extra }; }
  status() { return { available: !!this.doc, documentId: this.doc?.id, contentRevision: this.rev }; }
  query(operation, payload) {
    const all = this.reads(), snapshot = 'native-object:' + this.doc.id + ':' + this.rev;
    if (operation === 'query.document.object') return this.envelope(operation, payload, { atRevision: this.rev, documentSnapshotId: snapshot, contextId: 'scene-root', frame: 0 });
    if (operation === 'query.document.object.history') return all.history;
    if (operation === 'query.document.object.component.read') return payload.selector.kind === 'contexts' ? all.contexts : all.layers;
    throw new Error('unexpected query ' + operation);
  }
  write(frames) {
    const file = path.join(this.dir, 'saved/legacy-animation.json');
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, JSON.stringify({ format: this.faults.saveLegacyFormat ? undefined : 'nemo.native-project-document', totalFrames: frames, layers: this.doc.names.map(name => ({ name })), comps: this.doc.comps }));
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
    if (phase === 'save-first') {
      if (f.silentSave) { this.toast = 'Saved: animation'; return this.evidence({ nameDialogOpened: false }); }
      this.write(this.doc.frames); this.toast = 'Saved: legacy-animation';
      if (f.overwriteOriginal) fs.appendFileSync(path.join(this.dir, 'legacy-source/animation.json'), ' ');
      return this.evidence({ nameDialogOpened: true, savedFile: legacy.SAVED });
    }
    if (phase === 'save-original-refused') {
      this.toast = f.silentRefusal ? this.toast : f.savedOverOriginal ? 'Saved: animation' : legacy.REFUSED_SAVE;
      return { observation: 'refused', productFailure: false };
    }
    if (phase === 'legacy-edit') { this.push(); if (!f.editInvisible) this.doc.frames++; return this.evidence(); }
    if (phase === 'legacy-undo') {
      this.redoStack.push(clone(this.doc)); this.doc = this.undoStack.pop(); this.rev++;
      if (f.badUndo) this.doc.frames++;
      return this.evidence();
    }
    if (phase === 'legacy-redo') { this.undoStack.push(clone(this.doc)); this.doc = this.redoStack.pop(); this.rev++; return this.evidence(); }
    if (phase === 'save-again') { this.write(f.saveNoEdit ? this.doc.frames - 1 : this.doc.frames); return this.evidence({ savedFile: legacy.SAVED }); }
    if (phase === 'reopen-saved') {
      const saved = JSON.parse(fs.readFileSync(path.join(this.dir, 'saved/legacy-animation.json'), 'utf8'));
      if (f.touchSaved) fs.appendFileSync(path.join(this.dir, 'saved/legacy-animation.json'), ' ');
      this.newDocument({ totalFrames: saved.totalFrames, layers: saved.layers, symbols: Object.fromEntries(saved.comps.map((n, i) => [i, { totalFrames: n }])) });
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
      return { status: 'ok', nodes: [{ role: 'AXStaticText', labels: [], value: world.toast }] };
    },
  };
}

module.exports = { DrawWorld, drawSession, LegacyWorld, legacySession, COMPONENT, RECTS };
