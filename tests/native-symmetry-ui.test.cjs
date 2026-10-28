'use strict';
const ProjectFile = require('../src/js/adapters/native-object-project-file.js');
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), vm = require('node:vm');
const Contract = require('../src/js/adapters/native-symmetry-contract.js');
const clone = value => JSON.parse(JSON.stringify(value));
function node() {
  const listeners = new Map();
  return {
    addEventListener(kind, action) { if (!listeners.has(kind)) listeners.set(kind, []); listeners.get(kind).push(action); },
    removeEventListener(kind, action) { listeners.set(kind, (listeners.get(kind) || []).filter(a => a !== action)); },
    fire(kind, extra = {}) {
      const event = { button: 0, stopped: false, stopImmediatePropagation() { this.stopped = true; }, preventDefault() {}, ...extra };
      for (const action of [...(listeners.get(kind) || [])]) { action(event); if (event.stopped) break; }
      return event;
    }
  };
}
function filesInHtml() {
  const wanted = ['symmetry-bridge.js', 'adapters/native-symmetry-contract.js', 'adapters/native-application.js',
    'adapters/native-symmetry-desktop.js', 'adapters/native-symmetry-ui.js'];
  const html = fs.readFileSync(require.resolve('../src/index.html'), 'utf8');
  const actual = [...html.matchAll(/<script\b[^>]*\bsrc="js\/([^"\n]+)"[^>]*>/g)].map(m => m[1]).filter(f => wanted.includes(f));
  assert.deepEqual(actual, wanted, 'production order loads the contract before the SDK and the model/UI last');
  return actual;
}
// The native side mirrors object_project_symmetry_{edit,read}.rs for a 320x180 canvas.
function axisFor(mode) {
  const r = Math.min(320, 180) * 0.4;
  return mode === 'x' ? { x1: 0, y1: 90, x2: 320, y2: 90 } : mode === 'free' ? { x1: 160 - r, y1: 90 - r, x2: 160 + r, y2: 90 + r }
    : { x1: 160, y1: 0, x2: 160, y2: 180 };
}
function effective(m) {
  const mode = m.symmetryMode || 'y';
  return { enabled: m.symmetryEnabled || false, mode, axis: m.symmetryAxis || axisFor(mode),
    radialCenter: m.symmetryRadialCenter || { x: 160, y: 90 }, radialSectors: m.symmetryRadialSectors || 6,
    extend: m.symmetryExtend === undefined ? true : m.symmetryExtend };
}
function apply(m, a) {
  const reset = mode => { if (mode === 'radial') m.symmetryRadialCenter = { x: 160, y: 90 }; else m.symmetryAxis = axisFor(mode); };
  if (a.kind === 'setEnabled') {
    m.symmetryEnabled = a.enabled;
    if (a.enabled && !m.symmetryAxis) m.symmetryAxis = axisFor(m.symmetryMode || 'y');
    if (a.enabled && !m.symmetryRadialCenter) m.symmetryRadialCenter = { x: 160, y: 90 };
  }
  else if (a.kind === 'setMode') { reset(a.mode); m.symmetryMode = a.mode; }
  else if (a.kind === 'resetGuide') reset(m.symmetryMode);
  else if (a.kind === 'setAxis') m.symmetryAxis = { x1: a.x1, y1: a.y1, x2: a.x2, y2: a.y2 };
  else if (a.kind === 'setRadialCenter') m.symmetryRadialCenter = { x: a.x, y: a.y };
  else if (a.kind === 'setSectors') m.symmetryRadialSectors = Math.max(2, Math.min(24, a.sectors));
  else if (a.kind === 'setExtend') m.symmetryExtend = a.extend;
  else throw Error('unexpected symmetry intent');
}
function fixture(options = {}) {
  const area = node(), events = node();
  let info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 's0',
    canvasW: 320, canvasH: 180, fps: 24, totalFrames: 40, layers: [{ layerUid: 'drawing', visible: true }] };
  const metadata = clone(options.metadata || {});
  let revision = 0, serial = 0, active = true, refuse = false, refuseReads = false, staleEdits = 0, block = null, running = 0, tail = Promise.resolve(), controller, context;
  let renders = 0;
  const calls = [], notices = [];
  const projected = pin => ({ ...pin, presentation: { ...pin, owner: 'native', status: 'presented' } });
  async function dispatch(request) {
    calls.push(clone(request));
    if (block && request.operation === Contract.READ) { const held = block; block = null; held.entered(); await held.wait; }
    const response = { apiVersion: 2, requestId: request.requestId, instanceId: info.instanceId,
      documentId: info.documentId, contentRevision: revision, ok: true };
    if (staleEdits && request.operation === Contract.EDIT) { staleEdits--; response.ok = false; response.error = { code: 'stale_revision', message: 'Moved' }; }
    else if (refuse && request.operation === Contract.EDIT || refuseReads && request.operation === Contract.READ) { response.ok = false; response.error = { code: 'unavailable', message: 'Unavailable' }; }
    else if (request.operation === Contract.EDIT) {
      assert.equal(request.expectedRevision, revision);
      const before = JSON.stringify(metadata); apply(metadata, request.payload.action);
      const applied = before !== JSON.stringify(metadata); if (applied) revision++;
      response.contentRevision = revision; response.result = { applied, historyEntriesAdded: Number(applied) };
    } else {
      assert.equal(request.operation, Contract.READ); assert.equal(request.payload.atRevision, revision);
      response.result = { instanceId: info.instanceId, documentId: info.documentId, atRevision: revision,
        documentSnapshotId: 's' + revision, contextId: 'scene-root', result: { effective: effective(metadata), ...clone(metadata) } };
    }
    return vm.runInContext('JSON.parse(' + JSON.stringify(JSON.stringify(response)) + ')', context);
  }
  const session = { info: () => info, isActive: () => active, requestId: () => 'symmetry-' + (++serial),
    readSubscriptionGeneration: () => 1, readPresentedObjectSession: () => active && !running ? projected(info) : null,
    runResourceCommand(action) {
      running++;
      const work = tail.then(() => { const origin = info;
        return action({ dispatch: q => root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('ui', { dispatch }).dispatch(q),
          readPresentedObjectSession: () => active && running === 1 && info === origin ? projected(origin) : null,
          async reconcile(next) {
            assert.equal(next, revision); info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision };
            controller.willProject(info); assert.equal(ProjectFile.refreshLayerTable(browserRoot, info), true);
            controller.presented(info); return info;
          } });
      }).finally(() => { running--; controller.presented(info); });
      tail = work.catch(() => {}); return work;
    }
  };
  const state = { tool: 'brush', playing: false, symmetryEnabled: false, symmetryMode: 'y', symmetryAxis: null,
    symmetryRadialCenter: null, symmetryRadialSectors: 6, symmetryExtend: true, canvasW: 320, canvasH: 180 };
  const document = { readyState: 'complete', getElementById: id => id === 'canvas-area' ? area : null, addEventListener() {} };
  const root = { document, state, view: { zoom: 1 }, Point: function (x, y) { this.x = x; this.y = y; },
    __TAURI__: options.browser ? undefined : { core: {} }, setTimeout, clearTimeout,
    addEventListener: events.addEventListener.bind(events), removeEventListener: events.removeEventListener.bind(events),
    NemoNativeObjectProject: { desktop: () => session },
    NemoNativeObjectProjectFile: { projectionReadyForUI: (r, value) => active && !running && ProjectFile.projectionReadyForUI(r, value) },
    showToast: text => notices.push(text), renderLabsFloatPanel() {},
    // renderEdge: a repaint asks native for a preview, whose presentation calls presented() again (settingsNotice).
    SMEngineBridge: { isEnabled: () => true, screenToWorld: (x, y) => [x, y], suspend() {}, resume() {},
      renderNow() { renders++; if (options.renderEdge && renders < 200) setImmediate(() => controller.presented(info)); } } };
  root.globalThis = root; root.window = root; context = vm.createContext(root);
  const browserRoot = vm.runInContext('globalThis', context);
  ProjectFile.projectView(browserRoot, info);
  for (const file of filesInHtml()) vm.runInContext(fs.readFileSync(require.resolve('../src/js/' + file), 'utf8'), context, { filename: file });
  controller = root.NemoNativeSymmetryController;
  async function settled() {
    for (let i = 0; i < 30; i++) { await tail; await new Promise(resolve => setImmediate(resolve)); if (!controller.isPending()) return; }
    throw Error('symmetry controller did not settle');
  }
  const edits = () => calls.filter(c => c.operation === Contract.EDIT).map(c => c.payload.action);
  const shown = () => ({ enabled: state.symmetryEnabled, mode: state.symmetryMode, axis: clone(state.symmetryAxis),
    radialCenter: clone(state.symmetryRadialCenter), radialSectors: state.symmetryRadialSectors, extend: state.symmetryExtend });
  // Another writer (an undo, say) changes the document while this UI is busy.
  function external(change) {
    change(metadata); revision++;
    info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision };
    controller.willProject(info); assert.equal(ProjectFile.refreshLayerTable(browserRoot, info), true); controller.presented(info);
  }
  function holdNextRead() {
    let entered, release; const wait = new Promise(r => { release = r; }), started = new Promise(r => { entered = r; });
    block = { entered, wait }; return { started, release };
  }
  const run = code => vm.runInContext(code, context);
  function replaceDocument() {
    info = { ...info, documentId: 'd2', lifecycleGeneration: 2 };
    controller.willProject(info); ProjectFile.projectView(browserRoot, info);
  }
  return { root, state, area, controller, calls, notices, metadata, settled, edits, shown, external, holdNextRead, run,
    refuse: v => { refuse = v; }, refuseReads: v => { refuseReads = v; }, replaceDocument, staleEdits: n => { staleEdits = n; },
    renders: () => renders };
}

