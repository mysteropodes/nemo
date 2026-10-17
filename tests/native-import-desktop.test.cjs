'use strict';
// L10-J: the desktop composition. A fake window carries the real Paper.js, a presented native session (helpers/native-import-session.cjs)
// and a DOM registry; the importers run end to end from the DOM event to the native batch, and no legacy importer global exists.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const desktop = require('../src/js/adapters/native-import-desktop.js');
const MODULES = { NemoNativeImportCore: 'core', NemoNativeImportSvg: 'svg', NemoNativeImportDrop: 'drop', NemoNativeImportFigma: 'figma', NemoNativeImportPsd: 'psd', NemoNativeImportMontage: 'montage' };
const helper = require('./helpers/native-import-paper.cjs');
const { makeSession } = require('./helpers/native-import-session.cjs');

const SVG = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixtures', 'native-import-adapters', 'svg-flows.json'), 'utf8'));
const FIGMA = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixtures', 'native-import-adapters', 'figma-walk.json'), 'utf8'));
const realPaper = helper.load();
const tick = () => new Promise((resolve) => setImmediate(resolve));

function fakeWindow(options) {
  const o = options || {};
  const h = makeSession(Object.assign({ layers: [{ layerUid: 'L0', name: 'Layer 1', visible: true, locked: false, parentLayerUid: null, color: '#a1b2c3' }] }, o.session));
  const listeners = {}, toasts = [], store = {};
  const el = (id) => ({ id, value: '', files: null, clicks: 0, click() { this.clicks += 1; }, addEventListener(t, fn) { (listeners[`${id}:${t}`] = listeners[`${id}:${t}`] || []).push(fn); }, removeEventListener(t, fn) { listeners[`${id}:${t}`] = (listeners[`${id}:${t}`] || []).filter((f) => f !== fn); } });
  const els = {};
  ['canvas-area', 'timeline-area', 'btn-import-psd', 'psd-input', 'figma-token', 'figma-token-save', 'figma-url', 'figma-import-btn'].forEach((id) => { els[id] = el(id); });
  const doc = { readyState: 'complete', getElementById: (id) => els[id] || null, addEventListener: (t, fn) => { (listeners[`document:${t}`] = listeners[`document:${t}`] || []).push(fn); }, removeEventListener: (t, fn) => { listeners[`document:${t}`] = (listeners[`document:${t}`] || []).filter((f) => f !== fn); } };
  const tree = o.tree || null;
  const root = {
    document: doc, state: { layers: [{ layerUid: 'L0' }], activeLayerIdx: 0, currentFrame: 0 }, _layerSel: [0],
    NemoNativeObjectProject: { desktop: () => h.session, selectLayerUI: () => true },
    NemoNativeObjectProjectFile: { projectionReadyForUI: () => true },
    paper: { project: { importSVG: () => (typeof tree === 'function' ? tree() : tree) }, Matrix: realPaper.Matrix, Path: realPaper.Path, CompoundPath: realPaper.CompoundPath, Segment: realPaper.Segment, Point: realPaper.Point },
    SM: { t: (k) => SVG.templates.en[k] || k }, showToast: (m) => toasts.push(m),
    localStorage: { getItem: (k) => (k in store ? store[k] : null), setItem: (k, v) => { store[k] = v; } },
    NemoNativeProjectSettingsController: { edit: async (a) => { o.settings && o.settings.push(a); return { status: 'applied-presented' }; } },
    clearTimeout: (id) => { if (o.cleared) o.cleared.push(id); },
    fetch: o.fetch,
  };
  for (const [global, file] of Object.entries(MODULES)) root[global] = require(`../src/js/adapters/native-import-${file}.js`);
  h.session.isActive = () => true;
  return { root, h, listeners, toasts, els, store };
}
const flow = (name) => SVG.flows.find((f) => f.name === name);
const fire = (w, key, event) => Promise.all((w.listeners[key] || []).map((fn) => fn(event)));
function dropEvent(files) {
  const flags = { prevented: false };
  return { e: { dataTransfer: { types: ['Files'], files, dropEffect: '' }, currentTarget: { classList: { add() {}, remove() {} } }, preventDefault() { flags.prevented = true; } }, flags };
}
const svgFile = (name) => ({ name, type: 'image/svg+xml', text: async () => '<svg/>' });

