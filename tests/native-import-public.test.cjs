'use strict';
// L10-J: the public declaration and the shape of the adapters, checked on the draft files alone (no shared file needed):
// descriptor <-> contract agreement, the module layering of the app-js profile, size ceilings, the legacy-free source scan, and the
// browser load path (every file evaluated as a classic script in index.html order, then the desktop composition installing itself).
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const contract = require('../src/js/adapters/native-import-contract.js');
const seam = require('../src/js/adapters/native-import-seam.js');

const ROOT = path.join(__dirname, '..');
const read = (rel) => fs.readFileSync(path.join(ROOT, rel), 'utf8');
const ADAPTERS = 'src/js/adapters/';
// file -> [layer, size profile hard max]; the layering and sizes the app-js profile patch declares.
const MODULES = {
  'native-import-contract.js': ['domain', 400], 'native-import-seam.js': ['domain', 400], 'native-import-core.js': ['application', 400],
  'native-import-svg.js': ['application', 400], 'native-import-figma.js': ['adapters', 500], 'native-import-drop.js': ['adapters', 500],
  'native-import-psd.js': ['adapters', 500], 'native-import-montage.js': ['adapters', 500], 'native-import-desktop.js': ['adapters', 500],
  'native-import-png-stage.js': ['adapters', 500], 'native-import-figma-plan.js': ['application', 400],
  'native-import-png-provider.js': ['application', 400], 'font-assets.js': ['adapters', 500],
  'native-import-diagnostic.js': ['adapters', 500],
};
const ALLOWED = { domain: ['domain'], application: ['domain'], adapters: ['application', 'domain'] };
const cap = JSON.parse(read('engineering/application/capabilities-v2/native-object-import.json'));
const example = cap.examples[0];

test('the descriptor declares exactly the import batch operation, owner-only and fallback-free', () => {
  assert.equal(cap.id, 'native.object.import');
  assert.deepEqual(cap.operations, [contract.OP]);
  assert.equal(cap.availability.state, 'available');
  assert.deepEqual(cap.authority, { document: 'native-engine::object_snapshot::ObjectSnapshot', adapterOwnsWritableMirror: false, fallback: 'none' });
  assert.equal(cap.resourceBoundary.maxRequestBytes, contract.MAX_REQUEST_BYTES);
  assert.equal(cap.resourceBoundary.maxReadResponseBytes, 4096);
  assert.deepEqual(cap.input, { $ref: '../native-transport-v2.schema.json#/$defs/ObjectImportBatchPayload' });
  assert.deepEqual(cap.commonErrorCodes, ['invalid_request', 'wrong_instance', 'wrong_document', 'stale_revision', 'unavailable', 'not_found', 'cancelled_before_dispatch', 'internal']);
  assert.deepEqual(cap.ports.map((p) => [p.label, p.state]), [['ui', 'pending'], ['mcp', 'pending']], 'installed acceptance is not claimed');
});

test('the descriptor example is a request and receipt the JS contract admits, correlated and inside the bounds', () => {
  const request = example.request, response = example.response;
  assert.ok(contract.bytes(JSON.stringify(request)) <= contract.MAX_REQUEST_BYTES);
  assert.doesNotThrow(() => contract.payload(request.operation, request.payload));
  assert.doesNotThrow(() => contract.result(request.operation, response.result));
  assert.doesNotThrow(() => contract.correlate(request, response));
  assert.equal(response.contentRevision, request.expectedRevision + 1);
  assert.ok(seam.admitted(request.operation) && seam.requiresRevision(request.operation));
  assert.equal(response.result.layers.length, request.payload.layers.length);
  assert.equal(response.result.objects.length, request.payload.records.length, 'created plus skipped equals declared');
  assert.deepEqual(response.result.skipped, request.payload.skipped);
});

