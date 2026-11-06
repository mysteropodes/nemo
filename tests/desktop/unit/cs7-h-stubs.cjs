'use strict';
// Disposable stand-ins for the frame-edit-ui and settings-profile scenarios: a tiny timeline model that
// answers the read-only Nemo MCP queries, and a tiny settings dialog that answers the UI32 helper commands.
// No app, helper or installed run is involved. Faults are injected by name to prove the oracle fails them.
const fs = require('node:fs');
const path = require('node:path');
const { encodeRGBA, canvas, fillRect } = require('../../fixtures/lib/png.cjs');

const identity = { sourceSha: 'a'.repeat(40), tree: 'b'.repeat(40), driverSha: 'c'.repeat(40),
  executableSha256: 'd'.repeat(64), sessionId: 'one-lifetime', pid: 123, dirty: false };
const clone = value => JSON.parse(JSON.stringify(value));

// ---- timeline model ----------------------------------------------------------------------------
const normalize = rows => { for (let i = 1; i < rows.k.length; i++) if (rows.k[i] === 'held') rows.c[i] = rows.c[i - 1]; };
const rowsOf = (k, c) => { const r = { k: [...k], c: [...c] }; normalize(r); return r; };
function applyEdit(rows, kind, c, span) {
  const r = clone(rows);
  if (kind === 'insertFrame' || kind === 'extendExposure') { r.k.splice(c + 1, 0, 'held'); r.c.splice(c + 1, 0, false); }
  else if (kind === 'insertKey') r.k[c] = 'key';
  else if (kind === 'insertBlankKey') { r.k[c] = 'key'; r.c[c] = false; }
  else if (kind === 'clearKey') r.k[c] = 'held';
  else if (kind === 'duplicateKey') { r.k.splice(c + 1, 0, 'key'); r.c.splice(c + 1, 0, true); }
  else if (kind === 'removeFrames') { r.k.splice(span[0], span[1] - span[0] + 1); r.c.splice(span[0], span[1] - span[0] + 1); }
  normalize(r); return r;
}
const GESTURES = {
  'scene-f5': ['scene', 'insertFrame', 0], 'scene-f6': ['scene', 'insertKey', 1], 'scene-f7': ['scene', 'insertBlankKey', 2],
  'scene-menu-insert-frame': ['scene', 'insertFrame', 2], 'scene-menu-insert-keyframe': ['scene', 'insertKey', 3],
  'scene-menu-insert-blank': ['scene', 'insertBlankKey', 5], 'scene-clear-key': ['scene', 'clearKey', 5],
  'scene-extend-exposure': ['scene', 'extendExposure', 1], 'scene-duplicate-key': ['scene', 'duplicateKey', 1],
  'scene-delete-frames': ['scene', 'removeFrames', 7, [7, 8]],
  'component-f5': ['component', 'insertFrame', 0], 'component-f6': ['component', 'insertKey', 1],
  'component-extend-exposure': ['component', 'extendExposure', 1], 'component-f7': ['component', 'insertBlankKey', 2],
  'component-duplicate-key': ['component', 'duplicateKey', 1], 'component-clear-key': ['component', 'clearKey', 2],
  'component-delete-frames': ['component', 'removeFrames', 1, [1, 1]],
};
const COMPONENT = 'native-component:1:0';