test('a confirmed native read projects the original import values into the existing guide fields', async () => {
  const f = fixture({ metadata: { symmetryEnabled: true, symmetryMode: '', symmetryRadialSectors: 0, symmetryExtend: false } });
  await f.controller.presented(); await f.settled();
  assert.deepEqual(f.shown(), { enabled: true, mode: 'y', axis: { x1: 160, y1: 0, x2: 160, y2: 180 },
    radialCenter: { x: 160, y: 90 }, radialSectors: 6, extend: false });
  assert.deepEqual(f.edits(), [], 'reading never writes');
});

test('original mode, reset and guide drag gestures each send one native edit and show only confirmed values', async () => {
  const f = fixture({ metadata: { symmetryEnabled: true } });
  await f.controller.presented(); await f.settled();
  f.root.setSymmetryMode('radial'); await f.settled();
  assert.equal(f.state.symmetryMode, 'radial'); assert.deepEqual(clone(f.state.symmetryRadialCenter), { x: 160, y: 90 });
  f.area.fire('pointerdown', { clientX: 160, clientY: 90 });
  f.area.fire('pointermove', { clientX: 40, clientY: 30 });
  assert.deepEqual(clone(f.state.symmetryRadialCenter), { x: 40, y: 30 }, 'the drag moves only the presented guide');
  assert.equal(f.edits().length, 1, 'no edit until release');
  f.area.fire('pointerup', { clientX: 40, clientY: 30 }); await f.settled();
  f.root.resetSymmetryGuide(); await f.settled();
  assert.deepEqual(f.edits(), [{ kind: 'setMode', mode: 'radial' }, { kind: 'setRadialCenter', x: 40, y: 30 }, { kind: 'resetGuide' }]);
  assert.deepEqual(f.shown(), effective(f.metadata));
  f.root.setSymmetryMode('free'); await f.settled();
  f.area.fire('pointerdown', { clientX: 160 + 72, clientY: 90 + 72 });
  f.area.fire('pointermove', { clientX: 300, clientY: 170 });
  f.area.fire('pointerup', { clientX: 300, clientY: 170 }); await f.settled();
  assert.deepEqual(f.edits().slice(3), [{ kind: 'setMode', mode: 'free' }, { kind: 'setAxis', x1: 88, y1: 18, x2: 300, y2: 170 }]);
  assert.deepEqual(f.shown(), effective(f.metadata));
  assert.deepEqual(f.notices, []);
});

