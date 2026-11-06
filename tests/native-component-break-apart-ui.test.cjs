'use strict';
// D-comp-break-apart: the three "Break apart the component" surfaces (the panel button #btn-comp-detach, the layer context menu and the
// Motion row menu) all end in SM.convertComponentToLayer. On a native object project that entry sends ONE
// command.document.object.component.breakApart request for the active component layer (one history entry), keeps the layer selected and
// never reaches the legacy writer; baseline refusals (inside a component, not a component) run before anything is sent and a native
// refusal says why and writes nothing. The real SM literal and the real listener line of timeline.js run against the real adapters.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ConvertUI = require('../src/js/adapters/native-component-convert-ui.js');
const Component = require('../src/js/adapters/native-object-component.js');
const SRC = path.join(__dirname, '..', 'src', 'js');
const read = (name) => fs.readFileSync(path.join(SRC, name), 'utf8');
const OP = 'command.document.object.component.breakApart';

function realEntries(effects) {
  const text = read('timeline.js');
  const start = text.indexOf('  convertActiveLayerToComponent:function(){');
  const mid = text.indexOf('  convertComponentToLayer:function()', start);
  const end = text.indexOf('\n', mid);
  assert.ok(start > 0 && mid > start, 'SM literals located');
  const context = vm.createContext({ _layerSel: [], state: { activeLayerIdx: 0 },
    convertComponentToLayer: (i) => effects.push(['legacy-break', i]), convertLayerToComponent: () => {}, convertLayersToComponent: () => {} });
  const api = vm.runInContext('({' + text.slice(start, end).trim().replace(/,$/, '') + '})', context);
  return { context, convertActiveLayerToComponent: api.convertActiveLayerToComponent, convertComponentToLayer: api.convertComponentToLayer };
}
function listener(root, id) {
  const line = read('timeline.js').split('\n').find((l) => l.includes("getElementById('" + id + "').addEventListener('click'"));
  assert.ok(line, id + ' listener located');
  const listeners = {}, element = { addEventListener: (type, fn) => { listeners[type] = fn; } };
  vm.runInContext(line, vm.createContext({ document: { getElementById: () => element }, window: root }));
  return listeners.click;
}

function fixture(options) {
  options = options || {};
  const effects = [], calls = [], toasts = [], selects = [];
  let info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 's4', canvasW: 320, canvasH: 180, totalFrames: 24,
    layers: [{ layerUid: 'a', name: 'Ink', kind: 'drawing' }, { layerUid: 'b', name: 'Paint (Comp)', kind: 'component', componentId: 'sym_x' }, { layerUid: 'c', name: 'Back', kind: 'drawing' }] };
  const real = realEntries(effects);
  const control = { response: null };
  const session = {
    isActive: () => true, blocksLegacy: () => true, info: () => info, requestId: (label) => label + '-' + (calls.length + 1),
    async runResourceCommand(action) {
      return action({
        async dispatch(request) {
          calls.push(JSON.parse(JSON.stringify(request)));
          return control.response ? control.response(request) : { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd',
            contentRevision: info.contentRevision + 1, ok: true, result: { applied: true, historyEntriesAdded: 1 } };
        },
        async reconcile(revision) {
          if (revision === undefined) return info;
          info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision,
            layers: info.layers.map((l) => (l.layerUid === calls[calls.length - 1].payload.layerUid ? { layerUid: l.layerUid, name: 'Paint', kind: 'drawing' } : l)) };
          return info;
        },
      });
    },
  };
  const root = {
    state: { activeLayerIdx: options.active === undefined ? 1 : options.active, activeSymbolId: options.inside ? 'sym_x' : null, layers: info.layers.map((l) => ({ ...l })) }, _layerSel: [1],
    SM: { convertActiveLayerToComponent: real.convertActiveLayerToComponent, convertComponentToLayer: real.convertComponentToLayer, t: (key) => key },
    showToast: (text) => toasts.push(text),
    NemoNativeObjectProject: { desktop: () => (options.legacyDocument ? null : session) },
    NemoNativeObjectComponent: Component,
  };
  const legacyBreak = real.convertComponentToLayer;
  const ports = {
    currentSession: () => (options.legacyDocument ? null : session), projectionReady: () => true,
    selectLayer(index, preserve, uid, expected) { selects.push([index, preserve, uid, expected.contentRevision]); root.state.activeLayerIdx = index; return true; },
  };
  const composed = ConvertUI.compose(root, ports, { bindPolicyEntries: () => true, disposePolicyEntries: () => {} });
  return { root, composed, calls, toasts, selects, effects, control, legacyBreak, info: () => info, detach: listener(root, 'btn-comp-detach') };
}
const settle = async () => { for (let i = 0; i < 12; i++) await new Promise((done) => setImmediate(done)); };

