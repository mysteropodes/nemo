'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const { defaultState, extractFunction, loadMotion } = require('./fixtures/lib/sandbox.cjs');

const ProjectDocument = require('../src/js/project-document.js');
const NativeOpacityContract = require('../src/js/application/native-opacity-contract.js');
const NativeOpacityLifecycle = require('../src/js/application/native-opacity-lifecycle.js');
const NativeOpacityReplacement = require('../src/js/application/native-opacity-replacement.js');
const NativeOpacityExportWorkflow = require('../src/js/application/native-opacity-export-workflow.js');
const NativeOpacityPreviewWorkflow = require('../src/js/application/native-opacity-preview-workflow.js');
const NativeOpacityV1 = require('../src/js/application/native-opacity-v1.js');
const NativeOpacityOperations = require('../src/js/application/native-opacity-operations.js');
const NativeOpacityViewport = require('../src/js/application/native-opacity-viewport.js');
const NativeLegacySurface = require('../src/js/adapters/native-opacity-legacy-surface.js');
const NativeProjectEntry = require('../src/js/adapters/native-opacity-project-entry.js');
const NativeMotionSurface = require('../src/js/adapters/native-opacity-motion-surface.js');
const MotionCanvasIntent = require('../src/js/adapters/motion-canvas-intent.js');
const SelectCanvasIntent = require('../src/js/adapters/select-canvas-intent.js');
const ComponentExposedProperties = require('../src/js/domain/component/exposed-properties.js');
const OpacityApplication = require('../src/js/application/opacity-application.js');
const ApplicationMcp = require('../src/js/adapters/application-mcp.js');
const NativeApplication = require('../src/js/adapters/native-application.js');
const NativeEditor = require('../src/js/adapters/native-opacity-editor.js');
const NativeSelection = require('../src/js/adapters/native-opacity-selection.js');
const NativePreview = require('../src/js/adapters/native-opacity-preview.js');
const NativeExport = require('../src/js/adapters/native-opacity-export.js');
const OpacityCapability = require('../src/js/application/opacity-capability.js');
const ExportSvgSequence = require('../src/js/adapters/export-svg-sequence.js');
const ROOT = path.resolve(__dirname, '..');
const SHELL_PATH = path.join(ROOT, 'tests/animation/fixtures/curve-workflow.json');
const NATIVE_PATH = path.join(ROOT, 'native-engine/tests/fixtures/opacity-v2/project.json');
const BOOTSTRAP_PATH = path.join(ROOT, 'src/js/bootstrap/native-opacity-application.js');
const SHELL_SHA = 'dceb05d13576a4dda0eb1a1a9d8c0184e8617e9a3a2662150ee54f4badedf08d';
const NATIVE_SHA = '895ca05a43295104301c472287149b1c01aa6ef681db80dcbb79e58a55b36050';

function bytes(file) { return fs.readFileSync(file); }
function json(file) { return JSON.parse(bytes(file)); }
function sha(file) { return crypto.createHash('sha256').update(bytes(file)).digest('hex'); }
function clone(value) { return structuredClone(value); }
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
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