test('installing wires the drop targets, paste, the PSD and Figma panels and exposes only native importers', () => {
  const w = fakeWindow({ tree: helper.mkNode(realPaper, flow('plain-one').tree) });
  const controller = desktop.install(w.root);
  assert.equal(desktop.install(w.root), controller, 'one controller per window');
  assert.equal(w.root.NemoNativeImportController, controller);
  assert.equal(w.root.SMSvgImport.native, true, 'the legacy callers (timeline.js menu entry, paste fallback arming) reach the native adapter');
  assert.deepEqual(Object.keys(w.listeners).sort(), [
    'btn-import-psd:click', 'canvas-area:dragleave', 'canvas-area:dragover', 'canvas-area:drop', 'document:paste', 'figma-import-btn:click', 'figma-token-save:click',
    'psd-input:change', 'timeline-area:dragleave', 'timeline-area:dragover', 'timeline-area:drop',
  ]);
  assert.equal(w.root.SMFigmaImport, undefined);
  assert.equal(w.root.SMPsdImport, undefined);
  assert.equal(w.root.SMLabs, undefined);
  assert.equal(w.root.SMSvgImport.isSvgFile({ name: 'a.SVG', type: '' }), true);
});

test('an SVG dropped on the canvas becomes one native batch, with the baseline summary and per-file toasts', async () => {
  const w = fakeWindow({ tree: helper.mkNode(realPaper, flow('plain-one').tree) });
  desktop.install(w.root);
  const { e, flags } = dropEvent([svgFile('logo.svg')]);
  await fire(w, 'canvas-area:drop', e);
  assert.equal(flags.prevented, true);
  assert.equal(w.h.calls.length, 1);
  assert.deepEqual(w.h.calls[0].records.map((r) => r.sourceId), ['svg:0:1:0']);
  assert.deepEqual(w.toasts, ['Import : 1 SVG (vecteurs)', '1 vector shape(s) imported']);
  assert.equal(w.h.state.contentRevision, 8);
});

test('a drop that mixes an SVG with a group no native provider can take applies nothing and says so', async () => {
  const w = fakeWindow({ tree: helper.mkNode(realPaper, flow('plain-one').tree) });
  desktop.install(w.root);
  await fire(w, 'canvas-area:drop', dropEvent([svgFile('a.svg'), { name: 'b.png', type: 'image/png', text: async () => '' }]).e);
  assert.equal(w.h.calls.length, 0);
  assert.deepEqual(w.toasts, ['Nothing was imported: image cannot be imported natively yet.']);
  w.toasts.length = 0;
  await fire(w, 'canvas-area:drop', dropEvent([{ name: 'doc.pdf', type: 'application/pdf', text: async () => '' }]).e);
  assert.deepEqual(w.toasts, ['Use Import Reference to choose a PNG file for this native project.'], 'only the baseline reference toast, no second line');
});

test('pasted SVG markup is imported and the armed paste-frames fallback is cancelled', async () => {
  const cleared = [];
  const w = fakeWindow({ tree: helper.mkNode(realPaper, flow('plain-one').tree), cleared });
  w.root._svgPasteFallback = 77;
  desktop.install(w.root);
  let prevented = false;
  await fire(w, 'document:paste', { target: { tagName: 'DIV' }, preventDefault: () => { prevented = true; }, clipboardData: { items: [], getData: (t) => (t === 'text/plain' ? '<svg><path/></svg>' : '') } });
  assert.equal(prevented, true);
  assert.deepEqual(cleared, [77]);
  assert.equal(w.root._svgPasteFallback, null);
  assert.equal(w.h.calls.length, 1);
});

test('the menu entry picks a file through the desktop dialog and imports it; a cancel imports nothing', async () => {
  const w = fakeWindow({ tree: helper.mkNode(realPaper, flow('plain-one').tree) });
  const seen = [];
  w.root.__TAURI__ = { dialog: { open: async (o) => { seen.push(o); return '/tmp/x.svg'; } }, fs: { readTextFile: async (p) => { seen.push(p); return '<svg/>'; } } };
  desktop.install(w.root);
  const out = await w.root.SMSvgImport.openFile();
  assert.equal(out.status, 'imported');
  assert.deepEqual(seen, [{ title: 'Importer un SVG', multiple: false, filters: [{ name: 'SVG', extensions: ['svg'] }] }, '/tmp/x.svg']);
  const w2 = fakeWindow({});
  w2.root.__TAURI__ = { dialog: { open: async () => null }, fs: {} };
  desktop.install(w2.root);
  assert.deepEqual(await w2.root.SMSvgImport.openFile(), { status: 'cancelled' });
  assert.equal(w2.h.calls.length, 0);
});

