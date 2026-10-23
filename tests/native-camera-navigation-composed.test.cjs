'use strict';

// Intended destination: tests/native-camera-navigation-composed.test.cjs.
// Source base: cdd7f6a388cc17ad4b8da62d9cb59d498d6ff09e.
// Characterizes actual JS composition against FIXED SUPPLIED native responses.
// It does not execute Rust, native Camera sampling, pixels, installed AX or input.
// Full camera.js and the actual saved UI/File.bind entry run. Only project.js
// Open helpers, goToFrame and the exact btn-nf registration are
// extracted from app.js/timeline.js, following the existing navigation test seam.
// Operations.install publishes the actual routed cutover through a test surface;
// unrelated global writer wrappers/full opacity bootstrap are outside this test.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const { extractFunction } = require('./fixtures/lib/sandbox.cjs');
const Project = require('../src/js/adapters/native-object-project.js');
const Desktop = require('../src/js/adapters/native-object-project-desktop.js');
const Files = require('../src/js/adapters/native-object-project-file.js');
const SavedUI = require('../src/js/adapters/project-native-save.js');
const ProjectDocument = require('../src/js/project-document.js');
const SaveWorkflow = require('../src/js/application/native-object-save-as-workflow.js');
const SavedAcknowledgement = require('../src/js/application/native-object-saved-acknowledgement.js');
const Underlay = require('../src/js/adapters/native-viewport-underlay.js');
const Motion = require('../src/js/adapters/native-opacity-motion-surface.js');
const Operations = require('../src/js/application/native-opacity-operations.js');
const LegacySurface = require('../src/js/adapters/native-opacity-legacy-surface.js');
const Contract = require('../src/js/application/native-opacity-contract.js');
const Viewport = require('../src/js/application/native-opacity-viewport.js');
const V1 = require('../src/js/application/native-opacity-v1.js');

const read = file => fs.readFileSync(path.join(__dirname, '../src/js', file), 'utf8');
const plain = value => JSON.parse(JSON.stringify(value));
const forbidden = label => () => assert.fail('Unexpected legacy/persistent operation: ' + label);
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function element(id) {
  const listeners = new Map();
  const attributes = new Map();
  const registrations = [];
  const invocations = [];
  return {
    id, hidden: false, value: '', listeners, registrations, invocations,
    addEventListener(type, listener, options) {
      const entries = listeners.get(type) || [];
      // Browser listener identity semantics, preserving DISTINCT registrations.
      if (entries.some(entry => entry.listener === listener)) return;
      entries.push({ listener, once: !!(options && options.once) });
      listeners.set(type, entries);
      registrations.push(type);
    },
    removeEventListener(type, listener) {
      listeners.set(type, (listeners.get(type) || []).filter(entry => entry.listener !== listener));
    },
    setAttribute(name, value) { attributes.set(name, String(value)); },
    getAttribute(name) { return attributes.has(name) ? attributes.get(name) : null; },
    dispatch(type) {
      const event = { type, target: this, currentTarget: this,
        preventDefault() {}, stopPropagation() {} };
      return (listeners.get(type) || []).slice().map(entry => {
        invocations.push(type);
        if (entry.once) this.removeEventListener(type, entry.listener);
        return entry.listener.call(this, event);
      });
    },
    count(type) { return (listeners.get(type) || []).length; }
  };
}

// Literal independent response fixtures. Intermediate matrices are deliberately
// supplied host boundary data, not an oracle for native interpolation at 1..3.
const CANVAS = {
  mode: 'canvas', cameraAvailable: true, navigation: { zoom: 1, pan: [0, 0] },
  documentToComposition: [1, 0, 0, 1, 0, 0],
  compositionToDocument: [1, 0, 0, 1, 0, 0]
};
const LOCKED_ZERO = {
  mode: 'stored-camera', cameraAvailable: true, navigation: { zoom: 1, pan: [0, 0] },
  documentToComposition: [1, 0, 0, 1, 0, 0],
  compositionToDocument: [1, 0, 0, 1, 0, 0]
};
const LOCKED_FOUR = {
  mode: 'stored-camera', cameraAvailable: true, navigation: { zoom: 1, pan: [0, 0] },
  documentToComposition: [2, 0, 0, 2, -160, -90],
  compositionToDocument: [.5, 0, 0, .5, 80, 45]
};
const PREVIEWS = [
  { frame: 0, view: CANVAS },
  { frame: 0, view: CANVAS }, // actual openPath reveal requests a second preview
  { frame: 0, view: LOCKED_ZERO },
  { frame: 1, view: LOCKED_ZERO },
  { frame: 2, view: LOCKED_ZERO },
  { frame: 3, view: LOCKED_ZERO },
  { frame: 4, view: LOCKED_FOUR }
];

