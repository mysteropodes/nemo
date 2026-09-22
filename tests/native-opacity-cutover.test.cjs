'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const ProjectDocument = require('../src/js/project-document.js');
const NativeOpacityContract = require('../src/js/application/native-opacity-contract.js');
const NativeOpacityLifecycle = require('../src/js/application/native-opacity-lifecycle.js');
const NativeOpacityOperations = require('../src/js/application/native-opacity-operations.js');
const NativeLegacySurface = require('../src/js/adapters/native-opacity-legacy-surface.js');
const NativeMotionSurface = require('../src/js/adapters/native-opacity-motion-surface.js');
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
const { defaultState, extractFunction, loadMotion: loadLegacyMotion } = require('./fixtures/lib/sandbox.cjs');
function loadMotion(state, hooks = {}) {
  return loadLegacyMotion(state, { ...hooks, beforeMotion(sb) {
    sb.NemoNativeOpacityLegacySurface = NativeLegacySurface;
    sb.NemoNativeOpacityMotionSurface = NativeMotionSurface;
    if (hooks.beforeMotion) hooks.beforeMotion(sb);
  } });
}

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
function productionViewKernels() {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/motion.js'), 'utf8'), scope = {};
  vm.runInNewContext(['seedElementHolder', 'expressionSeed', 'expressionSnapshotValue']
    .map((name) => extractFunction(source, name)).join('\n'), scope);
  return scope;
}

function realTauriTransportFactory(options = {}) {
  return (state, host) => {
    const listeners = new Map();
    const calls = [];
    const subscriptionId = () => `subscription-${state.hostGeneration}`;
    const binding = () => ({ ...clone(state.identity), lifecycleGeneration: state.hostGeneration,
      subscriptionId: subscriptionId() });
    const invoke = async (command, args) => {
      calls.push([command, clone(args)]);
      if (command === 'nemo_native_status') return { apiVersion: 2, available: true, ...clone(state.identity) };
      if (command === 'nemo_native_revision_sync') {
        if (args.request.action === 'binding' || args.request.action === 'subscribe') return binding();
        if (args.request.action === 'acknowledge' && options.ackFailure) throw new Error('revision ack failed');
        return null;
      }
      if (command === 'nemo_native_dispatch') return host.dispatch(args.request);
      throw new Error(`unexpected Tauri command ${command}`);
    };
    const transport = ApplicationMcp.createNativeTauriTransport(invoke);
    return {
      transport, calls,
      async listen(name, callback) { listeners.set(name, callback); return () => listeners.delete(name); },
      async emit(event) {
        const listener = listeners.get('nemo-native-revision');
        assert.equal(typeof listener, 'function');
        await listener({ payload: clone(event) });
      },
      async mcpSet(value, requestId = `mcp-write-${state.identity.contentRevision + 1}`) {
        const fromRevision = state.identity.contentRevision;
        const response = await host.dispatch({ apiVersion: 2, requestId,
          instanceId: state.identity.instanceId, documentId: state.identity.documentId,
          expectedRevision: fromRevision, operation: 'command.document.apply',
          payload: { command: 'layer.opacity.set', stableTarget: { layerUid: 'r08_curve_layer' }, value } });
        await this.emit({ instanceId: state.identity.instanceId, documentId: state.identity.documentId,
          lifecycleGeneration: state.hostGeneration, fromRevision,
          toRevision: state.identity.contentRevision, requestId });
        return response;
      },
    };
  };
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
    activations: 0, hostGeneration: 0, connected: false, subscriptions: [], previewConsumers: [], exportConsumers: [],
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
      assert.strictEqual(value, prepared);
      if (options.bootstrapGate) await options.bootstrapGate;
      state.activations++;
      state.hostGeneration = state.activations;
      state.identity = { instanceId: 'instance-a', documentId: `native-document-${state.activations}`, contentRevision: 0 };
      state.document = clone(prepared.projection); state.history.length = 0; state.redo.length = 0;
      return { apiVersion: 2, instanceId: state.identity.instanceId,
        documentId: state.identity.documentId, contentRevision: state.identity.contentRevision,
        resourceCount: prepared.resources.length, viewportAvailable: true };
    },
    async connect() {
      if (transportBridge) return transportBridge.transport.connect();
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
      return { ...clone(state.identity), lifecycleGeneration: state.hostGeneration,
        subscriptionId: `subscription-${state.hostGeneration}` };
    },
    async disconnect() {
      state.disconnects++;
      state.connected = false;
      if (transportBridge) return transportBridge.transport.disconnect();
      const current = state.subscriptions.at(-1); if (current) current.active = false;
      if (options.disconnectGate) await options.disconnectGate;
      if (options.disconnectFailure) throw new Error('disconnect failed');
    },
    async release(request) {
      state.releases++;
      if (options.releaseGate) await options.releaseGate;
      if (options.releaseFailure) throw new Error('release failed');
      return options.releaseReceipt ? options.releaseReceipt(fullReleaseReceipt(request), request) : fullReleaseReceipt(request);
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
      if (options.previewGate) await options.previewGate;
      return { workId: `preview-${state.previews.length}`, viewGeneration: state.previews.length,
        status: 'presented' };
    },
    async bindOutput(request) { state.outputs.push(request); },
    currentFrame() { return 10; },
    afterChange() {
      state.afterChanges = (state.afterChanges || 0) + 1;
      if (options.afterChange) options.afterChange(controller, state);
    },
    sleep() { return Promise.resolve(); },
  }, { contract: NativeOpacityContract, lifecycle: { create(ports, contract) {
    return state.lifecycle = NativeOpacityLifecycle.create(ports, contract);
  } },
    operations: NativeOpacityOperations, motionSurface: NativeMotionSurface });
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

test('N20 characterization is pinned to the two approved fixture bytes', () => {
  assert.equal(sha(SHELL_PATH), SHELL_SHA);
  assert.equal(sha(NATIVE_PATH), NATIVE_SHA);
});

test('N20 split exposes one lifecycle authority and stateless frozen contracts', () => {
  assert.deepEqual(Object.keys(NativeOpacityLifecycle), ['create']);
  assert.deepEqual(Object.keys(NativeOpacityOperations), ['create']);
  assert.equal(Object.isFrozen(NativeOpacityContract), true);
  assert.equal(Object.isFrozen(NativeOpacityContract.READS), true);
  assert.equal(Object.isFrozen(NativeOpacityContract.WRITES), true);
  assert.equal(Object.isFrozen(NativeOpacityContract.PROPERTY_WRITES), true);

  const lifecycleSource = fs.readFileSync(path.join(ROOT,
    'src/js/application/native-opacity-lifecycle.js'), 'utf8');
  const operationsSource = fs.readFileSync(path.join(ROOT,
    'src/js/application/native-opacity-operations.js'), 'utf8');
  const coreSource = fs.readFileSync(path.join(ROOT,
    'src/js/application/opacity-application.js'), 'utf8');
  assert.match(lifecycleSource, /var phase = 'legacy', prepared = null, identity = null/);
  assert.doesNotMatch(operationsSource, /\bvar\s+(?:phase|prepared|identity|evaluations|persistence)\b/,
    'operations adapts the lifecycle and cannot create a second authority/cache');
  assert.doesNotMatch(coreSource, /function buildCaches|function requestRelease|v1Retained/,
    'the core is legacy service plus thin native composition only');
});

test('component exposed-property domain preserves classic and CommonJS call order and entry identity', () => {
  const classic = {};
  for (const name of ['window', 'document', 'state', 'SM', 'SMMotion', 'Date', 'Math']) {
    Object.defineProperty(classic, name, { get() { throw new Error(`unexpected global ${name}`); } });
  }
  vm.runInNewContext(fs.readFileSync(path.join(ROOT, 'src/js/domain/component/exposed-properties.js'), 'utf8'), classic);
  for (const api of [ComponentExposedProperties, classic.NemoComponentExposedProperties]) {
    assert.deepEqual(Object.keys(api), ['expose', 'applyOverrides']);
    assert.equal(Object.isFrozen(api), true);
    const events = [], symbol = {}, symbols = { chosen: symbol }, value = { preserved: true };
    const clock = () => { events.push('clock'); assert.equal(Array.isArray(symbol.exposedProps), true);
      assert.equal(symbol.exposedProps.length, 0); return 1296; };
    const random = () => { events.push('random'); return 0.5; };
    const metadata = (key, label, actual) => {
      events.push('metadata'); assert.equal(symbol.exposedProps.length, 1);
      assert.equal(symbol.exposedProps[0].key, key); assert.equal(label, 'Opacity'); assert.equal(actual, value);
    };
    assert.equal(api.expose(symbols, 'missing', 'stroke', 'opacity', 'Opacity', value, clock, random, metadata), null);
    assert.deepEqual(events, []);
    const entry = api.expose(symbols, 'chosen', 'stroke', 'opacity', 'Opacity', value, clock, random, metadata);
    assert.deepEqual(events, ['clock', 'random', 'metadata']);
    assert.equal(entry, symbol.exposedProps[0]); assert.equal(entry.default, value);
    assert.deepEqual(Object.keys(entry), ['key', 'label', 'targetStrokeId', 'targetField', 'default']);
    assert.equal(JSON.stringify(entry), JSON.stringify({ key: 'ep_100_i', label: 'Opacity',
      targetStrokeId: 'stroke', targetField: 'opacity', default: value }));
    const existing = symbol.exposedProps, failure = new Error('metadata failed');
    assert.throws(() => api.expose(symbols, 'chosen', 's2', '__visible', 'Visible', false,
      () => 0, () => 0, () => { throw failure; }), (error) => error === failure);
    assert.equal(symbol.exposedProps, existing); assert.equal(existing.length, 2, 'metadata errors propagate after push');
    const separate = { other: { exposedProps: null } }, providerFailure = new Error('clock failed');
    assert.throws(() => api.expose(separate, 'other', 's', 'opacity', 'A', 1,
      () => { throw providerFailure; }, () => { throw new Error('random must not run'); }), (error) => error === providerFailure);
    assert.equal(Array.isArray(separate.other.exposedProps), true, 'falsy initialization precedes clock');
    assert.equal(separate.other.exposedProps.length, 0); assert.equal(existing.length, 2, 'calls retain no symbol map');
    const malformed = { other: { exposedProps: {} } };
    assert.throws(() => api.expose(malformed, 'other', 's', 'opacity', 'A', 1, () => 0, () => 0),
      (error) => error.name === 'TypeError');
    assert.equal(Array.isArray(malformed.other.exposedProps), false, 'truthy invalid storage is not silently replaced');
  }
});

test('component overrides preserve declaration order, output identity, clone timing and sticky visibility', () => {
  const classic = {};
  vm.runInNewContext(fs.readFileSync(path.join(ROOT, 'src/js/domain/component/exposed-properties.js'), 'utf8'), classic);
  for (const api of [ComponentExposedProperties, classic.NemoComponentExposedProperties]) {
  const untouched = { strokeId: 'untouched' }, changed = { strokeId: 'painted', nested: { keep: true } };
  const visible = { strokeId: 'visible' }, hidden = { strokeId: 'hidden' }, strokes = [untouched, changed, visible, hidden];
  const declarations = [
    ['painted', 'opacity', 150], ['hidden', '__visible', 0], ['painted', 'fillColor', '#f00'],
    ['visible', '__visible', 100], ['hidden', 'opacity', -10], ['hidden', '__visible', 100],
    ['painted', 'opacity', 25],
  ].map(([targetStrokeId, targetField, value], i) => ({ key: 'k' + i, targetStrokeId, targetField, value }));
  const sym = { exposedProps: declarations }, layer = {}, events = [];
  const result = api.applyOverrides(sym, strokes, layer, 17, () => true,
    (ep) => events.push('meta-' + ep.key), (actual, key, frame) => {
      assert.equal(actual, layer); assert.equal(frame, 17); events.push('read-' + key);
      return [declarations.find((ep) => ep.key === key).value];
    });
  assert.deepEqual(events, declarations.map((ep) => 'meta-' + ep.key).concat([
    'read-k0', 'read-k2', 'read-k6', 'read-k3', 'read-k1', 'read-k4', 'read-k5']));
  assert.equal(result.length, 3); assert.equal(result[0], untouched); assert.equal(result[2], visible);
  assert.notEqual(result[1], changed); assert.notEqual(result[1].nested, changed.nested);
  assert.equal(result[1].opacity, 0.25); assert.equal(result[1].fillColor, '#f00');
  assert.equal(changed.opacity, undefined); assert.equal(hidden.opacity, undefined);
  const noCall = () => { throw new Error('unexpected callback'); };
  for (const empty of [{}, { exposedProps: [] }]) assert.equal(api.applyOverrides(empty, strokes, layer, 0, noCall, noCall, noCall), strokes);
  assert.equal(api.applyOverrides(sym, strokes, layer, 0, () => false, noCall, noCall), strokes);
  const broken = { strokeId: 'painted' }; broken.cycle = broken;
  const reads = [];
  assert.throws(() => api.applyOverrides(sym, [broken], layer, 0, () => true, () => {},
    (_, key) => { reads.push(key); return [50]; }), (error) => error.name === 'TypeError');
  assert.deepEqual(reads, ['k0'], 'clone failure occurs after the first value read and before later declarations');
  const metadataFailure = new Error('metadata failed');
  assert.throws(() => api.applyOverrides(sym, strokes, layer, 0, () => true,
    () => { throw metadataFailure; }, noCall), (error) => error === metadataFailure);
  let clones = 0;
  const counted = { strokeId: 'painted', toJSON() { clones++; return { strokeId: 'painted' }; } };
  api.applyOverrides(sym, [counted], layer, 0, () => true, () => {}, () => [50]);
  assert.equal(clones, 1, 'several non-visibility overrides clone a stroke only once');
  }
});

test('production component rendering applies overrides after transforms with lazy Motion method receivers', () => {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/app.js'), 'utf8');
  const stroke = { isRaster: true, strokeId: 's', opacity: 1, x: 0 };
  const sym = { layers: [{ frames: [{ isKeyframe: true, strokes: [stroke] }] }],
    exposedProps: [{ key: 'opacity', targetStrokeId: 's', targetField: 'opacity', label: 'Alpha', default: 100 }] };
  const layer = { symbolId: 'symbol', frames: [{ isKeyframe: true }], symMatrix: [1, 0, 0, 1, 5, 0] };
  const events = [], scope = { state: { symbols: { symbol: sym }, layers: [layer] },
    NemoComponentExposedProperties: ComponentExposedProperties,
    layerHasTimeRange: () => false, resolveSymbolFrameIdx: () => 0, symMatrixOf: () => layer.symMatrix,
    cloneStrokeForTransform: (value) => structuredClone(value),
    applyMatrixToStrokeData(value) { events.push('transform'); value.x = 5; return value; } };
  scope.window = scope;
  const motion = { computeMotionMatFor: () => null,
    get registerExposedPropMeta() { events.push('lookup-meta'); return function () {
      assert.equal(this, motion); events.push('metadata');
    }; },
    get valueAtFrame() { events.push('lookup-value'); return function (actual, key, frame) {
      assert.equal(this, motion); assert.equal(actual, layer); assert.equal(key, 'opacity'); assert.equal(frame, 17);
      events.push('value'); return [150];
    }; } };
  scope.SMMotion = motion;
  vm.runInNewContext(extractFunction(source, 'getEffectiveStrokes'), scope);
  const result = scope.getEffectiveStrokes(0, 17);
  assert.deepEqual(events, ['transform', 'lookup-meta', 'lookup-meta', 'metadata', 'lookup-value', 'value']);
  assert.equal(result[0].x, 5); assert.equal(result[0].opacity, 1); assert.equal(stroke.x, 0);
  scope.SMMotion = undefined; events.length = 0;
  assert.equal(scope.getEffectiveStrokes(0, 17)[0].x, 5);
  assert.deepEqual(events, ['transform'], 'unavailable Motion leaves transformed output unchanged');
});

