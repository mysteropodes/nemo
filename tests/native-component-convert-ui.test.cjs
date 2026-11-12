'use strict';
// #2256 leaf 1: every ordinary "Convert to Component" entry (the Component panel button, the layer and Motion context menus, the
// keyboard menu) ends in SM.convertActiveLayerToComponent. On a native object project that entry sends the existing native convert
// transaction (NemoNativeObjectComponent, the #2244 request) as ONE history entry and selects the result layer; a refusal shows the
// baseline notice and writes nothing; the legacy writers are never called. The real listener body and the real SM entry of
// timeline.js are extracted and run; the real adapter child, intent runner and contract judge the receipts.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ConvertUI = require('../src/js/adapters/native-component-convert-ui.js');
const Component = require('../src/js/adapters/native-object-component.js');
const SRC = path.join(__dirname, '..', 'src', 'js');
const read = (name) => fs.readFileSync(path.join(SRC, name), 'utf8');
const OP = 'command.document.object.component.convert';

// The real SM entry literal of timeline.js, with the legacy writers it reaches replaced by recorders.
function realEntry(effects) {
  const text = read('timeline.js'), start = text.indexOf('  convertActiveLayerToComponent:function(){');
  const end = text.indexOf('  convertComponentToLayer:function()', start);
  assert.ok(start > 0 && end > start, 'SM.convertActiveLayerToComponent literal located');
  const context = vm.createContext({ _layerSel: [], state: { activeLayerIdx: 0 },
    convertLayerToComponent: (i) => effects.push(['legacy-one', i]), convertLayersToComponent: (l) => effects.push(['legacy-many', l.slice()]) });
  return { context, fn: vm.runInContext('({' + text.slice(start, end).trim().replace(/,$/, '') + '})', context).convertActiveLayerToComponent };
}
// The real `btn-comp` listener line of timeline.js, run against a fake element.
function realButton(root) {
  const line = read('timeline.js').split('\n').find((l) => l.includes("getElementById('btn-comp').addEventListener('click'"));
  assert.ok(line, 'btn-comp listener located');
  const listeners = {}, element = { addEventListener: (type, fn) => { listeners[type] = fn; } };
  vm.runInContext(line, vm.createContext({ document: { getElementById: (id) => { assert.equal(id, 'btn-comp'); return element; } }, window: root }));
  return listeners.click;
}

function fixture(options) {
  options = options || {};
  const effects = [], calls = [], toasts = [], selects = [];
  let info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 's4', canvasW: 320, canvasH: 180, totalFrames: 24,
    layers: [{ layerUid: 'a', name: 'Ink' }, { layerUid: 'b', name: 'Paint' }, { layerUid: 'c', name: 'Back' }] };
  const { fn } = realEntry(effects);
  const control = { gate: null, response: null, fail: null };
  let pendingRoute = false, owned = true, active = true;
  const session = {
    isActive: () => active, blocksLegacy: () => owned, info: () => info, requestId: (label) => label + '-' + (calls.length + 1),
    async runResourceCommand(action) {
      assert.equal(pendingRoute, false, 'the existing single resource queue is used');
      pendingRoute = true;
      try {
        return await action({
          async dispatch(request) {
            calls.push(JSON.parse(JSON.stringify(request)));
            if (control.gate) await control.gate;
            if (control.fail) throw control.fail;
            return control.response ? control.response(request) : { apiVersion: 2, requestId: request.requestId, instanceId: info.instanceId, documentId: info.documentId,
              contentRevision: info.contentRevision + 1, ok: true,
              result: request.payload.layerUids.length > 1 ? { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:5:0' } : { applied: true, historyEntriesAdded: 1 } };
          },
          async reconcile(revision) {
            if (revision === undefined) return info;
            const ids = calls[calls.length - 1].payload.layerUids;
            const kept = info.layers.filter((l) => !ids.includes(l.layerUid) || ids.length === 1);
            const layers = ids.length > 1
              ? info.layers.flatMap((l, k) => (k === info.layers.findIndex((x) => x.layerUid === ids[0]) ? [{ layerUid: 'native-layer:5:0', name: 'Composant' }] : ids.includes(l.layerUid) ? [] : [l]))
              : kept;
            info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision, layers };
            return info;
          },
        });
      } finally { pendingRoute = false; }
    },
  };
  const root = {
    state: { activeLayerIdx: 1, activeSymbolId: null, layers: info.layers.map((l) => ({ ...l })) }, _layerSel: [1],
    SM: { convertActiveLayerToComponent: fn, t: (key) => key }, showToast: (text) => toasts.push(text),
    NemoNativeObjectProject: { desktop: () => (options.legacyDocument ? null : session) },
    NemoNativeObjectComponent: Component,
  };
  const legacy = fn;
  // The entry hands its notices and the project facts through the existing ports.
  const ports = {
    currentSession: () => (options.legacyDocument ? null : (owned ? session : null)), projectionReady: () => true,
    selectLayer(index, preserve, uid, expected) { selects.push([index, preserve, uid, expected.contentRevision]); root.state.activeLayerIdx = index; root._layerSel = [index]; return true; },
  };
  if (options.badSource) root.badComponentSourceReason = (row) => (row.layerUid === options.badSource ? 'un composant' : null);
  const policy = { bindPolicyEntries: () => true, disposePolicyEntries: () => effects.push('policy-disposed') };
  const composed = ConvertUI.compose(root, ports, policy);
  // `Component.desktop` reads the session through NemoNativeObjectProject.desktop(root); the legacy writers are recorded in `effects`.
  return { root, composed, calls, toasts, selects, effects, control, legacy, session, info: () => info, setOwned: (v) => { owned = v; }, setActive: (v) => { active = v; },
    click: realButton(root) };
}
const settle = async () => { for (let i = 0; i < 12; i++) await new Promise((done) => setImmediate(done)); };