test('a refused edit restores the guide to native truth and warns once', async () => {
  const f = fixture({ metadata: { symmetryEnabled: true, symmetryMode: 'radial' } });
  await f.controller.presented(); await f.settled();
  const before = f.shown();
  f.refuse(true);
  f.area.fire('pointerdown', { clientX: 160, clientY: 90 });
  f.area.fire('pointermove', { clientX: 10, clientY: 10 });
  f.refuseReads(true);
  f.area.fire('pointerup', { clientX: 10, clientY: 10 }); await f.settled();
  assert.equal(f.notices.length, 1);
  assert.deepEqual(f.shown(), before, 'the controller re-reads native truth on its own');
  assert.deepEqual(f.metadata, { symmetryEnabled: true, symmetryMode: 'radial' }, 'nothing was stored');
});

test('without a native host the original writers keep their local behaviour and send nothing', async () => {
  const f = fixture({ browser: true });
  f.root.setSymmetryMode('x');
  assert.equal(f.state.symmetryMode, 'x'); assert.deepEqual(clone(f.state.symmetryAxis), { x1: 0, y1: 90, x2: 320, y2: 90 });
  assert.equal(f.controller.route({ kind: 'resetGuide' }), false);
  assert.deepEqual(f.calls, []);
});

test('a gesture made while a native read is loading is applied after it, never dropped', async () => {
  const f = fixture({ metadata: { symmetryEnabled: true } });
  await f.controller.presented(); await f.settled();
  const gate = f.holdNextRead();
  f.external(m => { m.symmetryExtend = false; });
  await gate.started;
  f.root.setSymmetryMode('x');
  assert.equal(f.state.symmetryMode, 'x', 'the original local effect is immediate');
  gate.release(); await f.settled(); await f.controller.whenSettled(); await f.settled();
  assert.deepEqual(f.edits(), [{ kind: 'setMode', mode: 'x' }]);
  assert.deepEqual(f.shown(), effective(f.metadata));
  assert.equal(f.metadata.symmetryExtend, false, 'the other writer\'s change survives');
  assert.deepEqual(f.notices, []);
});

