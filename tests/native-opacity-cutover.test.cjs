'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const ProjectDocument = require('../src/js/project-document.js');
const OpacityApplication = require('../src/js/application/opacity-application.js');
const NativeApplication = require('../src/js/adapters/native-application.js');
const NativeEditor = require('../src/js/adapters/native-opacity-editor.js');
const NativeSelection = require('../src/js/adapters/native-opacity-selection.js');
const NativePreview = require('../src/js/adapters/native-opacity-preview.js');
const NativeExport = require('../src/js/adapters/native-opacity-export.js');

const ROOT = path.resolve(__dirname, '..');
const SHELL_PATH = path.join(ROOT, 'tests/animation/fixtures/curve-workflow.json');
const NATIVE_PATH = path.join(ROOT, 'native-engine/tests/fixtures/opacity-v2/project.json');
const BOOTSTRAP_PATH = path.join(ROOT, 'src/js/bootstrap/opacity-application.js');
const SHELL_SHA = 'dceb05d13576a4dda0eb1a1a9d8c0184e8617e9a3a2662150ee54f4badedf08d';
const NATIVE_SHA = '895ca05a43295104301c472287149b1c01aa6ef681db80dcbb79e58a55b36050';

function bytes(file) { return fs.readFileSync(file); }
function json(file) { return JSON.parse(bytes(file)); }
function sha(file) { return crypto.createHash('sha256').update(bytes(file)).digest('hex'); }
function clone(value) { return structuredClone(value); }

function staticSource(value = 25) {
  const source = json(SHELL_PATH);
  source.layers[0].motionStatic = { opacity: [value] };
  return source;
}

function keyedSource() {
  const source = staticSource();
  source.layers[0].motion.opacity = json(NATIVE_PATH).layers[0].motion.opacity;
  return source;
}

function serializeResponse(prepared, document, identity = {
  instanceId: 'instance-a', documentId: 'native-document-1', contentRevision: 0,
}) {
  return {
    apiVersion: 2,
    requestId: 'serialize-1',
    instanceId: identity.instanceId,
    documentId: identity.documentId,
    contentRevision: identity.contentRevision,
    ok: true,
    result: {
      atRevision: identity.contentRevision,
      documentSnapshotId: `native-opacity:${identity.documentId}:${identity.contentRevision}`,
      document,
    },
  };
}

