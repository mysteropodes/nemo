'use strict';
// #2361 Stop-at-axis symmetry with a pressure ribbon, live against the baseline: symmetry-bridge.js onStrokeCommitted
// clips the drawn ribbon's OUTLINE at the axis (clipPathAtAxis) and keeps its centreline whole, then mirrors the clipped
// outline (adoptFreshIdentity maps the whole centreline; a linked fill's copy gets an outline rebuilt from it). The
// native copies() must produce the same stack from the unclipped ribbon the native Draw client sends.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Symmetry = require('../src/js/adapters/native-object-draw-symmetry.js');
const ROOT = path.join(__dirname, '..');
const paper = require('./helpers/native-import-paper.cjs').load();
const read = (f) => fs.readFileSync(path.join(ROOT, 'src/js', f), 'utf8');
const tools = read('tools.js'), app = read('app.js'), sym = read('symmetry-bridge.js');
function baselineFunction(text, name) {
  const start = text.indexOf('function ' + name + '('); assert.ok(start >= 0, name);
  let at = text.indexOf('{', start), depth = 1, end = at + 1;
  while (depth) { if (text[end] === '{') depth++; if (text[end] === '}') depth--; end++; }
  return text.slice(start, end);
}
let ids = 0;
const ctx = vm.createContext({ Path: paper.Path, Segment: paper.Segment, Point: paper.Point, CompoundPath: paper.CompoundPath, console, Math,
  state: { smoothing: 0, symmetryEnabled: true, symmetryExtend: false },
  ensureStrokeId: (p) => { if (!p.data) p.data = {}; if (!p.data.strokeId) p.data.strokeId = 's' + (++ids); return p.data.strokeId; },
  tagOwner: () => {} });
for (const n of ['rebuildVectorBrushOutline', 'sampleVectorBrushCenterline', 'buildWidthProfile', 'widthAtFrac', 'buildVariableWidthPath',
  '_polylineCrossCandidates', '_ribbonHasRealSelfIntersections', '_flattenResolvedRibbon', '_cleanupSelfIntersectingOutline',
  'buildClosedRingOutline', '_polyClosestPair', 'simplifyIfNeeded']) vm.runInContext(baselineFunction(tools, n), ctx, { filename: 'tools.js:' + n });
vm.runInContext(baselineFunction(app, 'applyBrushKeyline'), ctx, { filename: 'app.js:applyBrushKeyline' });
for (const n of ['axisDir', 'reflectPoint', 'reflectVector', 'sideOfXY', 'clipPathAtAxis', 'ensureSymmetryAxis', 'ensureSymmetryRadialCenter',
  'sectorCount', 'adoptFreshIdentity', 'onStrokeCommitted']) vm.runInContext(baselineFunction(sym, n), ctx, { filename: 'symmetry-bridge.js:' + n });

const pair = (p) => [p.x, p.y];
const seg = (s) => ({ point: pair(s.point), handleIn: pair(s.handleIn), handleOut: pair(s.handleOut) });
const centre = (cs) => cs.map((c) => ({ point: c.point.slice(), handleIn: (c.handleIn || [0, 0]).slice(), handleOut: (c.handleOut || [0, 0]).slice(), width: c.width }));
// The drawn ribbon (and its linked fill below it) as the baseline lays it out on commit.
function ribbon(cs, profile, layer, linked) {
  const p = new paper.Path({ insert: false });
  p.data = { isVectorBrush: true, centerSegments: JSON.parse(JSON.stringify(cs)), widthProfile: JSON.parse(JSON.stringify(profile)) };
  p.strokeWidth = 4; p.fillColor = '#112233';
  let fill = null;
  if (linked) {
    fill = new paper.Path({ insert: false, closed: true }); fill.fillColor = '#445566'; fill.data = { isLinkedFillCompanion: true };
    p.data.linkedFill = fill; layer.addChild(fill);
  }
  layer.addChild(p); ctx.rebuildVectorBrushOutline(p);
  return { p, fill };
}
// Every item from the drawn ribbon's fill (or the ribbon) upward, bottom to top.
function baselineStack(layer, first) {
  return layer.children.slice(layer.children.indexOf(first)).map((k) => ({ companion: !!(k.data && k.data.isLinkedFillCompanion),
    segments: k.segments.map(seg), closed: k.closed, centerSegments: k.data.centerSegments ? centre(k.data.centerSegments) : null }));
}
function nativeStack(list) {
  return list.map((d) => ({ companion: d.isLinkedFillCompanion === true, segments: d.segments.map((s) => ({ point: s.point.slice(),
    handleIn: s.handleIn.slice(), handleOut: s.handleOut.slice() })), closed: !!d.closed, centerSegments: d.centerSegments ? centre(d.centerSegments) : null }));
}
function near(actual, expected, where) {
  if (typeof expected === 'number') { assert.ok(Math.abs(actual - expected) < 1e-9, `${where}: ${actual} vs ${expected}`); return; }
  if (Array.isArray(expected)) { assert.equal(actual.length, expected.length, where + ' length'); expected.forEach((e, i) => near(actual[i], e, where + '[' + i + ']')); return; }
  if (expected && typeof expected === 'object') { for (const k of Object.keys(expected)) near(actual[k], expected[k], where + '.' + k); return; }
  assert.equal(actual, expected, where);
}
const straight = [[0, 50, 2], [60, 50, 6], [120, 50, 6], [180, 50, 2]].map(([x, y, w]) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0], width: w }));
const profile = [{ t: 0, width: 2 }, { t: 0.3, width: 6 }, { t: 0.7, width: 6 }, { t: 1, width: 2 }];
const curved = [{ point: [20, 40], handleIn: [0, 0], handleOut: [30, -20], width: 3 }, { point: [110, 60], handleIn: [-20, 20], handleOut: [20, -10], width: 7 },
  { point: [200, 30], handleIn: [-15, -5], handleOut: [0, 0], width: 4 }];
