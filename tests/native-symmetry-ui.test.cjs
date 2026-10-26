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
  if (a.kind === 'setEnabled') m.symmetryEnabled = a.enabled;
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
  let revision = 0, serial = 0, active = true, refuse = false, running = 0, tail = Promise.resolve(), controller, context;
  const calls = [], notices = [];
  const projected = pin => ({ ...pin, presentation: { ...pin, owner: 'native', status: 'presented' } });
  async function dispatch(request) {
    calls.push(clone(request));
    const response = { apiVersion: 2, requestId: request.requestId, instanceId: info.instanceId,
      documentId: info.documentId, contentRevision: revision, ok: true };
    if (refuse && request.operation === Contract.EDIT) { response.ok = false; response.error = { code: 'unavailable', message: 'Unavailable' }; }
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
    SMEngineBridge: { isEnabled: () => true, screenToWorld: (x, y) => [x, y], suspend() {}, resume() {}, renderNow() {} } };
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
  return { root, state, area, controller, calls, notices, metadata, settled, edits, shown, refuse: v => { refuse = v; } };
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