test('Break apart sends ONE native breakApart for the active component layer, keeps it selected and never calls the legacy writer', async () => {
  const f = fixture();
  assert.equal(f.composed.bindPolicyEntries(), true);
  assert.notEqual(f.root.SM.convertComponentToLayer, f.legacyBreak, 'the native entry replaced the guarded legacy entry');
  assert.equal(await f.root.SM.convertComponentToLayer(), true); await settle();
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0].operation, OP);
  assert.deepEqual(f.calls[0].payload, { command: 'object.component.breakApart', contextId: 'scene-root', layerUid: 'b' });
  assert.equal(f.calls[0].expectedRevision, 4);
  assert.deepEqual(f.effects, [], 'no legacy writer ran');
  assert.deepEqual(f.selects, [[1, false, 'b', 5]]);
  assert.deepEqual(f.toasts, ['toastComponentBrokenToLayer']);
  assert.equal(f.info().contentRevision, 5, 'exactly one revision advanced');
});

test('the real #btn-comp-detach listener reaches the native entry (one request per click)', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  f.detach.call(null); await settle();
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0].payload.layerUid, 'b');
  assert.deepEqual(f.effects, []);
});

test('the baseline guards run before anything is sent: inside a component, and a layer that is not a component', async () => {
  const inside = fixture({ inside: true });
  inside.composed.bindPolicyEntries();
  assert.equal(await inside.root.SM.convertComponentToLayer(), false);
  assert.deepEqual(inside.toasts, ['toastCloseComponentFirst']);
  assert.equal(inside.calls.length, 0);
  const plain = fixture({ active: 0 });
  plain.composed.bindPolicyEntries();
  assert.equal(await plain.root.SM.convertComponentToLayer(), false);
  assert.deepEqual(plain.toasts, ['toastNotAComponent']);
  assert.equal(plain.calls.length, 0);
  assert.deepEqual(plain.effects, []);
});

test('a native refusal shows why, selects nothing and writes nothing', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  f.control.response = (request) => ({ apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: 4, ok: false,
    error: { code: 'unavailable', message: 'component-break-apart: Motion on the inner layers cannot be baked into the layer yet.' } });
  assert.equal(await f.root.SM.convertComponentToLayer(), false);
  assert.equal(f.calls.length, 1);
  assert.deepEqual(f.toasts, ['Component could not be broken apart: component-break-apart: Motion on the inner layers cannot be baked into the layer yet.']);
  assert.deepEqual(f.selects, []);
  assert.deepEqual(f.effects, []);
  assert.equal(f.info().contentRevision, 4, 'nothing committed');
});

test('a legacy document keeps its own entry and sends no native request', async () => {
  const f = fixture({ legacyDocument: true });
  f.composed.bindPolicyEntries();
  await f.root.SM.convertComponentToLayer();
  assert.equal(f.calls.length, 0);
  assert.equal(f.effects.length, 1, 'the original entry ran, unchanged');
  assert.equal(f.effects[0][0], 'legacy-break');
});