test('a locked active layer, an unconfirmed receipt and an oversize batch each leave a typed notice and publish nothing wrong', async () => {
  const locked = fakeWindow({ tree: helper.mkNode(realPaper, flow('plain-one').tree), session: { layers: [{ layerUid: 'L0', name: 'Layer 1', visible: true, locked: true, parentLayerUid: null, color: '#a1b2c3' }] } });
  desktop.install(locked.root);
  await locked.root.SMSvgImport.importString('<svg/>');
  assert.deepEqual(locked.toasts, [SVG.templates.en.toastInvalidLockedLayer]);
  assert.equal(locked.h.calls.length, 0);
  const big = fakeWindow({ tree: helper.mkNode(realPaper, flow('rand-0').tree) });
  desktop.install(big.root);
  await big.root.SMSvgImport.importString('<svg/>');
  assert.match(big.toasts[0], /^The import is larger than the native request bound \(4096 bytes\)\. Nothing was imported\.$/);
  const lost = fakeWindow({ tree: helper.mkNode(realPaper, flow('plain-one').tree), session: { fault: 'transport' } });
  desktop.install(lost.root);
  await lost.root.SMSvgImport.importString('<svg/>');
  assert.deepEqual(lost.toasts, ['The import was not confirmed. Do not retry until the document is synchronized.']);
  const none = fakeWindow({ session: { declared: false } });
  desktop.install(none.root);
  await none.root.SMSvgImport.importString('<svg/>');
  assert.deepEqual(none.toasts, ['Native import is unavailable.']);
});

test('the PSD panel reads the file, refuses the raster batch through the owner and leaves the canvas alone', async () => {
  const settings = [];
  const w = fakeWindow({ settings });
  const psd = { width: 800, height: 600, children: [{ name: 'Fond', canvas: { width: 800, height: 600 }, left: 0, top: 0, opacity: 1 }] };
  desktop.install(w.root, { readPsd: async () => psd });
  w.els['psd-input'].files = [{ name: 'a.psd' }];
  await fire(w, 'psd-input:change', { target: w.els['psd-input'] });
  await tick();
  assert.equal(w.els['psd-input'].value, '');
  assert.deepEqual(w.toasts, ['Import PSD…', 'This import needs raster-asset, which the native engine does not provide yet. Nothing was imported.']);
  assert.deepEqual(settings, [], 'no canvas resize without a committed import');
  assert.equal(w.h.state.contentRevision, 7);
  await fire(w, 'btn-import-psd:click', {});
  assert.equal(w.els['psd-input'].clicks, 1);
});