const UNAVAILABLE_PREVIEWS = [
  { frame: 0, view: { ...CANVAS, cameraAvailable: false } },
  { frame: 0, view: { ...CANVAS, cameraAvailable: false } }
];
const ABSENT_CAMERA_PREVIEWS = [{ frame: 0 }, { frame: 0 }];

function fixture(previewFixtures = PREVIEWS) {
  const calls = [], snapshots = [], notices = [], publication = [];
  const classes = new Set(), styles = new Map();
  const ids = ['btn-camera', 'btn-cam-addkey', 'btn-cam-ease-edit', 'btn-nf', 'tl-cf', 'drawing-canvas', 'start-screen'];
  const nodes = Object.fromEntries(ids.map(id => [id, element(id)]));
  nodes['drawing-canvas'].getBoundingClientRect = () => ({ left: 66, top: 102, width: 800, height: 450 });
  const screenClasses = new Set();
  nodes['start-screen'].classList = {
    contains: key => screenClasses.has(key), add: key => screenClasses.add(key),
    remove: key => screenClasses.delete(key)
  };
  const document = element('document');
  document.readyState = 'loading';
  document.getElementById = id => nodes[id] || null;
  document.documentElement = {
    clientWidth: 1200, clientHeight: 900,
    classList: { contains: key => classes.has(key), add: key => classes.add(key), remove: key => classes.delete(key) },
    style: {
      getPropertyValue: key => styles.get(key) || '', getPropertyPriority: () => '',
      setProperty: (key, value) => styles.set(key, value), removeProperty: key => styles.delete(key)
    }
  };
  const context = vm.createContext({});
  const root = vm.runInContext('globalThis', context);
  Object.assign(root, element('window'));
  const state = { currentFrame: 0, totalFrames: 21, playing: false,
    canvasW: 320, canvasH: 180, cameraView: false, cameraKeys: [], cameraLayerOn: false };
  let installed = false, previewIndex = 0, policyWait = null, navigationWait = null;
  const pin = {
    apiVersion: 2, instanceId: 'composed-camera-instance', documentId: 'composed-camera-document',
    lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 'composed-camera-snapshot',
    documentDigest: 'a'.repeat(64), byteCount: 4269,
    canvasW: 320, canvasH: 180, fps: 24, totalFrames: 21,
    layers: [{ layerUid: 'drawing', visible: true }], reference: null
  };
  const view = Object.freeze({ update() {} }); // allowed presentation-only backdrop refresh
  const paper = new Proxy(Object.freeze({ view }), {
    get(target, key) {
      assert.equal(key, 'view', 'native route must not reach Paper document APIs');
      return target.view;
    },
    set: forbidden('Paper write')
  });
  Object.assign(root, {
    state, document, devicePixelRatio: 2, stageLayer: { visible: true }, paper,
    NemoNativeObjectProject: Project, NemoNativeObjectProjectDesktop: Desktop,
    NemoNativeObjectProjectFile: Files, NemoNativeViewportUnderlay: Underlay,
    NemoNativeObjectSaveAsWorkflow: SaveWorkflow,
    NemoNativeObjectSavedAcknowledgement: SavedAcknowledgement,
    SMProjectDocument: ProjectDocument,
    requestAnimationFrame: callback => setImmediate(callback),
    NemoNativeOpacityMotionSurface: Motion,
    SM: { t: key => key, setTool: forbidden('legacy camera tool'), importJSON: forbidden('legacy import') },
    SMReference: { refreshNative() {} }, SMProject: { refreshActiveTabDirtyDot() {} },
    NemoNativeObjectUI: {
      // Inspector/row DOM painter is a boundary; the actual Project.paintUI and
      // Desktop.didPaintUI chain still reaches full camera.js's lock hook.
      render(paintedRoot, info) {
        assert.strictEqual(paintedRoot, root);
        assert.equal(info.contentRevision, 0);
        return true;
      },
      refreshPolicy() {
        if (policyWait) { const waiter = policyWait; policyWait = null; waiter.resolve(); }
      }
    },
    updateUI(frameOnly) {
      // Retain the production native paint chain; omit unrelated inspector DOM.
      if (session.isActive()) assert.equal(Project.paintUI(root, frameOnly), true);
      publication.push({ frameOnly, frame: state.currentFrame, cameraView: state.cameraView });
      if (navigationWait) { const waiter = navigationWait; navigationWait = null; waiter.resolve(); }
    },
    showToast(message) {
      notices.push(message);
      if (navigationWait) { navigationWait.reject(new Error(message)); navigationWait = null; }
    },
    pushUndo: forbidden('camera undo'), renderLayerList: forbidden('legacy layer list'),
    renderTimeline: forbidden('legacy timeline'), saveAllLayerFrames: forbidden('save Paper frames'),
    loadFrame: forbidden('load Paper frame'), clearSel: forbidden('legacy selection'),
    renderOS: forbidden('legacy onion'), renderArcs: forbidden('legacy arcs'),
    stopPlay: forbidden('unexpected playback stop')
  });
  root.window = root;
  async function invoke(command, args) {
    const request = args && (args.request || JSON.parse(args.requestJson));
    calls.push({ command, request: request && plain(request) });
    if (command === 'nemo_native_object_client_status') return { ...pin, available: installed };
    if (command === 'nemo_native_object_bootstrap') {
      assert.equal(request.sourcePath, '/Camera-view.json');
      assert.equal(request.projectOnly, true);
      installed = true;
      return { ...pin, requestId: request.requestId, resourceCount: 0, viewportAvailable: false };
    }
    if (command === 'nemo_native_object_reference') {
      assert.deepEqual(request.action, { kind: 'inspect' });
      return { ...pin, requestId: request.requestId };
    }
    assert.equal(command, 'nemo_native_object_preview', 'no native writer/query fallback');
    const expected = previewFixtures[previewIndex];
    assert.ok(expected, 'no additional preview requests');
    assert.equal(request.frame, expected.frame);
    assert.equal(request.viewMode || 'canvas', expected.view ? expected.view.mode : 'canvas');
    for (const key of ['instanceId', 'documentId', 'contentRevision', 'documentSnapshotId', 'lifecycleGeneration']) {
      assert.equal(request[key], pin[key]);
    }
    assert.equal(Object.hasOwn(request, 'navigation'), false);
    const response = { status: 'presented', workId: 'composed-camera-work-' + previewIndex,
      viewGeneration: 1 };
    if (expected.view !== undefined) response.view = plain(expected.view);
    previewIndex++;
    return response;
  }
  root.__TAURI__ = { core: { invoke }, event: { listen: async () => () => {} },
    dialog: { ask: forbidden('unexpected replacement confirmation'), save: forbidden('save picker') },
    fs: { readTextFile: forbidden('legacy project byte loader') }
  };
  // Subscription transport is a fake boundary; production createPorts performs
  // registration and its actual active-binding checks. No mutation is allowed.
  root.NemoApplicationMcpTransport = {
    createNativeTauriTransport() {
      let connected = false, binding = null;
      return {
        async connect() { connected = true; },
        async subscribeRevisions() { binding = { ...pin, subscriptionId: 'composed-camera-subscription' }; return binding; },
        status: () => connected ? binding : null, isSynchronizing: () => false,
        async disconnect() { connected = false; },
        dispatchObject: forbidden('subscription mutation')
      };
    }
  };
  const session = Project.desktop(root); // invokes ACTUAL Desktop.createPorts
  function waitForPolicyPaint() {
    assert.equal(policyWait, null);
    policyWait = deferred();
    return policyWait.promise;
  }
  function record(label) {
    const shown = session.readPresentedObjectSession();
    snapshots.push({ label, frame: state.currentFrame, cameraView: state.cameraView,
      cameraAvailable: shown && shown.presentation.view.cameraAvailable,
      aria: nodes['btn-camera'].getAttribute('aria-pressed'),
      listenerCount: nodes['btn-camera'].count('click'),
      presentation: shown && plain(shown.presentation) });
  }
  return { root, context, state, session, pin, nodes, document, calls, snapshots, publication, notices,
    waitForPolicyPaint, record, previewCount: () => previewIndex,
    waitForNavigationPaint() {
      assert.equal(navigationWait, null); navigationWait = deferred(); return navigationWait.promise;
    }
  };
}

