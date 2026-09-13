'use strict';
// P26 — engine image-store budget: bookkeeping + LRU eviction policy.
//
// Pure half: independent sequences against application/render/image-budget.js
// with a recording `retire` port. Binding half: the REAL engine-bridge.js
// image block sliced from source (registerCachedImage, registerImagePixels,
// the module binding, enforceImageBudget and the stats/budget API) run
// beside the real module with a fake engine that records register/retire
// calls — proving eviction clears the upload gate and the next use
// re-uploads, and that a failed retire leaves both untouched.
//
// Not covered here, and stated in the PR: the WebGPU pixel A/B under a tiny
// budget (CLAUDE.md §5quinquies) needs a WebGPU-capable browser or the
// packaged app; the headless harness has none.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const modulePath = path.join(root, 'src/js/application/render/image-budget.js');
const NemoImageBudget = require(modulePath);
const moduleSource = fs.readFileSync(modulePath, 'utf8');
const bridgeSource = fs.readFileSync(path.join(root, 'src/js/engine-bridge.js'), 'utf8');

const KB = 1024;
function recorder(result) {
  const calls = [];
  return { calls, retire(ids) { calls.push(ids.slice()); if (result instanceof Error) throw result; return result; } };
}

// ---- check 1: protection of the current build, LRU order, accounting -------

test('bytes and counters follow dimensions: w*h*4 per id, re-registration overwrites, total sums', () => {
  const b = NemoImageBudget.create({ budgetBytes: 10 * KB * KB });
  b.noteRegistered('a', 10, 10);
  b.noteRegistered('b', 20, 5);
  assert.equal(b.totalBytes(), 400 + 400);
  b.noteRegistered('a', 3, 3);
  assert.equal(b.totalBytes(), 36 + 400, 're-registering replaces the entry, never double counts');
  assert.deepEqual(b.stats(), { jsBytes: 436, budgetBytes: 10 * KB * KB, evictions: 0, count: 2 });
});

test('an image emitted by the current build is never a candidate; least-recently-used inactive images retire first', () => {
  const b = NemoImageBudget.create({ budgetBytes: 1000 });
  // Build 1 uploads three 400-byte images (1200 > 1000, but all three are in use → nothing dropped).
  b.beginBuild();
  b.noteRegistered('old', 10, 10); b.noteRegistered('mid', 10, 10); b.noteRegistered('cur', 10, 10);
  let r = recorder(true);
  assert.deepEqual(b.endBuild(r.retire), []);
  assert.deepEqual(r.calls, [], 'nothing on screen is ever retired, even over budget');
  // Build 2 only draws `cur`, and touches `mid` before `old` is ever seen again → `old` is the least recently used.
  b.beginBuild(); b.touch('cur');
  r = recorder(true);
  assert.deepEqual(b.endBuild(r.retire), ['old']);
  assert.deepEqual(r.calls, [['old']]);
  assert.deepEqual(b.stats(), { jsBytes: 800, budgetBytes: 1000, evictions: 1, count: 2 });
  assert.equal(b.has('old'), false); assert.equal(b.has('mid'), true);
});

test('eviction drops oldest-first only until the total fits, and touches outside a build still age the LRU', () => {
  const b = NemoImageBudget.create({ budgetBytes: 1000 });
  b.noteRegistered('a', 10, 10); b.noteRegistered('b', 10, 10); b.noteRegistered('c', 10, 10); b.noteRegistered('d', 10, 10);
  b.touch('a');                      // a is now the most recently used; b is the oldest
  b.beginBuild(); b.touch('d');      // d is protected
  const r = recorder(true);
  // total 1600 > 1000: drop b (→1200), then c (→800 ≤ 1000); a survives.
  assert.deepEqual(b.endBuild(r.retire), ['b', 'c']);
  assert.equal(b.totalBytes(), 800);
  assert.equal(b.building(), false, 'endBuild closes the build set');
});

test('under budget nothing is retired and the retire port is not called; the build set is still closed', () => {
  const b = NemoImageBudget.create({ budgetBytes: 10000 });
  b.beginBuild(); b.noteRegistered('a', 10, 10);
  const r = recorder(true);
  assert.deepEqual(b.endBuild(r.retire), []);
  assert.deepEqual(r.calls, []);
  assert.equal(b.building(), false);
  // Without a retire port (engine without retire_images) the policy is inert but consistent.
  b.beginBuild(); assert.deepEqual(b.endBuild(null), []); assert.equal(b.building(), false);
});

// ---- check 2: failed retire leaves bookkeeping untouched; success clears ----