test('retained component app facade keeps admission first and every dependency lazy', () => {
  const app = fs.readFileSync(path.join(ROOT, 'src/js/app.js'), 'utf8');
  const events = [], symbols = { chosen: {} }, scope = { NemoComponentExposedProperties: ComponentExposedProperties };
  scope.window = scope;
  scope.n20AllowLegacyWrite = (kind) => { events.push(kind); return false; };
  for (const name of ['state', 'Date', 'Math', 'SMMotion']) Object.defineProperty(scope, name, {
    configurable: true, get() { throw new Error(`denied facade accessed ${name}`); } });
  vm.runInNewContext(extractFunction(app, 'exposeSymbolProperty'), scope);
  const original = scope.exposeSymbolProperty;
  NativeLegacySurface.wrapWriter(scope, 'exposeSymbolProperty', () => false, 'public-wrapper');
  assert.equal(original('chosen', 's', 'opacity', 'Opacity', 25), false);
  assert.deepEqual(events, ['symbol-expose-property']);
  const motion = { registerExposedPropMeta(key, label, value) {
    assert.equal(this, motion); assert.equal(symbols.chosen.exposedProps[0].key, key);
    events.push(['metadata', label, value]);
  } };
  for (const [name, value] of Object.entries({ state: { symbols },
    Date: { now() { events.push('clock'); return 1296; } },
    Math: { random() { events.push('random'); return 0.5; } }, SMMotion: motion })) {
    Object.defineProperty(scope, name, { configurable: true, value });
  }
  scope.n20AllowLegacyWrite = () => true; events.length = 0;
  assert.equal(original('missing', 's', 'opacity', 'Opacity', 25), null); assert.deepEqual(events, []);
  const result = original('chosen', 's', 'opacity', 'Opacity', 25);
  assert.equal(result, symbols.chosen.exposedProps[0]); assert.equal(result.key, 'ep_100_i');
  assert.deepEqual(events, ['clock', 'random', ['metadata', 'Opacity', 25]]);
});

test('classic startup loads frozen guard modules before the production first-layer creation', () => {
  const html = fs.readFileSync(path.join(ROOT, 'src/index.html'), 'utf8');
  const scripts = [...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"/g)].map((match) => match[1]);
  const sequence = ['js/application/native-opacity-contract.js', 'js/application/native-opacity-lifecycle.js',
    'js/application/native-opacity-operations.js', 'js/application/opacity-application.js',
    'js/adapters/native-opacity-legacy-surface.js', 'js/adapters/native-opacity-motion-surface.js',
    'js/domain/component/exposed-properties.js'];
  const appAt = scripts.indexOf('js/app.js');
  assert.deepEqual(scripts.slice(appAt - sequence.length, appAt), sequence);
  const scope = { state: { layers: [], totalFrames: 21 }, userLayers: [],
    LAYER_COLOR_PALETTE: ['red'], arcLayer: { activate() {} },
    Layer: function () { this.insertBelow = () => {}; } };
  scope.window = scope;
  for (const file of sequence) vm.runInNewContext(fs.readFileSync(path.join(ROOT, 'src', file), 'utf8'), scope, { filename: file });
  const app = fs.readFileSync(path.join(ROOT, 'src/js/app.js'), 'utf8');
  vm.runInNewContext(extractFunction(app, 'nextLayerColor') + '\n' + extractFunction(app, 'createUserLayer') + '\n' +
    app.match(/^createUserLayer\('Layer 1'\);$/m)[0], scope);
  assert.equal(scope.state.layers.length, 1); assert.equal(scope.state.layers[0].name, 'Layer 1');
  assert.equal(scope.userLayers.length, 1);
  scope.n20AllowLegacyWrite = () => false;
  assert.throws(() => scope.createUserLayer('denied'), { name: 'NemoNativeReleaseRequired' });
  assert.equal(scope.state.layers.length, 1); assert.equal(scope.userLayers.length, 1);
});

test('Motion surface is frozen and stateless with live native reads, detached views and key suppression', async () => {
  const kernels = productionViewKernels();
  const classic = {};
  for (const name of ['window', 'document', 'state', 'SM', 'SMMotion']) {
    Object.defineProperty(classic, name, { get() { throw new Error(`unexpected global ${name}`); } });
  }
  vm.runInNewContext(fs.readFileSync(path.join(ROOT, 'src/js/adapters/native-opacity-motion-surface.js'), 'utf8'), classic);
  const harness = nativeHarness(staticSource());
  await harness.controller.activate(harness.prepared);
  const holder = { layerUid: harness.prepared.layerUid };
  Object.defineProperty(holder, 'motion', { get() { throw new Error('retained Position/opacity tracks must not be read'); } });
  for (const api of [NativeMotionSurface, classic.NemoNativeOpacityMotionSurface]) {
    assert.equal(Object.isFrozen(api), true); assert.equal(api.requireAvailable(api), api);
    assert.equal(api.read(null, holder, 'opacity', 10).handled, false);
    assert.equal(api.read({ blocksLegacy: () => false }, holder, 'position', 10).handled, false);
    const calls = [];
    const controller = { ...harness.controller, prepared() { calls.push('prepared'); return harness.controller.prepared(); },
      projectSelection(value, frame) { return harness.controller.projectSelection(JSON.parse(JSON.stringify(value)), frame); } };
    for (const [frame, position] of [[0, [0, 0]], [10, [64, 0]], [20, [128, 0]]]) {
      const result = api.read(controller, holder, 'position', frame);
      assert.equal(Object.isFrozen(result), true); assert.equal(Object.isFrozen(result.value), true);
      assert.deepEqual(Array.from(result.value), position);
    }
    assert.equal(calls.length, 3, 'every native read revalidates live prepared state');
    assert.deepEqual(Array.from(api.read(controller, holder, 'opacity', 10).value), [25]);
    const overlay = api.positionOverlayPlan(controller, holder, [0, 10, 20]);
    assert.equal(Object.isFrozen(overlay.samples), true); assert.equal(Object.isFrozen(overlay.samples[0].position), true);
    assert.deepEqual(JSON.parse(JSON.stringify(overlay.samples)), [
      { outerFrame: 0, position: [0, 0] }, { outerFrame: 10, position: [64, 0] }, { outerFrame: 20, position: [128, 0] }]);
    assert.equal(overlay.showLegacyKeys, false); assert.equal(overlay.allowKeyHitTest, false);
    assert.equal(api.nativeKeyInteractionPlan(controller, holder).allowKeyHitTest, false);
    assert.throws(() => api.read(controller, { layerUid: 'other' }, 'position', 0), /identity/);
    assert.throws(() => api.read(controller, holder, 'opacity', 21), /frame/);
    assert.throws(() => api.read({ blocksLegacy: () => 1 }, holder, 'position', 0), /malformed/);
    const failure = new Error('live fence');
    const fenced = { ...controller, prepared() { throw failure; } };
    for (const attempt of [() => api.read(fenced, holder, 'position', 0),
      () => api.positionOverlayPlan(fenced, holder, [0]), () => api.nativeKeyInteractionPlan(fenced, holder)]) {
      assert.throws(attempt, (error) => error === failure);
    }
    const item = { data: { paramShape: { kind: 'rect', tl: 40 } } };
    const a = api.detachedElementView(item, kernels.seedElementHolder), b = api.detachedElementView(item, kernels.seedElementHolder);
    assert.notEqual(a, b); assert.equal(Object.isFrozen(a.motionStatic.cornerTL), true);
    assert.deepEqual(Array.from(a.motionStatic.cornerTL), [40]); assert.equal(item.data.elementMotion, undefined);
    const expression = { expressions: { opacity: { code: 'value', enabled: true, nested: { a: 1 } } } };
    const view = api.detachedExpressionView(expression, 'opacity', kernels.expressionSeed);
    assert.notEqual(view, expression.expressions.opacity); assert.equal(Object.isFrozen(view.nested), true);
    assert.equal(api.detachedExpressionView({}, 'opacity', kernels.expressionSeed).enabled, false);
    assert.equal(api.expressionSnapshot(null, 'opacity'), null);
    assert.throws(() => api.detachedElementView(item, null), /callback/);
    assert.throws(() => api.detachedExpressionView(expression, 'opacity', null), /callback/);
    assert.throws(() => api.expressionSnapshot(expression, 'opacity', null, null), /callbacks/);
    const seedFailure = new Error('seed failure');
    assert.throws(() => api.detachedElementView(item, () => { throw seedFailure; }), (error) => error === seedFailure);
  }
  const plan = NativeMotionSurface.renderedOpacityRoute(harness.controller, holder, 'opacity');
  assert.equal(Object.isFrozen(plan), true); assert.equal(plan.layerUid, holder.layerUid);
  const first = { legacy: (...args) => harness.controller.legacyIntent(...args) };
  assert.equal(NativeMotionSurface.routeDimension(plan, first, [25], 0, 40, 10), true);
  assert.throws(() => harness.controller.prepared(), /pending/);
  const second = { legacy: (...args) => harness.controller.legacyIntent(...args) };
  assert.equal(NativeMotionSurface.routeDimension(plan, second, [25], 0, 60, 10), true);
  await harness.controller.flush();
  assert.deepEqual(harness.controller.valueAtFrame(holder.layerUid, 10), [60]);
});

test('legacy Motion evaluation remains lazy without the surface while native composition fails closed', () => {
  const state = { ...defaultState(), layers: clone(staticSource().layers), currentFrame: 0, activeLayerIdx: 0 };
  const motion = loadLegacyMotion(state);
  assert.deepEqual(Array.from(motion.SMMotion.valueAtFrame(state.layers[0], 'opacity', 0)), [25]);
  assert.deepEqual(Array.from(motion.SMMotion.valueAtFrame(state.layers[0], 'position', 10)), [64, 0]);
  motion.SMMotion.setLayerValue(0, 'scale', [80, 90]);
  assert.deepEqual(Array.from(motion.SMMotion.valueAtFrame(state.layers[0], 'scale', 0)), [80, 90]);
  motion.sandbox.NemoNativeOpacityCutover = { blocksLegacy: () => true };
  assert.throws(() => motion.SMMotion.valueAtFrame(state.layers[0], 'opacity', 0), /surface is unavailable/);
  const base = { contract: NativeOpacityContract, lifecycle: NativeOpacityLifecycle, operations: NativeOpacityOperations };
  for (const surface of [undefined, {}, Object.freeze({ requireAvailable() {} })]) {
    assert.throws(() => OpacityApplication.createNative({}, { ...base, motionSurface: surface }), /Motion surface/);
  }
  const modules = { ...base, motionSurface: NativeMotionSurface };
  const controller = OpacityApplication.createNative({}, modules);
  modules.motionSurface = null;
  assert.throws(() => controller.activate({}), /Motion surface/);
  assert.equal(controller.status(), 'legacy', 'adapter disappearance cannot transfer authority');
});

function motionViewFixture(withSurface) {
  const holder = clone(staticSource().layers[0]);
  const bounds = { center: { x: 30, y: 70 }, left: 20, top: 60, right: 40, bottom: 80, width: 20, height: 20 };
  const shapes = [
    ['rect', { kind: 'rect', tl: 40, tr: 0, br: 7, bl: 12 }, { cornerTL: [40], cornerTR: [0], cornerBR: [7], cornerBL: [12] }],
    ['rect-default', { kind: 'rect' }, { cornerTL: [0], cornerTR: [0], cornerBR: [0], cornerBL: [0] }],
    ['ellipse', { kind: 'ellipse', startAngle: 15, sweep: 0, innerRadius: 0.2 }, { arcStart: [15], arcSweep: [0], arcInner: [20] }],
    ['ellipse-default', { kind: 'ellipse' }, { arcStart: [0], arcSweep: [359.9], arcInner: [0] }],
    ['star', { kind: 'star', innerRatio: 0, cornerRadius: 4 }, { starInner: [0], starCorner: [4] }],
    ['star-default', { kind: 'star' }, { starInner: [50], starCorner: [0] }],
  ];
  const paper = { bounds, opacity: 0.25, children: shapes.map(([strokeId, paramShape]) => ({
    data: { strokeId, paramShape }, bounds, children: [],
  })) };
  const state = { ...defaultState(), layers: [holder], currentFrame: 0, activeLayerIdx: 0, appMode: 'motion' };
  const loader = withSurface ? loadMotion : loadLegacyMotion;
  const motion = loader(state, { beforeMotion(sb) {
    sb.userLayers = [paper]; sb._layerSel = [0]; sb._layerIndexByUid = () => -1;
    sb._motionExpandedLayer = 0;
  } });
  return { motion, state, holder, paper, shapes };
}

test('real no-surface Motion materializes legacy shape defaults and missing expression snapshots', () => {
  const { motion, holder, paper, shapes } = motionViewFixture(false);
  assert.equal(motion.sandbox.NemoNativeOpacityMotionSurface, undefined);
  const paperBefore = JSON.stringify(paper);
  for (const [id, params, expected] of shapes) {
    motion.sandbox._motionExpandedElement = id;
    const target = motion.SMMotion.activeMotionTarget();
    assert.equal(target.strokeId, id);
    assert.strictEqual(target.holder, holder.elementMotion[id]);
    assert.equal(Object.isFrozen(target.holder), false);
    assert.deepEqual(JSON.parse(JSON.stringify(target.holder)), { paramShapeKind: params.kind, motionStatic: expected });
    assert.strictEqual(motion.SMMotion.ensureElementHolder(holder, id), target.holder);
  }
  const fresh = motionViewFixture(false);
  const snapshot = fresh.motion.SMMotion.exprSnapshotFor({ uid: fresh.holder.layerUid, elem: 'rect' }, 'opacity');
  assert.deepEqual(Object.keys(snapshot), ['code', 'enabled', 'lastError', 'errorLine']);
  assert.deepEqual({ ...snapshot }, { code: '', enabled: false, lastError: null, errorLine: undefined });
  assert.equal(Object.isFrozen(snapshot), false);
  assert.deepEqual(JSON.parse(JSON.stringify(fresh.holder.elementMotion.rect)), {
    paramShapeKind: 'rect', motionStatic: shapes[0][2],
    expressions: { opacity: { code: '', enabled: false, lastError: null } },
  });
  assert.equal(JSON.stringify(paper), paperBefore);
});

test('real surface-present Motion returns detached frozen shape and expression views without mirror writes', () => {
  const { motion, state, holder, paper, shapes } = motionViewFixture(true);
  const before = JSON.stringify({ state, paper });
  for (const [id, params, expected] of shapes) {
    motion.sandbox._motionExpandedElement = id;
    const first = motion.SMMotion.activeMotionTarget().holder;
    const second = motion.SMMotion.activeMotionTarget().holder;
    assert.notStrictEqual(first, second);
    assert.equal(Object.isFrozen(first), true); assert.equal(Object.isFrozen(first.motionStatic), true);
    assert.ok(Object.values(first.motionStatic).every(Object.isFrozen));
    assert.deepEqual(JSON.parse(JSON.stringify(first)), { paramShapeKind: params.kind, motionStatic: expected });
  }
  const snapshot = motion.SMMotion.exprSnapshotFor({ uid: holder.layerUid }, 'opacity');
  assert.equal(Object.isFrozen(snapshot), true);
  assert.deepEqual(Object.keys(snapshot), ['code', 'enabled', 'lastError', 'errorLine']);
  assert.deepEqual({ ...snapshot }, { code: '', enabled: false, lastError: null, errorLine: undefined });
  assert.equal(motion.SMMotion.exprSnapshotFor({ uid: holder.layerUid, elem: 'rect' }, 'opacity'), null);
  assert.equal(JSON.stringify({ state, paper }), before);
});

test('real Motion panel builds the expression editor before adapter availability and stays detached with it', () => {
  for (const withSurface of [false, true]) {
    const { motion, state, holder, paper } = motionViewFixture(withSurface);
    const created = [], createElement = motion.sandbox.document.createElement;
    motion.sandbox.document.createElement = function (tag) {
      const node = createElement(tag);
      node.children = []; node.listeners = new Map();
      node.addEventListener = (kind, callback) => node.listeners.set(kind, callback);
      node.appendChild = function (child) { this.children.push(child); return child; };
      created.push(node); return node;
    };
    const panel = motion.sandbox.document.createElement('div');
    motion.sandbox.document.getElementById = (id) => id === 'motion-props-body' ? panel : null;
    motion.sandbox._exprEditorOpen = { holder, prop: 'opacity' };
    const before = JSON.stringify({ state, paper });
    motion.SMMotion.renderMotionPropsPanel();
    assert.ok(created.some((node) => node.className === 'lrow motion-expr-editor'));
    assert.ok(created.some((node) => node.className === 'motion-expr-grip'));
    if (withSurface) assert.equal(JSON.stringify({ state, paper }), before);
    else assert.deepEqual(JSON.parse(JSON.stringify(holder.expressions)), {
      opacity: { code: '', enabled: false, lastError: null },
    });
    assert.equal(JSON.stringify(paper), JSON.stringify(JSON.parse(before).paper));
  }
});

test('canonical shape seeding preserves the original live getter order', () => {
  for (const [kind, fields] of [['rect', ['tl', 'tr', 'br', 'bl']],
    ['ellipse', ['startAngle', 'sweep', 'sweep', 'innerRadius']],
    ['star', ['innerRatio', 'innerRatio', 'cornerRadius']]]) {
    const { motion, holder, paper } = motionViewFixture(false), calls = [];
    const shape = {};
    Object.defineProperty(shape, 'kind', { get() { calls.push('kind'); return kind; } });
    for (const field of new Set(fields)) Object.defineProperty(shape, field, { get() { calls.push(field); return 1; } });
    const data = { strokeId: 'getter' };
    Object.defineProperty(data, 'paramShape', { get() { calls.push('paramShape'); return shape; } });
    const item = {};
    Object.defineProperty(item, 'data', { get() { calls.push('data'); return data; } });
    paper.children = [item];
    motion.SMMotion.ensureElementHolder(holder, 'getter');
    const conditions = ['rect', 'ellipse', 'star'].indexOf(kind) + 1;
    assert.deepEqual(calls, ['data', ...Array.from({ length: conditions }, () =>
      ['data', 'data', 'paramShape', 'data', 'paramShape', 'kind']).flat(), 'data', 'paramShape', ...fields]);
  }
});

test('N20 file-local legacy admission fails closed without relying on app.js globals', () => {
  for (const file of ['motion.js', 'timeline.js', 'tweens.js']) {
    const source = fs.readFileSync(path.join(ROOT, 'src/js', file), 'utf8');
    const start = source.indexOf('var n20AllowLegacyWrite=');
    const end = source.indexOf('\n};', source.indexOf('var n20RequireLegacyWrite=', start)) + 3;
    assert.ok(start >= 0 && end > start, file);
    const context = { window: {} };
    vm.createContext(context);
    vm.runInContext(source.slice(start, end), context, { filename: `${file}:n20-admission` });
    assert.equal(context.n20AllowLegacyWrite('absent'), true, `${file}: absent preserves legacy`);
    context.window.NemoNativeOpacityLegacyAdmission = {};
    assert.equal(context.n20AllowLegacyWrite('malformed'), false, `${file}: malformed denies`);
    context.window.NemoNativeOpacityLegacyAdmission = { allow() { throw new Error('boom'); } };
    assert.equal(context.n20AllowLegacyWrite('throwing'), false, `${file}: throwing denies`);
    context.window.NemoNativeOpacityLegacyAdmission = { allow() { return true; } };
    assert.equal(context.n20RequireLegacyWrite('exact-true'), true, `${file}: exact true admits`);
    context.window.NemoNativeOpacityLegacyAdmission = { allow() { return 1; } };
    assert.throws(() => context.n20RequireLegacyWrite('non-boolean'), (error) =>
      error && error.name === 'NemoNativeReleaseRequired', `${file}: non-boolean denies`);
  }
});

test('production file-local initialization preserves valid centrals and rejects every present invalid guard', () => {
  for (const file of ['motion.js', 'timeline.js', 'tweens.js']) {
    const source = fs.readFileSync(path.join(ROOT, 'src/js', file), 'utf8');
    const start = source.indexOf('var n20AllowLegacyWrite=');
    const end = source.indexOf('\n};', source.indexOf('var n20RequireLegacyWrite=', start)) + 3;
    const cases = [
      ['null central', null, { allow: () => true }, false],
      ['object central', {}, { allow: () => true }, false],
      ['string central', 'allow', { allow: () => true }, false],
      ['denying central', () => false, { allow: () => true }, false],
      ['nonboolean central', () => 1, { allow: () => true }, false],
      ['throwing central', () => { throw new Error('central failure'); }, { allow: () => true }, false],
      ['allowing central', () => true, null, true],
      ['absent admission', undefined, undefined, true],
      ['null admission', undefined, null, false],
      ['object admission', undefined, {}, false],
      ['string admission', undefined, 'allow', false],
      ['nonboolean admission', undefined, { allow: () => 1 }, false],
      ['throwing admission', undefined, { allow() { throw new Error('admission failure'); } }, false],
      ['allowing admission', undefined, { allow: () => true }, true],
    ];
    for (const [label, central, admission, allowed] of cases) {
      const context = { window: { NemoNativeOpacityLegacyAdmission: admission } };
      if (central !== undefined) context.n20AllowLegacyWrite = central;
      vm.runInNewContext(source.slice(start, end), context, { filename: `${file}:production-initialization` });
      assert.equal(typeof context.n20AllowLegacyWrite, 'function', `${file}: ${label}`);
      if (typeof central === 'function') assert.strictEqual(context.n20AllowLegacyWrite, central);
      if (allowed) assert.equal(context.n20RequireLegacyWrite(label), true, `${file}: ${label}`);
      else assert.throws(() => context.n20RequireLegacyWrite(label), { name: 'NemoNativeReleaseRequired' }, `${file}: ${label}`);
    }
    const context = { window: {} };
    Object.defineProperty(context.window, 'NemoNativeOpacityLegacyAdmission', {
      get() { throw new Error('admission getter failure'); },
    });
    vm.runInNewContext(source.slice(start, end), context, { filename: `${file}:production-initialization` });
    assert.throws(() => context.n20RequireLegacyWrite('getter'), { name: 'NemoNativeReleaseRequired' }, file);
  }
});

test('central denial dominates production reorder, import and frame history retained originals', () => {
  const app = fs.readFileSync(path.join(ROOT, 'src/js/app.js'), 'utf8');
  const timeline = fs.readFileSync(path.join(ROOT, 'src/js/timeline.js'), 'utf8');
  const tweens = fs.readFileSync(path.join(ROOT, 'src/js/tweens.js'), 'utf8');
  const start = timeline.indexOf('  importJSON:function(json,silent){');
  const end = timeline.indexOf('\n  getState:function()', start);
  const importSource = timeline.slice(start, end).replace('  importJSON:', '').replace(/},\s*$/, '}');
  const cases = [
    ['central false, absent port', () => false, undefined],
    ['central false, allowing port', () => false, { allow: () => true }],
    ['central malformed', {}, { allow: () => true }],
    ['central throws', () => { throw new Error('deny'); }, { allow: () => true }],
    ['central nonboolean', () => 1, { allow: () => true }],
    ['fallback denies', undefined, { allow: () => false }],
    ['fallback malformed', undefined, {}],
    ['fallback null', undefined, null],
    ['fallback throws', undefined, { allow() { throw new Error('deny'); } }],
    ['fallback nonboolean', undefined, { allow: () => 1 }],
  ];
  for (const [label, central, admission] of cases) {
    const effects = { parses: 0, captures: 0 };
    const scope = { state: { layers: [{ name: 'a' }, { name: 'b' }], currentFrame: 0,
      undoStack: [], undoLabels: [], redoStack: [], redoLabels: [] },
      n20AllowLegacyWrite: central, NemoNativeOpacityLegacyAdmission: admission,
      SMProjectDocument: { parse() { effects.parses++; } },
      NemoFrameHistoryEntry: { capture() { effects.captures++; } } };
    scope.window = scope;
    vm.createContext(scope);
    vm.runInContext(`${extractFunction(app, 'reorderLayersAtGap')}\n` +
      `${extractFunction(tweens, 'pushUndoActiveFrame')}\nthis.importJSON = (${importSource});`, scope);
    // Retain the production entry points before the public wrappers are installed.
    const originalReorder = scope.reorderLayersAtGap;
    const originalImport = scope.importJSON;
    const originalHistory = scope.pushUndoActiveFrame;
    for (const name of ['reorderLayersAtGap', 'importJSON', 'pushUndoActiveFrame']) {
      NativeLegacySurface.wrapWriter(scope, name, () => false, 'public-wrapper');
    }
    const before = JSON.stringify(scope.state);
    assert.equal(originalReorder([0], 2, true), false, label);
    for (const attempt of [() => originalImport('{}', true), originalHistory]) {
      assert.throws(attempt, (error) => error.name === 'NemoNativeReleaseRequired', label);
    }
    assert.equal(JSON.stringify(scope.state), before, label);
    assert.deepEqual(effects, { parses: 0, captures: 0 }, label);
  }
});