function nativeHarness(source, options = {}) {
  const prepared = ProjectDocument.prepareNativeOpacity(source);
  const state = {
    identity: { instanceId: 'instance-a', documentId: 'native-document-1', contentRevision: 0 },
    document: clone(prepared.projection), history: [], redo: [], releases: 0,
    disconnects: 0, imports: [], previews: [], outputs: [], evaluations: 0,
  };
  function response(request, result) {
    return { apiVersion: 2, requestId: request.requestId, instanceId: state.identity.instanceId,
      documentId: state.identity.documentId, contentRevision: state.identity.contentRevision,
      ok: true, result };
  }
  const transport = {
    async dispatch(request) {
      const operation = request.operation;
      if (operation === 'query.document.serialize') return response(request, {
        atRevision: state.identity.contentRevision,
        documentSnapshotId: `native-opacity:${state.identity.documentId}:${state.identity.contentRevision}`,
        document: clone(state.document),
      });
      if (operation === 'query.document.evaluate') {
        state.evaluations++;
        const keyed = state.document.layers[0].motion;
        const characterized = request.payload.frame === 0 ? 20
          : request.payload.frame === 10 ? 50 : request.payload.frame === 20 ? 80
            : state.document.layers[0].motionStatic.opacity[0];
        const value = options.badEvaluation && request.payload.frame === 10
          ? 99 : keyed ? characterized : state.document.layers[0].motionStatic.opacity[0];
        return response(request, {
          documentSnapshotId: `native-opacity:${state.identity.documentId}:${state.identity.contentRevision}`,
          documentId: state.identity.documentId, contentRevision: state.identity.contentRevision,
          contextId: request.payload.contextId, frame: request.payload.frame,
          layers: [{ layerUid: 'r08_curve_layer', value }],
        });
      }
      if (operation === 'command.document.apply') {
        if (options.mutationGate) await options.mutationGate;
        state.history.push(state.document.layers[0].motionStatic.opacity[0]); state.redo.length = 0;
        state.document.layers[0].motionStatic.opacity = [request.payload.value];
        state.identity.contentRevision++;
        return response(request, { applied: true, historyEntriesAdded: 1 });
      }
      if (operation === 'history.undo' || operation === 'history.redo') {
        const from = operation === 'history.undo' ? state.history : state.redo;
        const to = operation === 'history.undo' ? state.redo : state.history;
        const prior = from.pop();
        if (prior !== undefined) {
          to.push(state.document.layers[0].motionStatic.opacity[0]);
          state.document.layers[0].motionStatic.opacity = [prior];
        }
        state.identity.contentRevision++;
        return response(request, { applied: prior !== undefined, historyEntriesAdded: 0 });
      }
      if (operation === 'query.document.snapshot.acquire') return response(request, {
        atRevision: state.identity.contentRevision,
        documentSnapshotId: `native-opacity:${state.identity.documentId}:${state.identity.contentRevision}`,
      });
      if (operation === 'job.export.png.begin') return response(request, {
        jobId: 'native-job-1', status: 'succeeded', pinnedRevision: request.expectedRevision,
        documentSnapshotId: `native-opacity:${state.identity.documentId}:${state.identity.contentRevision}`,
        progress: 1, artifact: { target: request.payload.outputHandle,
          files: request.payload.frames.map((frame) => `frame-${frame.sourceFrame}.png`) },
        cleanup: { status: 'complete' }, externalEffectDisposition: 'committed',
      });
      throw new Error(`unexpected native operation ${operation}`);
    },
  };
  const application = NativeApplication.createNativeApplicationAdapter('n20-test', transport);
  const controller = OpacityApplication.createNative({
    document: ProjectDocument, editor: NativeEditor, selection: NativeSelection,
    preview: NativePreview.createNativeOpacityPreviewAdapter(),
    exporter: NativeExport.createNativeOpacityExportAdapter(),
    async bootstrap(value) {
      assert.strictEqual(value, prepared);
      if (options.bootstrapGate) await options.bootstrapGate;
      return { apiVersion: 2, instanceId: state.identity.instanceId,
        documentId: state.identity.documentId, contentRevision: state.identity.contentRevision,
        resourceCount: prepared.resources.length, viewportAvailable: true };
    },
    async connect() { return clone(state.identity); }, application() { return application; },
    disconnect() { state.disconnects++; },
    async release(request) {
      state.releases++;
      if (options.releaseFailure) throw new Error('release failed');
      return { apiVersion: 2, instanceId: request.instanceId, documentId: request.documentId,
        contentRevision: request.expectedRevision, lifecycleGeneration: state.releases,
        status: 'succeeded', authorityRemovalCompleted: true, reentryAvailable: true, error: null };
    },
    legacyImport(bytes) { state.imports.push(JSON.parse(bytes)); return true; },
    async previewHost(request) {
      state.previews.push(request);
      return { workId: `preview-${state.previews.length}`, viewGeneration: state.previews.length,
        status: 'presented' };
    },
    async bindOutput(request) { state.outputs.push(request); },
    currentFrame() { return 10; }, afterChange() {}, sleep() { return Promise.resolve(); },
  });
  return { prepared, state, controller };
}

test('N20 characterization is pinned to the two approved fixture bytes', () => {
  assert.equal(sha(SHELL_PATH), SHELL_SHA);
  assert.equal(sha(NATIVE_PATH), NATIVE_SHA);
});