test('ag-psd reaches the adapter only through the retained legacy loader port, and a missing loader is a typed failure', async () => {
  const psd = { width: 800, height: 600, children: [{ name: 'Fond', canvas: { width: 800, height: 600 }, left: 0, top: 0, opacity: 1 }] };
  const w = fakeWindow();
  const loads = [], buffers = [];
  w.root.SMAgPsdLoader = { load: async () => { loads.push(1); return { readPsd: (buffer) => { buffers.push(buffer); return psd; } }; } };
  desktop.install(w.root);
  w.els['psd-input'].files = [{ name: 'a.psd', arrayBuffer: async () => 'bytes' }];
  await fire(w, 'psd-input:change', { target: w.els['psd-input'] });
  await tick();
  assert.deepEqual([loads.length, buffers], [1, ['bytes']]);
  assert.equal(w.toasts[1], 'This import needs raster-asset, which the native engine does not provide yet. Nothing was imported.');
  const bare = fakeWindow();
  desktop.install(bare.root);
  bare.els['psd-input'].files = [{ name: 'a.psd', arrayBuffer: async () => 'bytes' }];
  await fire(bare, 'psd-input:change', { target: bare.els['psd-input'] });
  await tick();
  assert.deepEqual(bare.toasts, ['Import PSD…', `${bare.root.SM.t('toastPsdImportFailedSuffix')}The ag-psd loader is unavailable`]);
  assert.equal(bare.h.calls.length, 0);
  const source = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'adapters', 'native-import-desktop.js'), 'utf8');
  assert.equal(/\bimport\s*\(\s*['"`]/.test(source.replace(/\/\/.*$/gm, '')), false, 'the profiled adapter carries no dynamic import of the unprofiled vendor module');
});

test('the layer select port re-checks the session at the moment of selection and never selects through a swapped session', async () => {
  const g = FIGMA.glue.find((x) => x.id === 'file-url');
  const fetchFn = async (url) => ({ ok: true, status: 200, json: async () => (/\/images$/.test(url) ? { images: {} } : g.responses.files.json) });
  const attempt = async (swapAfterReadiness) => {
    const other = makeSession();
    const w = fakeWindow({ fetch: fetchFn, tree: () => helper.mkNode(realPaper, flow('plain-one').tree) });
    const picked = [];
    let swapped = false;
    w.root.NemoNativeObjectProject.desktop = () => (swapped ? other.session : w.h.session);
    w.root.NemoNativeObjectProject.selectLayerUI = (...args) => { picked.push(args[3]); return true; };
    // The last check of `available` (projection ready) runs after the batch was sent; the session is swapped right after it passes.
    w.root.NemoNativeObjectProjectFile.projectionReadyForUI = () => { if (swapAfterReadiness && w.h.calls.length === 1) swapped = true; return true; };
    desktop.install(w.root);
    const out = await w.root.NemoNativeImportController.importFigma('https://www.figma.com/design/AbC123/My-file', 'tok');
    return { w, out, picked, swapped };
  };
  const swappedRun = await attempt(true);
  assert.equal(swappedRun.w.h.calls.length, 1);
  assert.deepEqual([swappedRun.swapped, swappedRun.picked.length], [true, 0], 'the UI selection was not asked through the other session');
  const control = await attempt(false);
  assert.deepEqual(control.picked, ['native-layer:8:0'], 'without the swap the same flow does reach the UI selection of the new layer');
});

test('the Figma panel keeps the baseline token storage and strings, and imports a vector-only file natively', async () => {
  const settings = [];
  const g = FIGMA.glue.find((x) => x.id === 'file-url');
  const fetches = [];
  const fetchFn = async (url, init) => { fetches.push([url, init && init.headers]); return { ok: true, status: 200, json: async () => (/\/images$/.test(url) ? { images: {} } : g.responses.files.json) }; };
  const w = fakeWindow({ settings, fetch: fetchFn, tree: () => helper.mkNode(realPaper, flow('plain-one').tree) });
  desktop.install(w.root);
  await fire(w, 'figma-import-btn:click', {});
  assert.deepEqual(w.toasts, ["Colle ton token Figma personnel d'abord"]);
  w.els['figma-token'].value = 'tok';
  w.toasts.length = 0;
  await fire(w, 'figma-token-save:click', {});
  assert.equal(w.store['sm-figma-token'], 'tok');
  assert.deepEqual(w.toasts, ['Token Figma enregistré']);
  w.toasts.length = 0;
  await fire(w, 'figma-import-btn:click', {});
  assert.deepEqual(w.toasts, ['hsFigmaPaste'], 'no URL: the baseline i18n key');
  w.els['figma-url'].value = 'https://www.figma.com/design/AbC123/My-file';
  w.toasts.length = 0;
  await fire(w, 'figma-import-btn:click', {});
  await tick();
  assert.deepEqual(fetches.map((f) => f[0]), ['https://api.figma.com/v1/files/AbC123?geometry=paths', 'https://api.figma.com/v1/files/AbC123/images']);
  assert.deepEqual(fetches[0][1], { 'X-Figma-Token': 'tok' });
  assert.deepEqual(w.h.calls[0].layers, [{ sourceId: 'figma:0', name: 'Net' }]);
  assert.deepEqual(settings, [{ kind: 'setCanvasSize', width: 100, height: 80 }], 'the canvas follows the first frame after the commit');
  assert.equal(w.toasts[0], 'Import Figma…');
  assert.equal(w.toasts[w.toasts.length - 1], '1 frame(s), 1 forme(s), 0 texte(s), 0 image(s) importé(s)');
});

test('missing importer modules are a load error, never a silent legacy fallback', () => {
  const w = fakeWindow({});
  delete w.root.NemoNativeImportSvg;
  assert.throws(() => desktop.install(w.root), /importer modules are unavailable/);
  assert.equal(w.root.SMSvgImport, undefined);
});

test('notices map every non-success status, stay silent for success and cancel, and dispose retires the runner', () => {
  const n = desktop.noticeFor;
  assert.equal(n({ status: 'imported' }), null);
  assert.equal(n({ status: 'cancelled' }), null);
  assert.equal(n({ status: 'nothing' }), null);
  assert.equal(n(null), null);
  assert.equal(n({ status: 'stale' }), 'The document changed during the import. Nothing was imported.');
  assert.equal(n({ status: 'busy' }), 'Another import is still running.');
  assert.equal(n({ status: 'refused', reason: 'layer' }), null, 'the baseline locked-layer toast is not repeated');
  assert.equal(n({ status: 'refused', code: 'not_found' }), 'The native engine refused the import (not_found). Nothing was imported.');
  assert.equal(n({ status: 'invalid', reason: 'segment_count' }), 'The import was refused before sending (segment_count). Nothing was imported.');
  assert.equal(n({ status: 'unavailable', blocked: [{ group: 'other', reason: 'use-import-reference' }] }), null);
  const w = fakeWindow({});
  const controller = desktop.install(w.root);
  controller.dispose();
  assert.equal(controller.core.begin().reason, 'retired');
});