class World {
  constructor(reportDirectory, faults = {}) {
    this.dir = reportDirectory; this.faults = faults; this.documentId = 'doc-1'; this.rev = 10; this.undoStack = []; this.redoStack = [];
    const scene = { 'layer-1': rowsOf(['key', ...Array(119).fill('held')], [true, ...Array(119).fill(true)]),
      'layer-2': rowsOf(['key', ...Array(119).fill('held')], [true, ...Array(119).fill(true)]) };
    this.state = { scene, component: { 'comp-layer': rowsOf(['key'], [true]) } };
    this.undoStack = [1, 2, 3].map(() => clone(this.state)); this.fileBytes = null;
  }
  edit(ctx, kind, c, span) {
    const times = this.faults.double === kind ? 2 : 1;
    this.undoStack.push(clone(this.state)); this.redoStack = [];
    for (let t = 0; t < times; t++) {
      const layers = this.state[ctx];
      const uid = ctx === 'scene' ? 'layer-1' : 'comp-layer';
      if (this.faults.noop === kind) continue;
      layers[uid] = applyEdit(layers[uid], kind, c, span);
      const grows = layers[uid].k.length;
      for (const other of Object.keys(layers)) if (other !== uid) {   // untargeted layers are padded/trimmed to the same length
        const o = layers[other];
        while (o.k.length < grows) { o.k.push('held'); o.c.push(true); }
        while (o.k.length > grows) { o.k.pop(); o.c.pop(); }
      }
      this.rev++;
    }
    if (this.faults.noop === kind) this.undoStack.pop();
    if (this.faults.leak === kind && ctx === 'component') this.growScene();
  }
  growScene() { for (const uid of Object.keys(this.state.scene)) this.state.scene[uid] = applyEdit(this.state.scene[uid], 'insertFrame', 0); }
  step(direction) {
    if (direction === 'undo') {
      this.redoStack.push(clone(this.state)); this.state = this.undoStack.pop();
      if (this.faults.badUndo) this.growScene();
    } else { this.undoStack.push(clone(this.state)); this.state = this.redoStack.pop(); }
    this.rev++;
  }
  envelope(operation, payload, result) {
    return { request: { operation, payload }, response: { apiVersion: 2, instanceId: 'instance-A', documentId: this.documentId,
      contentRevision: this.rev, ok: true, result } };
  }
  runs(rows) {
    const out = []; let start = 0;
    for (let i = 1; i <= rows.k.length; i++) {
      if (i === rows.k.length || rows.k[i] !== rows.k[start] || rows.c[i] !== rows.c[start] || rows.k[i] === 'key') {
        out.push([start, i - start, rows.k[start], start, rows.c[start], false]); start = i;
      }
    }
    return out;
  }
  reads(ctx) {
    const snapshot = 'native-object:' + this.documentId + ':' + this.rev, contextId = ctx === 'scene' ? 'scene-root' : COMPONENT;
    return {
      history: this.envelope('query.document.object.history', { atRevision: this.rev, documentSnapshotId: snapshot },
        { atRevision: this.rev, documentSnapshotId: snapshot, undoDepth: this.undoStack.length, redoDepth: this.redoStack.length, offset: 0, entries: [] }),
      layers: Object.entries(this.state[ctx]).map(([layerUid, rows]) => this.envelope('query.document.object.layer.timing',
        { atRevision: this.rev, documentSnapshotId: snapshot, contextId, layerUid, frame: 0, view: 'cells' },
        { kind: 'layerCells', atRevision: this.rev, documentSnapshotId: snapshot, contextId, layerUid, frame: 0,
          cells: { runs: this.runs(rows), nextFrame: null } })),
    };
  }
  evidence(ctx, extra = {}) {
    const out = { observation: 'did it', productFailure: false, ...extra };
    out.reads = this.reads(ctx);
    if (ctx === 'component') out.sceneLayers = this.reads('scene').layers;
    return out;
  }
  document() { return { format: 'nemo', formatVersion: 1, totalFrames: this.state.scene['layer-1'].k.length, layers: Object.keys(this.state.scene).map(uid => ({ uid })) }; }
  async operator(request, drawingPng) {
    const phase = request.phase, faults = this.faults;
    if (faults.failureAt === phase) return { observation: 'the app showed an error', productFailure: true };
    if (faults.malformedAt === phase) return { observation: 'looks right', productFailure: false };
    if (faults.verdictAt === phase) return { ...this.evidence('scene'), pass: true };
    if (phase === 'window-placement') return { observation: 'placed', productFailure: false };
    if (phase === 'scene-setup') return this.evidence('scene', { layers: { drawing: 'layer-1', component: 'layer-2' } });
    if (phase === 'layer-row-styling') return { observation: 'rows measured', productFailure: false, rects: drawingPng.rects };
    if (GESTURES[phase]) {
      const [ctx, kind, frame, span] = GESTURES[phase];
      this.edit(ctx, kind, frame, span);
      const selection = { layerUid: ctx === 'scene' ? 'layer-1' : 'comp-layer', frame, ...(span ? { span } : {}) };
      return this.evidence(ctx, { selection });
    }
    const ctx = phase.startsWith('component') ? 'component' : 'scene';
    if (phase.endsWith('-undo')) { this.step('undo'); return this.evidence(ctx); }
    if (phase.endsWith('-redo')) { this.step('redo'); return this.evidence(ctx); }
    if (phase.endsWith('-save')) {
      const file = ctx === 'scene' ? 'saved/frame-edit-scene.json' : 'saved/frame-edit-component.json';
      fs.mkdirSync(path.join(this.dir, 'saved'), { recursive: true });
      this.fileBytes = JSON.stringify(this.document()); fs.writeFileSync(path.join(this.dir, file), this.fileBytes);
      return this.evidence(ctx, { savedFile: file });
    }
    if (phase.endsWith('-reopen')) {
      const file = ctx === 'scene' ? 'saved/frame-edit-scene.json' : 'saved/frame-edit-component.json';
      this.documentId = ctx === 'scene' ? 'doc-2' : 'doc-3'; this.undoStack = []; this.redoStack = []; this.rev = 1;
      if (faults.touchFileOnOpen) fs.appendFileSync(path.join(this.dir, file), ' ');
      if (faults.lostEditOnReopen) this.growScene();
      return this.evidence(ctx, { savedFile: file });
    }
    if (phase === 'component-enter') return this.evidence('component', { component: { context: COMPONENT, layerUid: 'comp-layer' } });
    throw new Error('unknown phase ' + phase);
  }
}

