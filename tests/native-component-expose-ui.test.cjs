'use strict';
// #2256: the canvas context menu's "Expose opacity / visibility as a Component property" entries under the native owner. On a native object
// project each entry is ONE command.document.object.component.expose request (symbolId = the held component, an ep_ key minted by the
// intent, targetStrokeId = the presented selection's native id, field opacity | __visible); the toast follows ok:true only, a typed
// refusal shows its reason, and the legacy SM.exposeSymbolProperty is never called. The real select-bridge onContext hook runs against
// the real adapter, intent runner and contract.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const ExposeUI = require('../src/js/adapters/native-component-expose-ui.js');
const Component = require('../src/js/adapters/native-object-component.js');
const OP = 'command.document.object.component.expose';

function fixture(options) {
  options = options || {};
  const calls = [], toasts = [], legacy = [], prompts = [];
  const info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 's4', canvasW: 320, canvasH: 180, totalFrames: 24,
    layers: [{ layerUid: 'a', name: 'Ink' }] };
  const control = { response: null };
  const session = {
    isActive: () => true, blocksLegacy: () => !options.legacyDocument, info: () => info, requestId: (label) => label + '-' + (calls.length + 1),
    async runResourceCommand(action) {
      return action({
        async dispatch(request) {
          calls.push(JSON.parse(JSON.stringify(request)));
          return control.response ? control.response(request) : { apiVersion: 2, requestId: request.requestId, instanceId: info.instanceId, documentId: info.documentId,
            contentRevision: info.contentRevision + 1, ok: true, result: { applied: true, historyEntriesAdded: 1 } };
        },
        async reconcile() { return info; },
      });
    },
  };
  const root = {
    state: { activeSymbolId: options.scene ? null : 'sym_1', activeLayerIdx: 0 },
    SM: { exposeSymbolProperty: (...args) => { legacy.push(args); return false; } },
    prompt: (text, fallback) => { prompts.push([text, fallback]); return options.cancel ? null : (options.answer || fallback); },
    showToast: (text) => toasts.push(text),
    NemoNativeObjectProject: { desktop: () => (options.legacyDocument ? null : session) },
    NemoNativeObjectComponent: Component,
    NemoNativeComponentContext: { contextId: () => (options.scene ? 'scene-root' : 'sym_1') },
  };
  const item = { opacity: 1, data: { strokeId: 'st-7', nativeStyle: { opacity: options.opacity === undefined ? 0.5 : options.opacity } } };
  return { root, calls, toasts, legacy, prompts, control, item };
}
const settle = async () => { for (let i = 0; i < 12; i++) await new Promise((done) => setImmediate(done)); };

test('the opacity entry sends ONE native expose with the held component, a minted key, the presented stroke id and the baseline default', async () => {
  const f = fixture();
  const items = ExposeUI.menuItems(f.root, [f.item]);
  assert.deepEqual(items.map((i) => i.label), ["Exposer l'opacité comme propriété de Component…", 'Exposer la visibilité comme propriété de Component…']);
  await items[0].action(); await settle();
  assert.deepEqual(f.prompts, [['Nom de cette propriété (visible dans le panneau Motion de chaque instance) :', 'Opacité']]);
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0].operation, OP);
  const p = f.calls[0].payload;
  assert.match(p.key, /^ep_[0-9a-z]+_[0-9a-z]{1,4}$/);
  assert.deepEqual({ ...p, key: 'K' }, { command: 'object.component.expose', symbolId: 'sym_1', key: 'K', targetStrokeId: 'st-7', targetField: 'opacity', label: 'Opacité', default: 50 });
  assert.deepEqual(f.toasts, ['Propriété "Opacité" exposée sur ce Component.']);
  assert.deepEqual(f.legacy, [], 'the legacy writer is never called');
});

test('the visibility entry exposes __visible with default 100', async () => {
  const f = fixture();
  await ExposeUI.menuItems(f.root, [f.item])[1].action(); await settle();
  assert.equal(f.calls.length, 1);
  assert.deepEqual({ ...f.calls[0].payload, key: 'K' }, { command: 'object.component.expose', symbolId: 'sym_1', key: 'K', targetStrokeId: 'st-7', targetField: '__visible', label: 'Visible', default: 100 });
  assert.deepEqual(f.toasts, ['Propriété "Visible" exposée sur ce Component.']);
});

test('a typed refusal shows its reason and no success toast; a cancelled prompt sends nothing', async () => {
  const f = fixture();
  f.control.response = (request) => ({ apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: 4, ok: false,
    error: { code: 'unavailable', message: 'refused by the engine' } });
  const out = await ExposeUI.menuItems(f.root, [f.item])[0].action(); await settle();
  assert.equal(out, false); assert.equal(f.calls.length, 1);
  assert.equal(f.toasts.length, 1); assert.match(f.toasts[0], /^Impossible d'exposer : /); assert.doesNotMatch(f.toasts[0], /exposée/);
  assert.deepEqual(f.legacy, []);
  const g = fixture({ cancel: true });
  await ExposeUI.menuItems(g.root, [g.item])[0].action(); await settle();
  assert.deepEqual([g.calls.length, g.toasts.length], [0, 0]);
});

test('no entries at the scene root or for a multi-selection; a legacy document answers null so the original entries stand', () => {
  assert.deepEqual(ExposeUI.menuItems(fixture({ scene: true }).root, [fixture().item]), []);
  const f = fixture();
  assert.deepEqual(ExposeUI.menuItems(f.root, [f.item, f.item]), []);
  assert.deepEqual(ExposeUI.menuItems(f.root, [{ data: {} }]), []);
  assert.equal(ExposeUI.menuItems(fixture({ legacyDocument: true }).root, [f.item]), null);
});

test('select-bridge routes the Expose entries through the native adapter and keeps the legacy writer for a legacy document', () => {
  const text = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'select-bridge.js'), 'utf8');
  const at = text.indexOf('NemoNativeComponentExposeUI.menuItems(window, selectedPaths)');
  assert.ok(at > 0, 'the hook calls the native adapter');
  const rest = text.slice(at, at + 900);
  assert.match(rest, /if \(nativeExpose\) \{[\s\S]*\} else if \(state\.activeSymbolId/, 'the legacy block is only the else branch of the native answer');
});