function nativeHarness(source, options = {}) {
  const prepared = ProjectDocument.prepareNativeOpacity(source);
  const state = {
    identity: { instanceId: 'instance-a', documentId: 'native-document-1', contentRevision: 0 },
    document: clone(prepared.projection), history: [], redo: [], releases: 0,
    disconnects: 0, imports: [], previews: [], outputs: [], evaluations: 0,
    activations: 0, replacements: [], hostGeneration: 0, connected: false,
    subscriptions: [], previewConsumers: [], exportConsumers: [],
    dispatches: [],
  };
  function response(request, result) {
    return { apiVersion: 2, requestId: request.requestId, instanceId: state.identity.instanceId,
      documentId: state.identity.documentId, contentRevision: state.identity.contentRevision,
      ok: true, result };
  }
  const transport = {
    async dispatch(request) {
      const operation = request.operation;
      state.dispatches.push(clone(request));
      if (options.dispatchGate) await options.dispatchGate(request, state);
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
        if (prior === undefined) return { apiVersion: 2, requestId: request.requestId,
          instanceId: state.identity.instanceId, documentId: state.identity.documentId,
          contentRevision: state.identity.contentRevision, ok: false,
          error: { code: 'not_found', message: 'No history entry is available.' } };
        to.push(state.document.layers[0].motionStatic.opacity[0]);
        state.document.layers[0].motionStatic.opacity = [prior];
        state.identity.contentRevision++;
        return response(request, { applied: true, historyEntriesAdded: 0 });
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
  const transportBridge = options.transportFactory ? options.transportFactory(state, transport) : null;
  const applicationTransport = transportBridge ? transportBridge.transport : transport;
  const application = NativeApplication.createNativeApplicationAdapter('n20-test', applicationTransport);
  function fullReleaseReceipt(request, change = {}) {
    return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId,
      documentId: request.documentId, contentRevision: request.expectedRevision,
      lifecycleGeneration: state.hostGeneration, status: 'succeeded', retrieved: false,
      authorityRemovalCompleted: true, cancelledTransactionId: null, cancelledTransaction: null,
      undoDepth: state.history.length, redoDepth: state.redo.length, reconciledExports: [],
      cancelledPreviewWorkIds: [], unresolvedPreviewWorkIds: [], disposedViewportWorkIds: [],
      reconciliationStages: { transaction: 'complete', exports: 'complete', preview: 'complete' },
      viewportStatus: 'already_absent', reentryAvailable: true, error: null, ...change };
  }
  const controller = OpacityApplication.createNative({
    surface: options.surface,
    document: ProjectDocument, editor: NativeEditor, selection: NativeSelection,
    createPreview() {
      const value = NativePreview.createNativeOpacityPreviewAdapter();
      state.previewConsumers.push(value); return value;
    },
    createExporter() {
      const value = NativeExport.createNativeOpacityExportAdapter();
      state.exportConsumers.push(value); return value;
    },
    async bootstrap(value) {
      if (options.bootstrapGate) await options.bootstrapGate;
      state.activations++;
      state.hostGeneration = state.activations;
      state.identity = { instanceId: 'instance-a', documentId: `native-document-${state.activations}`, contentRevision: 0 };
      state.document = clone(value.projection); state.history.length = 0; state.redo.length = 0;
      if (options.bootstrapFailure && options.bootstrapFailure(state)) throw new Error('bootstrap reply lost');
      return { apiVersion: 2, instanceId: state.identity.instanceId,
        documentId: state.identity.documentId, contentRevision: state.identity.contentRevision,
        resourceCount: value.resources.length, viewportAvailable: true };
    },
    async connect() {
      if (transportBridge) return transportBridge.transport.connect();
      const old = state.subscriptions.at(-1); if (old) old.active = false;
      state.connected = true; return clone(state.identity);
    },
    connectionStatus() {
      return transportBridge ? transportBridge.transport.status() : state.connected ? clone(state.identity) : null;
    },
    application() { return application; },
    async subscribeRevisions(synchronize) {
      if (transportBridge) return transportBridge.transport.subscribeRevisions(transportBridge.listen, synchronize);
      const subscription = { synchronize, hostGeneration: state.hostGeneration, active: true };
      state.subscriptions.push(subscription);
      const subscribedBinding = clone(state.identity);
      if (options.revisionDuringSubscribe && state.hostGeneration > 1) {
        const fromRevision = state.identity.contentRevision;
        state.document.layers[0].motionStatic.opacity = [options.revisionDuringSubscribe];
        state.identity.contentRevision++;
        state.earlyRevision = synchronize({ instanceId: state.identity.instanceId,
          documentId: state.identity.documentId, lifecycleGeneration: state.hostGeneration,
          fromRevision, toRevision: state.identity.contentRevision, requestId: 'early-b-revision' });
      }
      return { ...subscribedBinding, lifecycleGeneration: state.hostGeneration,
        subscriptionId: `subscription-${state.hostGeneration}` };
    },
    async disconnect() {
      state.disconnects++;
      state.connected = false;
      if (transportBridge) return transportBridge.transport.disconnect();
      const current = state.subscriptions.at(-1); if (current) current.active = false;
      if (options.disconnectGate) await options.disconnectGate;
      if (options.disconnectFailure) throw new Error('disconnect failed');
      if (options.connectionStuck) state.connected = true;
    },
    async release(request) {
      state.releases++;
      if (options.releaseGate) await options.releaseGate;
      if (options.releaseFailure) throw new Error('release failed');
      return options.releaseReceipt ? options.releaseReceipt(fullReleaseReceipt(request), request) : fullReleaseReceipt(request);
    },
    async replace(request) {
      state.replacements.push(clone(request));
      if (options.replaceGate) await options.replaceGate;
      if (options.replaceFailure) throw options.replaceFailure;
      state.document = clone(request.projection);
      state.identity = { instanceId: state.identity.instanceId,
        documentId: `native-document-${state.replacements.length + 1}`, contentRevision: 0 };
      state.history.length = 0; state.redo.length = 0; state.hostGeneration++;
      const receipt = { requestId: request.requestId, retrieved: false,
        documentId: state.identity.documentId, contentRevision: 0,
        resourceCount: request.resources.length, cancelledPreviewWorkIds: [], reconciledExports: [] };
      return options.replaceReceipt ? options.replaceReceipt(receipt) : receipt;
    },
    legacyImport(bytes, silent) {
      state.imports.push(JSON.parse(bytes));
      return options.legacyImport ? options.legacyImport(bytes, silent, controller, state) : true;
    },
    capabilities() { return [OpacityCapability.DESCRIPTOR, ExportSvgSequence.DESCRIPTOR].map((descriptor) => ({
      id: descriptor.id, version: descriptor.version, handlerKey: descriptor.handlerKey,
      descriptor: clone(descriptor),
    })); },
    async previewHost(request) {
      state.previews.push(request);
      if (options.previewGate) await (typeof options.previewGate === 'function'
        ? options.previewGate(request, state) : options.previewGate);
      const receipt = { workId: `preview-${state.previews.length}`,
        viewGeneration: state.previews.length, status: 'presented' };
      return options.previewReceipt ? options.previewReceipt(receipt, request) : receipt;
    },
    async bindOutput(request) { state.outputs.push(request); if (options.outputGate) await options.outputGate; },
    currentFrame() { return options.ui ? options.ui.state.currentFrame : 10; },
    afterChange() {
      state.afterChanges = (state.afterChanges || 0) + 1;
      if (options.afterChange) options.afterChange(controller, state);
    },
    sleep() { return Promise.resolve(); },
  }, { contract: NativeOpacityContract, lifecycle: { create(ports, contract, replacement, exportWorkflow) {
    return state.lifecycle = NativeOpacityLifecycle.create(ports, contract, replacement, exportWorkflow, NativeOpacityPreviewWorkflow);
  } },
    replacement: NativeOpacityReplacement, exportWorkflow: NativeOpacityExportWorkflow,
    previewWorkflow: NativeOpacityPreviewWorkflow,
    operations: NativeOpacityOperations, v1: NativeOpacityV1,
    viewport: NativeOpacityViewport, motionSurface: NativeMotionSurface });
  async function externalOpacity(value, requestId = `external-${state.identity.contentRevision + 1}`) {
    const fromRevision = state.identity.contentRevision;
    state.document.layers[0].motionStatic.opacity = [value];
    state.identity.contentRevision++;
    if (transportBridge) return transportBridge.emit({ instanceId: state.identity.instanceId,
      documentId: state.identity.documentId, lifecycleGeneration: state.hostGeneration,
      fromRevision, toRevision: state.identity.contentRevision, requestId });
    const subscription = state.subscriptions.at(-1);
    const event = { instanceId: state.identity.instanceId, documentId: state.identity.documentId,
      lifecycleGeneration: subscription.hostGeneration, fromRevision,
      toRevision: state.identity.contentRevision, requestId };
    return subscription.synchronize(event);
  }
  return { prepared, state, controller, fullReleaseReceipt, externalOpacity, transportBridge };
}


function stateBytes(h) {
  return JSON.stringify({ observation: h.state.lifecycle.inspect(), document: h.state.document,
    history: h.state.history, redo: h.state.redo, persistence: h.controller.persistenceJSON(),
    releases: h.state.releases, disconnects: h.state.disconnects, imports: h.state.imports,
    bootstraps: h.state.activations, dispatches: h.state.dispatches, hostGeneration: h.state.hostGeneration });
}
function surfaceHarness(source, options = {}) {
  let publications, resized;
  const ui = options.ui || { state: { currentFrame: 10 }, _curFrame: 10,
    __TAURI__: { core: { invoke() {} } }, SM: { importJSON() {} } };
  ui.userLayers ||= [{ children: [] }];
  ui.SMProjectDocument = ProjectDocument;
  const projection = NativeLegacySurface.desktopPorts(ui, {}).surface;
  const surface = {
    snapshotUiProjection: projection.snapshotUiProjection,
    installUiProjection: projection.installUiProjection,
    restoreUiProjection: projection.restoreUiProjection,
    refreshUiProjection: projection.refreshUiProjection,
    paintUiProjection() { if (options.paintUiProjection) options.paintUiProjection(h.controller, h.state); },
    blockPublication() { if (options.blockPublication) options.blockPublication(h.controller, h.state); },
    installGuard() {}, allow() { return !h.controller.blocksLegacy(); },
    wrap() { return () => {}; },
    extensionOpen() { return !!options.extension; }, toast() {},
    publish(admission, cutover, project, handlers, resize) { publications = { admission, cutover, project, handlers, resize }; return () => {}; },
    defer(callback) { resized = callback; return 1; }, cancel() {},
    resize(current) { return options.resize ? options.resize(current) : Promise.reject(new Error('resize failed')); },
  };
  const h = nativeHarness(source, { ...options, ui, surface });
  h.controller.install();
  return { ...h, ui, published: () => publications, resizeCallback: () => resized };
}

test('terminal desktop UI paint does not enqueue an unverified native frame', () => {
  let paints = 0, presentations = 0;
  const ui = { state: {}, __TAURI__: { core: { invoke() {} } },
    SM: { importJSON() {} }, updateUI() { paints++; },
    SMEngineBridge: { renderNow() { presentations++; } } };
  NativeLegacySurface.desktopPorts(ui, {}).surface.paintUiProjection();
  assert.equal(paints, 1); assert.equal(presentations, 0);
});

test('host replacement receipt admits only a distinct revision-zero document', () => {
  const current = { instanceId: 'instance-a', documentId: 'native-document-1', contentRevision: 3 };
  const request = { requestId: 'replace-1' };
  const prepared = { resources: [{ resourceId: 'geometry-a' }] };
  const receipt = { requestId: 'replace-1', retrieved: false, documentId: 'native-document-2',
    contentRevision: 0, resourceCount: 1, cancelledPreviewWorkIds: [], reconciledExports: [] };
  assert.strictEqual(NativeOpacityContract.validateReplacement(receipt, current, request, prepared), receipt);
  assert.strictEqual(NativeOpacityContract.validateReplacement({ ...receipt, retrieved: true }, current, request, prepared).retrieved, true);
  for (const changed of [{ documentId: current.documentId }, { contentRevision: 1 },
    { requestId: 'wrong' }, { resourceCount: 0 }, { cancelledPreviewWorkIds: ['duplicate', 'duplicate'] }]) {
    assert.throws(() => NativeOpacityContract.validateReplacement({ ...receipt, ...changed }, current, request, prepared));
  }
});

test('N20B freezes independent fixtures and static history/keyed evaluation oracles', async () => {
  assert.equal(sha(SHELL_PATH), SHELL_SHA);
  assert.equal(sha(NATIVE_PATH), NATIVE_SHA);
  const h = nativeHarness(staticSource());
  await h.controller.activate(h.prepared);
  assert.deepEqual(h.controller.valueAtFrame('r08_curve_layer', 10), [25]);
  await h.controller.setOpacity('r08_curve_layer', 40);
  await h.controller.setOpacity('r08_curve_layer', 60);
  await h.controller.history('undo');
  assert.deepEqual(h.controller.valueAtFrame('r08_curve_layer', 10), [40]);
  const k = nativeHarness(keyedSource());
  await k.controller.activate(k.prepared);
  assert.deepEqual([0, 10, 20].map(f => k.controller.valueAtFrame('r08_curve_layer', f)[0]), [20, 50, 80]);
});

test('unsupported B preflight never changes A or B, including extension and no-A imports', async () => {
  const unsupported = staticSource(); unsupported.extraContent = { unknown: true };
  for (const active of [false, true]) for (const extension of [false, true]) {
    const h = surfaceHarness(staticSource(), { extension });
    if (active) { await h.controller.activate(h.prepared); await h.controller.setOpacity('r08_curve_layer', 40); }
    const incomingFiles = ['{', JSON.stringify(unsupported)];
    for (const incoming of incomingFiles) {
      const before = stateBytes(h), input = incoming, session = h.state.lifecycle.inspect().session;
      assert.equal(await h.published().project.importJSON(incoming, false), false);
      assert.equal(incoming, input);
      assert.equal(stateBytes(h), before);
      assert.strictEqual(h.state.lifecycle.inspect().session, session);
      await Promise.resolve(); assert.equal(stateBytes(h), before);
    }
  }
});

test('active native Open replaces A with a fresh subscribed B before UI publication', async () => {
  const h = surfaceHarness(staticSource());
  await h.controller.activate(h.prepared);
  await h.controller.setOpacity('r08_curve_layer', 40);
  const before = h.state.lifecycle.inspect();
  const oldSubscription = h.state.subscriptions.at(-1);
  const first = await h.published().project.importJSON(JSON.stringify(staticSource(60)), true, true);
  assert.equal(first.owner, 'native');
  assert.equal(h.state.replacements.length, 1);
  assert.equal(h.state.replacements[0].documentId, before.identity.documentId);
  assert.equal(h.state.replacements[0].expectedRevision, before.identity.contentRevision);
  const current = h.state.lifecycle.inspect();
  assert.equal(current.identity.documentId, 'native-document-2');
  assert.equal(current.identity.contentRevision, 0);
  assert.notStrictEqual(current.session, before.session);
  assert.ok(current.generation > before.generation);
  assert.ok(h.state.subscriptions.at(-1).hostGeneration > oldSubscription.hostGeneration);
  assert.equal(oldSubscription.active, false);
  assert.deepEqual(h.state.history, []);
  assert.equal(h.ui.state.layers, undefined, 'B remains unpublished in the UI until final reveal');
  const final = await h.published().project.finishOpenAfterReveal(first);
  assert.equal(final.status, 'presented');
  assert.equal(h.ui.state.layers[0].name, 'R08 rectangle');
  assert.deepEqual(h.controller.valueAtFrame('r08_curve_layer', 0), [60]);
});

test('replacement drains an admitted A preview before changing lifecycle phase', async () => {
  const hostGate = deferred();
  const h = nativeHarness(staticSource(), { previewGate(request) {
    if (request.documentId === 'native-document-1') return hostGate.promise;
  } });
  assert.equal(await h.controller.activate(h.prepared), true);
  const preview = h.state.lifecycle.presentPreview(10);
  for (let spin = 0; spin < 10 && !h.state.previews.length; spin++) await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.state.previews.length, 1);
  const replacing = h.state.lifecycle.replace(ProjectDocument.prepareNativeOpacity(staticSource(60)));
  assert.equal(h.state.lifecycle.inspect().phase, 'native');
  await assert.rejects(h.state.lifecycle.presentPreview(0), /not active/);
  assert.equal(h.state.replacements.length, 0);
  hostGate.resolve();
  assert.equal((await preview).status, 'presented');
  await replacing;
  assert.equal(h.state.lifecycle.inspect().phase, 'native');
  assert.equal(h.state.lifecycle.inspect().identity.documentId, 'native-document-2');
});