test('lifecycle exposes frozen observations and fresh opaque sessions across A release B reentry', async () => {
  const harness = nativeHarness(staticSource());
  await harness.controller.activate(harness.prepared);
  const first = harness.state.lifecycle.inspect();
  assert.equal(Object.isFrozen(first), true);
  assert.equal(Object.isFrozen(first.identity), true);
  assert.equal(Object.isFrozen(first.prepared), true);
  assert.equal(Object.isFrozen(first.session), true);
  assert.deepEqual(Object.keys(first.session), []);
  assert.equal(first.cycle, undefined);
  for (const mutate of [() => { first.phase = 'legacy'; },
    () => { first.identity.contentRevision = 100; },
    () => { first.session.active = false; },
    () => { first.prepared.shell.layers.length = 0; }]) assert.throws(mutate, TypeError);
  assert.equal(harness.controller.isActive(), true);
  assert.deepEqual(harness.controller.valueAtFrame('r08_curve_layer', 10), [25]);
  const oldCallback = harness.state.subscriptions[0].synchronize;
  await harness.controller.releaseCurrent('opaque-session-a');
  assert.equal(harness.state.lifecycle.inspect().session, null);
  await harness.controller.activate(harness.prepared);
  const second = harness.state.lifecycle.inspect();
  assert.notStrictEqual(second.session, first.session);
  assert.equal(Object.isFrozen(second.session), true);
  await assert.rejects(oldCallback({ instanceId: first.identity.instanceId,
    documentId: first.identity.documentId, lifecycleGeneration: 1,
    fromRevision: 0, toRevision: 1, requestId: 'old-callback' }), /closed lifecycle/);
  assert.equal(harness.controller.status(), 'native');
  assert.equal(harness.controller.identity().documentId, second.identity.documentId);
});

test('retained app originals reach a production require choke before any mirror mutation', () => {
  const app = fs.readFileSync(path.join(ROOT, 'src/js/app.js'), 'utf8');
  const tweens = fs.readFileSync(path.join(ROOT, 'src/js/tweens.js'), 'utf8');
  const cases = [
    ['convertSelectionToObjectDuplicator', [[0, 1]]], ['convertLayerToComponent', [0]],
    ['convertLayersToComponent', [[0, 1]]], ['splitLayerIntoElements', [0]],
    ['convertComponentToLayer', [0], (state) => { state.layers[0].symbolId = 'symbol'; }],
    ['convertLayerToLFSGroup', [0]],
    ['convertLFSGroupToLayer', [0], (state) => { state.layers[0].lfsGroup = true; }],
    ['setElemHidden', [0, ['stroke'], true]], ['insertFrame', []], ['insertKeyframe', []],
    ['insertKeyframeAt', [0, 1]], ['insertBlankKeyframe', []], ['clearKeyframe', []],
    ['removeTweenSpan', [0, 0, 10]], ['convertToKeyframes', []], ['removeFrameSpan', []],
    ['createUserLayer', ['blocked']], ['mergeRemoteSnapshot', [{ layers: [] }, {}]],
    ['rigResetPose', [{}]], ['rigCommitFrame', [{}]], ['rigSetIK', [{}, {}, {}, {}, false]],
    ['exposeSymbolProperty', ['symbol', 'stroke', 'opacity', 'Opacity', 25]],
    ['convertLayerToStrokeFillShadowFolder', [0]], ['removeFrame', []],
    ['splitLayerIntoElementsCore', [0, { silent: true }]],
    ['mergeLayersIntoOne', [[0, 1], { silent: true }]], ['propagateLFSFill', [0, 'fill']],
    ['duplicateSelectedFrames', []], ['saveActiveLayerFrame', []], ['saveAllLayerFrames', []],
  ];
  for (const [name, args, prepare] of cases) {
    const state = { ...clone(staticSource()), currentFrame: 0, activeLayerIdx: 0,
      symbols: { symbol: {} }, undoStack: [], undoLabels: [], redoStack: [], redoLabels: [] };
    state.layers.push(clone(state.layers[0]));
    if (prepare) prepare(state);
    const scope = { state, userLayers: [{ children: [] }, { children: [] }],
      _sel: { frames: [] }, _layerSel: [], _elemHidden: {},
      selBounds() { return { minF: 0, maxF: 1 }; }, SM: { t: (key) => key },
      showToast() {}, badComponentSourceReason() { return null; },
      n20AllowLegacyWrite() { return false; } };
    scope.window = scope;
    scope.n20RequireLegacyWrite = (kind) => NativeLegacySurface.requireLegacyWrite(scope, kind, scope.n20AllowLegacyWrite);
    vm.createContext(scope);
    const names = new Set([name, 'saveActiveLayerFrame', 'saveAllLayerFrames', 'splitLayerIntoElementsCore']);
    vm.runInContext([...names].map((value) => extractFunction(app, value)).join('\n') + '\n' +
      ['pushUndo', 'pushUndoLayers'].map((value) => extractFunction(tweens, value)).join('\n'), scope);
    const original = scope[name];
    NativeLegacySurface.wrapWriter(scope, name, () => false, 'public-wrapper');
    const before = JSON.stringify({ state, paper: scope.userLayers });
    let denied = false;
    try { denied = original(...args) === false; }
    catch (error) { assert.equal(error.name, 'NemoNativeReleaseRequired', name); denied = true; }
    assert.equal(denied, true, name);
    assert.equal(JSON.stringify({ state, paper: scope.userLayers }), before, name);
  }
});

function installRetainedAdmission(scope, harness) {
  const releases = [];
  vm.runInNewContext(fs.readFileSync(path.join(ROOT, 'src/js/application/native-edit-guard.js'), 'utf8'), scope);
  scope.NemoNativeOpacityLegacyAdmission = scope.SMNativeEditGuard.install({
    getNativeIdentity: () => harness.state.lifecycle.getNativeIdentity(),
    requestRelease(request) { releases.push(request); return harness.state.lifecycle.requestRelease(request); },
  });
  scope.n20AllowLegacyWrite = (kind) => NativeLegacySurface.allowLegacyWrite(scope, kind);
  scope.n20RequireLegacyWrite = (kind) => NativeLegacySurface.requireLegacyWrite(scope, kind, scope.n20AllowLegacyWrite);
  return releases;
}