test('static admission retains one frozen legacy shell and projects 21 immutable geometry resources', () => {
  const source = staticSource();
  const admitted = ProjectDocument.prepareNativeOpacity(source);
  assert.equal(admitted.opacityMode, 'static');
  assert.deepEqual(admitted.projection, {
    format: 'nemo.native-opacity-document', formatVersion: 1, totalFrames: 21,
    layers: [{ layerUid: 'r08_curve_layer', motionStatic: { opacity: [25] } }],
  });
  assert.equal(admitted.resources.length, 21);
  assert.equal(admitted.frames.length, 21);
  for (const frame of [0, 10, 20]) {
    const resource = admitted.resources[frame];
    const layer = resource.layers[0];
    const translated = [
      layer.bounds[0] + layer.transform[4], layer.bounds[1] + layer.transform[5],
      layer.bounds[2] + layer.transform[4], layer.bounds[3] + layer.transform[5],
    ];
    assert.deepEqual(translated, frame === 0 ? [20, 60, 40, 80]
      : frame === 10 ? [84, 60, 104, 80] : [148, 60, 168, 80]);
    assert.deepEqual(admitted.frames[frame], {
      sourceFrame: frame,
      geometryHandle: { resourceId: resource.resourceId, resourceVersion: resource.resourceVersion },
    });
  }
  assert.equal(Object.isFrozen(admitted), true);
  assert.equal(Object.isFrozen(admitted.shell.layers[0].frames), true);
  assert.notStrictEqual(admitted.shell, source);
  assert.throws(() => { admitted.shell.layers[0].name = 'changed'; }, TypeError);
  assert.equal(source.layers[0].name, 'R08 rectangle');
});

test('keyed admission overlays only the approved opacity oracle and preserves the legacy position track', () => {
  const source = keyedSource();
  const admitted = ProjectDocument.prepareNativeOpacity(JSON.stringify(source));
  assert.equal(admitted.opacityMode, 'keyed');
  assert.deepEqual(admitted.projection, json(NATIVE_PATH));
  assert.deepEqual(admitted.shell.layers[0].motion.position, json(SHELL_PATH).layers[0].motion.position);
  assert.deepEqual(admitted.shell.layers[0].motion.opacity.keys.map((key) => [key.frame, key.v[0]]), [[0, 20], [20, 80]]);
});

test('admission rejects the complete unsupported source before transfer instead of stripping it', () => {
  const mutations = [
    ['unknown root field', (d) => { d.extra = true; }],
    ['non-JSON unknown root field', (d) => { d.extra = undefined; }],
    ['unknown layer field', (d) => { d.layers[0].extra = true; }],
    ['expression', (d) => { d.layers[0].expressions = { opacity: { enabled: true, code: '50' } }; }],
    ['effect', (d) => { d.layers[0].effects = []; }],
    ['nested content', (d) => { d.layers[0].symbolId = 'nested'; }],
    ['media', (d) => { d.mediaLibrary = []; }],
    ['text', (d) => { d.layers[0].isTextLayer = true; }],
    ['mask', (d) => { d.layers[0].matteMode = 'alpha'; }],
    ['second layer', (d) => { d.layers.push(clone(d.layers[0])); }],
    ['unbounded id', (d) => { d.layers[0].layerUid = 'x'.repeat(129); }],
    ['wrong canvas', (d) => { d.canvasW = 321; }],
    ['wrong semantic frames', (d) => { d.totalFrames = 120; }],
    ['wrong stored slots', (d) => { d.layers[0].frames.length = 21; }],
    ['changed geometry', (d) => { d.layers[0].frames[0].strokes[0].segments[0].point[0] = 21; }],
    ['changed paint', (d) => { d.layers[0].frames[0].strokes[0].fillColor = '#00ff00'; }],
    ['incompatible position', (d) => { d.layers[0].motion.position.keys[1].v[0] = 127; }],
    ['invalid static opacity', (d) => { d.layers[0].motionStatic.opacity = [101]; }],
  ];
  for (const [label, mutate] of mutations) {
    const source = staticSource();
    mutate(source);
    assert.throws(() => ProjectDocument.prepareNativeOpacity(source), /unsupported|invalid|exact|bounded|opacity/i, label);
  }

  for (const mutate of [
    (d) => { d.layers[0].motion.opacity.keys[0].curvePoints[1].y = 0.2; },
    (d) => { d.layers[0].motion.opacity.keys[0].hOut = [1, 0]; },
    (d) => { d.layers[0].motion.opacity.keys.push(clone(d.layers[0].motion.opacity.keys[1])); },
    (d) => { d.layers[0].motion.opacity.keys[1].v = [79]; },
  ]) {
    const source = keyedSource();
    mutate(source);
    assert.throws(() => ProjectDocument.prepareNativeOpacity(source), /unsupported|invalid|exact|opacity/i);
  }
});

