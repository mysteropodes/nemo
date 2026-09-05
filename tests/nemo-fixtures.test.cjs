'use strict';
// The R03 fixture corpus: the committed files are what the generator writes,
// every entry is well-formed, and the shipped evaluators agree with the
// independent expectations wherever they can run under Node.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const ROOT = path.resolve(__dirname, '..');
const DIR = path.join(ROOT, 'tests', 'fixtures');
const gen = require('./fixtures/generate.cjs');
const lib = require('./fixtures/lib.cjs');
const motionEval = require('./bench/motion-eval.cjs');

const manifest = JSON.parse(fs.readFileSync(path.join(DIR, 'manifest.json'), 'utf8'));
const byId = Object.fromEntries(manifest.fixtures.map((f) => [f.id, f]));
const load = (id) => JSON.parse(fs.readFileSync(path.join(DIR, byId[id].file), 'utf8'));
const close = (a, b, tol, msg) => { assert.equal(a.length, b.length, msg); for (let i = 0; i < a.length; i++) assert.ok(Math.abs(a[i] - b[i]) <= tol, `${msg}: [${a}] vs [${b}]`); };

test('manifest: schema, unique ids, committed files match their recorded hash and size', () => {
  assert.equal(manifest.schema, 'nemo.fixtures/1');
  assert.ok(manifest.fixtures.length >= 13);
  const ids = new Set();
  for (const f of manifest.fixtures) {
    for (const k of ['id', 'kind', 'file', 'areas', 'features', 'generation', 'sha256', 'bytes', 'capabilities', 'backend', 'tolerance', 'expectations', 'consumers']) assert.ok(k in f, f.id + ' lacks ' + k);
    assert.ok(!ids.has(f.id), 'duplicate ' + f.id); ids.add(f.id);
    const text = fs.readFileSync(path.join(DIR, f.file), 'utf8');
    assert.equal(lib.sha256(text), f.sha256, f.id + ' sha256');
    assert.equal(Buffer.byteLength(text), f.bytes, f.id + ' bytes');
    assert.equal(f.generation.script, 'tests/fixtures/generate.cjs');
  }
  for (const g of manifest.generated) { assert.equal(g.committed, false); assert.match(g.sha256, /^[0-9a-f]{64}$/); }
});

test('fixture areas are surface-inventory areas, and every acceptance topic is covered', () => {
  const inv = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering', 'inventory', 'surfaces.json'), 'utf8'));
  const areas = new Set(Object.keys(inv.counts.byArea));
  for (const f of manifest.fixtures) for (const a of f.areas) assert.ok(areas.has(a), f.id + ' area ' + a + ' is not an inventory area');
  const features = new Set(manifest.fixtures.flatMap((f) => f.features));
  for (const topic of ['single-keyframe', 'keyframes', 'expressions', 'held-frames', 'component-instance', 'track-matte', 'vector-text', 'image-mesh', 'raster', 'legacy-schema', 'pointer-script', 'export']) assert.ok(features.has(topic), 'no fixture tagged ' + topic);
});

test('generate.cjs --check: the committed corpus is exactly what the generator writes', () => {
  const r = spawnSync(process.execPath, [path.join(DIR, 'generate.cjs'), '--check'], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /^fixtures up to date/);
});

test('project fixtures satisfy the structural rules importJSON enforces', () => {
  for (const f of manifest.fixtures.filter((x) => x.kind === 'project' && x.id !== 'migration-legacy-frames-only')) {
    const d = load(f.id);
    assert.ok(d.version <= 13, f.id);
    assert.ok(Array.isArray(d.layers) && d.layers.length, f.id + ' layers');
    const uids = new Set();
    for (const ld of d.layers) {
      assert.ok(Array.isArray(ld.frames), f.id + ' frames');
      assert.equal(ld.frames.length, d.totalFrames, f.id + ' frame count');
      for (const fr of ld.frames) assert.ok(Array.isArray(fr.strokes) && typeof fr.isKeyframe === 'boolean', f.id + ' frame record');
      assert.ok(!uids.has(ld.layerUid), f.id + ' duplicate layerUid'); uids.add(ld.layerUid);
    }
    assert.ok(d.waOut < d.totalFrames && d.waIn === 0, f.id + ' work area');
    for (const sym of Object.values(d.symbols || {})) for (const sl of sym.layers) assert.equal(sl.frames.length, sym.totalFrames, f.id + ' symbol frames');
  }
});