test('18 retained raw Motion callbacks hit the production REQUIRE before their first write', async () => {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/motion.js'), 'utf8');
  const tweens = fs.readFileSync(path.join(ROOT, 'src/js/tweens.js'), 'utf8');
  const cases = [
    ['applyCurveToSelection', ['hold']], ['nudgeSelectedKeys', [1]], ['deleteSelectedKeys', []],
    ['pasteKeys', []], ['toggleLayerDuplicator', [0]], ['buildKeySelectionLockMenuItems', []],
    ['setLayerTimeLink', [0, 1, 'both']], ['setExpressionCode', ['holder', 'position', 'value + 1']],
    ['distributeKeys', []], ['flipKeys', []], ['setSelectedEaseInfluence', ['out', 50]],
    ['colorSelectedKeys', ['#ff0000']], ['subdivideKeys', []], ['enableTimeRemap', [0]],
    ['disableTimeRemap', [0]], ['staggerSelectedLayers', [1]],
    ['applyExprCode', [{ uid: 'r08_curve_layer' }, 'position', 'value + 1']],
    ['setExprEnabled', [{ uid: 'r08_curve_layer' }, 'position', true]],
  ];
  for (const [name, values] of cases) {
    const harness = nativeHarness(staticSource());
    const holder = clone(staticSource().layers[0]);
    holder.expressions = { position: { code: 'value', enabled: false } };
    holder.symbolId = 'symbol';
    if (name === 'disableTimeRemap') holder.timeRemap = { keys: [{ frame: 0, v: [0] }] };
    const state = { ...defaultState(), layers: [holder, clone(holder)], activeLayerIdx: 0,
      currentFrame: 0, totalFrames: 21, symbols: { symbol: { totalFrames: 21 } } };
    const scope = { state, _layerSel: [0, 1], userLayers: [{ opacity: 0.25, children: [] }],
      _motionKeySel: holder.motion.position.keys.slice(0, name === 'nudgeSelectedKeys' ? 1 : 2)
        .map((key) => ({ holder, prop: 'position', key })),
      _keyClip: [{ prop: 'position', dt: 1, v: [42, 0] }], SM: { t: (key) => key },
      trackFor: (target, prop) => target.motion[prop], keyAt: (track, frame) => track.keys.find((key) => key.frame === frame),
      timeLinkWouldCycle: () => false, holderFromRef: () => holder,
      ensureExpr: (target, prop) => target.expressions[prop],
      pushUndoLayers() { throw new Error('native denial must precede capture'); },
    };
    scope.window = scope;
    const releases = installRetainedAdmission(scope, harness);
    const named = source.replace(`${name}: function (`, `function ${name}(`);
    vm.runInNewContext(extractFunction(named, name) + '\n' + extractFunction(tweens, 'pushUndo'), scope);
    const original = name === 'buildKeySelectionLockMenuItems'
      ? scope[name]().find((entry) => !entry.disabled).action : scope[name];
    assert.equal(releases.length, 0, 'callback is captured before native activation');
    await harness.controller.activate(harness.prepared);
    const before = JSON.stringify({ state, paper: scope.userLayers });
    assert.throws(() => original(...values.map((value) => value === 'holder' ? holder : value)),
      { name: 'NemoNativeReleaseRequired' }, name);
    assert.equal(releases.length, 1, name);
    assert.equal(releases[0].kind, 'history-checkpoint', name);
    assert.equal(JSON.stringify({ state, paper: scope.userLayers }), before, name);
    await harness.controller.releaseCurrent('retained-raw-test-complete');
  }
});

test('20 production Motion publications protect references captured before native bootstrap', async () => {
  const cases = [
    ['addExprControl', (h) => [h, 'number', 'New control'], null],
    ['renameExprControl', (h) => [h, 'xc_first', 'Renamed'], false],
    ['removeExprControl', (h) => [h, 'xc_first'], false],
    ['moveExprControl', (h) => [h, 'xc_first', 1], false],
    ['migrateLegacyCurves', () => [], 0],
    ['setKeyAtFrame', (h) => [h, 'position', 5, [10, 20]], false],
    ['toggleLayer3D', () => [0], false], ['setDuplicatorEditSource', () => [0, true], false],
    ['setLayerParent', () => [0, 'parent'], false], ['setLayerParentB', () => [0, 'parent'], false],
    ['setLayerFollowPath', () => [0, 'parent'], false], ['addTextAnimator', () => [0, ['position'], {}], null],
    ['removeTextAnimator', () => [0, 'animator'], false],
    ['upsertBlendKeyAt', (h) => [h, 5, 'multiply'], false], ['removeBlendKeyAt', (h) => [h, 0], false],
    ['removeParentKeyAt', (h) => [h, 0], false], ['shiftLayerMotionKeys', () => [0, 1], false],
    ['setExprGlobals', () => ['const updated = 1;'], false],
    ['shiftKeySelection', (h) => [[{ holder: h, prop: 'position', key: h.motion.position.keys[0] }], 1], false],
    ['onEaseSegChanged', (h) => [h.motion.position.keys[0]], false],
  ];
  for (const [name, args, deniedValue] of cases) {
    const harness = nativeHarness(staticSource());
    const holder = clone(staticSource().layers[0]);
    Object.assign(holder, { exprControls: [{ key: 'xc_first', name: 'First', type: 'number', default: [0] },
      { key: 'xc_second', name: 'Second', type: 'number', default: [0] }],
    textAnimators: [{ id: 'animator', props: ['position'] }], duplicator: { enabled: true },
    blendKeys: [{ frame: 0, mode: 'normal' }], parentKeys: [{ frame: 0, uid: null }] });
    holder.motion.position.keys[0].curvePoints = [
      { x: 0, y: 0 }, { x: 0.42, y: 0 }, { x: 0.58, y: 1 }, { x: 1, y: 1 }];
    const state = { ...defaultState(), layers: [holder, { layerUid: 'parent' }], currentFrame: 0, activeLayerIdx: 0 };
    const motion = loadMotion(state);
    const original = motion.SMMotion[name];
    assert.equal(motion.sandbox.NemoOpacityApplication, undefined, 'capture precedes operations/bootstrap');
    const releases = installRetainedAdmission(motion.sandbox, harness);
    motion.sandbox.userLayers = [{ opacity: 0.25, children: [] }];
    await harness.controller.activate(harness.prepared);
    const before = JSON.stringify({ state, paper: motion.sandbox.userLayers });
    assert.equal(original(...args(holder)), deniedValue, name);
    assert.equal(releases.length, 1, name);
    assert.equal(JSON.stringify({ state, paper: motion.sandbox.userLayers }), before, name);
    await harness.controller.releaseCurrent('retained-publication-test-complete');
  }
});

test('captured production 3D DOM listener resolves the guarded publication dynamically', async () => {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/motion.js'), 'utf8');
  const harness = nativeHarness(staticSource());
  const holder = clone(staticSource().layers[0]);
  const state = { ...defaultState(), layers: [holder], currentFrame: 0, activeLayerIdx: 0 };
  const motion = loadMotion(state);
  const scope = motion.sandbox, listeners = new Map();
  Object.assign(scope, { li: 0, ld: holder, ICO_3D: '3D', row: { appendChild() {} } });
  scope.document.createElement = () => ({ addEventListener: (kind, callback) => listeners.set(kind, callback) });
  const start = source.indexOf("      var d3 = document.createElement('div');");
  const end = source.indexOf('\n      row.appendChild(d3);', start) + '\n      row.appendChild(d3);'.length;
  assert.ok(start >= 0 && end > start);
  vm.runInNewContext(source.slice(start, end), scope, { filename: 'motion.js production 3D row builder' });
  const callback = listeners.get('click'), original = motion.SMMotion.toggleLayer3D;
  let dynamicCalls = 0;
  motion.SMMotion.toggleLayer3D = function (...args) { dynamicCalls++; return original.apply(this, args); };
  const releases = installRetainedAdmission(scope, harness);
  scope.userLayers = [{ opacity: 0.25, children: [] }];
  await harness.controller.activate(harness.prepared);
  const before = JSON.stringify({ state, paper: scope.userLayers });
  callback({ stopPropagation() {} });
  assert.equal(dynamicCalls, 1); assert.equal(releases.length, 1);
  assert.equal(releases[0].kind, 'motion-toggle-layer-3d');
  assert.equal(JSON.stringify({ state, paper: scope.userLayers }), before);
  await harness.controller.releaseCurrent('retained-3d-test-complete');
});

test('an expression resize begun before activation cannot persist from its retained mouseup', async () => {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/motion.js'), 'utf8');
  const harness = nativeHarness(staticSource());
  const holder = { expressions: { opacity: { code: 'value', enabled: false, editorHeight: 40 } } };
  const nodes = [], documentListeners = new Map();
  function element(tag) {
    const node = { tag, style: {}, children: [], listeners: new Map(), value: '', offsetHeight: 40,
      classList: { add() {}, remove() {} }, appendChild(child) { this.children.push(child); },
      addEventListener(kind, callback) { this.listeners.set(kind, callback); } };
    nodes.push(node); return node;
  }
  const scope = { document: { createElement: element, createTextNode: (text) => ({ text }),
    addEventListener: (kind, callback) => documentListeners.set(kind, callback),
    removeEventListener: (kind) => documentListeners.delete(kind) },
  state: { layers: [holder], currentFrame: 0 }, userLayers: [{ children: [] }],
  SM: { t: (key) => key }, PROP_DIM: { opacity: 1 }, PROP_UNIT: {},
  nativeMotionSurface: () => NativeMotionSurface, ownedRawValueAtFrame: () => [25], exprGlobals: () => '',
  renderTimeline() { throw new Error('denied mouseup cannot render'); } };
  scope.window = scope;
  const releases = installRetainedAdmission(scope, harness);
  vm.runInNewContext(['expressionSeed', 'ensureExpr', 'buildExprEditorRow'].map((name) => extractFunction(source, name)).join('\n'), scope);
  scope.buildExprEditorRow(holder, 'opacity');
  nodes.find((node) => node.className === 'motion-expr-grip').listeners.get('mousedown')({
    preventDefault() {}, stopPropagation() {}, clientY: 10 });
  documentListeners.get('mousemove')({ clientY: 70 });
  const up = documentListeners.get('mouseup');
  assert.equal(typeof up, 'function'); assert.equal(releases.length, 0);
  await harness.controller.activate(harness.prepared);
  const before = JSON.stringify({ state: scope.state, paper: scope.userLayers });
  assert.throws(up, { name: 'NemoNativeReleaseRequired' });
  assert.equal(releases.length, 1); assert.equal(releases[0].kind, 'motion-expression-editor-height');
  assert.equal(JSON.stringify({ state: scope.state, paper: scope.userLayers }), before);
  assert.equal(documentListeners.size, 0);
  await harness.controller.releaseCurrent('retained-resize-test-complete');
});

test('operations owns real resize callbacks and cancels or fences work across release and reentry', async () => {
  const listeners = new Map(), timers = new Map(), cancelled = [], calls = [];
  let nextTimer = 0, rejectResize;
  const root = { __TAURI__: { core: { invoke(command, args) {
    calls.push([command, args]);
    return new Promise((_resolve, reject) => { rejectResize = reject; });
  } } }, devicePixelRatio: 1,
  document: { getElementById() { return { getBoundingClientRect() {
    return { left: 0, top: 0, width: 320, height: 180 };
  } }; } },
  setTimeout(callback) { timers.set(++nextTimer, callback); return nextTimer; },
  clearTimeout(timer) { cancelled.push(timer); timers.delete(timer); },
  addEventListener(type, callback) { listeners.set(type, callback); },
  removeEventListener(type, callback) { if (listeners.get(type) === callback) listeners.delete(type); },
  SM: { importJSON() { return true; } }, SMEngineBridge: {},
  SMNativeEditGuard: { install(lifecycle) { return { allow: () => !lifecycle.blocksLegacy() }; } },
  NemoApplication: { handle: () => 'legacy', capabilities: () => [] },
  NemoOpacityApplication: { meta: () => ({}), legacy: () => 'legacy' } };
  const surface = NativeLegacySurface.desktopPorts(root, {}).surface;
  const harness = nativeHarness(staticSource(), { surface });
  const installation = harness.controller.install();
  assert.strictEqual(harness.controller.install(), installation);
  await harness.controller.activate(harness.prepared);
  listeners.get('resize')();
  const staleTimer = nextTimer, staleCallback = timers.get(staleTimer);
  await harness.controller.releaseCurrent('resize-a');
  assert.deepEqual(cancelled, [staleTimer]);
  await harness.controller.activate(harness.prepared);
  await staleCallback();
  assert.equal(calls.length, 0, 'a retained cancelled A callback cannot resize B');
  listeners.get('resize')();
  const pending = timers.get(nextTimer)();
  assert.equal(calls.length, 1);
  assert.equal(calls[0][0], 'nemo_native_viewport_resize');
  await harness.controller.releaseCurrent('resize-b');
  await harness.controller.activate(harness.prepared);
  rejectResize(new Error('late B resize failed'));
  await pending;
  assert.equal(harness.state.releases, 2, 'late B failure cannot request release of C');
  assert.equal(harness.controller.status(), 'native');
  assert.throws(() => installation.dispose(), /must release/);
  await harness.controller.releaseCurrent('dispose-c');
  installation.dispose();
  assert.equal(listeners.size, 0);
  assert.equal(root.NemoNativeOpacityCutover, undefined);
  assert.equal(root.NemoApplication.handle(), 'legacy');
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

  const released = await harness.controller.releaseCurrent('replacement');
  assert.deepEqual(released, { documentId: 'native-document-1', generation: 1,
    owner: 'legacy', status: 'released' });
  assert.equal(harness.controller.status(), 'legacy');
  assert.equal(harness.state.releases, 1);
  assert.equal(harness.state.disconnects, 1);
  assert.equal(harness.state.imports[0].layers[0].motionStatic.opacity[0], 40);
});

test('afterChange observes readable post-write caches for UI, v1, and external native mutations', async () => {
  const source = staticSource();
  const observations = [];
  let motion;
  const harness = nativeHarness(source, { afterChange(controller) {
    const observation = {
      revision: controller.identity().contentRevision,
      opacity: controller.valueAtFrame('r08_curve_layer', 10)[0],
      position: Array.from(motion.SMMotion.rawValueAtFrame(source.layers[0], 'position', 10)),
      persisted: JSON.parse(controller.persistenceJSON()).layers[0].motionStatic.opacity[0],
    };
    observation.previewAdmitted = controller.renderPreview(10);
    observation.readableAfterPreview = controller.persistenceJSON() !== null;
    observations.push(observation);
  } });
  const motionState = defaultState();
  Object.assign(motionState, { layers: source.layers, activeLayerIdx: 0,
    currentFrame: 10, totalFrames: 21, appMode: 'motion' });
  motion = loadMotion(motionState, { beforeMotion(sb) {
    sb.NemoNativeOpacityCutover = Object.freeze({
      blocksLegacy: () => harness.controller.blocksLegacy(),
      isActive: () => harness.controller.isActive(),
      prepared: () => harness.controller.prepared(),
      identity: () => harness.controller.identity(),
      projectSelection: (selection, frame) => harness.controller.projectSelection(
        JSON.parse(JSON.stringify(selection)), frame),
    });
  } });

  assert.equal(await harness.controller.activate(harness.prepared), true);
  await harness.controller.setOpacityFromUi('r08_curve_layer', 40, 'ui-after-change');
  const identity = harness.controller.identity();
  const v1 = await harness.controller.handleV1({ apiVersion: 1, requestId: 'v1-after-change',
    instanceId: identity.instanceId, documentId: identity.documentId,
    expectedRevision: identity.contentRevision, operation: 'property.set',
    payload: { layerId: 'r08_curve_layer', property: 'opacity', value: 60 } });
  assert.equal(v1.ok, true);
  await harness.externalOpacity(70, 'external-after-change');
  await harness.controller.flush();

  assert.deepEqual(observations, [
    { revision: 0, opacity: 25, position: [64, 0], persisted: 25,
      previewAdmitted: true, readableAfterPreview: true },
    { revision: 1, opacity: 40, position: [64, 0], persisted: 40,
      previewAdmitted: true, readableAfterPreview: true },
    { revision: 2, opacity: 60, position: [64, 0], persisted: 60,
      previewAdmitted: true, readableAfterPreview: true },
    { revision: 3, opacity: 70, position: [64, 0], persisted: 70,
      previewAdmitted: true, readableAfterPreview: true },
  ]);
  assert.deepEqual(harness.state.previews.map((request) => request.frame), [10, 10, 10, 10]);
});