function installOrdinaryOpen(f, context) {
  const { root, state, nodes } = f;
  const effects = [], diagnostics = [], cameraObservations = [];
  const meta = { tabId: null, path: null, name: 'Untitled' };
  // These are the lower project.js tab/recents/start-screen DOM boundaries.
  // Actual SavedUI.bindProjectUI -> Files.bind -> File.create/enter owns Open,
  // confirmation, occlusion, projection, reveal/RAF, commit and repaint order.
  root.nativeSavedUI = SavedUI.bindProjectUI(root, {
    read: () => ({ ...meta, canvasW: state.canvasW, canvasH: state.canvasH, fps: state.fps }),
    show() { effects.push('show'); nodes['start-screen'].classList.remove('hid'); },
    hide() { effects.push('hide'); nodes['start-screen'].classList.add('hid'); },
    touchRecent(file, name, info, strict) {
      assert.equal(file, '/Camera-view.json'); assert.equal(name, 'Camera-view');
      assert.equal(info.contentRevision, 0); assert.equal(strict, true); effects.push('recent');
    },
    commit(file, name) { meta.path = file; meta.name = name; effects.push('commit'); },
    renderRecents() { effects.push('recents'); },
    opened() { meta.tabId = 'camera-tab'; effects.push('opened'); },
    repaint() { effects.push('repaint'); },
    notice(message) { f.notices.push(message); },
    serialize: forbidden('legacy serialization'), prepare: forbidden('legacy save preparation'),
    markSaved: forbidden('legacy saved marker'), remember: forbidden('legacy autosave'),
    isDirty: () => true, invalidate: forbidden('unexpected save invalidation')
  });
  root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__ = (event, code) => {
    (event === 'camera-control' || event === 'camera-presented' ? cameraObservations : diagnostics).push([event, code]);
  };
  root.nativeInactiveTabs = { openPath: forbidden('inactive project branch') };
  root.inactiveEntry = { openPath: forbidden('inactive opacity branch') };
  const project = read('project.js');
  vm.runInContext('var activeTabId = null;\n' +
    extractFunction(project, 'p39bUseInactiveEntry') + '\n' +
    extractFunction(project, 'tauriOk') + '\nasync ' + extractFunction(project, 'openPath'),
  context, { filename: 'src/js/project.js:ordinary-openPath' });
  root.SMProject.openPath = root.openPath;
  return { effects, diagnostics, cameraObservations, meta };
}