test('keyed fixtures: the real rawValueAtFrame matches the independent expectations', () => {
  const api = motionEval.load();
  for (const id of ['keyed-position-default-ease', 'keyed-position-linear']) {
    const f = byId[id], ld = load(id).layers[0];
    for (const [frame, want] of Object.entries(f.expectations.at)) close(api.rawValueAtFrame(ld, f.expectations.prop, Number(frame)), want, f.tolerance.value, id + ' @' + frame);
    for (const [prop, want] of Object.entries(f.expectations.defaults || {})) close(api.rawValueAtFrame(ld, prop, 3), want, 0, id + ' default ' + prop);
  }
  const h = byId['keyed-hold-and-static'], hl = load('keyed-hold-and-static').layers[0];
  for (const [frame, want] of Object.entries(h.expectations.position.at)) close(api.rawValueAtFrame(hl, 'position', Number(frame)), want, h.tolerance.value, 'hold @' + frame);
  for (const [frame, want] of Object.entries(h.expectations.opacity.at)) close(api.rawValueAtFrame(hl, 'opacity', Number(frame)), want, 0, 'static opacity @' + frame);
  for (const [prop, want] of Object.entries(h.expectations.defaults)) close(api.rawValueAtFrame(hl, prop, 7), want, 0, 'default ' + prop);
});

test('component fixture: the real resolveSymbolFrameIdx matches the documented placement rule', () => {
  const api = motionEval.load();
  const f = byId['component-instances'], d = load('component-instances');
  const sym = d.symbols.sym_blink;
  for (const ld of d.layers) {
    const want = f.expectations.instances[ld.layerUid].componentFrameByMainFrame;
    const got = []; for (let fr = 0; fr < d.totalFrames; fr++) got.push(api.resolveSymbolFrameIdx(sym, ld, fr));
    assert.deepEqual(got, want, ld.name);
  }
});

test('held-frames fixture: the inheritance table follows from the frame records', () => {
  const f = byId['held-frames-on-sixes'], d = load('held-frames-on-sixes');
  const frames = d.layers[0].frames;
  const keys = frames.map((x, i) => (x.isKeyframe ? i : null)).filter((x) => x !== null);
  assert.deepEqual(keys, f.expectations.keyframes);
  for (let i = 0; i < frames.length; i++) {
    const k = f.expectations.effectiveKeyframeByFrame[i];
    assert.equal(gen.effectiveKeyFor(frames, i), k, 'frame ' + i);
    const eff = frames[k].strokes;
    assert.equal(eff.length, f.expectations.strokesPerEffectiveFrame);
    assert.equal(lib.bounds(eff[0])[0], f.expectations.rectXByKeyframe[k], 'rect x at key ' + k);
  }
});

test('export fixture: per-frame content matches the recorded probes', () => {
  const f = byId['export-12f-320x240'], d = load('export-12f-320x240');
  assert.equal(d.totalFrames, f.expectations.frames); assert.equal(d.canvasW, f.expectations.width); assert.equal(d.canvasH, f.expectations.height); assert.equal(d.fps, f.expectations.fps);
  for (const p of f.expectations.perFrame) {
    const b = lib.bounds(d.layers[0].frames[p.frame].strokes[0]);
    assert.deepEqual(b, p.squareBounds);
    const [x, y] = p.probeInside.at; assert.ok(x >= b[0] && x <= b[0] + b[2] && y >= b[1] && y <= b[1] + b[3], 'inside probe within the square, frame ' + p.frame);
    const [ox, oy] = p.probeOutside.at; assert.ok(ox > b[0] + b[2] || oy < b[1], 'outside probe clear of the square, frame ' + p.frame);
  }
});