test('persistence composes only a revision-matched native opacity overlay onto the frozen shell', () => {
  const admitted = ProjectDocument.prepareNativeOpacity(staticSource());
  const native = clone(admitted.projection);
  native.layers[0].motionStatic.opacity = [40];
  const identity = { instanceId: 'instance-a', documentId: 'native-document-1', contentRevision: 2 };
  const response = serializeResponse(admitted, native, identity);
  const composed = ProjectDocument.composeNativeOpacity(admitted, response, identity);
  assert.equal(composed.layers[0].motionStatic.opacity[0], 40);
  assert.deepEqual(composed.layers[0].motion.position, admitted.shell.layers[0].motion.position);
  assert.deepEqual(composed.layers[0].frames, admitted.shell.layers[0].frames);
  assert.equal(Object.isFrozen(composed), true);
  assert.deepEqual(admitted.shell.layers[0].motionStatic.opacity, [25]);

  for (const mutate of [
    (r) => { r.instanceId = 'other'; },
    (r) => { r.documentId = 'other'; },
    (r) => { r.contentRevision = 3; },
    (r) => { r.result.atRevision = 3; },
    (r) => { r.result.document.layers[0].layerUid = 'other'; },
    (r) => { r.result.extra = true; },
    (r) => { r.result.document.extra = true; },
  ]) {
    const bad = clone(response); mutate(bad);
    assert.throws(() => ProjectDocument.composeNativeOpacity(admitted, bad, identity), /identity|revision|unknown|exact|unsupported/i);
  }
  const tooLarge = clone(response);
  tooLarge.padding = 'x'.repeat(4096);
  assert.throws(() => ProjectDocument.composeNativeOpacity(admitted, tooLarge, identity), /4096|unknown/i);
});

test('native controller atomically owns evaluation, mutation, history, preview, export, persistence, and release', async () => {
  const harness = nativeHarness(staticSource());
  assert.equal(await harness.controller.activate(harness.prepared), true);
  assert.equal(harness.controller.status(), 'native');
  assert.deepEqual(harness.controller.valueAtFrame('r08_curve_layer', 10), [25]);
  assert.equal(harness.state.evaluations, 21);
  assert.equal(JSON.parse(harness.controller.persistenceJSON()).layers[0].motionStatic.opacity[0], 25);

  await harness.controller.setOpacity('r08_curve_layer', 40);
  let current = harness.controller.identity();
  const v1 = await harness.controller.handleV1({ apiVersion: 1, requestId: 'v1-set',
    instanceId: current.instanceId, documentId: current.documentId,
    expectedRevision: current.contentRevision, operation: 'property.set',
    payload: { layerId: 'r08_curve_layer', property: 'opacity', value: 60, frame: 10 } });
  assert.equal(v1.ok, true);
  await harness.controller.history('undo');
  assert.deepEqual(harness.controller.valueAtFrame('r08_curve_layer', 10), [40]);
  assert.equal(JSON.parse(harness.controller.persistenceJSON()).layers[0].motionStatic.opacity[0], 40);

  assert.equal(harness.controller.renderPreview(10), true);
  await harness.controller.flush();
  assert.equal(harness.state.previews.length, 1);
  const selection = harness.controller.projectSelection({ activeLayerUid: 'r08_curve_layer',
    selected: [{ layerUid: 'r08_curve_layer', opacityMode: 'static' }] }, 10);
  assert.equal(selection.selected[0].value, 40);
  const artifact = await harness.controller.exportPng('/tmp/n20-output', [0, 10, 20]);
  assert.equal(artifact.status, 'succeeded');
  assert.equal(harness.state.outputs.length, 1);

  const released = await harness.controller.release({ kind: 'replacement' });
  assert.deepEqual(released, { documentId: 'native-document-1', generation: 1,
    owner: 'legacy', status: 'released' });
  assert.equal(harness.controller.status(), 'legacy');
  assert.equal(harness.state.releases, 1);
  assert.equal(harness.state.disconnects, 1);
  assert.equal(harness.state.imports[0].layers[0].motionStatic.opacity[0], 40);
});

test('activation mismatch rolls host ownership back without exposing a writable mirror', async () => {
  const harness = nativeHarness(staticSource(), { badEvaluation: true });
  assert.equal(await harness.controller.activate(harness.prepared), false);
  assert.equal(harness.controller.status(), 'legacy');
  assert.equal(harness.state.releases, 1);
  assert.equal(harness.state.disconnects, 1);
  assert.equal(harness.state.imports.length, 0);
  assert.equal(harness.controller.persistenceJSON(), null);
});