test('rendered opacity input callbacks preserve rapid no-await values and immediate undo ordering', async () => {
  const source = staticSource();
  const legacyBytes = JSON.stringify(source);
  const paperLayer = { opacity: 0.25, children: [], bounds: {
    left: 20, top: 60, right: 40, bottom: 80, width: 20, height: 20,
    center: { x: 30, y: 70 },
  } };
  const paperProject = { activeLayer: paperLayer, itemCount: 1 };
  const paperBytes = JSON.stringify(paperProject);
  let motion = null;
  let inInputCallback = false;
  let synchronousRenders = 0;
  let refreshes = 0;
  const harness = nativeHarness(source, { afterChange() {
    if (!motion) return;
    refreshes++;
    motion.sandbox.renderLayerList();
  } });
  const motionState = defaultState();
  Object.assign(motionState, { layers: source.layers, activeLayerIdx: 0,
    currentFrame: 10, totalFrames: 21, appMode: 'motion' });
  motion = loadMotion(motionState, { beforeMotion(sb) {
    sb.userLayers = [paperLayer]; sb.view = { zoom: 1 };
    sb.SM.setActiveLayer = () => {};
    sb._layerSel = [0]; sb._layerSelAnchor = 0;
    sb._layerIndexByUid = () => -1;
    sb.n20AllowLegacyWrite = () => false;
    sb.n20RequireLegacyWrite = (kind) => { throw new Error(`legacy writer denied: ${kind}`); };
    sb.NemoNativeOpacityCutover = Object.freeze({
      blocksLegacy: () => harness.controller.blocksLegacy(),
      isActive: () => harness.controller.isActive(),
      prepared: () => harness.controller.prepared(),
      identity: () => harness.controller.identity(),
      projectSelection: (selection, frame) => harness.controller.projectSelection(
        JSON.parse(JSON.stringify(selection)), frame),
    });
    sb.NemoOpacityApplication = { legacy: (...args) => harness.controller.legacyIntent(...args) };
  } });

  const created = [];
  const createElement = motion.sandbox.document.createElement;
  motion.sandbox.document.createElement = function (tag) {
    const element = createElement(tag);
    const listeners = {};
    element.children = [];
    element._listeners = listeners;
    element.addEventListener = function (type, listener) {
      (listeners[type] || (listeners[type] = [])).push(listener);
    };
    element.appendChild = function (child) { this.children.push(child); return child; };
    created.push(element);
    return element;
  };
  const panelBody = motion.sandbox.document.createElement('div');
  Object.defineProperty(panelBody, 'innerHTML', {
    configurable: true, get() { return ''; }, set() { this.children.length = 0; },
  });
  motion.sandbox.document.getElementById = (id) => id === 'motion-props-body' ? panelBody : null;
  motion.sandbox.renderLayerList = function () {
    if (inInputCallback) synchronousRenders++;
    motion.SMMotion.renderMotionPropsPanel();
  };
  motion.sandbox.renderTimeline = function () { if (inInputCallback) synchronousRenders++; };

  assert.equal(await harness.controller.activate(harness.prepared), true);
  const opacityRow = created.filter((element) => element._smProp === 'opacity').at(-1);
  assert.ok(opacityRow, 'production renderer emitted the opacity property row');
  function descendants(element) {
    return [element].concat((element.children || []).flatMap(descendants));
  }
  const input = descendants(opacityRow).find((element) => element.type === 'number' &&
    String(element.className).includes('motion-val'));
  assert.ok(input && input._listeners.change && input._listeners.change.length === 1,
    'production scrubField installed its opacity change callback');

  function commitInput(value) {
    input.value = String(value);
    inInputCallback = true;
    try { assert.doesNotThrow(() => input._listeners.change[0]()); }
    finally { inInputCallback = false; }
  }

  // Invoke the actual rendered input callback twice. There is deliberately
  // no await before the controller's UI-shaped undo.
  commitInput(40);
  commitInput(60);
  const undo = harness.controller.historyFromUi('undo', 'ui-undo-after-scrub');
  assert.equal(harness.controller.persistenceJSON(), null);
  await undo;
  await harness.controller.flush();

  assert.deepEqual(harness.controller.valueAtFrame('r08_curve_layer', 10), [40]);
  assert.deepEqual(Array.from(motion.SMMotion.valueAtFrame(source.layers[0], 'opacity', 10)), [40]);
  assert.deepEqual(Array.from(motion.SMMotion.rawValueAtFrame(source.layers[0], 'opacity', 10)), [40]);
  assert.equal(JSON.parse(harness.controller.persistenceJSON()).layers[0].motionStatic.opacity[0], 40);
  assert.deepEqual(harness.state.dispatches.filter((request) => request.operation === 'command.document.apply')
    .map((request) => request.payload.value), [40, 60]);
  assert.deepEqual(harness.state.history, [25]);
  assert.deepEqual(harness.state.redo, [60]);
  assert.equal(synchronousRenders, 0, 'native input callbacks defer cache-reading renders to afterChange');
  assert.equal(refreshes, 2, 'activation and the final queued mutation each publish one readable refresh');
  assert.equal(JSON.stringify(source), legacyBytes, 'native UI callbacks never mutate the frozen JS shell');
  assert.equal(JSON.stringify(paperProject), paperBytes, 'native UI callbacks never mutate the Paper mirror');
});

test('v1 facade rejects stale identity and malformed read payloads as structured failures', async () => {
  const harness = nativeHarness(staticSource());
  assert.equal(await harness.controller.activate(harness.prepared), true);
  const current = harness.controller.identity();
  const set = { apiVersion: 1, requestId: 'identity-set', instanceId: current.instanceId,
    documentId: current.documentId, expectedRevision: current.contentRevision,
    operation: 'property.set', payload: { layerId: 'r08_curve_layer', property: 'opacity', value: 40, frame: 10 } };
  for (const [label, change, code] of [
    ['missing instance', (r) => { delete r.instanceId; }, 'wrong_document'],
    ['empty instance', (r) => { r.instanceId = ''; }, 'wrong_document'],
    ['wrong instance', (r) => { r.instanceId = 'other-instance'; }, 'wrong_document'],
    ['missing document', (r) => { delete r.documentId; }, 'wrong_document'],
    ['empty document', (r) => { r.documentId = ''; }, 'wrong_document'],
    ['wrong document', (r) => { r.documentId = 'other-document'; }, 'wrong_document'],
    ['stale revision', (r) => { r.expectedRevision++; }, 'stale_revision'],
  ]) {
    const request = clone(set); change(request);
    const result = harness.controller.handleV1(request);
    assert.equal(result.ok, false, label);
    assert.equal(result.error.code, code, label);
  }

  const get = { apiVersion: 1, requestId: 'identity-get', operation: 'property.get',
    payload: { layerId: 'r08_curve_layer', property: 'opacity', frame: 10 } };
  for (const [label, change, code] of [
    ['wrong read instance', (r) => { r.instanceId = 'other-instance'; }, 'wrong_document'],
    ['empty read document', (r) => { r.documentId = ''; }, 'wrong_document'],
    ['wrong layer', (r) => { r.payload.layerId = 'other-layer'; }, 'invalid_request'],
    ['wrong property', (r) => { r.payload.property = 'position'; }, 'invalid_request'],
    ['negative frame', (r) => { r.payload.frame = -1; }, 'invalid_request'],
    ['fractional frame', (r) => { r.payload.frame = 1.5; }, 'invalid_request'],
    ['overflow frame', (r) => { r.payload.frame = 21; }, 'invalid_request'],
  ]) {
    const request = clone(get); change(request);
    const result = harness.controller.handleV1(request);
    assert.equal(result.ok, false, label);
    assert.equal(result.error.code, code, label);
  }
  const malformed = harness.controller.handleV1({ apiVersion: 1, requestId: 'bad-read',
    operation: 'property.get', payload: null });
  assert.equal(malformed.ok, false);
  assert.equal(malformed.error.code, 'invalid_request');
  assert.deepEqual(harness.controller.valueAtFrame('r08_curve_layer', 10), [25]);

  for (const [label, identityFields] of [
    ['null instance', { instanceId: null }],
    ['null document', { documentId: null }],
    ['both null', { instanceId: null, documentId: null }],
  ]) {
    const result = harness.controller.handleV1({ ...clone(get), ...identityFields,
      requestId: `nullable-read-${label.replaceAll(' ', '-')}` });
    assert.equal(result.ok, true, label); assert.equal(result.result.value, 25, label);
  }

  const nullGet = harness.controller.handleV1({ ...clone(get), requestId: 'null-frame-get',
    payload: { ...get.payload, frame: null } });
  assert.equal(nullGet.ok, true); assert.equal(nullGet.result.value, 25);
  for (const [label, frame] of [['null', null], ['negative', -1], ['far future', Number.MAX_SAFE_INTEGER]]) {
    const snapshot = harness.controller.handleV1({ apiVersion: 1,
      requestId: `compat-snapshot-${label.replaceAll(' ', '-')}`,
      instanceId: null, documentId: null, operation: 'snapshot', payload: { frame } });
    assert.equal(snapshot.ok, true, label);
    assert.equal(snapshot.result.frame, 10, label);
    assert.equal(snapshot.result.layers[0].opacity, 25, label);
  }
  const nullKey = harness.controller.handleV1({ ...clone(set), requestId: 'null-frame-key',
    operation: 'property.key.set', payload: { ...set.payload, frame: null } });
  assert.equal(nullKey.ok, false); assert.equal(nullKey.error.code, 'invalid_request');
});

test('keyed graph reads use native opacity and never evaluate or mutate the frozen JS mirror', async () => {
  const harness = nativeHarness(keyedSource());
  assert.equal(await harness.controller.activate(harness.prepared), true);
  const mirror = keyedSource();
  mirror.layers[0].motion.opacity.keys[0].v = [1];
  mirror.layers[0].motion.opacity.keys[1].v = [2];
  mirror.layers[0].motion.position.keys[0].v = [900, 700];
  mirror.layers[0].motion.position.keys[1].v = [999, 777];
  const state = defaultState();
  Object.assign(state, { layers: mirror.layers, activeLayerIdx: 0,
    currentFrame: 10, totalFrames: 21, appMode: 'motion' });
  const paperLayer = { opacity: 0.01, children: [], bounds: {
    left: 20, top: 60, right: 40, bottom: 80, width: 20, height: 20,
    center: { x: 30, y: 70 },
  } };
  const paperProject = { activeLayer: paperLayer, itemCount: 1 };
  const documentBytes = JSON.stringify(state);
  const paperBytes = JSON.stringify(paperProject);
  const calls = [];
  let legacyCurveEvaluations = 0;
  const motion = loadMotion(state, { beforeMotion(sb) {
    sb.userLayers = [paperLayer];
    sb.view = { zoom: 1 };
    sb._layerSel = [0]; sb._layerSelAnchor = 0;
    sb._layerIndexByUid = () => -1;
    sb.SMAnimationCurve.evalCurvePoints = () => {
      legacyCurveEvaluations++;
      throw new Error('legacy JS curve evaluator was reached');
    };
    sb.n20AllowLegacyWrite = () => false;
    sb.n20RequireLegacyWrite = (kind) => { throw new Error(`legacy writer denied: ${kind}`); };
    sb.NemoNativeOpacityCutover = Object.freeze({
      blocksLegacy: () => true,
      isActive: () => harness.controller.isActive(),
      prepared: () => harness.controller.prepared(),
      identity: () => harness.controller.identity(),
      projectSelection(selection, frame) {
        calls.push([selection.activeLayerUid, frame]);
        return harness.controller.projectSelection(JSON.parse(JSON.stringify(selection)), frame);
      },
    });
  } });
  assert.deepEqual(Array.from(motion.SMMotion.rawValueAtFrame(mirror.layers[0], 'opacity', 10)), [50]);
  assert.deepEqual(Array.from(motion.SMMotion.valueAtFrame(mirror.layers[0], 'opacity', 10)), [50]);
  const overlay = motion.SMMotion.buildOverlayItems();
  assert.equal(overlay.length > 0, true, 'native Position retains its resource-backed box overlay');
  assert.equal(motion.SMMotion.onDown({ point: { x: 30, y: 70 }, altKey: false }), false,
    'the retained frame-zero Position key is not an active hit target');
  for (const [frame, expected] of [[0, [0, 0]], [10, [64, 0]], [20, [128, 0]]]) {
    state.currentFrame = frame;
    assert.deepEqual(Array.from(motion.SMMotion.getLayerValue(0, 'position')), expected, `getLayerValue frame ${frame}`);
    assert.deepEqual(Array.from(motion.SMMotion.rawValueAtFrame(mirror.layers[0], 'position', frame)), expected,
      `raw graph frame ${frame}`);
    assert.deepEqual(Array.from(motion.SMMotion.valueAtFrame(mirror.layers[0], 'position', frame)), expected,
      `evaluated frame ${frame}`);
  }
  const panelBody = motion.sandbox.document.createElement('div');
  panelBody.children = [];
  panelBody.appendChild = function (child) { this.children.push(child); };
  const created = [];
  const createElement = motion.sandbox.document.createElement;
  motion.sandbox.document.createElement = function (tag) {
    const element = createElement(tag); created.push(element); return element;
  };
  motion.sandbox.document.getElementById = (id) => id === 'motion-props-body' ? panelBody : null;
  motion.sandbox._exprEditorOpen = { holder: mirror.layers[0], prop: 'position' };
  motion.SMMotion.renderMotionPropsPanel();
  assert.equal(panelBody.children.length > 1, true, 'the production inspector/display path rendered');
  assert.equal(created.some((element) => element.className === 'lrow motion-expr-editor'), true,
    'the production fx editor row used the owned raw-value path');
  assert.deepEqual(calls, [['r08_curve_layer', 10], ['r08_curve_layer', 10], ['r08_curve_layer', 20]]);
  assert.equal(legacyCurveEvaluations, 0);
  state.currentFrame = 10;
  assert.equal(JSON.stringify(state), documentBytes);
  assert.equal(JSON.stringify(paperProject), paperBytes);

  const malformedPrepared = clone(harness.prepared);
  malformedPrepared.resources[10].layers[0].bounds[0] = 21;
  let malformedEvaluations = 0;
  const malformed = loadMotion(state, { beforeMotion(sb) {
    sb.userLayers = [paperLayer]; sb.view = { zoom: 1 };
    sb._layerSel = [0]; sb._layerSelAnchor = 0;
    sb.SMAnimationCurve.evalCurvePoints = () => { malformedEvaluations++; throw new Error('legacy evaluator reached'); };
    sb.NemoNativeOpacityCutover = { blocksLegacy: () => true, isActive: () => true,
      identity: () => harness.controller.identity(), prepared: () => malformedPrepared,
      projectSelection: () => ({ selected: [{ value: 50 }] }) };
  } });
  assert.throws(() => malformed.SMMotion.rawValueAtFrame(mirror.layers[0], 'position', 10),
    /geometry projection is unavailable/);
  assert.throws(() => malformed.SMMotion.buildOverlayItems(), /geometry projection is unavailable/);
  assert.equal(malformedEvaluations, 0);

  harness.state.connected = false;
  assert.throws(() => motion.SMMotion.rawValueAtFrame(mirror.layers[0], 'position', 10),
    /disconnected|not readable|not active/);
  assert.throws(() => motion.SMMotion.buildOverlayItems(), /disconnected|not readable|not active/);
  assert.equal(legacyCurveEvaluations, 0, 'passive loss cannot fall back to the JS evaluator');
});