test('rapid sector steps keep the local value and reach native in order', async () => {
  const f = fixture({ metadata: { symmetryEnabled: true, symmetryMode: 'radial' } });
  await f.controller.presented(); await f.settled();
  // The Labs port's own body (labs-float-panel.js setSectors), run in the page realm.
  for (const value of [5, 4, 3]) f.run(`state.symmetryRadialSectors = Math.max(2, Math.min(24, ${value}));
    NemoNativeSymmetryController.route({ kind: 'setSectors', sectors: state.symmetryRadialSectors });`);
  assert.equal(f.state.symmetryRadialSectors, 3);
  await f.controller.whenSettled(); await f.settled();
  assert.deepEqual(f.edits(), [{ kind: 'setSectors', sectors: 5 }, { kind: 'setSectors', sectors: 3 }], 'queued steps coalesce');
  assert.equal(f.metadata.symmetryRadialSectors, 3); assert.equal(f.state.symmetryRadialSectors, 3);
});

test('another writer\'s revision during a guide drag neither moves the dragged guide nor loses the release', async () => {
  const f = fixture({ metadata: { symmetryEnabled: true, symmetryMode: 'radial' } });
  await f.controller.presented(); await f.settled();
  f.area.fire('pointerdown', { clientX: 160, clientY: 90 });
  f.area.fire('pointermove', { clientX: 50, clientY: 60 });
  f.external(m => { m.symmetryRadialSectors = 12; }); await f.settled();
  assert.deepEqual(clone(f.state.symmetryRadialCenter), { x: 50, y: 60 }, 'the held guide is not overwritten');
  f.area.fire('pointermove', { clientX: 70, clientY: 80 });
  f.area.fire('pointerup', { clientX: 70, clientY: 80 }); await f.controller.whenSettled(); await f.settled();
  assert.deepEqual(f.edits(), [{ kind: 'setRadialCenter', x: 70, y: 80 }]);
  assert.deepEqual(f.shown(), effective(f.metadata)); assert.equal(f.state.symmetryRadialSectors, 12);
});