test('B revision delivered during subscription is synchronized before replacement returns', async () => {
  const h = nativeHarness(staticSource(), { revisionDuringSubscribe: 70 });
  assert.equal(await h.controller.activate(h.prepared), true);
  await h.state.lifecycle.replace(ProjectDocument.prepareNativeOpacity(staticSource(60)));
  await h.state.earlyRevision;
  assert.equal(h.state.lifecycle.inspect().phase, 'native');
  assert.equal(h.state.lifecycle.inspect().identity.contentRevision, 1);
  assert.deepEqual(h.controller.valueAtFrame('r08_curve_layer', 0), [70]);
});

test('stale host rejection fences A until its actual revision is reconciled', async () => {
  const refusal = Object.assign(new Error('stale native revision'), { code: 'stale_revision' });
  const h = nativeHarness(staticSource(), { replaceFailure: refusal });
  assert.equal(await h.controller.activate(h.prepared), true);
  await assert.rejects(h.state.lifecycle.replace(ProjectDocument.prepareNativeOpacity(staticSource(60))), /stale native revision/);
  assert.equal(h.state.lifecycle.inspect().phase, 'indeterminate');
});

test('busy local replacement refusal retains A frame token for later resize', async () => {
  const gate = deferred();
  const h = surfaceHarness(staticSource(), { outputGate: gate.promise, resize: async () => {} });
  const first = await h.published().project.importJSON(JSON.stringify(staticSource()), true, true);
  await h.published().project.finishOpenAfterReveal(first);
  const previews = h.state.previews.length;
  const exporting = h.state.lifecycle.exportPng('busy-output', [0]);
  for (let spin = 0; spin < 10 && !h.state.outputs.length; spin++) await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.state.outputs.length, 1);
  assert.equal(await h.published().project.importJSON(JSON.stringify(staticSource(60)), true, true), false);
  assert.equal(h.state.replacements.length, 0);
  assert.equal(h.state.lifecycle.inspect().phase, 'native');
  gate.resolve(); await exporting;
  h.published().resize(); await h.resizeCallback()();
  assert.ok(h.state.previews.length > previews);
  assert.equal(h.state.previews.at(-1).documentId, 'native-document-1');
});