test('production core, lexical, and public Motion writers stop before touching JS or Paper mirrors', () => {
  const tweenSource = fs.readFileSync(path.join(ROOT, 'src/js/tweens.js'), 'utf8');
  const layer = clone(staticSource().layers[0]);
  layer.nativeVideo = { frameCount: 21 };
  layer.motion.position.keys[0].curvePoints = [
    { x: 0, y: 0 }, { x: 0.42, y: 0 }, { x: 0.58, y: 1 }, { x: 1, y: 1 },
  ];
  const state = defaultState();
  Object.assign(state, { layers: [layer, clone(layer)], activeLayerIdx: 0,
    currentFrame: 10, totalFrames: 21, appMode: 'motion', exprGlobals: 'unchanged' });
  const paperProject = { activeLayer: { opacity: 0.25 }, itemCount: 1 };
  const documentBytes = JSON.stringify(state);
  const paperBytes = JSON.stringify(paperProject);
  const denied = [];
  const motion = loadMotion(state, { beforeMotion(sb) {
    sb.userLayers = [paperProject.activeLayer];
    sb.n20AllowLegacyWrite = (kind) => { denied.push(kind); return false; };
    sb.n20RequireLegacyWrite = (kind) => { denied.push(kind); throw new Error(`legacy writer denied: ${kind}`); };
    vm.runInNewContext(extractFunction(tweenSource, 'pushUndo'), sb);
    sb.NemoNativeOpacityCutover = Object.freeze({ blocksLegacy: () => true,
      isActive: () => true,
      prepared: () => ({ layerUid: layer.layerUid, opacityMode: 'static' }),
      identity: () => ({ instanceId: 'test', documentId: 'document', contentRevision: 0 }),
      projectSelection: () => ({ selected: [{ value: 25 }] }) });
    sb.NemoOpacityApplication = { legacy: () => false };
  } });

  motion.SMMotion.setLayerValue(0, 'position', [100, 100]);
  assert.throws(() => motion.SMMotion.setLayerTimeLink(0, 1, 'both'), /legacy writer denied: history-checkpoint/);
  assert.throws(() => motion.SMMotion.setExpressionCode(layer, 'position', 'value + 1'), /legacy writer denied: history-checkpoint/);
  assert.throws(() => motion.SMMotion.applyExprCode({ uid: layer.layerUid }, 'position', 'value + 1'), /legacy writer denied: motion-expression-create/);
  assert.equal(motion.SMMotion.setExprGlobals('value + 1'), false);
  assert.throws(() => motion.SMMotion.enableTimeRemap(0), /legacy writer denied: history-checkpoint/);
  assert.equal(motion.SMMotion.shiftLayerMotionKeys(0, 1), false);
  assert.equal(motion.SMMotion.migrateLegacyCurves(), 0);
  assert.equal(motion.SMMotion.exprSnapshotFor({ uid: layer.layerUid, elem: 'missing-shape' }, 'position'), null);
  assert.throws(() => motion.SMMotion.ensureElementHolder(layer, 'missing-shape'), /legacy writer denied/);

  const appSource = fs.readFileSync(path.join(ROOT, 'src/js/app.js'), 'utf8');
  const direct = { state: { totalFrames: 21, layers: clone(state.layers) }, userLayers: [],
    n20AllowLegacyWrite(kind) { denied.push(kind); return false; },
    n20RequireLegacyWrite(kind) { denied.push(kind); throw new Error(`legacy writer denied: ${kind}`); } };
  direct.window = direct;
  vm.runInNewContext(`${extractFunction(appSource, 'createUserLayer')}\n${extractFunction(appSource, 'mergeRemoteSnapshot')}\n` +
    'this.__core = { createUserLayer, mergeRemoteSnapshot };', direct, { filename: 'src/js/app.js (N20 direct writers)' });
  const directBytes = JSON.stringify(direct.state);
  assert.throws(() => direct.__core.createUserLayer('forbidden'), /legacy writer denied/);
  assert.equal(direct.__core.mergeRemoteSnapshot({ layers: [] }, { id: 'remote' }), false);
  assert.equal(JSON.stringify(direct.state), directBytes);
  assert.deepEqual(direct.userLayers, []);

  const timelineSource = fs.readFileSync(path.join(ROOT, 'src/js/timeline.js'), 'utf8');
  const motionSource = fs.readFileSync(path.join(ROOT, 'src/js/motion.js'), 'utf8');
  const selectedPath = { data: { paramShape: { kind: 'rect', tl: 2, tr: 2, br: 2, bl: 2 } } };
  const lexical = { selectedPaths: [selectedPath], state: {}, window: {},
    _motionKeySel: [{ key: { frame: 0 } }, { key: { frame: 10 } }],
    n20AllowLegacyWrite(kind) { denied.push(kind); return false; },
    n20RequireLegacyWrite(kind) { denied.push(kind); throw new Error(`legacy writer denied: ${kind}`); },
    pushUndoLayers() { throw new Error('pushUndoLayers must not be reached'); } };
  lexical.window = lexical;
  const lexicalBytes = JSON.stringify(selectedPath);
  vm.runInNewContext(`${extractFunction(tweenSource, 'pushUndo')}\n${extractFunction(timelineSource, 'commitCornerEdit')}\n` +
    `${extractFunction(motionSource, 'setSelectedEaseInfluence')}\n` +
    `${extractFunction(motionSource, 'colorSelectedKeys')}\n${extractFunction(motionSource, 'subdivideKeys')}\n` +
    'this.__writers = { commitCornerEdit, setSelectedEaseInfluence, colorSelectedKeys, subdivideKeys };',
  lexical, { filename: 'N20 lexical writers' });
  assert.throws(() => lexical.__writers.commitCornerEdit('tl', 99), /legacy writer denied/);
  assert.throws(() => lexical.__writers.setSelectedEaseInfluence('out', 50), /legacy writer denied: history-checkpoint/);
  assert.throws(() => lexical.__writers.colorSelectedKeys('#ff0000'), /legacy writer denied: history-checkpoint/);
  assert.throws(() => lexical.__writers.subdivideKeys(), /legacy writer denied: history-checkpoint/);
  assert.equal(JSON.stringify(selectedPath), lexicalBytes);

  assert.equal(JSON.stringify(state), documentBytes, 'serialized document bytes remain unchanged');
  assert.equal(JSON.stringify(paperProject), paperBytes, 'Paper project bytes remain unchanged');
  for (const reason of ['motion-set-value', 'motion-expression-create', 'motion-expression-globals',
    'motion-shift-layer-keys', 'motion-migrate-legacy-curves', 'motion-element-holder-create',
    'create-layer', 'remote-snapshot-merge', 'history-checkpoint']) {
    assert.equal(denied.includes(reason), true, reason);
  }
});

test('retained production Motion menu and expression callbacks deny before persisted writes', () => {
  const motionSource = fs.readFileSync(path.join(ROOT, 'src/js/motion.js'), 'utf8');
  const tweenSource = fs.readFileSync(path.join(ROOT, 'src/js/tweens.js'), 'utf8');
  for (const existing of [false, true]) {
    const holder = existing ? { expressions: { opacity: { code: 'value', enabled: false } } } : {};
    const selected = [{ key: { frame: 0, value: [25] } }];
    const nodes = [], documentListeners = new Map(), denied = [];
    function element(tag) {
      const node = { tag, style: {}, children: [], listeners: new Map(), value: '',
        classList: { add() {}, remove() {} },
        appendChild(child) { this.children.push(child); },
        addEventListener(name, callback) { this.listeners.set(name, callback); } };
      nodes.push(node); return node;
    }
    const scope = {
      document: { createElement: element, createTextNode: (text) => ({ text }),
        addEventListener(name, callback) { documentListeners.set(name, callback); },
        removeEventListener(name) { documentListeners.delete(name); } },
      state: { currentFrame: 0, layers: [holder] }, _motionKeySel: selected,
      SM: { t: (key) => key }, PROP_DIM: { opacity: 1 }, PROP_UNIT: {},
      nativeMotionSurface: () => NativeMotionSurface,
      ownedRawValueAtFrame: () => [25], exprGlobals: () => '',
      n20AllowLegacyWrite(kind) { denied.push(kind); return false; },
      n20RequireLegacyWrite(kind) { return NativeLegacySurface.requireLegacyWrite(scope, kind, scope.n20AllowLegacyWrite); },
      renderTimeline() { throw new Error('denied callback must not render'); },
    };
    scope.window = scope;
    vm.runInNewContext(['expressionSeed', 'ensureExpr', 'buildExprEditorRow', 'buildKeySelectionLockMenuItems']
      .map((name) => extractFunction(motionSource, name)).join('\n') + '\n' +
      extractFunction(tweenSource, 'pushUndo'), scope, { filename: 'N20 retained Motion callbacks' });
    const before = JSON.stringify({ holder, selected });
    const row = scope.buildExprEditorRow(holder, 'opacity');
    const menu = scope.buildKeySelectionLockMenuItems();
    assert.ok(row, 'the production expression editor builds without materializing state');
    assert.equal(JSON.stringify({ holder, selected }), before);
    // Call listeners retained from production row/menu construction, not a
    // replacement SMMotion property or a source-only assertion about guards.
    const checkbox = nodes.find((node) => node.tag === 'input');
    const textarea = nodes.find((node) => node.tag === 'textarea');
    const grip = nodes.find((node) => node.className === 'motion-expr-grip');
    checkbox.checked = true;
    assert.throws(() => checkbox.listeners.get('change')(), { name: 'NemoNativeReleaseRequired' });
    textarea.value = 'value + 10';
    assert.throws(() => textarea.listeners.get('blur')(), { name: 'NemoNativeReleaseRequired' });
    grip.listeners.get('mousedown')({ preventDefault() {}, stopPropagation() {}, clientY: 10 });
    assert.equal(documentListeners.size, 0, 'denied resize does not install a later persisted-height callback');
    for (const entry of menu.filter((entry) => !entry.disabled)) {
      assert.throws(() => entry.action(), { name: 'NemoNativeReleaseRequired' });
    }
    assert.equal(JSON.stringify({ holder, selected }), before, 'all retained callbacks leave document bytes intact');
    assert.ok(denied.includes('history-checkpoint'));
    assert.ok(denied.includes('motion-expression-editor-height'));
    assert.equal(denied.filter((kind) => kind === 'history-checkpoint').length, 5);
  }
});

test('native v1 writes return post-write revisions and preserve exact retry, capability, and trace contracts', async () => {
  const harness = nativeHarness(staticSource());
  assert.equal(await harness.controller.activate(harness.prepared), true);
  const identity = harness.controller.identity();
  const command = (requestId, expectedRevision, operation, payload) => ({ apiVersion: 1,
    requestId, instanceId: identity.instanceId, documentId: identity.documentId,
    expectedRevision, operation, payload });
  const set40 = command('legal request id with spaces', 0, 'property.set', {
    layerId: 'r08_curve_layer', property: 'opacity', value: 40,
  });
  const first = await harness.controller.handleV1(set40);
  assert.equal(first.ok, true); assert.equal(first.revision, 1); assert.equal(first.result.value, 40);
  const second = await harness.controller.handleV1(command('set-60', 1, 'property.set', {
    layerId: 'r08_curve_layer', property: 'opacity', value: 60,
  }));
  assert.equal(second.ok, true); assert.equal(second.revision, 2); assert.equal(second.result.value, 60);
  const replayed = harness.controller.handleV1(clone(set40));
  assert.deepEqual(replayed, first, 'completed retry returns the original response at revision 1');
  assert.deepEqual(harness.controller.valueAtFrame('r08_curve_layer', 10), [60]);
  const undo = await harness.controller.handleV1(command('undo-after-60', 2, 'history.undo', {}));
  assert.equal(undo.ok, true); assert.equal(undo.revision, 3); assert.deepEqual(undo.result, { applied: true });
  assert.deepEqual(harness.controller.valueAtFrame('r08_curve_layer', 10), [40]);

  const capabilities = harness.controller.handleV1({ apiVersion: 1, requestId: 'native-capabilities',
    instanceId: identity.instanceId, documentId: identity.documentId,
    operation: 'capabilities', payload: {} });
  assert.equal(capabilities.ok, true); assert.equal(capabilities.revision, 3);
  assert.deepEqual(capabilities.result.descriptors.map((descriptor) => descriptor.id), ['opacity', 'export.svg.frame']);
  assert.deepEqual(capabilities.result.properties, [{ id: 'opacity', min: 0, max: 100,
    unit: 'percent', animated: true }]);

  const traceRequest = { apiVersion: 1, requestId: 'trace-one', operation: 'diagnostics.trace', payload: {} };
  const trace = harness.controller.handleV1(traceRequest);
  assert.deepEqual(trace.result.entries.map((entry) => [entry.request.requestId, entry.revision, entry.ok]), [
    ['legal request id with spaces', 1, true], ['set-60', 2, true], ['undo-after-60', 3, true],
  ]);
  trace.result.entries[0].request.requestId = 'mutated-copy';
  assert.equal(harness.controller.handleV1(traceRequest).result.entries[0].request.requestId,
    'legal request id with spaces', 'trace responses are detached');
});

test('native v1 in-flight and retained requestId behavior is single-writer and fingerprinted', async () => {
  let finishMutation;
  const gate = new Promise((resolve) => { finishMutation = resolve; });
  const harness = nativeHarness(staticSource(), { mutationGate: gate });
  assert.equal(await harness.controller.activate(harness.prepared), true);
  const identity = harness.controller.identity();
  const firstRequest = { apiVersion: 1, requestId: 'inflight-one', instanceId: identity.instanceId,
    documentId: identity.documentId, expectedRevision: 0, operation: 'property.set',
    payload: { layerId: 'r08_curve_layer', property: 'opacity', value: 40 } };
  const first = harness.controller.handleV1(firstRequest);
  const duplicate = harness.controller.handleV1(clone(firstRequest));
  assert.equal(duplicate.error.code, 'unavailable', 'identical in-flight retry never joins unresolved work');
  const changed = harness.controller.handleV1({ ...clone(firstRequest),
    payload: { ...firstRequest.payload, value: 50 } });
  assert.equal(changed.error.code, 'invalid_request');
  const competingRequest = { ...clone(firstRequest), requestId: 'competing-write',
    payload: { ...firstRequest.payload, value: 60 } };
  const competing = harness.controller.handleV1(competingRequest);
  assert.equal(competing.error.code, 'unavailable');
  finishMutation();
  const completed = await first;
  assert.equal(completed.revision, 1);
  assert.equal(harness.state.dispatches.filter((request) => request.operation === 'command.document.apply').length, 1);
  assert.deepEqual(harness.controller.handleV1(firstRequest), completed);
  assert.equal(harness.controller.handleV1(competingRequest).error.code, 'stale_revision',
    'a non-retained competitor becomes stale only after the first write advances');
  assert.equal(harness.controller.status(), 'native');
});

test('native v1 semantic failures are retained, transient fences are retryable, and empty history stays native', async () => {
  const harness = nativeHarness(staticSource());
  assert.equal(await harness.controller.activate(harness.prepared), true);
  const identity = harness.controller.identity();
  const invalid = { apiVersion: 1, requestId: 'semantic-invalid', instanceId: identity.instanceId,
    documentId: identity.documentId, expectedRevision: 0, operation: 'property.set',
    payload: { layerId: 'absent', property: 'opacity', value: 40 } };
  assert.equal(harness.controller.handleV1(invalid).error.code, 'invalid_request');
  const correctedReuse = { ...clone(invalid), payload: { ...invalid.payload, layerId: 'r08_curve_layer' } };
  assert.equal(harness.controller.handleV1(correctedReuse).error.code, 'invalid_request');
  assert.equal(harness.state.releases, 0);

  const emptyUndo = await harness.controller.handleV1({ ...clone(invalid), requestId: 'empty-undo',
    operation: 'history.undo', payload: {} });
  assert.equal(emptyUndo.error.code, 'history_unavailable');
  assert.equal(emptyUndo.revision, 0);
  assert.equal(harness.controller.status(), 'native');

  let finishExport;
  const exportGate = new Promise((resolve) => { finishExport = resolve; });
  const fenced = nativeHarness(staticSource(), { async dispatchGate(request) {
    if (request.operation === 'job.export.png.begin') await exportGate;
  } });
  assert.equal(await fenced.controller.activate(fenced.prepared), true);
  const exporting = fenced.controller.exportPng('/tmp/n20-v1-transient-fence', [10]);
  const fencedIdentity = fenced.controller.identity();
  const retryable = { apiVersion: 1, requestId: 'retry-after-export', instanceId: fencedIdentity.instanceId,
    documentId: fencedIdentity.documentId, expectedRevision: 0, operation: 'property.set',
    payload: { layerId: 'r08_curve_layer', property: 'opacity', value: 45 } };
  assert.equal(fenced.controller.handleV1(retryable).error.code, 'unavailable');
  finishExport(); await exporting;
  const applied = await fenced.controller.handleV1(retryable);
  assert.equal(applied.ok, true); assert.equal(applied.revision, 1);
});