function installActualCutoverRouter(root, session) {
  // Exercise Operations' actual objectProject()/routed() publication, not a
  // hand-written facade that forwards Next directly to a Project method.
  // Unrelated legacy lifecycle execution and global writer wrappers are excluded.
  const lifecycle = new Proxy({}, { get: (_target, key) => forbidden('opacity lifecycle ' + String(key)) });
  const surface = {
    installGuard() {}, wrap() { return () => {}; },
    publish(_admission, cutover) { root.NemoNativeOpacityCutover = cutover; return () => {}; }
  };
  const desktopPorts = LegacySurface.desktopPorts(root, {});
  assert.strictEqual(desktopPorts.objectProject(), session);
  Operations.create(lifecycle, { ...desktopPorts, surface }, Contract, Viewport, V1).install();
  assert.notEqual(root.NemoNativeOpacityCutover, session);
  assert.strictEqual(root.NemoNativeOpacityCutover.identity(), session.info());
}

test('composed native Camera initialization keeps one lock through actual Next navigation to frame four',
  { timeout: 3000 }, async t => {
    const f = fixture(), { root, session, state, nodes } = f;
    const context = f.context;
    vm.runInContext(read('camera.js'), context, { filename: 'src/js/camera.js' });
    assert.equal(f.document.count('DOMContentLoaded'), 1);
    assert.equal(nodes['btn-camera'].count('click'), 0);
    f.document.readyState = 'interactive';
    f.document.dispatch('DOMContentLoaded');
    assert.equal(nodes['btn-camera'].count('click'), 1);
    assert.equal(nodes['btn-cam-addkey'].count('click'), 1);
    assert.equal(nodes['btn-cam-ease-edit'].count('click'), 1);

    const opened = installOrdinaryOpen(f, context);
    const openingPaint = f.waitForPolicyPaint();
    assert.equal(await root.SMProject.openPath('/Camera-view.json'), true);
    await openingPaint; // actual production presented callback, no timing sleep
    assert.deepEqual(opened.effects, ['show', 'hide', 'recent', 'commit', 'recents', 'opened', 'repaint']);
    assert.equal(opened.meta.path, '/Camera-view.json');
    assert.equal(nodes['start-screen'].classList.contains('hid'), true);
    assert.deepEqual(opened.diagnostics.filter(([event]) => event === 'file-stage').map(([, code]) => code),
      ['projection', 'ui-refresh', 'hide-start', 'layout-wait', 'layout-settled']);
    assert.equal(opened.diagnostics.at(-1)[0], 'file-completed');
    const originalInfo = session.info();
    assert.equal(Files.projectionReadyForUI(root, originalInfo), true);
    assert.equal(f.previewCount(), 2);
    assert.equal(nodes['btn-camera'].hidden, false);
    assert.equal(nodes['btn-camera'].getAttribute('aria-pressed'), 'false');
    f.record('opened-canvas-zero');
    assert.deepEqual(opened.cameraObservations.filter(([event]) => event === 'camera-control'), []);
    assert.deepEqual(opened.cameraObservations.at(-1), ['camera-presented', 'canvas']);

    // Once ordinary Open has projected UI, legacy persistent camera writes fail.
    for (const key of ['cameraKeys', 'cameraLayerOn']) {
      const retained = state[key];
      Object.freeze(retained);
      Object.defineProperty(state, key, { get: () => retained, set: forbidden('state.' + key) });
    }
    installActualCutoverRouter(root, session);
    const app = read('app.js'), timeline = read('timeline.js');
    vm.runInContext('var _nativeFrameNavigator = null;\n' + extractFunction(app, 'goToFrame'), context,
      { filename: 'src/js/app.js:goToFrame' });
    const nextLines = timeline.split('\n').filter(line =>
      line.startsWith("document.getElementById('btn-nf').addEventListener('click',"));
    assert.equal(nextLines.length, 1, 'extract exactly the shipped Next registration');
    vm.runInContext(nextLines[0], context, { filename: 'src/js/timeline.js:btn-nf' });
    assert.equal(nodes['btn-nf'].count('click'), 1);

    const lockPaint = f.waitForPolicyPaint();
    assert.equal(session.readPresentedObjectSession().presentation.view.cameraAvailable, true);
    const lockResults = nodes['btn-camera'].dispatch('click');
    assert.equal(lockResults.length, 1);
    assert.equal(state.cameraView, true);
    assert.equal(nodes['btn-camera'].getAttribute('aria-pressed'), 'true');
    await Promise.all(lockResults);
    await lockPaint;
    assert.equal(nodes['btn-camera'].hidden, false);
    f.record('locked-zero');
    // Selection witnesses the completed synchronous assignment path; presented
    // reports correlated completion mode, without proving native pixels or frame.
    assert.deepEqual(opened.cameraObservations.filter(([event]) => event === 'camera-control'),
      [['camera-control', 'selected-stored-camera']]);
    assert.deepEqual(opened.cameraObservations.at(-1), ['camera-presented', 'stored-camera']);
    for (let frame = 1; frame <= 4; frame++) {
      const navigationPaint = f.waitForNavigationPaint(), policyPaint = f.waitForPolicyPaint();
      nodes['btn-nf'].dispatch('click'); // actual listener -> goToFrame -> cutover -> Project
      await navigationPaint;
      await policyPaint;
      assert.equal(state.currentFrame, frame);
      assert.equal(root._curFrame, frame);
      assert.equal(Number(nodes['tl-cf'].value), frame + 1);
      assert.equal(state.cameraView, true);
      assert.equal(nodes['btn-camera'].getAttribute('aria-pressed'), 'true');
      assert.equal(nodes['btn-camera'].count('click'), 1);
      assert.strictEqual(session.info(), originalInfo);
      assert.equal(session.info().contentRevision, 0);
      const shown = session.readPresentedObjectSession();
      assert.ok(shown);
      assert.equal(shown.presentation.frame, frame);
      assert.equal(shown.presentation.view.mode, 'stored-camera');
      f.record('locked-next-' + frame);
      assert.deepEqual(opened.cameraObservations.at(-1), ['camera-presented', 'stored-camera']);
    }
    const final = session.readPresentedObjectSession().presentation;
    assert.deepEqual(plain(final.view.documentToComposition), [2, 0, 0, 2, -160, -90]);
    assert.deepEqual(plain(final.view.compositionToDocument), [.5, 0, 0, .5, 80, 45]);
    assert.equal(f.previewCount(), PREVIEWS.length);
    assert.equal(nodes['btn-camera'].invocations.filter(kind => kind === 'click').length, 1);
    assert.equal(nodes['btn-nf'].invocations.filter(kind => kind === 'click').length, 4);
    assert.deepEqual(f.calls.filter(call => call.command === 'nemo_native_object_preview')
      .map(call => [call.request.frame, call.request.viewMode || 'canvas']),
    [[0, 'canvas'], [0, 'canvas'], [0, 'stored-camera'], [1, 'stored-camera'],
      [2, 'stored-camera'], [3, 'stored-camera'], [4, 'stored-camera']]);
    assert.deepEqual(f.notices, ['Opened: Camera-view']);
    t.diagnostic(JSON.stringify({ scope: 'JS composition, fixed native replies only',
      snapshots: f.snapshots, uiPublication: f.publication }));
  });