test('ambiguous replacement error fences A even when its transport still appears bound', async () => {
  const h = nativeHarness(staticSource(), { replaceFailure: new Error('reply lost') });
  assert.equal(await h.controller.activate(h.prepared), true);
  await assert.rejects(h.state.lifecycle.replace(ProjectDocument.prepareNativeOpacity(staticSource(60))), /reply lost/);
  assert.equal(h.state.lifecycle.inspect().phase, 'indeterminate');
});

test('native Open hides the populated startup Paper group only after the final frame', async () => {
  const paper = { children: [{ name: 'boot artwork' }], visible: true };
  const ui = { state: {}, userLayers: [paper], __TAURI__: { core: { invoke() {} } },
    SM: { importJSON() {} } };
  const h = surfaceHarness(staticSource(), { ui });
  const first = await h.published().project.importJSON(JSON.stringify(staticSource()));
  assert.equal(paper.visible, true);
  await h.published().project.finishOpenAfterReveal(first);
  assert.equal(paper.visible, false);
  assert.equal(ui.state.layers[0].name, 'R08 rectangle');
});

test('replacement with an unpresented first frame fences B and blocks stale A publication', async () => {
  let blocked = 0;
  const h = surfaceHarness(staticSource(), {
    previewReceipt(receipt, request) {
      return request.documentId === 'native-document-2' ? { ...receipt, status: 'failed' } : receipt;
    }, blockPublication() { blocked++; },
  });
  await h.controller.activate(h.prepared);
  assert.equal(await h.published().project.importJSON(JSON.stringify(staticSource(60)), true, true), false);
  assert.equal(h.state.lifecycle.inspect().phase, 'indeterminate');
  assert.equal(blocked, 1);
  assert.equal(h.controller.blocksLegacy(), true);
});

test('replacement with a failed final frame fences B and blocks stale A publication', async () => {
  let blocked = 0, bPresentations = 0;
  const h = surfaceHarness(staticSource(), {
    previewReceipt(receipt, request) {
      return request.documentId === 'native-document-2' && ++bPresentations === 2
        ? { ...receipt, status: 'failed' } : receipt;
    }, blockPublication() { blocked++; },
  });
  await h.controller.activate(h.prepared);
  const first = await h.published().project.importJSON(JSON.stringify(staticSource(60)), true, true);
  assert.equal(first.status, 'presented');
  await assert.rejects(h.published().project.finishOpenAfterReveal(first), /receipt.status is unknown/);
  assert.equal(h.state.lifecycle.inspect().phase, 'indeterminate');
  assert.equal(blocked, 1);
  assert.equal(h.controller.blocksLegacy(), true);
});

test('first static and keyed native opens await the lifecycle-owned frame-0 presentation', async () => {
  for (const source of [staticSource(60), keyedSource()]) {
    let present;
    const gate = new Promise(resolve => { present = resolve; });
    const h = surfaceHarness(source, { previewGate: gate });
    const incoming = JSON.stringify(source);
    let settled = false;
    const opening = h.published().project.importJSON(incoming, true).then(value => {
      settled = true; return value;
    });
    while (h.state.previews.length < 1) await new Promise(resolve => setImmediate(resolve));
    assert.equal(settled, false, 'the host has not confirmed presentation');
    assert.equal(h.state.imports.length, 0, 'no legacy import');
    assert.equal(h.state.previewConsumers.length, 1, 'one lifecycle-owned preview consumer');
    present();
    const receipt = await opening;
    assert.equal(receipt.owner, 'native');
    assert.equal(receipt.status, 'presented');
    assert.equal(receipt.frame, 0);
    assert.equal(receipt.documentId, h.controller.identity().documentId);
    assert.equal(receipt.contentRevision, h.controller.identity().contentRevision);
    assert.equal(receipt.lifecycleGeneration, h.state.lifecycle.inspect().generation);
    assert.deepEqual(receipt.geometryHandle, h.prepared.frames[0].geometryHandle,
      'frame-0 presentation uses the admitted immutable geometry');
    assert.deepEqual(h.state.previews[0].geometryHandle, h.prepared.frames[0].geometryHandle);
    assert.equal(h.state.previews.length, 1);
    assert.equal(h.controller.blocksLegacy(), true);
    assert.equal(incoming, JSON.stringify(source), 'source bytes remain unchanged');
  }
});

test('explicit Open can unocclude the first native frame before publishing success', async () => {
  const source = staticSource();
  const h = surfaceHarness(source, { previewReceipt(receipt) {
    return { ...receipt, status: h.state.previews.length === 1 ? 'deferred-occluded' : 'presented' };
  } });
  const first = await h.published().project.importJSON(JSON.stringify(source), true, true);
  const root = { NemoNativeOpacityProject: h.published().project,
    NemoNativeOpacityCutover: h.published().cutover };
  assert.equal(first.status, 'deferred-occluded');
  assert.equal(NativeProjectEntry.ready(root, first), true, 'provisional receipt admits reveal only');
  assert.equal(h.controller.status(), 'native');
  let hidden = 0, shown = 0;
  assert.equal(await NativeProjectEntry.reveal(root, first, {
    hide() { hidden++; }, show() { shown++; }, repaint() { throw new Error('Paper repaint'); },
    raf(callback) { callback(); },
  }), true);
  assert.equal(hidden, 1); assert.equal(shown, 0);
  assert.deepEqual(h.state.previews.map(request => request.frame), [0, 0]);
  assert.equal(h.controller.blocksLegacy(), true);
});

test('occlusion on the second native presentation fails closed after reveal', async () => {
  const source = staticSource();
  const h = surfaceHarness(source, { previewReceipt(receipt) {
    return { ...receipt, status: 'deferred-occluded' };
  } });
  const first = await h.published().project.importJSON(JSON.stringify(source), true, true);
  const root = { NemoNativeOpacityProject: h.published().project,
    NemoNativeOpacityCutover: h.published().cutover };
  let shown = 0;
  await assert.rejects(NativeProjectEntry.reveal(root, first, {
    hide() {}, show() { shown++; }, repaint() { throw new Error('Paper repaint'); },
    raf(callback) { callback(); },
  }), /not presented/);
  assert.equal(shown, 1);
  assert.equal(h.controller.status(), 'indeterminate');
  assert.equal(h.controller.blocksLegacy(), true);
  assert.equal(h.state.imports.length, 0);
});

test('failed, deferred and malformed first presentations cannot publish an open or reenter Paper', async () => {
  for (const status of ['failed-validation', 'deferred-timeout', 'deferred-occluded', 'stale-discarded', 'malformed']) {
    const h = surfaceHarness(staticSource(), { previewReceipt(receipt) {
      return status === 'malformed' ? { ...receipt, workId: '' } : { ...receipt, status };
    } });
    assert.equal(await h.published().project.importJSON(JSON.stringify(staticSource()), true), false, status);
    assert.equal(h.controller.blocksLegacy(), true, status);
    assert.equal(h.state.imports.length, 0, status);
    assert.equal(h.state.activations, 1, status);
    assert.equal(h.state.previews.length, 1, status);
  }
});