test('failed release is fail-closed and never reimports the cached shell', async () => {
  const harness = nativeHarness(staticSource(), { releaseFailure: true });
  assert.equal(await harness.controller.activate(harness.prepared), true);
  await assert.rejects(harness.controller.release({ kind: 'replacement' }), /release failed/);
  assert.equal(harness.controller.status(), 'indeterminate');
  assert.equal(harness.controller.blocksLegacy(), true);
  assert.equal(harness.state.imports.length, 0);
  assert.throws(() => harness.controller.getNativeIdentity(), /safely observable/);
});

test('persistence fails closed while an authoritative mutation is in flight', async () => {
  let finishMutation;
  const gate = new Promise((resolve) => { finishMutation = resolve; });
  const harness = nativeHarness(staticSource(), { mutationGate: gate });
  assert.equal(await harness.controller.activate(harness.prepared), true);
  const mutation = harness.controller.setOpacity('r08_curve_layer', 40);
  assert.equal(harness.controller.persistenceJSON(), null, 'a stale pre-mutation shell cannot be saved');
  finishMutation();
  await mutation;
  assert.equal(JSON.parse(harness.controller.persistenceJSON()).layers[0].motionStatic.opacity[0], 40);
});

test('project replacement waits for installation before one terminal release', async () => {
  let finishBootstrap;
  const gate = new Promise((resolve) => { finishBootstrap = resolve; });
  const harness = nativeHarness(staticSource(), { bootstrapGate: gate });
  const activation = harness.controller.activate(harness.prepared);
  assert.equal(harness.controller.status(), 'installing');
  assert.deepEqual(harness.controller.handleV1({ requestId: 'during-install', instanceId: 'legacy-instance',
    documentId: 'legacy-document' }), { apiVersion: 1, requestId: 'during-install',
    instanceId: 'legacy-instance', documentId: 'legacy-document', revision: 0, ok: false,
    error: { code: 'unavailable', message: 'Native opacity ownership is not dispatchable.' } });
  const release = harness.controller.release({ kind: 'document-replacement' });
  finishBootstrap();
  assert.equal(await activation, true);
  assert.deepEqual(await release, { documentId: 'native-document-1', generation: 1,
    owner: 'legacy', status: 'released' });
  assert.equal(harness.state.releases, 1);
  assert.equal(harness.controller.status(), 'legacy');
});

test('keyed opacity remains native-read-only and releases before a later legacy edit', async () => {
  const harness = nativeHarness(keyedSource());
  assert.equal(await harness.controller.activate(harness.prepared), true);
  assert.deepEqual([0, 10, 20].map((frame) => harness.controller.valueAtFrame('r08_curve_layer', frame)[0]), [20, 50, 80]);
  await assert.rejects(harness.controller.setOpacity('r08_curve_layer', 40), /read-only until release completes/);
  await harness.controller.flush();
  assert.equal(harness.controller.status(), 'legacy');
  assert.equal(harness.state.releases, 1);
  assert.deepEqual(harness.state.imports[0].layers[0].motion.opacity, keyedSource().layers[0].motion.opacity);
});

