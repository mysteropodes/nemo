'use strict';
// G16 (#2361): the baseline vector-brush dab stamper (tools.js @1065e9d8 BRUSH_PRESETS, seededRng, buildDabShape,
// buildBrushDabs and helpers) characterized over the shipped Paper.js for every built-in preset on fixed paths and
// seeds (native-object-vector-fixtures/brush-dab-oracle.json, generated outside the repository by
// gen-brush-dab-oracle.cjs, hashes inside). A native or ported stamper is measured against this oracle; this test
// keeps the in-tree JS stamper identical to it, so a change to either side is a visible, deliberate decision.
const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.join(__dirname, '..');
const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-vector-fixtures', 'brush-dab-oracle.json'), 'utf8'));
const tools = fs.readFileSync(path.join(ROOT, 'src/js/tools.js'), 'utf8');
const sha = (t) => crypto.createHash('sha256').update(t).digest('hex');
function block(start) {
  const at = tools.indexOf(start);
  assert.ok(at >= 0, start);
  let depth = 0, i = tools.indexOf('{', at);
  for (; i < tools.length; i++) { if (tools[i] === '{') depth++; else if (tools[i] === '}' && --depth === 0) break; }
  return tools.slice(at, tools[i + 1] === ';' ? i + 2 : i + 1);
}
const slice = (name) => (name === 'BRUSH_PRESETS' ? block('var BRUSH_PRESETS={') : /^[A-Z_]+$/.test(name)
  ? tools.slice(tools.indexOf(`var ${name}=`), tools.indexOf('\n', tools.indexOf(`var ${name}=`)))
  : block(`\nfunction ${name}(`).trimStart());

function stamper() {
  const sb = { Math, JSON };
  sb.window = sb; sb.self = sb; sb.navigator = { userAgent: 'node' };
  sb.HTMLCanvasElement = function HTMLCanvasElement() {};
  const ctx2d = new Proxy({}, { get: (t, k) => (k in t ? t[k] : function () { return 1; }), set: (t, k, v) => { t[k] = v; return true; } });
  const canvas = () => new Proxy(Object.assign(Object.create(sb.HTMLCanvasElement.prototype), { getContext: () => ctx2d, style: {}, width: 100, height: 100,
    get ownerDocument() { return sb.document; }, getBoundingClientRect: () => ({ left: 0, top: 0, right: 100, bottom: 100, width: 100, height: 100 }) }),
    { get: (o, k) => (k in o ? o[k] : (typeof k === 'string' ? function () { return null; } : undefined)), set: (o, k, v) => { o[k] = v; return true; } });
  sb.document = { createElement: canvas, addEventListener() {}, removeEventListener() {}, documentElement: { style: {}, clientLeft: 0, clientTop: 0 },
    getElementsByTagName: () => [], defaultView: sb, readyState: 'complete', body: {} };
  sb.pageXOffset = 0; sb.pageYOffset = 0; sb.getComputedStyle = () => ({}); sb.requestAnimationFrame = () => 0; sb.addEventListener = () => {}; sb.setTimeout = () => 0;
  const ctx = vm.createContext(sb);
  vm.runInContext(fs.readFileSync(path.join(ROOT, 'src/paper-full.min.js'), 'utf8') + ';paper.setup(document.createElement("canvas"));paper.install(this);', ctx);
  vm.runInContext(Object.keys(oracle.slices).map(slice).join('\n'), ctx);
  return vm.runInContext(`(function(spec, key, width, seed, profile){
    var p=new Path({insert:false});
    spec.segments.forEach(function(s){p.add(new Segment(new Point(s[0][0],s[0][1]),new Point(s[1][0],s[1][1]),new Point(s[2][0],s[2][1])));});
    return JSON.stringify(buildBrushDabs(p, BRUSH_PRESETS[key], width, seededRng(seed), profile||undefined).map(function(d){
      var items=d.className==='CompoundPath'?d.children:[d];
      return {kind:d.className, valueDelta:d.data.dabValueDelta, opacity:d.data.dabOpacity,
        parts:items.map(function(it){return {closed:it.closed, segments:it.segments.map(function(s){return [s.point.x,s.point.y,s.handleIn.x,s.handleIn.y,s.handleOut.x,s.handleOut.y];})};})};
    }));
  })`, ctx);
}
// `|| 0` folds -0 into 0, as the JSON oracle stores it.
const r6 = (v) => Math.round(v * 1e6) / 1e6 || 0, r3 = (v) => Math.round(v * 1e3) / 1e3 || 0;