test('close receipt retrieval is fingerprinted and retains protection after disposal and a microtask', async () => {
  const h = surfaceHarness(staticSource());
  await h.controller.activate(h.prepared);
  const identity = h.controller.getNativeIdentity();
  const reason = { ...identity, kind: 'explicit-close' };
  const close = await h.controller.release(reason);
  assert.deepEqual(close, { ...identity, owner: 'none', status: 'closed' });
  assert.strictEqual(await h.controller.release(clone(reason)), close);
  assert.strictEqual(await h.controller.releaseCurrent('explicit-close'), close);
  await assert.rejects(h.controller.release({ ...reason, kind: 'changed-body' }), /stale or malformed/);
  assert.equal(h.state.releases, 1); assert.equal(h.state.disconnects, 1); assert.equal(h.state.imports.length, 0);
  assert.equal(h.controller.blocksLegacy(), true);
  assert.throws(() => h.controller.install().dispose(), /must release/);
  const request = { apiVersion: 1, requestId: 'closed-read', operation: 'snapshot', payload: {} };
  for (let turn = 0; turn < 2; turn++) {
    assert.equal(h.published().admission.allow('retained-write'), false);
    assert.equal(h.published().handlers.legacy('set', staticSource().layers[0], [99]), false);
    assert.equal(h.published().handlers.handle(request).error.code, 'unavailable');
    await Promise.resolve();
  }
  const oldSubscription = h.state.subscriptions[0];
  const preparedB = ProjectDocument.prepareNativeOpacity(staticSource(60));
  await h.controller.activate(preparedB);
  assert.equal(h.state.activations, 2);
  assert.notEqual(h.controller.getNativeIdentity().generation, identity.generation);
  assert.equal(h.controller.identity().documentId, 'native-document-2');
  assert.deepEqual(h.controller.valueAtFrame('r08_curve_layer', 10), [60]);
  await assert.rejects(h.controller.release(reason), /stale or malformed/);
  await assert.rejects(oldSubscription.synchronize({}), /closed lifecycle/);
  assert.equal(h.controller.status(), 'native'); assert.equal(h.state.releases, 1);
});

test('release drains pending preview work while denying its late awaited presentation', async () => {
  let finishHost;
  const gate = new Promise((resolve) => { finishHost = resolve; });
  const h = nativeHarness(staticSource(), { previewGate: gate });
  assert.equal(await h.controller.activate(h.prepared), true);
  const awaited = h.state.lifecycle.presentPreview(10);
  for (let spin = 0; spin < 5 && h.state.previews.length === 0; spin++) await Promise.resolve();
  assert.equal(h.state.previews.length, 1);
  const closing = h.controller.releaseCurrent('preview-drain');
  assert.equal(h.controller.status(), 'release-requested');
  finishHost();
  await assert.rejects(awaited, /not active/);
  assert.equal((await closing).status, 'closed');
  assert.equal(h.state.releases, 1);
  assert.equal(h.state.disconnects, 1);
  await assert.rejects(h.state.lifecycle.presentPreview(10), /not active/);
  assert.equal(h.state.imports.length, 0);
});

test('late preview calls during and after release cannot poison closure or reentry', async () => {
  let finishPreview;
  const previewGate = new Promise((resolve) => { finishPreview = resolve; });
  let finishRelease;
  const releaseGate = new Promise((resolve) => { finishRelease = resolve; });
  const h = nativeHarness(staticSource(), { previewGate, releaseGate });
  assert.equal(await h.controller.activate(h.prepared), true);
  const admitted = h.state.lifecycle.presentPreview(0);
  const admittedDenial = assert.rejects(admitted, /not active/);
  for (let spin = 0; spin < 5 && h.state.previews.length === 0; spin++) await Promise.resolve();
  assert.equal(h.state.previews.length, 1);
  const closing = h.controller.releaseCurrent('late-preview');
  assert.equal(h.controller.status(), 'release-requested');
  await assert.rejects(h.state.lifecycle.presentPreview(0), /not active/);
  assert.equal(h.controller.status(), 'release-requested');
  assert.equal(h.state.previews.length, 1, 'late request cannot reach the host');
  finishPreview();
  await admittedDenial;
  for (let spin = 0; spin < 5 && h.controller.status() !== 'releasing'; spin++) await Promise.resolve();
  assert.equal(h.controller.status(), 'releasing');
  await assert.rejects(h.state.lifecycle.presentPreview(0), /not active/);
  assert.equal(h.controller.status(), 'releasing');
  assert.equal(h.state.previews.length, 1);
  finishRelease();
  assert.equal((await closing).status, 'closed');
  await assert.rejects(h.state.lifecycle.presentPreview(0), /not active/);
  await Promise.resolve();
  assert.equal(h.controller.status(), 'closed');
  assert.equal(h.state.previews.length, 1);
  assert.equal(await h.controller.activate(h.prepared), true);
  assert.equal((await h.state.lifecycle.presentPreview(0)).status, 'presented');
});

test('preview calls in legacy and installing phases reject before host admission', async () => {
  let finishBootstrap;
  const bootstrapGate = new Promise((resolve) => { finishBootstrap = resolve; });
  const h = nativeHarness(staticSource(), { bootstrapGate });
  await assert.rejects(h.state.lifecycle.presentPreview(0), /not active/);
  assert.equal(h.controller.status(), 'legacy');
  const activating = h.controller.activate(h.prepared);
  assert.equal(h.controller.status(), 'installing');
  await assert.rejects(h.state.lifecycle.presentPreview(0), /not active/);
  assert.equal(h.controller.status(), 'installing');
  assert.equal(h.state.previews.length, 0);
  finishBootstrap();
  assert.equal(await activating, true);
  assert.equal(h.controller.status(), 'native');
});

test('resize failure fences the current owner without closing or importing it', async () => {
  const h = surfaceHarness(staticSource());
  await h.controller.activate(h.prepared);
  h.published().resize();
  await h.resizeCallback()();
  assert.equal(h.controller.status(), 'indeterminate');
  assert.equal(h.controller.blocksLegacy(), true);
  assert.equal(h.state.releases, 0); assert.equal(h.state.imports.length, 0);
  assert.equal(h.state.disconnects, 0); assert.equal(h.controller.persistenceJSON(), null);
});