test('an edit native did not apply is re-read and retried once; a second refusal is surfaced', async () => {
  const f = fixture({ metadata: { symmetryEnabled: true } });
  await f.controller.presented(); await f.settled();
  f.staleEdits(1); f.root.setSymmetryMode('x'); await f.controller.whenSettled(); await f.settled();
  assert.deepEqual(f.edits(), [{ kind: 'setMode', mode: 'x' }, { kind: 'setMode', mode: 'x' }]);
  assert.equal(f.metadata.symmetryMode, 'x'); assert.deepEqual(f.notices, []);
  f.staleEdits(2); f.root.setSymmetryMode('free'); await f.controller.whenSettled(); await f.settled();
  assert.equal(f.notices.length, 1); assert.equal(f.state.symmetryMode, 'x', 'the last confirmed mode is shown again');
});

test('a drag that ends after editing stopped releases its hold, and refused intents drop what was queued behind them', async () => {
  const f = fixture({ metadata: { symmetryEnabled: true, symmetryMode: 'radial' } });
  await f.controller.presented(); await f.settled();
  f.area.fire('pointerdown', { clientX: 160, clientY: 90 });
  f.area.fire('pointermove', { clientX: 20, clientY: 20 });
  f.state.symmetryEnabled = false;
  f.area.fire('pointerup', { clientX: 20, clientY: 20 });
  f.external(m => { m.symmetryRadialSectors = 9; }); await f.settled();
  assert.equal(f.state.symmetryRadialSectors, 9, 'projection resumed after the drag');
  assert.deepEqual(f.shown(), effective(f.metadata));
  f.refuse(true);
  f.root.setSymmetryMode('x'); f.root.resetSymmetryGuide();
  await f.controller.whenSettled(); await f.settled();
  assert.deepEqual(f.edits(), [{ kind: 'setMode', mode: 'x' }], 'the queued reset never ran');
  assert.equal(f.notices.length, 1); assert.deepEqual(f.shown(), effective(f.metadata));
});

test('a confirmed read from a previous document is never shown in a later one', async () => {
  const f = fixture({ metadata: { symmetryEnabled: true, symmetryMode: 'radial' } });
  await f.controller.presented(); await f.settled();
  f.state.symmetryMode = 'y';
  f.replaceDocument(); f.refuseReads(true); await f.controller.presented(); await f.settled();
  assert.equal(f.state.symmetryMode, 'y', 'no other document\'s settings are projected');
});

// #2343 B1: every native presentation calls presented(), and a repaint asks native for another preview. Values already
// on screen must not repaint, or the queue never settles (4,582 previews in 77 s, installed).
test('an unchanged confirmed value requests no further native frame; a real change and a refusal each repaint once', async () => {
  const turns = async n => { for (let i = 0; i < n; i++) await new Promise(resolve => setImmediate(resolve)); };
  const f = fixture({ renderEdge: true, metadata: { symmetryEnabled: true } });
  await f.controller.presented(); await f.settled(); await turns(40);
  assert.equal(f.renders(), 1, 'the first confirmed projection paints once');
  for (let i = 0; i < 20; i++) { f.controller.presented(); await turns(2); }
  assert.equal(f.renders(), 1, 'presentations of unchanged values request nothing');
  f.external(m => { m.symmetryExtend = false; }); await f.settled(); await turns(40);
  assert.equal(f.renders(), 2, 'a changed confirmed value repaints once, then settles');
  assert.equal(f.shown().extend, false);
  // The original writer applies its local effect, then routes it (labs-floating-ui / symmetry-bridge).
  f.refuse(true); f.state.symmetryExtend = true; f.run('NemoNativeSymmetryController').route({ kind: 'setExtend', extend: true });
  await f.settled(); await turns(40);
  assert.equal(f.shown().extend, false, 'the refused intent is restored to native truth');
  assert.ok(f.renders() >= 3 && f.renders() <= 4, 'the restoration repaints and settles: ' + f.renders());
  const settledAt = f.renders(); await turns(40); assert.equal(f.renders(), settledAt);
});