test('the Component button on one layer sends ONE native convert, selects the layer and never calls a legacy writer', async () => {
  const f = fixture();
  assert.equal(f.composed.bindPolicyEntries(), true);
  assert.notEqual(f.root.SM.convertActiveLayerToComponent, f.legacy, 'the native entry replaced the guarded legacy entry');
  assert.equal(await f.root.SM.convertActiveLayerToComponent(), true); await settle();
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0].operation, OP);
  assert.deepEqual(f.calls[0].payload, { command: 'object.component.convert', contextId: 'scene-root', layerUids: ['b'] });
  assert.equal(f.calls[0].expectedRevision, 4);
  assert.deepEqual(f.effects, [], 'no legacy writer ran');
  assert.deepEqual(f.selects, [[1, false, 'b', 5]]);
  assert.deepEqual(f.toasts, ['toastComponentCreatedPaint (Comp)'], 'baseline notice: name plus (Comp)');
  assert.equal(f.info().contentRevision, 5, 'exactly one revision advanced');
});

test('the real btn-comp listener body reaches the native entry (one request per click)', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  f.click.call(null);
  // The listener runs in its own context where `window` is the root; it calls window.SM.convertActiveLayerToComponent().
  await settle();
  assert.equal(f.calls.length, 1);
  assert.deepEqual(f.effects, []);
});

test('several selected layers merge in document order into ONE native convert and the merged instance is selected', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  f.root._layerSel = [2, 0]; f.root.state.activeLayerIdx = 2;
  assert.equal(await f.root.SM.convertActiveLayerToComponent(), true);
  assert.deepEqual(f.calls.map((c) => c.payload), [{ command: 'object.component.convert', contextId: 'scene-root', layerUids: ['a', 'c'] }]);
  assert.deepEqual(f.effects, []);
  assert.deepEqual(f.selects, [[0, false, 'native-layer:5:0', 5]], 'the merged instance sits at the first slot');
  assert.deepEqual(f.toasts, ['toastComponentCreatedWith2 calques']);
});

test('a native refusal shows the baseline notice, selects nothing and writes nothing', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  f.control.response = (request) => ({ apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: 4, ok: false,
    error: { code: 'unavailable', message: 'component-convert-items' } });
  assert.equal(await f.root.SM.convertActiveLayerToComponent(), false);
  assert.equal(f.calls.length, 1);
  assert.deepEqual(f.toasts, ['Impossible de convertir : component-convert-items']);
  assert.deepEqual(f.selects, []);
  assert.deepEqual(f.effects, []);
  assert.equal(f.info().contentRevision, 4, 'nothing committed');
  assert.deepEqual(f.root.state.layers.map((l) => l.layerUid), ['a', 'b', 'c']);
});