for (const [mode, makeSource, frameZeroOpacity] of [
  ['static', staticSource, 25], ['keyed', keyedSource, 20],
]) {
  for (const previousFrame of [10, 59]) {
    test(`${mode} native Open retains old UI frame ${previousFrame} until final presentation`, async () => {
      const source = makeSource(), paperCalls = [], observed = [];
      const ui = { state: { currentFrame: previousFrame }, _curFrame: previousFrame,
        __TAURI__: { core: { invoke() {} } },
        SM: { importJSON() { paperCalls.push('import'); } },
        goToFrame() { paperCalls.push('navigate'); },
        loadFrame() { paperCalls.push('load'); },
        saveAllLayerFrames() { paperCalls.push('save-frame'); },
        pushUndo() { paperCalls.push('undo'); } };
      let h;
      h = surfaceHarness(source, { ui, resize: async () => {}, paintUiProjection() {
        observed.push([ui.state.currentFrame, ui._curFrame]);
      }, afterChange() {
        // Real afterChange asks SMEngineBridge to render the UI playhead.
        h.published().cutover.renderPreview(ui.state.currentFrame);
      } });
      const first = await h.published().project.importJSON(JSON.stringify(source));
      assert.equal(first.status, 'presented');
      assert.deepEqual(observed, [], 'candidate cannot repaint before final presentation');
      assert.deepEqual([ui.state.currentFrame, ui._curFrame], [previousFrame, previousFrame]);
      assert.deepEqual(h.state.previews.map(request => request.frame), [0]);
      const root = { NemoNativeOpacityProject: h.published().project,
        NemoNativeOpacityCutover: h.published().cutover };
      assert.equal(await NativeProjectEntry.reveal(root, first, {
        hide() {}, show() { throw new Error('unexpected failed reveal'); },
        repaint() { throw new Error('legacy repaint ran during native Open'); },
        raf(callback) { callback(); },
      }), true);
      assert.deepEqual(observed, [[0, 0]], 'UI refresh occurs only after final presentation');
      assert.deepEqual([ui.state.canvasW, ui.state.canvasH, ui.state.fps, ui.state.totalFrames], [320, 180, 24, 21]);
      assert.equal(ui.state.layers[0].name, 'R08 rectangle');
      assert.equal(ui.state.layers[0].motionStatic.opacity[0], mode === 'keyed' ? 25 : frameZeroOpacity,
        'the UI shadow preserves stored opacity while keyed frame evaluation stays native');
      assert.deepEqual(ui._layerSel, [0]);
      const snapshot = h.controller.handleV1({ apiVersion: 1, requestId: `frame-zero-${mode}-${previousFrame}`,
        operation: 'snapshot', payload: { frame: null } });
      assert.equal(snapshot.ok, true);
      assert.equal(snapshot.result.frame, 0);
      assert.equal(snapshot.result.totalFrames, 21);
      assert.equal(snapshot.result.layers[0].opacity, frameZeroOpacity);
      h.published().cutover.renderPreview(ui.state.currentFrame);
      await h.controller.flush();
      h.published().resize();
      await h.resizeCallback()();
      assert.deepEqual(h.state.previews.map(request => request.frame), [0, 0, 0, 0],
        'terminal UI paint never schedules a third native presentation');
      assert.deepEqual([ui.state.currentFrame, ui._curFrame], [0, 0]);
      assert.deepEqual(paperCalls, []);
      assert.equal(h.controller.status(), 'native');
    });
  }
}

test('unsupported and preactivation failed imports retain the previous UI playhead', async () => {
  const source = staticSource();
  for (const failsBootstrap of [false, true]) {
    const ui = { state: { currentFrame: 59 }, _curFrame: 59,
      __TAURI__: { core: { invoke() {} } }, SM: { importJSON() { throw new Error('Paper import'); } } };
    const h = surfaceHarness(source, { ui, bootstrapFailure: () => failsBootstrap });
    const result = await h.published().project.importJSON(failsBootstrap ? JSON.stringify(source) : '{invalid');
    assert.equal(result, false);
    assert.deepEqual([ui.state.currentFrame, ui._curFrame], [59, 59]);
    assert.equal(h.state.previews.length, 0);
  }
});

test('terminal Open rejects a changed receipt and duplicate publication without replacing the old view', async () => {
  const ui = { state: { currentFrame: 59, layers: [{ name: 'Keep' }] }, _curFrame: 59,
    _layerSel: [0], userLayers: [{ children: [] }],
    __TAURI__: { core: { invoke() {} } }, SM: { importJSON() {} } };
  const h = surfaceHarness(staticSource(), { ui });
  const first = await h.published().project.importJSON(JSON.stringify(staticSource()));
  await assert.rejects(h.published().project.finishOpenAfterReveal({ ...first }), /stale or already published/);
  assert.deepEqual([ui.state.currentFrame, ui._curFrame, ui.state.layers[0].name], [59, 59, 'Keep']);
  await h.published().project.finishOpenAfterReveal(first);
  assert.deepEqual([ui.state.currentFrame, ui._curFrame, ui.state.layers[0].name], [0, 0, 'R08 rectangle']);
  await assert.rejects(h.published().project.finishOpenAfterReveal(first), /stale or already published/);
  assert.equal(h.state.previews.length, 2, 'duplicate callback cannot request another frame');
});

test('a native revision before final reveal cannot publish a stale UI projection', async () => {
  const ui = { state: { currentFrame: 59, layers: [{ name: 'Keep' }] }, _curFrame: 59,
    userLayers: [{ children: [] }], __TAURI__: { core: { invoke() {} } }, SM: { importJSON() {} } };
  const h = surfaceHarness(staticSource(), { ui });
  const first = await h.published().project.importJSON(JSON.stringify(staticSource()));
  await h.controller.setOpacity('r08_curve_layer', 60);
  await assert.rejects(h.published().project.finishOpenAfterReveal(first), /stale or already published/);
  assert.deepEqual([ui.state.currentFrame, ui._curFrame, ui.state.layers[0].name], [59, 59, 'Keep']);
  assert.equal(h.state.previews.length, 1, 'stale callback cannot request a second frame');
  assert.equal(h.state.imports.length, 0);
});

test('renderer failure restores every touched UI field and leaves Open unpublished', async () => {
  const oldLayers = [{ name: 'Keep' }], oldSelection = [0], oldPaths = [{ id: 'old' }];
  const oldFrames = [{ layer: 0, frame: 4 }], oldShadow = { test: 'keep' };
  const ui = { state: { currentFrame: 59, canvasW: 800, layers: oldLayers }, _curFrame: 59,
    _layerSel: oldSelection, selectedPaths: oldPaths, _sel: { frames: oldFrames },
    _idxShadow: oldShadow, userLayers: [{ children: [{}], visible: true }],
    __TAURI__: { core: { invoke() {} } }, SM: { importJSON() {} } };
  let renders = 0;
  const h = surfaceHarness(staticSource(), { ui, paintUiProjection() {
    if (++renders === 1) {
      ui._sel.frames.push({ layer: 0, frame: 0 });
      ui._idxShadow.projected = true;
      throw new Error('renderer failed');
    }
  } });
  const first = await h.published().project.importJSON(JSON.stringify(staticSource()));
  await assert.rejects(h.published().project.finishOpenAfterReveal(first), /renderer failed/);
  assert.equal(renders, 2, 'old view is repainted after rollback');
  assert.equal(ui.state.currentFrame, 59); assert.equal(ui._curFrame, 59);
  assert.equal(ui.state.canvasW, 800); assert.strictEqual(ui.state.layers, oldLayers);
  assert.strictEqual(ui._layerSel, oldSelection); assert.strictEqual(ui.selectedPaths, oldPaths);
  assert.strictEqual(ui._sel.frames, oldFrames); assert.strictEqual(ui._idxShadow, oldShadow);
  assert.equal(ui.userLayers[0].visible, true);
  assert.equal(Object.hasOwn(ui.state, 'fps'), false);
  assert.equal(h.controller.blocksLegacy(), true);
});

test('accepted native edit, undo and redo refresh the detached UI opacity', async () => {
  const h = surfaceHarness(staticSource());
  const first = await h.published().project.importJSON(JSON.stringify(staticSource()));
  await h.published().project.finishOpenAfterReveal(first);
  assert.equal(h.ui.state.layers[0].motionStatic.opacity[0], 25);
  await h.controller.setOpacity('r08_curve_layer', 60);
  assert.equal(h.ui.state.layers[0].motionStatic.opacity[0], 60);
  await h.controller.history('undo');
  assert.equal(h.ui.state.layers[0].motionStatic.opacity[0], 25);
  await h.controller.history('redo');
  assert.equal(h.ui.state.layers[0].motionStatic.opacity[0], 60);
  assert.equal(h.state.imports.length, 0, 'no legacy document writer runs');
});