test('native v1 validates incompatible edits before one release and preserves exact retries during drain', async () => {
  const malformed = [
    ['property.key.set', { layerId: 'r08_curve_layer', property: 'opacity', frame: 99, value: 40 }],
    ['diagnostics.replay', { request: { operation: 'property.set', payload: { layerId: 'absent', property: 'opacity', value: 40 } } }],
    ['start', {}],
  ];
  for (const [operation, payload] of malformed) {
    const harness = nativeHarness(staticSource());
    assert.equal(await harness.controller.activate(harness.prepared), true);
    const current = harness.controller.identity();
    const request = { apiVersion: 1, requestId: `malformed-${operation}`, instanceId: current.instanceId,
      documentId: current.documentId, operation, payload };
    if (['property.key.set', 'diagnostics.replay'].includes(operation)) request.expectedRevision = 0;
    assert.equal(harness.controller.handleV1(request).error.code, 'invalid_request');
    assert.equal(harness.state.releases, 0); assert.equal(harness.controller.status(), 'native');
  }

  const incompatible = [
    [keyedSource(), 'property.set', { layerId: 'r08_curve_layer', property: 'opacity', value: 40 }],
    [staticSource(), 'property.key.set', { layerId: 'r08_curve_layer', property: 'opacity', frame: 10, value: 40 }],
    [staticSource(), 'property.animation.set', { layerId: 'r08_curve_layer', property: 'opacity', animated: true }],
    [staticSource(), 'diagnostics.replay', { request: { apiVersion: 1, requestId: 'recorded',
      operation: 'property.set', payload: { layerId: 'r08_curve_layer', property: 'opacity', value: 40 } } }],
    [staticSource(), 'start', { frameIdx: 10 }],
  ];
  for (const [source, operation, payload] of incompatible) {
    let finishRelease;
    const releaseGate = new Promise((resolve) => { finishRelease = resolve; });
    const harness = nativeHarness(source, { releaseGate });
    assert.equal(await harness.controller.activate(harness.prepared), true);
    const current = harness.controller.identity();
    const request = { apiVersion: 1, requestId: `incompatible-${operation}`, instanceId: current.instanceId,
      documentId: current.documentId, operation, payload };
    if (['property.set', 'property.key.set', 'property.animation.set', 'diagnostics.replay'].includes(operation)) {
      request.expectedRevision = 0;
    }
    const denied = harness.controller.handleV1(request);
    assert.equal(denied.error.code, 'unavailable');
    assert.equal(harness.controller.status(), 'release-requested');
    if (request.expectedRevision !== undefined) assert.deepEqual(harness.controller.handleV1(clone(request)), denied,
      'known retry is served while release drains');
    assert.equal(harness.state.releases, 0);
    finishRelease(); await harness.controller.flush();
    assert.equal(harness.state.releases, 1); assert.equal(harness.controller.status(), 'legacy');
  }
});

test('native v1 trace is bounded per lifecycle and cleared on reentry', async () => {
  const harness = nativeHarness(staticSource());
  assert.equal(await harness.controller.activate(harness.prepared), true);
  const identity = harness.controller.identity();
  for (let index = 0; index < 33; index++) {
    const result = harness.controller.handleV1({ apiVersion: 1, requestId: `invalid-${index}`,
      instanceId: identity.instanceId, documentId: identity.documentId, expectedRevision: 0,
      operation: 'property.set', payload: { layerId: `absent-${index}`, property: 'opacity', value: 40 } });
    assert.equal(result.error.code, 'invalid_request');
  }
  const traceRequest = { apiVersion: 1, requestId: 'bounded-trace', operation: 'diagnostics.trace', payload: {} };
  const trace = harness.controller.handleV1(traceRequest).result.entries;
  assert.equal(trace.length, 32); assert.equal(trace[0].request.requestId, 'invalid-1');
  await harness.controller.releaseCurrent('trace-lifecycle');
  assert.equal(await harness.controller.activate(harness.prepared), true);
  assert.deepEqual(harness.controller.handleV1(traceRequest).result.entries, []);
});

test('activation mismatch rolls host ownership back without exposing a writable mirror', async () => {
  const harness = nativeHarness(staticSource(), { badEvaluation: true });
  assert.equal(await harness.controller.activate(harness.prepared), false);
  assert.equal(harness.controller.status(), 'legacy');
  assert.equal(harness.state.releases, 1);
  assert.equal(harness.state.disconnects, 1);
  assert.equal(harness.state.imports.length, 0);
  assert.equal(harness.controller.persistenceJSON(), null);
  assert.throws(() => harness.controller.prepared(), /not active/);
});

test('transport loss after initial cache I/O cannot publish native activation', async () => {
  let dropped = false;
  const harness = nativeHarness(staticSource(), { dispatchGate(request, state) {
    if (!dropped && request.operation === 'query.document.evaluate' && request.payload.frame === 20) {
      dropped = true; state.connected = false;
    }
  } });
  assert.equal(await harness.controller.activate(harness.prepared), false);
  assert.equal(dropped, true);
  assert.equal(harness.controller.status(), 'legacy');
  assert.equal(harness.controller.persistenceJSON(), null);
  assert.equal(harness.state.releases, 1, 'the installed host authority is rolled back exactly once');
  assert.equal(harness.state.imports.length, 0);
  assert.equal(harness.state.afterChanges || 0, 0, 'unpublished caches never trigger a UI change');
});

test('failed release is fail-closed and never reimports the cached shell', async () => {
  const harness = nativeHarness(staticSource(), { releaseFailure: true });
  assert.equal(await harness.controller.activate(harness.prepared), true);
  await assert.rejects(harness.controller.releaseCurrent('replacement'), /release failed/);
  assert.equal(harness.controller.status(), 'indeterminate');
  assert.equal(harness.controller.blocksLegacy(), true);
  assert.equal(harness.state.imports.length, 0);
  assert.throws(() => harness.controller.getNativeIdentity(), /safely observable/);
});

test('accepted release publishes legacy before synchronous UI materialization and later legacy edits', async () => {
  const source = keyedSource();
  let legacyMotion = null;
  let legacyState = null;
  let observed = null;
  const harness = nativeHarness(source, { legacyImport(bytes, silent, controller) {
    const imported = JSON.parse(bytes);
    legacyState = defaultState();
    Object.assign(legacyState, { layers: imported.layers, activeLayerIdx: 0,
      currentFrame: 10, totalFrames: 21, appMode: 'motion' });
    const paperLayer = { children: [], bounds: { left: 20, top: 60, right: 40, bottom: 80,
      width: 20, height: 20, center: { x: 30, y: 70 } } };
    legacyMotion = loadMotion(legacyState, { beforeMotion(sb) {
      sb.userLayers = [paperLayer]; sb.view = { zoom: 1 };
      sb._layerSel = [0]; sb._layerSelAnchor = 0;
      sb._layerIndexByUid = () => -1;
      sb.SM.setActiveLayer = () => {};
      sb.n20AllowLegacyWrite = () => !controller.blocksLegacy();
      sb.n20RequireLegacyWrite = (kind) => {
        if (controller.blocksLegacy()) throw new Error(`legacy writer denied: ${kind}`);
        return true;
      };
      sb.NemoNativeOpacityCutover = Object.freeze({
        blocksLegacy: () => controller.blocksLegacy(),
        prepared: () => controller.prepared(),
        identity: () => controller.identity(),
        projectSelection: () => { throw new Error('native selection must not run during legacy reentry'); },
      });
      sb.NemoOpacityApplication = { legacy: (...args) => controller.legacyIntent(...args) };
    } });
    const panelBody = legacyMotion.sandbox.document.createElement('div');
    panelBody.children = [];
    panelBody.appendChild = function (child) { this.children.push(child); };
    legacyMotion.sandbox.document.getElementById = (id) => id === 'motion-props-body' ? panelBody : null;
    legacyMotion.SMMotion.renderMotionPropsPanel();
    observed = { status: controller.status(), blocksLegacy: controller.blocksLegacy(), silent,
      position: Array.from(legacyMotion.SMMotion.rawValueAtFrame(imported.layers[0], 'position', 10)),
      renderedRows: panelBody.children.length };
    return true;
  } });

  assert.equal(await harness.controller.activate(harness.prepared), true);
  await harness.controller.releaseCurrent('production-reentry');
  assert.deepEqual(observed, { status: 'legacy', blocksLegacy: false, silent: true,
    position: [64, 0], renderedRows: observed.renderedRows });
  assert.equal(observed.renderedRows > 1, true, 'legacy UI rendered synchronously during import');
  assert.equal(harness.controller.legacyIntent('set', legacyState.layers[0], [40]), null);
  legacyMotion.SMMotion.setLayerValue(0, 'position', [70, 0]);
  assert.deepEqual(Array.from(legacyMotion.SMMotion.rawValueAtFrame(legacyState.layers[0], 'position', 10)), [70, 0]);
});