test('a retire that returns false or throws leaves bookkeeping untouched; success forgets the ids', () => {
  for (const failing of [recorder(false), recorder(new Error('engine busy'))]) {
    const b = NemoImageBudget.create({ budgetBytes: 100 });
    b.noteRegistered('a', 10, 10); b.noteRegistered('b', 10, 10);
    b.beginBuild(); b.touch('b');
    assert.deepEqual(b.endBuild(failing.retire), []);
    assert.deepEqual(failing.calls, [['a']], 'the retire was attempted with the candidate');
    assert.equal(b.has('a'), true, 'a failed retire forgets nothing');
    assert.equal(b.stats().evictions, 0);
    assert.equal(b.building(), false, 'the build set is closed even after a failure');
    // Next build with a working engine retires it.
    b.beginBuild(); b.touch('b');
    const ok = recorder(true);
    assert.deepEqual(b.endBuild(ok.retire), ['a']);
    assert.equal(b.has('a'), false);
    assert.equal(b.stats().evictions, 1);
  }
});

test('the retire port receives a copy: mutating it cannot change what is forgotten', () => {
  const b = NemoImageBudget.create({ budgetBytes: 100 });
  b.noteRegistered('a', 10, 10); b.noteRegistered('b', 10, 10);
  b.beginBuild(); b.touch('b');
  const dropped = b.endBuild((ids) => { ids.push('b'); return true; });
  assert.deepEqual(dropped, ['a']);
  assert.equal(b.has('b'), true);
});

test('setBudgetBytes floors and clamps, and does not wrap at 2^31', () => {
  const b = NemoImageBudget.create({ budgetBytes: 384 * KB * KB });
  assert.equal(b.stats().budgetBytes, 384 * KB * KB, 'the existing default is kept');
  assert.equal(b.setBudgetBytes(4 * 1024 * 1024 * 1024), 4294967296, 'a 4GB budget stays 4GB (the old `| 0` bug landed on 1 byte)');
  assert.equal(b.setBudgetBytes(0.9), 1);
  assert.equal(b.setBudgetBytes(-5), 1);
  assert.equal(b.setBudgetBytes(1234.7), 1234);
});

// ---- check 3: the real engine-bridge binding -----------------------------------