for (const fails of [false, true]) {
  test(`first-open reveal joins a delayed host resize before ${fails ? 'failure' : 'success'}`, async () => {
    const source = staticSource(), gate = deferred();
    const h = surfaceHarness(source, { resize: () => gate.promise });
    const first = await h.published().project.importJSON(JSON.stringify(source));
    let shown = 0, settled = false;
    const root = { NemoNativeOpacityProject: h.published().project,
      NemoNativeOpacityCutover: h.published().cutover };
    const revealed = NativeProjectEntry.reveal(root, first, {
      hide() { h.published().resize(); }, show() { shown++; }, repaint() {},
      raf(callback) { callback(); },
    }).finally(() => { settled = true; });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(settled, false, 'Open must wait for the pending host resize');
    assert.deepEqual(h.state.previews.map(request => request.frame), [0]);
    if (fails) {
      gate.reject(new Error('resize failed during reveal'));
      await assert.rejects(revealed, /resize failed during reveal/);
      assert.equal(h.controller.status(), 'indeterminate');
      assert.equal(shown, 1, 'failed reveal restores the start screen');
      assert.deepEqual(h.state.previews.map(request => request.frame), [0]);
    } else {
      gate.resolve();
      assert.equal(await revealed, true);
      assert.equal(shown, 0);
      assert.deepEqual(h.state.previews.map(request => request.frame), [0, 0]);
      assert.equal(h.controller.status(), 'native');
    }
  });
}

test('first-open reveal joins an already-running resize without a duplicate presentation', async () => {
  const source = staticSource(), gate = deferred();
  const h = surfaceHarness(source, { resize: () => gate.promise });
  const first = await h.published().project.importJSON(JSON.stringify(source));
  h.published().resize();
  const resizing = h.resizeCallback()();
  let settled = false;
  const root = { NemoNativeOpacityProject: h.published().project,
    NemoNativeOpacityCutover: h.published().cutover };
  const revealed = NativeProjectEntry.reveal(root, first, {
    hide() {}, show() {}, repaint() {}, raf(callback) { callback(); },
  }).finally(() => { settled = true; });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(settled, false);
  gate.resolve();
  await Promise.all([resizing, revealed]);
  assert.deepEqual(h.state.previews.map(request => request.frame), [0, 0]);
  assert.equal(h.controller.status(), 'native');
});

test('a resize arriving during presentation is applied before the final receipt', async () => {
  const source = staticSource(), hostGate = deferred();
  const h = surfaceHarness(source, { resize: async () => {},
    previewGate(_request, state) {
      if (state.previews.length === 2) return hostGate.promise;
    } });
  await h.published().project.importJSON(JSON.stringify(source));
  let settled = false;
  const presenting = h.published().cutover.presentPreview(0).finally(() => { settled = true; });
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(h.state.previews.map(request => request.frame), [0, 0]);
  h.published().resize();
  const resizing = h.resizeCallback()();
  hostGate.resolve();
  await Promise.all([presenting, resizing]);
  assert.equal(settled, true);
  assert.deepEqual(h.state.previews.map(request => request.frame), [0, 0, 0]);
  assert.equal(h.controller.status(), 'native');
});

test('a later successful viewport resize presents the latest native frame again', async () => {
  const source = staticSource();
  const h = surfaceHarness(source, { resize: async () => {} });
  await h.published().project.importJSON(JSON.stringify(source));
  h.published().cutover.renderPreview(10);
  await h.controller.flush();
  h.published().resize();
  await h.resizeCallback()();
  assert.deepEqual(h.state.previews.map(request => request.frame), [0, 10, 10]);
  assert.equal(h.controller.status(), 'native');
});

test('failed initial or direct B bootstrap cannot restore legacy or admit another bootstrap', async () => {
  for (const afterClose of [false, true]) {
    const h = surfaceHarness(staticSource(), { bootstrapFailure: state => state.activations === (afterClose ? 2 : 1) });
    if (afterClose) {
      await h.controller.activate(h.prepared);
      await h.controller.releaseCurrent('close-before-b');
    }
    await assert.rejects(h.controller.activate(h.prepared), /bootstrap reply lost/);
    assert.equal(h.controller.status(), 'indeterminate');
    assert.equal(h.controller.blocksLegacy(), true);
    const before = stateBytes(h);
    await assert.rejects(async () => h.controller.activate(h.prepared), /activation requires/);
    assert.equal(stateBytes(h), before);
    assert.equal(h.state.imports.length, 0);
  }
});

test('mutable or mismatched prepared candidates fail before bootstrap and preserve verified closure', async () => {
  function freeze(value) {
    if (value && typeof value === 'object') { Object.values(value).forEach(freeze); Object.freeze(value); }
    return value;
  }
  const h = surfaceHarness(staticSource());
  await h.controller.activate(h.prepared);
  const reason = { ...h.controller.getNativeIdentity(), kind: 'candidate-validation' };
  const close = await h.controller.release(reason);
  const wrongProjection = clone(h.prepared); wrongProjection.projection.layers[0].motionStatic.opacity = [99];
  const wrongGeometry = clone(h.prepared); wrongGeometry.resources[0].layers[0].bounds = [0, 0, 1, 1];
  const wrongShell = clone(h.prepared); wrongShell.shell.layers[0].motionStatic.opacity = [60];
  const hiddenMismatch = clone(wrongProjection); hiddenMismatch.toJSON = () => h.prepared;
  const accessor = { ...h.prepared }; Object.defineProperty(accessor, 'projection', { enumerable: true, get: () => h.prepared.projection });
  let accessed = 0;
  const hiddenJSON = { ...h.prepared };
  Object.defineProperty(hiddenJSON, 'toJSON', { value: () => { accessed++; return h.prepared; } });
  const customPrototype = Object.assign(Object.create({ inherited: true }), h.prepared);
  const shellAccessor = { ...h.prepared };
  Object.defineProperty(shellAccessor, 'shell', { enumerable: true, get: () => { accessed++; return h.prepared.shell; } });
  const symbol = { ...h.prepared, [Symbol('hidden')]: 'forged' };
  const omittedData = freeze({ ...h.prepared, omitted: undefined });
  const arrayPrototype = clone(h.prepared);
  Object.setPrototypeOf(arrayPrototype.resources, Object.create(Array.prototype, {
    toJSON: { value: () => { accessed++; return h.prepared.resources; } },
  }));
  for (const candidate of [null, {}, clone(h.prepared), freeze(wrongProjection), freeze(wrongGeometry), freeze(wrongShell),
    freeze(hiddenMismatch), Object.freeze(accessor), Object.freeze(hiddenJSON), Object.freeze(customPrototype),
    Object.freeze(shellAccessor), Object.freeze(symbol), omittedData, freeze(arrayPrototype)]) {
    const before = stateBytes(h);
    assert.throws(() => h.controller.activate(candidate));
    assert.equal(stateBytes(h), before);
    assert.strictEqual(await h.controller.release(reason), close);
  }
  assert.equal(accessed, 0, 'admission rejects hidden serializers and shell accessors before invoking them');
});