test('N20 bootstrap stays browser-inert and binds only the accepted desktop host ports', async () => {
  const source = fs.readFileSync(BOOTSTRAP_PATH, 'utf8');
  const nativeBootstrap = source.slice(source.indexOf('// N20: install the native controller'));
  let browserCreates = 0;
  const browser = { NemoOpacityApplicationCore: { createNative() { browserCreates++; } } };
  browser.window = browser;
  vm.runInNewContext(nativeBootstrap, browser, { filename: 'opacity-bootstrap-browser.js' });
  assert.equal(browserCreates, 0);

  const events = [], invokes = [], imports = [];
  let ports;
  const binding = { instanceId: 'instance-a', documentId: 'native-document-1', contentRevision: 0 };
  const controller = {
    blocksLegacy() { return false; }, isActive() { return false; }, identity() { return null; },
    async activate(prepared) { events.push(['activate', prepared]); return true; },
    async release(reason) { events.push(['release', reason]); return null; },
    requestRelease() { return Promise.resolve(null); },
    handleV1() { return null; }, legacyIntent() { return null; },
  };
  async function invoke(command, args) {
    invokes.push([command, args]);
    if (command === 'nemo_mcp_identity') return { instanceId: 'instance-a' };
    if (command === 'nemo_native_bootstrap') return { apiVersion: 2, ...binding,
      resourceCount: args.request.resources.length, viewportAvailable: true };
    return { ok: true };
  }
  const desktop = {
    console, Promise, Math, Object, Error,
    __TAURI__: { core: { invoke } }, devicePixelRatio: 2,
    setTimeout(callback) { callback(); return 1; }, clearTimeout() {},
    document: { getElementById() { return { getBoundingClientRect() {
      return { left: 1, top: 2, width: 320, height: 180 };
    } }; } },
    localStorage: { getItem() { return null; } }, addEventListener(type) { events.push(['listen', type]); },
    state: { currentFrame: 10 },
    SM: { importJSON(value, silent) { imports.push([value, silent]); return true; } },
    SMProjectDocument: { prepareNativeOpacity(value) {
      if (value === 'unsupported') throw new Error('unsupported');
      return { projection: { format: 'nemo.native-opacity-document' }, resources: [{ resourceId: 'geometry/r08/0' }] };
    } },
    NemoApplication: { handle() { return 'legacy'; }, setInstanceId() {} },
    NemoOpacityApplication: { meta() { return { instanceId: 'legacy', documentId: 'legacy', revision: 0 }; }, legacy() { return 'legacy'; } },
    NemoApplicationMcpTransport: { createNativeTauriTransport(received) {
      assert.strictEqual(received, invoke);
      return { async connect() { events.push(['connect']); return binding; }, disconnect() { events.push(['disconnect']); } };
    } },
    NemoNativeApplicationAdapter: { createNativeApplicationAdapter(label) { events.push(['application', label]); return {}; } },
    NemoNativeOpacityPreviewAdapter: { createNativeOpacityPreviewAdapter() { return { kind: 'preview' }; } },
    NemoNativeOpacityExportAdapter: { createNativeOpacityExportAdapter() { return { kind: 'export' }; } },
    NemoOpacityApplicationCore: { createNative(received) { ports = received; events.push(['create']); return controller; } },
    SMNativeEditGuard: { install(received) { assert.strictEqual(received, controller); events.push(['guard-install']); } },
  };
  desktop.window = desktop;
  vm.runInNewContext(nativeBootstrap, desktop, { filename: 'opacity-bootstrap-desktop.js' });
  assert.deepEqual(events.slice(0, 2), [['create'], ['guard-install']], 'guard installs before any authority activation');
  assert.strictEqual(desktop.NemoNativeOpacityCutover, controller);

  const receipt = await ports.bootstrap({ projection: { format: 'nemo.native-opacity-document' }, resources: [{ resourceId: 'geometry/r08/0' }] });
  assert.equal(receipt.documentId, binding.documentId);
  assert.deepEqual(invokes.slice(0, 2).map(([command]) => command), ['nemo_mcp_identity', 'nemo_native_bootstrap']);
  assert.deepEqual(JSON.parse(JSON.stringify(invokes[1][1].request.viewport)), { cssBounds: { x: 1, y: 2, width: 320, height: 180 },
    physicalExtent: { width: 640, height: 360 }, compositionExtent: { width: 320, height: 180 }, reportedDpr: 2 });
  await ports.previewHost({ workId: 'preview' });
  await ports.bindOutput({ outputHandle: 'output/one' });
  await ports.release({ requestId: 'release-one' });
  assert.deepEqual(invokes.slice(2).map(([command]) => command), ['nemo_native_preview', 'nemo_native_bind_output', 'nemo_native_release']);

  assert.equal(await desktop.NemoNativeOpacityProject.importJSON('supported', false), true);
  assert.deepEqual(events.find(([kind]) => kind === 'activate')[1].resources, [{ resourceId: 'geometry/r08/0' }]);
  assert.equal(await desktop.NemoNativeOpacityProject.importJSON('unsupported', true), true);
  assert.deepEqual(imports, [['supported', false], ['unsupported', true]]);
});