// An owned-window capture the styling check can judge: dark rows, one text line (or two), optional light buttons.
function stylingPng({ light = false, wrapped = false } = {}) {
  const image = canvas(810, 710, [30, 30, 34, 255]);
  if (light) fillRect(image, 20, 100, 300, 200, [236, 236, 236, 255]);
  fillRect(image, 40, 112, 120, 8, [205, 205, 205, 255]);
  if (wrapped) fillRect(image, 40, 126, 90, 8, [205, 205, 205, 255]);
  return { png: encodeRGBA(image.width, image.height, image.rgba), rects: { layerRows: { x: 20, y: 100, w: 300, h: 200 }, longName: { x: 30, y: 106, w: 200, h: 34 } } };
}
function frameSession({ directory, world, styling = stylingPng(), buttons = true }) {
  return {
    start: async () => identity, identity: async () => ({ ...identity }), cleanup: async () => {},
    runtimeContext: async () => ({ identity: { ...identity }, snapshot: {}, window: { windowId: 7 } }),
    requestEvidence: async request => world.operator(request, styling),
    input: async (command, option) => {
      if (command === 'capture') { fs.writeFileSync(path.join(directory, option), styling.png); return { status: 'ok', capture: option }; }
      if (command === 'observe') {
        const labels = buttons ? ['Hide Layer 1 (layer-1)', 'Lock Layer 1', 'Mark shy Layer 1'] : [];
        return { status: 'ok', nodes: labels.map(l => ({ role: 'AXButton', labels: [l], identifiers: { resolved: '' } })) };
      }
      throw new Error('unexpected helper command ' + command);
    },
  };
}