test('baseline guards run before anything is sent: inside a component, and a component source', async () => {
  const inside = fixture();
  inside.composed.bindPolicyEntries(); inside.root.state.activeSymbolId = 'sym_1';
  assert.equal(await inside.root.SM.convertActiveLayerToComponent(), false);
  assert.deepEqual(inside.toasts, ['toastCloseComponentFirst']);
  const bad = fixture({ badSource: 'b' });
  bad.composed.bindPolicyEntries();
  assert.equal(await bad.root.SM.convertActiveLayerToComponent(), false);
  assert.deepEqual(bad.toasts, ['Impossible de convertir : un composant']);
  bad.root._layerSel = [0, 1]; bad.root.state.activeLayerIdx = 1;
  assert.equal(await bad.root.SM.convertActiveLayerToComponent(), false);
  assert.equal(bad.toasts[1], 'toastCannotConvertSelectionContainsun composant');
  assert.equal(inside.calls.length + bad.calls.length, 0, 'no request was sent');
  assert.deepEqual(inside.effects.concat(bad.effects), []);
});

test('a stale selection (rows no longer match the native projection) is refused without a request', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries(); f.root.state.layers[1] = { layerUid: 'zzz', name: 'Gone' };
  assert.equal(await f.root.SM.convertActiveLayerToComponent(), false);
  assert.equal(f.calls.length, 0);
  assert.deepEqual(f.toasts, ['Native component conversion is unavailable.']);
});

test('two quick clicks send one request; a transport failure is reported and never falls back to a legacy writer', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  let release; f.control.gate = new Promise((resolve) => { release = resolve; });
  const first = f.root.SM.convertActiveLayerToComponent(), second = f.root.SM.convertActiveLayerToComponent();
  assert.equal(await second, false, 'the second click while one is in flight does nothing');
  release(); assert.equal(await first, true);
  assert.equal(f.calls.length, 1);
  const g = fixture();
  g.composed.bindPolicyEntries(); g.control.fail = new Error('boom');
  assert.equal(await g.root.SM.convertActiveLayerToComponent(), false);
  assert.match(g.toasts.join('|'), /Impossible de convertir/);
  assert.deepEqual(g.effects, []);
});

test('a legacy document keeps the original entry; dispose restores it only when the native owner is gone', async () => {
  const f = fixture({ legacyDocument: true });
  const original = f.root.SM.convertActiveLayerToComponent;
  f.composed.bindPolicyEntries();
  f.root._layerSel = [1];
  f.root.SM.convertActiveLayerToComponent();
  assert.equal(f.calls.length, 0, 'no native request for a legacy document');
  assert.equal(f.toasts.length, 0);
  f.composed.disposePolicyEntries();
  assert.equal(f.root.SM.convertActiveLayerToComponent, original, 'released: the original entry is back');
  assert.ok(f.effects.includes('policy-disposed'));
  const g = fixture();
  g.composed.bindPolicyEntries();
  const native = g.root.SM.convertActiveLayerToComponent;
  g.composed.disposePolicyEntries();
  assert.equal(g.root.SM.convertActiveLayerToComponent, native, 'an active native owner keeps its entry, which now refuses');
  assert.equal(await native(), false);
  assert.equal(g.calls.length, 0);
});

test('every ordinary Convert entry routes through SM.convertActiveLayerToComponent and the child is composed into the UI ports', () => {
  const timeline = read('timeline.js'), motion = read('motion.js'), app = read('bootstrap/native-opacity-application.js');
  assert.match(timeline, /getElementById\('btn-comp'\)\.addEventListener\('click',function\(\)\{window\.SM\.convertActiveLayerToComponent\(\);\}\)/);
  assert.match(timeline, /label:SM\.t\('ctxConvertToComponent'\)[^\n]*window\.SM\.convertActiveLayerToComponent\(\)/);
  assert.match(timeline, /case'convertToComponent':window\.SM\.convertActiveLayerToComponent\(\)/);
  assert.match(motion, /ctxConvertToComponent[^\n]*window\.SM\.convertActiveLayerToComponent\(\)/);
  assert.match(app, /NemoNativeComponentConvertUI[\s\S]*convert\.compose\(root, base, composed\)/);
  const html = read('../index.html');
  assert.ok(html.includes('js/adapters/native-component-convert-ui.js'));
});