test('actual published and retained handlers never fall back to original meta or reads after close', async () => {
  let originals = 0;
  const guardScope = {};
  vm.runInNewContext(fs.readFileSync(path.join(ROOT, 'src/js/application/native-edit-guard.js'), 'utf8'), guardScope);
  const root = { __TAURI__: { core: { invoke() {} } }, SMEngineBridge: {}, SMNativeEditGuard: guardScope.SMNativeEditGuard,
    SM: { importJSON() { originals++; return true; } }, SMMotion: {}, addEventListener() {}, removeEventListener() {},
    NemoApplication: { capabilities: () => [], handle: () => { originals++; return 'original-read'; } },
    NemoOpacityApplication: { meta: () => { originals++; return { legacy: true }; }, legacy: () => { originals++; return 'original-write'; } } };
  const h = nativeHarness(staticSource(), { surface: NativeLegacySurface.desktopPorts(root, {}).surface });
  const installation = h.controller.install();
  const retainedRead = root.NemoApplication.handle;
  const retainedMeta = root.NemoOpacityApplication.meta;
  const retainedWrite = root.NemoOpacityApplication.legacy;
  await h.controller.activate(h.prepared);
  await h.controller.releaseCurrent('published-meta-close');
  for (let turn = 0; turn < 2; turn++) {
    for (const read of [retainedRead, root.NemoApplication.handle]) {
      assert.equal(read({ apiVersion: 1, requestId: 'closed-snapshot', operation: 'snapshot', payload: {} }).error.code, 'unavailable');
    }
    for (const meta of [retainedMeta, root.NemoOpacityApplication.meta]) assert.throws(meta, /identity/);
    for (const write of [retainedWrite, root.NemoOpacityApplication.legacy]) assert.equal(write('set', staticSource().layers[0], [99]), false);
    assert.throws(() => installation.dispose(), /must release/);
    assert.equal(originals, 0);
    await Promise.resolve();
  }
});

test('in-flight close reserves exactly one request fingerprint and drains to one ownerless receipt', async () => {
  let finish;
  const gate = new Promise(resolve => { finish = resolve; });
  const h = surfaceHarness(staticSource(), { releaseGate: gate });
  await h.controller.activate(h.prepared);
  const reason = { ...h.controller.getNativeIdentity(), kind: 'draining-close' };
  const closing = h.controller.release(reason);
  assert.strictEqual(h.controller.release(clone(reason)), closing);
  await assert.rejects(h.controller.release({ ...reason, kind: 'changed-during-drain' }), /stale or malformed/);
  await assert.rejects(h.controller.release({ ...reason, cancelled: true }), /stale or malformed/);
  await Promise.resolve();
  assert.equal(h.state.releases, 1);
  assert.equal(h.state.disconnects, 0);
  finish();
  const receipt = await closing;
  assert.strictEqual(await h.controller.release(reason), receipt);
  assert.equal(h.state.releases, 1); assert.equal(h.state.disconnects, 1);
  assert.equal(h.controller.blocksLegacy(), true); assert.equal(h.state.imports.length, 0);
});

test('failed, cancelled, changed and disconnected terminal receipts cannot admit any writer or re-entry', async () => {
  const cases = [
    { releaseFailure: true },
    { releaseReceipt: receipt => ({ ...receipt, status: 'cancelled', authorityRemovalCompleted: false }) },
    { releaseReceipt: receipt => ({ ...receipt, lifecycleGeneration: receipt.lifecycleGeneration + 1 }) },
    { releaseReceipt: receipt => ({ ...receipt, requestId: 'changed-request' }) },
    { disconnectFailure: true },
    { connectionStuck: true },
  ];
  for (const options of cases) {
    const h = surfaceHarness(staticSource(), options);
    await h.controller.activate(h.prepared);
    const reason = { ...h.controller.getNativeIdentity(), kind: 'failed-terminal' };
    await assert.rejects(h.controller.release(reason));
    assert.equal(h.controller.status(), 'indeterminate');
    assert.equal(h.controller.blocksLegacy(), true);
    const before = stateBytes(h);
    await assert.rejects(h.controller.release(reason), /stale or malformed/);
    assert.throws(() => h.controller.activate(h.prepared), /activation requires/);
    assert.equal(await h.published().project.importJSON(JSON.stringify(staticSource(60))), false);
    assert.equal(h.published().handlers.legacy('set', staticSource().layers[0], [99]), false);
    await Promise.resolve(); assert.equal(stateBytes(h), before);
    assert.equal(h.state.imports.length, 0);
  }
});

test('unsupported keyed and Motion edits preserve exact authoritative state immediately and after a microtask', async () => {
  const h = surfaceHarness(keyedSource());
  await h.controller.activate(h.prepared);
  const before = stateBytes(h), identity = h.controller.identity();
  await assert.rejects(h.controller.setOpacity('r08_curve_layer', 99), /read-only/);
  assert.equal(h.controller.legacyIntent('set', keyedSource().layers[0], [99]), false);
  assert.equal(h.controller.legacyIntent('key', keyedSource().layers[0], [99]), false);
  const request = { apiVersion: 1, requestId: 'unsupported-keyed', ...identity,
    expectedRevision: identity.contentRevision, operation: 'property.set',
    payload: { layerId: 'r08_curve_layer', property: 'opacity', value: 99 } };
  const denied = h.controller.handleV1(request);
  assert.equal(denied.error.code, 'unavailable');
  assert.deepEqual(h.controller.handleV1(clone(request)), denied);
  assert.equal(h.controller.handleV1({ ...request, payload: { ...request.payload, value: 70 } }).error.code, 'invalid_request');
  assert.equal(stateBytes(h), before);
  await Promise.resolve(); assert.equal(stateBytes(h), before);
});

test('retained actual raw and published Motion writers remain denied after ownerless close', async () => {
  const h = surfaceHarness(staticSource());
  const holder = clone(staticSource().layers[0]);
  const state = { ...defaultState(), layers: [holder], activeLayerIdx: 0 };
  const motion = loadMotion(state, { beforeMotion(scope) {
    scope.NemoNativeOpacityLegacySurface = NativeLegacySurface;
    scope.NemoNativeOpacityMotionSurface = NativeMotionSurface;
    scope.n20AllowLegacyWrite = () => !h.controller.blocksLegacy();
    scope.n20RequireLegacyWrite = kind => NativeLegacySurface.requireLegacyWrite(scope, kind, scope.n20AllowLegacyWrite);
  } });
  const published = motion.SMMotion.toggleLayer3D;
  const source = fs.readFileSync(path.join(ROOT, 'src/js/motion.js'), 'utf8');
  const scope = { state, n20RequireLegacyWrite: motion.sandbox.n20RequireLegacyWrite };
  const tweens = fs.readFileSync(path.join(ROOT, 'src/js/tweens.js'), 'utf8');
  vm.runInNewContext(extractFunction(source, 'setExpressionCode') + '\n' + extractFunction(tweens, 'pushUndo'), scope);
  const raw = scope.setExpressionCode;
  await h.controller.activate(h.prepared);
  await h.controller.releaseCurrent('retained-motion-close');
  const before = JSON.stringify(state), nativeBefore = stateBytes(h);
  for (let turn = 0; turn < 2; turn++) {
    assert.equal(published(0), false);
    assert.throws(() => raw(holder, 'position', '99'), { name: 'NemoNativeReleaseRequired' });
    assert.equal(JSON.stringify(state), before);
    assert.equal(stateBytes(h), nativeBefore);
    await Promise.resolve();
  }
});