for (const [label, replies] of [['false', UNAVAILABLE_PREVIEWS], ['absent', ABSENT_CAMERA_PREVIEWS]]) {
  test('composed ordinary Open refuses Camera when availability is ' + label,
    { timeout: 3000 }, async () => {
      const f = fixture(replies), { root, session, state, nodes } = f;
      const context = f.context;
      vm.runInContext(read('camera.js'), context, { filename: 'src/js/camera.js' });
      f.document.readyState = 'interactive'; f.document.dispatch('DOMContentLoaded');
      const opened = installOrdinaryOpen(f, context), painted = f.waitForPolicyPaint();
      assert.equal(await root.SMProject.openPath('/Camera-view.json'), true);
      await painted;
      assert.equal(opened.diagnostics.at(-1)[0], 'file-completed');
      const before = session.readPresentedObjectSession();
      assert.ok(before);
      assert.notEqual(before.presentation.view.cameraAvailable, true);
      assert.equal(nodes['btn-camera'].hidden, true);
      assert.equal(nodes['btn-camera'].getAttribute('aria-pressed'), 'false');
      const calls = f.calls.length, completed = opened.cameraObservations.slice();
      // Programmatically dispatch to test the actual handler's refusal too;
      // a real hidden DOM control is not asserted to be user-clickable.
      assert.deepEqual(nodes['btn-camera'].dispatch('click'), [undefined]);
      assert.equal(nodes['btn-camera'].count('click'), 1);
      assert.equal(state.cameraView, false);
      assert.equal(nodes['btn-camera'].getAttribute('aria-pressed'), 'false');
      assert.equal(f.calls.length, calls, 'refusal must request no preview or mutation');
      assert.deepEqual(opened.cameraObservations, [...completed, ['camera-control', 'native-refused']]);
      assert.equal(f.previewCount(), 2);
      assert.strictEqual(session.info().contentRevision, 0);
      assert.deepEqual(plain(session.readPresentedObjectSession()), plain(before));
      assert.deepEqual(f.notices, ['Opened: Camera-view']);
    });
}