test('every adapter file respects its layer, its size ceiling and the profile layering of its requires', () => {
  for (const [file, [layer, hardMax]] of Object.entries(MODULES)) {
    const text = read(ADAPTERS + file);
    assert.ok(text.split('\n').length <= hardMax, `${file} is ${text.split('\n').length} lines`);
    assert.ok(text.split('\n').length <= 300, `${file} stays under the warn threshold`);
    for (const [, dep] of text.matchAll(/require\('\.\/(native-[a-z-]+\.js)'\)/g)) {
      const owner = MODULES[dep] ? MODULES[dep][0] : /^native-(font-contract)\.js$/.test(dep) ? 'domain' : null;
      assert.ok(owner, `${file} requires an unknown module ${dep}`);
      assert.ok(ALLOWED[layer].includes(owner), `${file} (${layer}) may not require ${dep} (${owner})`);
    }
  }
});

test('no adapter reaches a legacy writer, importer or editor global', () => {
  const forbidden = /\b(saveActiveLayerFrame|saveAllLayerFrames|userLayers|pushUndo|ensureKeyframe|ensureStrokeId|tagOwner|createUserLayer|serP|desP|SMEngineBridge|SMLabs|SMReference|SMAudio|SMVectorText|SMMediaLibrary|SMFigmaImport|SMPsdImport|importImageFiles|importVideoFile|insertChild|updateUI|renderNow)\b/;
  for (const file of Object.keys(MODULES)) {
    const code = read(ADAPTERS + file).replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '').replace(/\s\/\/ .*$/gm, '');
    const hit = forbidden.exec(code);
    assert.equal(hit, null, `${file} mentions ${hit && hit[0]}`);
  }
});

test('loaded as classic scripts in index.html order every file defines its global and the desktop composition installs without a legacy importer', () => {
  const win = {
    paper: { project: {}, Matrix: function () {}, Path: function () {}, CompoundPath: function () {}, Segment: function () {}, Point: function () {} },
    state: { layers: [], activeLayerIdx: 0, currentFrame: 0 }, SM: { t: (k) => k }, document: null, console,
    NemoNativeObjectProject: { desktop: () => null },
  };
  win.window = win; win.globalThis = win;
  vm.createContext(win);
  const order = ['font-assets.js', 'native-import-contract.js', 'native-import-seam.js', 'native-import-core.js',
    'native-import-png-stage.js', 'native-import-svg.js', 'native-import-drop.js', 'native-import-figma-plan.js',
    'native-import-figma.js', 'native-import-psd.js', 'native-import-montage.js', 'native-import-png-provider.js', 'native-import-diagnostic.js'];
  win.NemoNativeFontContract = require('../src/js/adapters/native-font-contract.js');
  const globals = { 'native-import-contract.js': 'NemoNativeImportContract', 'native-import-seam.js': 'NemoNativeImportSeam', 'native-import-core.js': 'NemoNativeImportCore', 'native-import-svg.js': 'NemoNativeImportSvg',
    'native-import-drop.js': 'NemoNativeImportDrop', 'native-import-figma.js': 'NemoNativeImportFigma', 'native-import-psd.js': 'NemoNativeImportPsd', 'native-import-montage.js': 'NemoNativeImportMontage',
    'font-assets.js': 'NemoFontAssets', 'native-import-png-stage.js': 'NemoNativeImportPngStage',
    'native-import-figma-plan.js': 'NemoNativeImportFigmaPlan', 'native-import-png-provider.js': 'NemoNativeImportPngProvider',
    'native-import-diagnostic.js': 'NemoNativeImportDiagnostic' };
  for (const file of order) {
    vm.runInContext(read(ADAPTERS + file), win, { filename: file });
    assert.equal(typeof win[globals[file]], 'object', `${file} defines ${globals[file]}`);
  }
  assert.equal(win.NemoNativeImportSeam.admitted(contract.OP), true);
  assert.equal(win.NemoNativeImportSeam.admitted('command.document.object.asset.font.import'), true, 'the font contract is composed, not replaced');
  vm.runInContext(read(ADAPTERS + 'native-import-desktop.js'), win, { filename: 'native-import-desktop.js' });
  assert.equal(typeof win.NemoNativeImportDesktop.install, 'function');
  assert.equal(win.SMSvgImport.native, true);
  assert.equal(typeof win.NemoNativeImportController.openSvg, 'function');
  assert.equal(win.SMFigmaImport, undefined);
  assert.equal(win.NemoNativeImportController.core.begin().ok, false, 'no presented native session: typed unavailable, nothing else happens');
});