function slice(source, startMarker, endMarker, label) {
  const start = source.indexOf(startMarker);
  assert.notEqual(start, -1, `${label}: start marker not found — the source moved`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(end, -1, `${label}: end marker not found — the source moved`);
  return source.slice(start, end);
}

// The real image block: module binding + registerCachedImage + registerImagePixels
// (from `var _imageBudget =` to the raw-bytes variant), evaluated with the real
// module and a fake engine. `drawableToPixels` is stubbed to a fixed size.
function bridgeContext(engine) {
  const context = { console, JSON, Math, Object, Array, Map, Set, Number, String, Error, registeredImageIds: {}, engine,
    drawableToPixels(source) { return { pixels: new Uint8Array(source.w * source.h * 4), w: source.w, h: source.h }; } };
  context.window = context;
  vm.createContext(context);
  vm.runInContext(moduleSource, context, { filename: 'image-budget.js' });
  const block = slice(bridgeSource, '  var _imageBudget = NemoImageBudget.create(', '\n  // Raw-bytes variant (EXPERIMENTAL', 'engine-bridge image block');
  vm.runInContext(block, context, { filename: 'engine-bridge-image-block.js' });
  // The public API entries, as export.js-style object literals evaluated in the same scope.
  const stats = slice(bridgeSource, '    imageStoreStats: function () {', '\n    setImageBudgetBytes:', 'imageStoreStats').replace('    imageStoreStats: ', '').replace(/,\s*$/, '');
  const setBudget = slice(bridgeSource, '    setImageBudgetBytes: function (n) {', '\n', 'setImageBudgetBytes').replace('    setImageBudgetBytes: ', '').replace(/,\s*$/, '');
  context.imageStoreStats = vm.runInContext(`(${stats})`, context);
  context.setImageBudgetBytes = vm.runInContext(`(${setBudget})`, context);
  return context;
}

function fakeEngine(opts) {
  const e = { registered: [], retired: [], failRetire: !!(opts && opts.failRetire) };
  e.register_image = (id, pixels, w, h) => { e.registered.push([id, w, h, pixels.length]); };
  e.retire_images = (json) => { if (e.failRetire) throw new Error('recursive use of an object'); e.retired.push(JSON.parse(json)); };
  e.image_store_bytes = () => -1; e.image_store_size = () => -1;
  return e;
}

test('binding: a scene build registers through the gate, eviction clears the gate for retired ids only, and the next use re-uploads', () => {
  const engine = fakeEngine();
  const ctx = bridgeContext(engine);
  ctx.setImageBudgetBytes(1000);
  // Build 1: three 10x10 images (400 bytes each) all drawn → over budget but all protected.
  ctx._imageBudget.beginBuild();
  for (const id of ['old', 'mid', 'cur']) ctx.registerCachedImage(id, { w: 10, h: 10 });
  ctx.enforceImageBudget();
  assert.deepEqual(engine.registered.map((r) => r[0]), ['old', 'mid', 'cur']);
  assert.deepEqual(engine.retired, []);
  assert.deepEqual(Object.keys(ctx.registeredImageIds), ['old', 'mid', 'cur']);
  // Build 2 draws only `cur`: registerCachedImage early-outs on the gate (no re-upload) and touches it.
  ctx._imageBudget.beginBuild();
  ctx.registerCachedImage('cur', { w: 10, h: 10 });
  assert.equal(engine.registered.length, 3, 'gate hit: no second upload');
  ctx.enforceImageBudget();
  assert.deepEqual(engine.retired, [['old']], 'least recently used, not on screen');
  assert.deepEqual(Object.keys(ctx.registeredImageIds).sort(), ['cur', 'mid'], 'the gate is cleared only for the retired id');
  assert.equal(ctx.imageStoreStats().jsBytes, 800);
  assert.equal(ctx.imageStoreStats().evictions, 1);
  // Build 3 draws `old` again: the cleared gate makes it re-upload, and it is protected in this build.
  ctx._imageBudget.beginBuild();
  ctx.registerCachedImage('old', { w: 10, h: 10 });
  assert.equal(engine.registered.length, 4, 'evicted image re-uploads on next use');
  assert.deepEqual(engine.registered[3].slice(0, 3), ['old', 10, 10]);
  ctx.enforceImageBudget();
  assert.deepEqual(engine.retired, [['old'], ['mid']], 'now mid is the least recently used inactive image');
  assert.deepEqual(Object.keys(ctx.registeredImageIds).sort(), ['cur', 'old']);
});

test('binding: a throwing engine.retire_images leaves the gate and the bookkeeping untouched', () => {
  const engine = fakeEngine({ failRetire: true });
  const ctx = bridgeContext(engine);
  ctx.setImageBudgetBytes(500);
  ctx._imageBudget.beginBuild();
  ctx.registerImagePixels('a', { w: 10, h: 10 }); ctx.registerImagePixels('b', { w: 10, h: 10 });
  ctx.enforceImageBudget();
  ctx._imageBudget.beginBuild(); ctx.registerCachedImage('b', { w: 10, h: 10 });
  ctx.enforceImageBudget();
  assert.deepEqual(Object.keys(ctx.registeredImageIds).sort(), ['a', 'b']);
  assert.deepEqual(JSON.parse(JSON.stringify(ctx.imageStoreStats())), { jsBytes: 800, engineBytes: -1, engineCount: -1, budgetBytes: 500, evictions: 0 });
  // An engine without retire_images is inert too.
  delete engine.retire_images;
  ctx._imageBudget.beginBuild(); ctx.registerCachedImage('b', { w: 10, h: 10 }); ctx.enforceImageBudget();
  assert.deepEqual(Object.keys(ctx.registeredImageIds).sort(), ['a', 'b']);
});

test('routing guard: engine-bridge keeps only the gate and the retire call; the policy state lives in the module', () => {
  assert.doesNotMatch(bridgeSource, /_imgBytes|_imgLastUsed|_imgUsedThisBuild|_imgTick|_imgBudgetBytes|_imgEvictions|_imgTotalBytes/);
  assert.equal((bridgeSource.match(/_imageBudget\.beginBuild\(\)/g) || []).length, 1, 'one build opener in buildSceneJson');
  assert.equal((bridgeSource.match(/_imageBudget\.endBuild\(/g) || []).length, 1, 'one build closer, in enforceImageBudget');
  assert.match(bridgeSource, /engine\.retire_images\(JSON\.stringify\(ids\)\)/);
  const code = moduleSource.split('\n').filter((l) => !/^\s*\/\//.test(l)).join('\n');
  assert.doesNotMatch(code, /\bwindow\.|\bengine\.|\bstate\.|registeredImageIds|\bSM[A-Z]/, 'the module reads no globals and never touches the engine or the gate');
});