const curvedProfile = [{ t: 0, width: 3 }, { t: 0.5, width: 7 }, { t: 1, width: 4 }];
const CASES = [
  { name: 'vertical axis, linked fill', mode: 'y', axis: { x1: 100, y1: 0, x2: 100, y2: 200 }, linked: true, cs: straight, profile },
  { name: 'free axis, no fill', mode: 'free', axis: { x1: 60, y1: 0, x2: 160, y2: 160 }, linked: false, cs: curved, profile: curvedProfile },
  { name: 'horizontal axis, curved, linked fill', mode: 'x', axis: { x1: 0, y1: 48, x2: 320, y2: 48 }, linked: true, cs: curved, profile: curvedProfile },
  { name: 'axis beyond the stroke: nothing clipped', mode: 'y', axis: { x1: 260, y1: 0, x2: 260, y2: 200 }, linked: false, cs: straight, profile },
];

test('stop-at-axis clips a pressure ribbon\'s outline, keeps its centreline whole and mirrors as the baseline does', () => {
  for (const c of CASES) {
    Object.assign(ctx.state, { symmetryMode: c.mode, symmetryAxis: c.axis, symmetryExtend: false });
    // What the native client sends: the unclipped ribbon (and fill), before any symmetry.
    const sent = new paper.Layer({ insert: false }), a = ribbon(c.cs, c.profile, sent, c.linked);
    const dict = { isVectorBrush: true, widthProfile: c.profile, segments: a.p.segments.map(seg), closed: a.p.closed, centerSegments: centre(c.cs) };
    const companion = a.fill ? { isLinkedFillCompanion: true, segments: a.fill.segments.map(seg), closed: true } : null;
    const root = { state: { symmetryEnabled: true, symmetryMode: c.mode, symmetryAxis: c.axis, symmetryExtend: false },
      sampleVectorBrushCenterline: ctx.sampleVectorBrushCenterline, buildVariableWidthPath: ctx.buildVariableWidthPath };
    const native = nativeStack(Symmetry.copies(root, dict, { canvasW: 320, canvasH: 200 }, companion));
    // The baseline: commit the same ribbon, then onStrokeCommitted with Extend off.
    const kept = new paper.Layer({ insert: false }), b = ribbon(c.cs, c.profile, kept, c.linked);
    ctx.onStrokeCommitted(b.p, kept);
    const expected = baselineStack(kept, b.fill || b.p);
    near(native, expected, c.name);
    const drawn = native[c.linked ? 1 : 0];
    near(drawn.centerSegments, centre(c.cs), c.name + ': the drawn centreline is never clipped');
    if (c.name.startsWith('axis beyond')) assert.equal(drawn.segments.length, a.p.segments.length, 'an outline on one side is kept whole');
    else assert.ok(drawn.segments.length < a.p.segments.length, c.name + ': the outline is clipped');
  }
});

test('a ribbon outline that starts across the axis still refuses: no two anchors remain', () => {
  const root = { state: { symmetryEnabled: true, symmetryMode: 'y', symmetryAxis: { x1: 0, y1: 0, x2: 0, y2: 10 }, symmetryExtend: false } };
  const dict = { isVectorBrush: true, closed: true, centerSegments: centre(straight), widthProfile: profile,
    segments: [[1, 0], [-1, 0], [-1, 1]].map((p) => ({ point: p, handleIn: [0, 0], handleOut: [0, 0] })) };
  assert.throws(() => Symmetry.copies(root, dict, { canvasW: 320, canvasH: 200 }, null), /starts at the symmetry axis/);
});