test('the oracle is the pinned baseline run over every built-in preset', () => {
  assert.match(oracle.commit, /^1065e9d8[0-9a-f]{32}$/);
  assert.match(oracle.generatorSha256, /^[0-9a-f]{64}$/);
  assert.equal(oracle.presets, 28);
  assert.equal(oracle.cases.length, 57);
  assert.equal(oracle.cases.reduce((n, c) => n + c.count, 0), 3656);
  for (const c of oracle.cases) {
    assert.equal(c.summary.length, c.count, c.id);
    assert.ok(c.dabs.length === Math.min(oracle.full, c.count), c.id);
  }
});

test('the in-tree stamper is byte-identical to the characterized baseline slices', () => {
  for (const [name, hash] of Object.entries(oracle.slices)) assert.equal(sha(slice(name)), hash, name);
});

test('representative presets, seeds and a pressure profile reproduce the oracle dab for dab', () => {
  const run = stamper();
  const ids = ['charcoal-pencil/line', 'chalk-scribble/curve', 'charcoal-rough/curve', 'chalk-round/line/profile', oracle.cases[oracle.cases.length - 2].id];
  for (const id of ids) {
    const c = oracle.cases.find((x) => x.id === id);
    const dabs = JSON.parse(run(oracle.paths[c.path], c.preset, c.width, c.seed, c.profile || null));
    assert.equal(dabs.length, c.count, id);
    dabs.slice(0, oracle.full).forEach((d, i) => {
      assert.deepEqual(d.parts.map((p) => ({ closed: p.closed, segments: p.segments.map((s) => s.map(r6)) })), c.dabs[i].parts, `${id} dab ${i}`);
      assert.equal(r6(d.opacity), c.dabs[i].opacity, `${id} dab ${i} opacity`);
    });
    dabs.forEach((d, i) => {
      const pts = d.parts.flatMap((p) => p.segments.map((s) => [r6(s[0]), r6(s[1])]));
      const mean = (v) => v.reduce((a, b) => a + b, 0) / v.length;
      assert.deepEqual([mean(pts.map((q) => q[0])), mean(pts.map((q) => q[1]))].map(r3), c.summary[i].slice(0, 2), `${id} dab ${i} centroid`);
    });
  }
});

test('the mirrored seeded stroke cases are the baseline stamper over the drawn curve and its copy, with the seed the symmetry adapter derives', () => {
  const run = stamper();
  const Symmetry = require('../src/js/adapters/native-object-draw-symmetry.js');
  assert.equal(oracle.mirrorCases.length, 8);
  const flat = (v) => JSON.parse(JSON.stringify(v)); // the JSON oracle stores -0 as 0
  const mirror = flat(oracle.paths.curve.segments.map(([p, i, o]) => [[200 - p[0], p[1]], [-i[0], i[1]], [-o[0], o[1]]]));
  assert.deepEqual(oracle.paths.curveMirror.segments, mirror, 'the copy path is the drawn curve reflected across x = 100');
  const root = { state: { symmetryEnabled: true, symmetryMode: 'y', symmetryExtend: true, symmetryAxis: { x1: 100, y1: 0, x2: 100, y2: 200 } } };
  for (const c of oracle.mirrorCases) {
    if (c.path === 'curveMirror') {
      const drawn = { segments: oracle.paths.curve.segments.map(([p, i, o]) => ({ point: p.slice(), handleIn: i.slice(), handleOut: o.slice() })), closed: false,
        brushTexturePreset: c.preset, brushGroupId: 'bg1_1', strokeSeed: c.drawnSeed };
      const copy = Symmetry.copies(root, drawn, { canvasW: 200, canvasH: 200 }, null)[1];
      assert.equal(copy.strokeSeed, c.seed, `${c.id}: the adapter derives the copy seed`);
      assert.deepEqual(flat(copy.segments.map((s) => [s.point, s.handleIn, s.handleOut])), oracle.paths.curveMirror.segments, `${c.id}: the adapter reflects the curve`);
    }
    const dabs = JSON.parse(run(oracle.paths[c.path], c.preset, c.width, c.seed, null));
    assert.equal(dabs.length, c.count, c.id);
    dabs.slice(0, oracle.full).forEach((d, i) => {
      assert.deepEqual(d.parts.map((p) => ({ closed: p.closed, segments: p.segments.map((s) => s.map(r6)) })), c.dabs[i].parts, `${c.id} dab ${i}`);
      assert.equal(r6(d.opacity), c.dabs[i].opacity, `${c.id} dab ${i} opacity`);
    });
  }
});