test('failed synchronous legacy materialization returns the released lifecycle to indeterminate', async () => {
  for (const [label, legacyImport, error] of [
    ['false', () => false, /could not re-enter legacy/],
    ['throw', () => { throw new Error('legacy import exploded'); }, /legacy import exploded/],
  ]) {
    const harness = nativeHarness(staticSource(), { legacyImport });
    assert.equal(await harness.controller.activate(harness.prepared), true, label);
    await assert.rejects(harness.controller.releaseCurrent(`failed-import-${label}`), error, label);
    assert.equal(harness.controller.status(), 'indeterminate', label);
    assert.equal(harness.controller.blocksLegacy(), true, label);
    assert.equal(harness.controller.persistenceJSON(), null, label);
    assert.equal(harness.controller.legacyIntent('set', staticSource().layers[0], [40]), false, label);
    assert.equal(harness.state.imports.length, 1, label);
  }
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

test('external native revision synchronizes every cache atomically and fences new consumers before its first await', async () => {
  let releaseSync;
  const syncGate = new Promise((resolve) => { releaseSync = resolve; });
  let reachedSync;
  const syncStarted = new Promise((resolve) => { reachedSync = resolve; });
  const harness = nativeHarness(staticSource(), { async dispatchGate(request, state) {
    if (state.identity.contentRevision === 1 && request.operation === 'query.document.serialize') {
      reachedSync(); await syncGate;
    }
  } });
  assert.equal(await harness.controller.activate(harness.prepared), true);
  const syncing = harness.externalOpacity(55, 'external-mcp-write-1');
  assert.equal(harness.controller.persistenceJSON(), null);
  assert.throws(() => harness.controller.prepared(), /synchronization is pending/);
  assert.throws(() => harness.controller.valueAtFrame('r08_curve_layer', 10), /synchronization is pending/);
  await assert.rejects(harness.controller.setOpacity('r08_curve_layer', 65), /synchronization is pending/);
  assert.equal(harness.controller.renderPreview(10), true);
  assert.equal(harness.state.previews.length, 0, 'preview work is not admitted behind a stale cache');
  assert.throws(() => harness.controller.exportPng('/tmp/n20-stale-output', [10]), /synchronization is pending/);
  await syncStarted;
  releaseSync();
  assert.deepEqual(await syncing, { instanceId: 'instance-a', documentId: 'native-document-1',
    lifecycleGeneration: 1, fromRevision: 0, toRevision: 1, requestId: 'external-mcp-write-1' });
  assert.deepEqual(harness.controller.valueAtFrame('r08_curve_layer', 10), [55]);
  assert.equal(JSON.parse(harness.controller.persistenceJSON()).layers[0].motionStatic.opacity[0], 55);
  assert.equal(harness.controller.identity().contentRevision, 1);
  await harness.controller.releaseCurrent('external-write-release');
  assert.equal(harness.state.imports[0].layers[0].motionStatic.opacity[0], 55);
});

test('actual bundled transport refreshes UI, save, and release after an external MCP write', async () => {
  const harness = nativeHarness(staticSource(), { transportFactory: realTauriTransportFactory() });
  assert.equal(await harness.controller.activate(harness.prepared), true);
  const reply = await harness.transportBridge.mcpSet(55, 'bundled-mcp-write-1');
  assert.equal(reply.contentRevision, 1);
  assert.deepEqual(harness.controller.valueAtFrame('r08_curve_layer', 10), [55]);
  assert.equal(JSON.parse(harness.controller.persistenceJSON()).layers[0].motionStatic.opacity[0], 55);
  assert.equal(harness.controller.identity().contentRevision, 1);
  assert.equal(harness.transportBridge.calls.some(([command, args]) => command === 'nemo_native_revision_sync' &&
    args.request.action === 'acknowledge'), true);
  await harness.controller.releaseCurrent('bundled-mcp-release');
  assert.equal(harness.state.imports[0].layers[0].motionStatic.opacity[0], 55);
});

test('revision acknowledgment failure passively disconnects and invalidates refreshed controller caches', async () => {
  const harness = nativeHarness(staticSource(), {
    transportFactory: realTauriTransportFactory({ ackFailure: true }),
  });
  assert.equal(await harness.controller.activate(harness.prepared), true);
  await harness.transportBridge.mcpSet(55, 'bundled-mcp-ack-failure');
  assert.equal(harness.transportBridge.transport.status(), null);
  assert.equal(harness.controller.persistenceJSON(), null);
  assert.equal(harness.controller.status(), 'indeterminate');
  assert.equal(harness.controller.blocksLegacy(), true);
  assert.throws(() => harness.controller.valueAtFrame('r08_curve_layer', 10), /not active/);
  await assert.rejects(harness.controller.releaseCurrent('ack-failure-release'), /cannot release safely/);
  assert.equal(harness.state.imports.length, 0);
});

test('passive native transport loss invalidates every cached consumer without restoring legacy writers', async () => {
  const harness = nativeHarness(staticSource());
  assert.equal(await harness.controller.activate(harness.prepared), true);
  assert.deepEqual(harness.controller.valueAtFrame('r08_curve_layer', 10), [25]);
  const previews = harness.state.previews.length;

  // N19F invalidates its binding without a callback when a revision ack,
  // malformed event, or host disconnect fails. The next synchronous boundary
  // must observe status=null and poison the local lifecycle before using cache.
  harness.state.connected = false;
  assert.equal(harness.controller.persistenceJSON(), null);
  assert.equal(harness.controller.status(), 'indeterminate');
  assert.equal(harness.controller.blocksLegacy(), true);
  assert.equal(harness.controller.isActive(), false);
  assert.throws(() => harness.controller.identity(), /indeterminate/);
  assert.throws(() => harness.controller.valueAtFrame('r08_curve_layer', 10), /not active/);
  assert.throws(() => harness.controller.projectSelection({ activeLayerUid: 'r08_curve_layer',
    selected: [{ layerUid: 'r08_curve_layer', opacityMode: 'static' }] }, 10), /not active/);
  assert.equal(harness.controller.renderPreview(10), true);
  assert.equal(harness.state.previews.length, previews);
  assert.throws(() => harness.controller.exportPng('/tmp/n20-disconnected-output', [10]), /not active/);
  assert.throws(() => harness.controller.setOpacity('r08_curve_layer', 40), /not active/);
  await assert.rejects(harness.controller.releaseCurrent('passive-disconnect-release'), /cannot release safely/);
  assert.equal(harness.controller.legacyIntent('set', staticSource().layers[0], [40]), false);
  assert.equal(harness.state.imports.length, 0);
});

test('transport loss while preview host work is pending rejects the stale receipt before registration', async () => {
  let finishPreview;
  const previewGate = new Promise((resolve) => { finishPreview = resolve; });
  const harness = nativeHarness(staticSource(), { previewGate });
  assert.equal(await harness.controller.activate(harness.prepared), true);
  assert.equal(harness.controller.renderPreview(10), true);
  for (let spin = 0; spin < 5 && harness.state.previews.length === 0; spin++) await Promise.resolve();
  assert.equal(harness.state.previews.length, 1);
  harness.state.connected = false;
  finishPreview(); await harness.controller.flush();
  assert.equal(harness.controller.status(), 'indeterminate');
  assert.equal(harness.controller.persistenceJSON(), null);
  assert.throws(() => harness.state.previewConsumers[0].receive({
    workId: 'preview-1', viewGeneration: 1, status: 'presented',
  }), /not registered/);
  assert.equal(harness.controller.blocksLegacy(), true);
});

test('release admits only an exact request-bound N19B terminal receipt', async () => {
  const corruptions = [
    ['missing field', (receipt) => { delete receipt.retrieved; }],
    ['request identity', (receipt) => { receipt.requestId = 'other-request'; }],
    ['document identity', (receipt) => { receipt.documentId = 'other-document'; }],
    ['revision', (receipt) => { receipt.contentRevision++; }],
    ['host generation', (receipt) => { receipt.lifecycleGeneration++; }],
    ['status', (receipt) => { receipt.status = 'failed'; }],
    ['retrieval', (receipt) => { receipt.retrieved = true; }],
    ['authority removal', (receipt) => { receipt.authorityRemovalCompleted = false; }],
    ['cancelled transaction pair', (receipt) => { receipt.cancelledTransactionId = 'transaction-1'; }],
    ['history depth', (receipt) => { receipt.undoDepth = -1; }],
    ['export reconciliation', (receipt) => { receipt.reconciledExports = [{ jobId: 'job-1' }]; }],
    ['cancelled preview ids', (receipt) => { receipt.cancelledPreviewWorkIds = ['bad id']; }],
    ['unresolved preview work', (receipt) => { receipt.unresolvedPreviewWorkIds = ['preview-1']; }],
    ['disposed viewport ids', (receipt) => { receipt.disposedViewportWorkIds = ['bad id']; }],
    ['reconciliation stages', (receipt) => { receipt.reconciliationStages.preview = 'pending'; }],
    ['viewport status', (receipt) => { receipt.viewportStatus = 'pending'; }],
    ['reentry', (receipt) => { receipt.reentryAvailable = false; }],
    ['terminal error', (receipt) => { receipt.error = { code: 'internal' }; }],
    ['unknown field', (receipt) => { receipt.extra = true; }],
  ];
  for (const [label, corrupt] of corruptions) {
    const harness = nativeHarness(staticSource(), { releaseReceipt(receipt) {
      corrupt(receipt); return receipt;
    } });
    assert.equal(await harness.controller.activate(harness.prepared), true, label);
    await assert.rejects(harness.controller.releaseCurrent('receipt-negative-control'),
      /successful terminal receipt/, label);
    assert.equal(harness.controller.status(), 'indeterminate', label);
    assert.equal(harness.state.imports.length, 0, label);
  }
});

test('release closes admission synchronously, drains only prior work, and rejects stale local generations', async () => {
  let finishMutation;
  const mutationGate = new Promise((resolve) => { finishMutation = resolve; });
  const harness = nativeHarness(staticSource(), { mutationGate });
  assert.equal(await harness.controller.activate(harness.prepared), true);
  const firstIdentity = harness.controller.getNativeIdentity();
  await assert.rejects(harness.controller.release({ kind: 'partial' }), /stale or malformed/);
  await assert.rejects(harness.controller.release({ kind: 'wrong-generation',
    documentId: firstIdentity.documentId, generation: firstIdentity.generation + 1 }), /stale or malformed/);
  const admitted = harness.controller.setOpacity('r08_curve_layer', 40);
  const release = harness.controller.releaseCurrent('writer-cutover');
  assert.equal(harness.controller.status(), 'release-requested');
  assert.throws(() => harness.controller.setOpacity('r08_curve_layer', 60), /not active/);
  assert.equal(harness.controller.renderPreview(10), true);
  assert.throws(() => harness.controller.exportPng('/tmp/n20-late-output', [10]), /not active/);
  assert.equal(harness.state.releases, 0, 'host release waits for already-admitted work');
  finishMutation();
  await admitted;
  assert.deepEqual(await release, { documentId: firstIdentity.documentId,
    generation: firstIdentity.generation, owner: 'legacy', status: 'released' });
  assert.equal(harness.state.imports[0].layers[0].motionStatic.opacity[0], 40);

  assert.equal(await harness.controller.activate(harness.prepared), true);
  const secondIdentity = harness.controller.getNativeIdentity();
  assert.notEqual(secondIdentity.generation, firstIdentity.generation);
  await assert.rejects(harness.controller.release({ kind: 'stale-callback',
    documentId: firstIdentity.documentId, generation: firstIdentity.generation }), /stale or malformed/);
  assert.equal(harness.controller.status(), 'native');
  assert.equal(harness.state.releases, 1);
});

test('preview, export, subscription and release state are lifecycle-scoped across reentry', async () => {
  const harness = nativeHarness(staticSource());
  assert.equal(await harness.controller.activate(harness.prepared), true);
  const firstSubscription = harness.state.subscriptions[0];
  const firstPreview = harness.state.previewConsumers[0];
  harness.controller.renderPreview(10); await harness.controller.flush();
  assert.equal((await harness.controller.exportPng('/tmp/n20-cycle-one', [10])).status, 'succeeded');
  await harness.controller.releaseCurrent('cycle-one');
  assert.throws(() => firstPreview.register({}, {}), /disposed/);

  assert.equal(await harness.controller.activate(harness.prepared), true);
  harness.controller.renderPreview(10); await harness.controller.flush();
  assert.equal((await harness.controller.exportPng('/tmp/n20-cycle-two', [10])).status, 'succeeded');
  assert.notStrictEqual(harness.state.previewConsumers[0], harness.state.previewConsumers[1]);
  assert.notStrictEqual(harness.state.exportConsumers[0], harness.state.exportConsumers[1]);
  await assert.rejects(firstSubscription.synchronize({ instanceId: 'instance-a',
    documentId: 'native-document-1', lifecycleGeneration: 1, fromRevision: 0,
    toRevision: 1, requestId: 'late-cycle-one' }), /closed lifecycle/);
  assert.equal(harness.controller.status(), 'native');
  assert.equal(harness.controller.identity().documentId, 'native-document-2');
});

test('project replacement waits for installation before one terminal release', async () => {
  let finishBootstrap;
  const gate = new Promise((resolve) => { finishBootstrap = resolve; });
  const harness = nativeHarness(staticSource(), { bootstrapGate: gate });
  const activation = harness.controller.activate(harness.prepared);
  assert.equal(harness.controller.status(), 'installing');
  assert.deepEqual(harness.controller.handleV1({ apiVersion: 1, requestId: 'during-install', instanceId: 'legacy-instance',
    documentId: 'legacy-document', operation: 'property.get',
    payload: { layerId: 'r08_curve_layer', property: 'opacity', frame: 10 } }), { apiVersion: 1, requestId: 'during-install',
    instanceId: 'legacy-instance', documentId: 'legacy-document', revision: 0, ok: false,
    error: { code: 'unavailable', message: 'Native opacity ownership is not dispatchable.' } });
  const release = harness.controller.releaseCurrent('document-replacement');
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
  let browserCreates = 0;
  const browser = { NemoOpacityApplicationCore: { createNative() { browserCreates++; } } };
  browser.window = browser;
  vm.runInNewContext(source, browser, { filename: 'opacity-bootstrap-browser.js' });
  assert.equal(browserCreates, 0);

  const events = [], invokes = [], imports = [], deniedEntries = [], toasts = [];
  let ports, nativeBlocked = false, pluginOpen = 0, panelOpen = 0;
  let delayedScript = null, delayedPlugin = null, delayedArchive = null;
  const serializedDocument = { totalFrames: 21, layers: [{ opacity: 25 }] };
  const paperProject = { activeLayer: { opacity: 1 }, itemCount: 1 };
  const activity = { scriptRuns: 0, pluginLoads: 0 };
  const binding = { instanceId: 'instance-a', documentId: 'native-document-1', contentRevision: 0 };
  const controller = {
    blocksLegacy() { return nativeBlocked; }, isActive() { return nativeBlocked; }, identity() { return null; },
    getNativeIdentity() { return null; },
    inspect() { return { phase: nativeBlocked ? 'native' : 'legacy', session: null }; },
    async activate(prepared) { events.push(['activate', prepared]); return true; },
    async releaseCurrent(reason) { events.push(['release', reason]); return null; },
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
    __TAURI__: { core: { invoke }, event: { async listen(name) {
      events.push(['tauri-listen', name]); return () => {};
    } } }, devicePixelRatio: 2,
    setTimeout(callback) { callback(); return 1; }, clearTimeout() {},
    document: { getElementById() { return { getBoundingClientRect() {
      return { left: 1, top: 2, width: 320, height: 180 };
    } }; } },
    localStorage: { getItem() { return null; } }, addEventListener(type) { events.push(['listen', type]); },
    state: { currentFrame: 10 },
    SM: { importJSON(value, silent) { imports.push([value, silent]); return true; },
      setTotalFrames() { serializedDocument.totalFrames = 99; paperProject.itemCount = 99; } },
    SMMotion: { setLayerValue() { serializedDocument.layers[0].opacity = 99; paperProject.activeLayer.opacity = 0.99; } },
    SMScript: {
      run() { activity.scriptRuns++; serializedDocument.layers[0].opacity = 88; paperProject.activeLayer.opacity = 0.88; },
      openFile() { delayedScript = () => { serializedDocument.layers[0].opacity = 77; paperProject.activeLayer.opacity = 0.77; }; },
      api() { return { layer() { return { set(_prop, value) {
        serializedDocument.layers[0].opacity = value[0]; paperProject.activeLayer.opacity = value[0] / 100;
      } }; } }; },
    },
    SMPlugin: {
      openFile() { delayedPlugin = () => { serializedDocument.layers[0].opacity = 76; paperProject.activeLayer.opacity = 0.76; }; },
      loadArchive() { delayedArchive = () => { serializedDocument.layers[0].opacity = 75; paperProject.activeLayer.opacity = 0.75; }; },
      loadFiles() { activity.pluginLoads++; serializedDocument.layers[0].opacity = 74; paperProject.activeLayer.opacity = 0.74; },
      openCount() { return pluginOpen; },
    },
    SMPanelUI: { openCount() { return panelOpen; } },
    SMProjectDocument: { prepareNativeOpacity(value) {
      if (value === 'unsupported') throw new Error('unsupported');
      return { projection: { format: 'nemo.native-opacity-document' }, resources: [{ resourceId: 'geometry/r08/0' }] };
    } },
    NemoApplication: { handle() { return 'legacy'; }, setInstanceId() {}, capabilities() { return []; } },
    NemoOpacityApplication: { meta() { return { instanceId: 'legacy', documentId: 'legacy', revision: 0 }; }, legacy() { return 'legacy'; } },
    NemoApplicationMcpTransport: { createNativeTauriTransport(received) {
      assert.strictEqual(received, invoke);
      return { async connect() { events.push(['connect']); return binding; },
        status() { return binding; },
        async subscribeRevisions(listen, synchronize) {
          events.push(['subscribe', listen, synchronize]);
          return { ...binding, lifecycleGeneration: 1, subscriptionId: 'subscription-1' };
        },
        async disconnect() { events.push(['disconnect']); } };
    } },
    NemoNativeApplicationAdapter: { createNativeApplicationAdapter(label) { events.push(['application', label]); return {}; } },
    NemoNativeOpacityPreviewAdapter: { createNativeOpacityPreviewAdapter() { return { kind: 'preview' }; } },
    NemoNativeOpacityExportAdapter: { createNativeOpacityExportAdapter() { return { kind: 'export' }; } },
    NemoNativeOpacityLegacySurface: NativeLegacySurface,
    NemoNativeOpacityMotionSurface: NativeMotionSurface,
    NemoOpacityApplicationCore: { createNative(received) {
      ports = received; events.push(['create']);
      return { install() { return NativeOpacityOperations.create(controller, received, NativeOpacityContract).install(); } };
    } },
    SMEngineBridge: {},
    SMNativeEditGuard: {
      install(received) { assert.strictEqual(received, controller); events.push(['guard-install']); return this; },
      allow(kind) { if (!nativeBlocked) return true; deniedEntries.push(kind); return false; },
    },
    showToast(message) { toasts.push(message); },
  };
  desktop.window = desktop;
  vm.runInNewContext(source, desktop, { filename: 'opacity-bootstrap-desktop.js' });
  assert.deepEqual(events.slice(0, 2), [['create'], ['guard-install']], 'guard installs before any authority activation');
  assert.strictEqual(desktop.SMEngineBridge.nativeEditGuard, desktop.SMNativeEditGuard);
  assert.equal(Object.isFrozen(desktop.NemoNativeOpacityCutover), true);
  assert.deepEqual(Object.keys(desktop.NemoNativeOpacityCutover).sort(), [
    'blocksLegacy', 'exportPng', 'historyFromUi', 'identity', 'isActive', 'persistenceJSON',
    'prepared', 'projectSelection', 'releaseCurrent', 'renderPreview',
  ]);
  for (const authorityKey of ['activate', 'requestRelease', 'getNativeIdentity', 'handleV1',
    'legacyIntent', 'setOpacity', 'history']) {
    assert.equal(desktop.NemoNativeOpacityCutover[authorityKey], undefined, authorityKey);
  }
  assert.deepEqual(ports.connectionStatus(), binding);

  const receipt = await ports.bootstrap({ projection: { format: 'nemo.native-opacity-document' }, resources: [{ resourceId: 'geometry/r08/0' }] });
  assert.equal(receipt.documentId, binding.documentId);
  assert.deepEqual(invokes.slice(0, 2).map(([command]) => command), ['nemo_mcp_identity', 'nemo_native_bootstrap']);
  assert.deepEqual(JSON.parse(JSON.stringify(invokes[1][1].request.viewport)), { cssBounds: { x: 1, y: 2, width: 320, height: 180 },
    physicalExtent: { width: 640, height: 360 }, compositionExtent: { width: 320, height: 180 }, reportedDpr: 2 });
  await ports.previewHost({ workId: 'preview' });
  await ports.bindOutput({ outputHandle: 'output/one' });
  await ports.release({ requestId: 'release-one' });
  assert.deepEqual(invokes.slice(2).map(([command]) => command), ['nemo_native_preview', 'nemo_native_bind_output', 'nemo_native_release']);

  pluginOpen = 1;
  assert.equal(await desktop.NemoNativeOpacityProject.importJSON('supported-open-plugin', false), true);
  assert.equal(events.filter(([kind]) => kind === 'activate').length, 0);
  pluginOpen = 0; panelOpen = 1;
  assert.equal(await desktop.NemoNativeOpacityProject.importJSON('supported-open-panel', false), true);
  assert.equal(events.filter(([kind]) => kind === 'activate').length, 0);
  panelOpen = 0;
  assert.equal(await desktop.NemoNativeOpacityProject.importJSON('supported', false), true);
  assert.deepEqual(events.find(([kind]) => kind === 'activate')[1].resources, [{ resourceId: 'geometry/r08/0' }]);

  nativeBlocked = true;
  const documentBytes = JSON.stringify(serializedDocument);
  const paperBytes = JSON.stringify(paperProject);
  assert.equal(desktop.SMScript.api(), false);
  assert.equal(desktop.SMScript.run('nemo.layer(0).set("opacity", [99])'), false);
  assert.equal(desktop.SMScript.openFile(), false);
  assert.equal(desktop.SMPlugin.openFile(), false);
  assert.equal(desktop.SMPlugin.loadArchive({}), false);
  assert.equal(desktop.SMPlugin.loadFiles({}), false);
  assert.equal(desktop.SM.setTotalFrames(99), false);
  assert.equal(desktop.SMMotion.setLayerValue(0, 'opacity', [99]), false);
  assert.equal(JSON.stringify(serializedDocument), documentBytes, 'serialized document bytes remain unchanged');
  assert.equal(JSON.stringify(paperProject), paperBytes, 'Paper project bytes remain unchanged');
  assert.equal(delayedScript, null); assert.equal(delayedPlugin, null); assert.equal(delayedArchive, null);
  assert.deepEqual(deniedEntries, ['script-api', 'script-run', 'script-openFile',
    'plugin-openFile', 'plugin-loadArchive', 'plugin-loadFiles',
    'timeline-setTotalFrames', 'motion-setLayerValue']);

  nativeBlocked = false;
  const retained = desktop.SMScript.api();
  desktop.SMScript.run('retained timer setup');
  desktop.SMScript.openFile();
  desktop.SMPlugin.openFile();
  desktop.SMPlugin.loadArchive({});
  desktop.SMPlugin.loadFiles({});
  assert.equal(await desktop.NemoNativeOpacityProject.importJSON('supported-after-extension', false), true);
  assert.equal(events.filter(([kind]) => kind === 'activate').length, 1,
    'page-lifetime extension exposure keeps later supported imports legacy-owned');
  retained.layer(0).set('opacity', [66]);
  delayedScript();
  delayedPlugin();
  delayedArchive();
  assert.equal(serializedDocument.layers[0].opacity, 75,
    'retained and delayed script/plugin capabilities only run while page stays legacy');
  assert.match(toasts.at(-1), /Reload or restart Nemo/);

  assert.equal(await desktop.NemoNativeOpacityProject.importJSON('unsupported', true), true);
  assert.deepEqual(imports, [['supported-open-plugin', false], ['supported-open-panel', false],
    ['supported', false], ['supported-after-extension', false], ['unsupported', true]]);
});