test('media fixtures: identical raster source on every frame; mesh normalized with one moved vertex', () => {
  const m = load('media-raster'), fm = byId['media-raster'];
  const srcs = new Set(m.layers[0].frames.map((fr) => fr.strokes[0].src));
  assert.equal(srcs.size, 1); assert.equal(lib.sha256([...srcs][0]), fm.expectations.sourceSha256);
  const r = m.layers[0].frames[0].strokes[0];
  assert.deepEqual([r.x - r.width / 2, r.y - r.height / 2, r.width, r.height], fm.expectations.bounds);
  assert.ok(r.src.startsWith('data:image/png;base64,'));

  const im = load('image-mesh'), fi = byId['image-mesh'];
  const mesh = im.imageMeshes[fi.expectations.meshId];
  assert.equal(im.layers[0].frames.filter((fr) => fr.strokes[0] && fr.strokes[0].meshId === fi.expectations.meshId).length, fi.expectations.framesTagged);
  assert.equal(mesh.outline.length, fi.expectations.outlineCount); assert.equal(mesh.verts.length, fi.expectations.vertexCount); assert.equal(mesh.offsets.length, mesh.verts.length);
  assert.equal(mesh.tris.length % 3, 0); assert.equal(mesh.tris.length / 3, fi.expectations.triangleCount);
  for (const v of mesh.verts) assert.ok(v[0] >= 0 && v[0] <= 1 && v[1] >= 0 && v[1] <= 1);
  const moved = mesh.offsets.map((o, i) => (o[0] || o[1] ? i : -1)).filter((i) => i >= 0);
  assert.deepEqual(moved, [fi.expectations.movedVertex.index]);
  const raster = im.layers[0].frames[0].strokes[0];
  assert.deepEqual([mesh.offsets[moved[0]][0] * raster.width, mesh.offsets[moved[0]][1] * raster.height], fi.expectations.movedVertex.worldDisplacementPx);
});

test('text, matte, expression, migration and interaction fixtures carry the recorded structure', () => {
  const t = load('text-root'), ft = byId['text-root'];
  const strokes = t.layers[0].frames[0].strokes;
  assert.ok(strokes[0].isTextRoot && strokes[0].text === ft.expectations.text && t.layers[0].isTextLayer);
  assert.deepEqual(strokes.slice(1).map((s) => s.vectorChar), ft.expectations.chars);
  assert.deepEqual(strokes.slice(1).map((s) => s.charIndex), [0, 1, 2, 3]);

  const mt = load('matte-alpha'), fmt = byId['matte-alpha'];
  const content = mt.layers.find((l) => l.matteMode);
  assert.equal(content.matteMode, 'alpha');
  assert.ok(mt.layers.some((l) => l.layerUid === content.matteSourceLayerUid), 'matte source resolves');
  assert.equal(content.matteSourceLayerUid, fmt.expectations.sourceLayerUid);

  const ex = load('expression-rotation');
  assert.deepEqual(ex.layers[0].expressions.rotation, { code: 'frame * 15', enabled: true, lastError: null });
  assert.deepEqual(byId['expression-rotation'].expectations.at['12'], [180]);

  const mg = load('migration-legacy-frames-only');
  assert.ok(!('layers' in mg) && !('version' in mg) && Array.isArray(mg.frames));
  assert.equal(byId['migration-legacy-frames-only'].expectations.afterImport.totalFrames, mg.frames.length);

  const it = JSON.parse(fs.readFileSync(path.join(DIR, byId['pen-stroke'].file), 'utf8'));
  assert.equal(it.schema, 'nemo.interaction/1');
  assert.equal(it.pointer[0].type, 'down'); assert.equal(it.pointer[it.pointer.length - 1].type, 'up');
  for (let i = 1; i < it.pointer.length; i++) assert.ok(it.pointer[i].t >= it.pointer[i - 1].t, 'monotonic time');
  for (const p of it.pointer) assert.ok(p.x >= 0 && p.x <= it.canvas.w && p.y >= 0 && p.y <= it.canvas.h);
});