// ---- settings dialog model ----------------------------------------------------------------------
const TABS = ['General', 'Updates', 'Collaboration', 'Feedback', 'Figma', 'Shortcuts', 'Labs', 'About'];
const HINTS = { Animator: 'Edits their own layers.', Supervisor: 'Edits any layer directly.', Producer: 'Read-only plus comments.' };
class SettingsApp {
  constructor(reportDirectory, faults = {}) {
    this.dir = reportDirectory; this.faults = faults;
    this.s = { project: false, menu: false, open: false, tab: 'General', name: 'Animateur 3I41', email: '', role: 'Animator', tool: 'hand', toast: '',
      sync: '', loading: 0, keys: { insertFrame: 'F5', play: 'Enter' }, colour: [0x4a, 0x9e, 0xff], alpha: 1, originals: {}, panel: false, closed: 0 };
    this.log = [];
  }
  node(id, role, extra = {}) { return { role, identifier: '', identifiers: { resolved: id }, labels: [], ...extra }; }
  nodes() {
    const s = this.s, out = [this.node('app-menu-btn', 'AXButton', { labels: ['Menu'] })];
    if (s.project) out.push(this.node('project-tabs-settings', 'AXButton'));
    const expose = !this.faults.noToolState;
    out.push(this.node('', 'AXButton', { labels: ['Hand — pan the canvas (H or Space)'], ...(expose ? { selected: s.tool === 'hand' } : {}) }));
    out.push(this.node('', 'AXButton', { labels: ['Brush (B)'], ...(expose ? { selected: s.tool === 'brush' } : {}) }));
    if (s.toast) out.push({ role: 'AXStaticText', identifier: '', identifiers: { resolved: '' }, labels: [s.toast] });
    if (!s.open) return out;
    out.push(this.node('settings-close', 'AXButton'));
    for (const tab of TABS) out.push(this.node('', 'AXButton', { labels: [tab] }));
    const pane = (this.faults.staticTabs ? 'pane' : 'pane:' + s.tab);
    out.push({ role: 'AXStaticText', identifier: '', identifiers: { resolved: '' }, labels: [pane] });
    if (s.tab === 'General') {
      out.push(this.node('profile-name', 'AXTextField', { value: s.name }), this.node('profile-email', 'AXTextField', { value: s.email }),
        this.node('profile-role', 'AXPopUpButton', { value: s.role }), this.node('profile-role-hint', 'AXStaticText', { labels: [HINTS[s.role]] }),
        this.node('profile-color-well', 'AXGroup'));
    } else if (s.tab === 'Collaboration') {
      out.push(this.node('sync-folder-path', 'AXTextField', { value: s.sync }));
      if (s.loading > 0) out.push({ role: 'AXStaticText', identifier: '', identifiers: { resolved: '' }, labels: ['Recherche…'] });
    } else if (s.tab === 'Shortcuts') {
      for (const [action, key] of Object.entries(s.keys)) out.push(this.node('', 'AXButton', { labels: [action, key] }));
    }
    return out;
  }
  png(name) {
    const s = this.s, image = canvas(300, 200, [40, 40, 44, 255]);
    const mix = s.colour.map((v, i) => Math.round(v * s.alpha + [40, 40, 44][i] * (1 - s.alpha)));
    fillRect(image, 50, 40, 24, 24, [...mix, 255]);
    fs.writeFileSync(path.join(this.dir, name), encodeRGBA(image.width, image.height, image.rgba));
  }
  async input(command, option, value) {
    const s = this.s; this.log.push(command);
    if (s.loading > 0) s.loading--;
    s.toast = command === 'tool.brush' ? s.toast : '';
    const ok = { status: 'ok', command };
    switch (command) {
      case 'observe': return { ...ok, nodes: this.nodes() };
      case 'capture': this.png(option); return { ...ok, capture: option };
      case 'settings.menu.open': s.menu = true; return ok;
      case 'settings.menu.entry': if (!s.menu) throw new Error('menu not open'); s.menu = false; s.open = true; return ok;
      case 'settings.open': if (!s.project) throw new Error('no project'); s.open = true; return ok;
      case 'settings.close': case 'settings.backdrop':
        if (command === 'settings.backdrop' && this.faults.backdropInert) return ok;
        if (this.faults.closeInert && command === 'settings.close') return ok;
        s.open = false; return ok;
      case 'settings.tab': s.tab = option; return ok;
      case 'profile.name': case 'profile.email': case 'profile.role': {
        const key = command.slice(8);
        if (s.originals[key] === undefined) s.originals[key] = s[key];
        s[key] = key === 'name' && value === '' ? 'Animateur 9Z9Z' : value; return ok;
      }
      case 'profile.name.restore': case 'profile.email.restore': case 'profile.role.restore': {
        const key = command.slice(8, -8); s[key] = s.originals[key]; return ok;
      }
      case 'tool.brush':
        if (s.role === 'Producer' && !this.faults.noGuard) s.toast = 'Profil "Producteur" : lecture seule + commentaires';
        else s.tool = 'brush';
        return ok;
      case 'sync.open-cancel': case 'sync.panel-observe': case 'sync.cancel-only': case 'sync.complete': return ok;
      case 'sync.check-empty': s.loading = this.faults.stuckSearch ? 99 : 1; return ok;
      default: throw new Error('unexpected helper command ' + command);
    }
  }
  // Closing and reopening Settings re-reads the stored profile; a faulty build keeps the original instead.
  afterClose() { if (this.faults.dropProfile) { this.s.name = this.s.originals.name ?? this.s.name; this.s.email = this.s.originals.email ?? this.s.email; } }
  async operator(request) {
    const s = this.s, phase = request.phase, faults = this.faults;
    if (faults.failureAt === phase) return { observation: 'broken', productFailure: true };
    if (faults.malformedAt === phase) return { observation: 'ok', productFailure: false, rects: {} };
    if (phase === 'window-placement') return { observation: 'placed', productFailure: false };
    if (phase === 'new-project') { s.project = true; return { observation: 'created', productFailure: false }; }
    if (phase === 'shortcut-rebind') { s.keys.insertFrame = 'F8'; return { observation: 'rebound', productFailure: false }; }
    if (phase === 'shortcut-reset') { if (!faults.resetBroken) s.keys.insertFrame = 'F5'; return { observation: 'reset', productFailure: false }; }
    if (phase === 'profile-color-measure') return { observation: 'measured', productFailure: false, rects: { well: { x: 50, y: 40, w: 24, h: 24 }, backdrop: { x: 20, y: 20 } } };
    if (phase === 'profile-color-opaque') { s.colour = [0x33, 0x66, 0x99]; s.alpha = 1; return { observation: 'set', productFailure: false }; }
    if (phase === 'profile-color-alpha') { if (!faults.alphaIgnored) s.alpha = 0.5; return { observation: 'set', productFailure: false }; }
    if (phase === 'profile-color-restore') { if (!faults.noRestore) { s.colour = [0x4a, 0x9e, 0xff]; s.alpha = 1; } return { observation: 'restored', productFailure: false }; }
    throw new Error('unknown phase ' + phase);
  }
}
function settingsSession({ app }) {
  const wrapped = {
    start: async () => identity, identity: async () => ({ ...identity }), cleanup: async () => {},
    runtimeContext: async () => ({ identity: { ...identity }, snapshot: {}, window: { windowId: 7 } }),
    requestEvidence: request => app.operator(request),
    input: async (command, option, value) => {
      const result = await app.input(command, option, value);
      if (command === 'settings.close' || command === 'settings.backdrop') app.afterClose();
      return result;
    },
  };
  return wrapped;
}
module.exports = { identity, World, frameSession, stylingPng, SettingsApp, settingsSession, clone };
