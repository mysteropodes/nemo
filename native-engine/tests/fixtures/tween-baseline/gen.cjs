'use strict';
// #2358: execute the independently pinned production baseline, never the native port.
// Run from any directory: node gen.cjs > oracle.json
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const cp = require('node:child_process');
const crypto = require('node:crypto');
const root = path.resolve(__dirname, '../../../..');
const baseline = '46442ea96c09e6bfaadf956eb8586e273a658085';
const read = (file) => cp.execFileSync('git', ['show', `${baseline}:${file}`], { cwd: root, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
const source = read('src/js/tweens.js');
const paper = require(path.join(root, 'tests/helpers/native-import-paper.cjs')).load();
const context = vm.createContext({
  Path: paper.Path, Point: paper.Point, Segment: paper.Segment, Math, console,
  window: {}, state: { layers: [{ layerUid: 'L0' }], activeLayerIdx: 0, motionArcs: {} },
});
context.window = context;
vm.runInContext(read('src/js/domain/tween/assignment.js'), context);
// All declarations and original flags, ending before the document-mutating entry point.
vm.runInContext(source.slice(0, source.indexOf('function generateTweens(')), context);
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const stroke = (points, extra = {}) => ({ segments: points.map(([x, y]) => seg(x, y)),
  closed: false, strokeColor: '#112233', strokeWidth: 4, strokeCap: 'round', strokeJoin: 'round', fillColor: null, opacity: 1, ...extra });
const copy = (x) => JSON.parse(JSON.stringify(x));
const cases = [];
const call = (operation, input, fn) => cases.push({ operation, input: copy(input), expected: copy(fn()) });
const a = stroke([[0, 0], [40, 0]]);
const b = stroke([[20, 30], [60, 30]], { strokeWidth: 8, opacity: 0.5 });
for (const t of [0, 0.25, 0.5, 1]) {
  call('interpolate', { a, b, t }, () => context.interpStroke(copy(a), copy(b), t, (v) => v, 0, 4, 0));
}
const turnA = stroke([[-40, -10], [40, -10], [40, 10], [-40, 10]], { closed: true, fillColor: '#cc5533' });
const turnB = stroke([[10, -40], [10, 40], [-10, 40], [-10, -40]], { closed: true, fillColor: '#3366cc' });
call('interpolate', { a: turnA, b: turnB, t: 0.5 }, () => context.interpStroke(copy(turnA), copy(turnB), 0.5, (v) => v, 0, 4, 0));
for (const [name, s] of [['line', a], ['closed', turnA], ['curved', { ...a, segments: [seg(0, 0), { ...seg(40, 20), handleIn: [-30, 0] }] }]]) {
  call('resample', { name, stroke: s, count: 6 }, () => context.resamplePJS(copy(s), 6));
}
const flipped = { ...b, segments: copy(b.segments).reverse() };
call('align', { a, b: flipped }, () => context.alignResampledPairJS(copy(a), copy(flipped)));
const matchA = [a, stroke([[0, 50], [40, 50]], { strokeColor: '#ff0000' })];
const matchB = [stroke([[20, 80], [60, 80]], { strokeColor: '#ff0000' }), b];
call('match', { a: matchA, b: matchB }, () => context.autoMatchJS(copy(matchA), copy(matchB)));
const partition = [a, { ...b, isBrushTextureCopy: true, brushGroupId: 'g1' }, { ...b, isLinkedFillCompanion: true, linkedFillId: 's1' },
  { ...b, isRevisionGhost: true }, { isRaster: true }, { ...b, tweenOn: true }];
for (const manual of [false, true]) call('partition', { strokes: partition, manual }, () => context.splitTweenables(copy(partition), manual));
// Relational matching, including nonuniform movement, unequal counts and repeated shapes.
for (let variant = 0; variant < 6; variant++) {
  const left = Array.from({ length: 8 }, (_, i) => stroke([[i % 4 * 35, Math.floor(i / 4) * 55],
    [i % 4 * 35 + 10 + i * 2, Math.floor(i / 4) * 55 + 4],
    [i % 4 * 35 + 22 + i * 3, Math.floor(i / 4) * 55]], { strokeColor: i % 3 ? '#112233' : '#bb1122' }));
  const right = left.map((s, i) => ({ ...copy(s), segments: s.segments.map((p) => ({ ...copy(p),
    point: [p.point[0] + 18 + (i < 4 ? variant * 4 : -variant * 2), p.point[1] + 12 + (i % 3) * variant * 3] })) }));
  right.reverse();
  if (variant === 4) right.splice(2, 1);
  if (variant === 5) right.push(stroke([[240, 90], [280, 90]], { strokeColor: '#00ff00' }));
  call('match', { name: `relational-${variant}`, a: left, b: right }, () => context.autoMatchJS(copy(left), copy(right)));
}
const hairpin = stroke([[0, 0], [35, 0], [70, 0], [100, 0], [100, 10], [70, 10], [35, 10], [0, 10]]);
const hairpinB = { ...copy(hairpin), segments: copy(hairpin.segments).reverse() };
call('align', { a: hairpin, b: hairpinB }, () => context.alignResampledPairJS(copy(hairpin), copy(hairpinB)));
const toolsSource = read('src/js/tools.js');
const rngStart = toolsSource.indexOf('function seededRng(');
vm.runInContext(toolsSource.slice(rngStart, toolsSource.indexOf('// Tip-shape geometry', rngStart)), context);
for (const seed of [0, 1, 0xffffffff, 123456789]) {
  call('rng', { seed, count: 16 }, () => { const rng = context.seededRng(seed); return Array.from({ length: 16 }, () => rng()); });
}
// Longer deforming strokes exercise the original intrinsic/MLS/fold/handle strategy probes.
for (let variant = 0; variant < 5; variant++) {
  const a = stroke(Array.from({ length: 18 }, (_, i) => [i * 8, Math.sin(i * 0.35) * 22]));
  const b = stroke(Array.from({ length: 18 }, (_, i) => {
    if (variant === 0) return [i * 8 + 12, Math.sin(i * 0.35) * 22 + 9];
    if (variant === 1) return [-Math.sin(i * 0.35) * 22, i * 8];
    if (variant === 2) return [i * 8, Math.sin(i * 0.25 + 0.2) * 48];
    if (variant === 3) return [Math.sin(i * 0.15) * 92, i * 5];
    return [i * 6, Math.sin(i * 0.6) * 18];
  }));
  for (const t of [0.15, 0.5, 0.85]) call('interpolate', { name: `deform-${variant}`, a, b, t },
    () => context.interpStroke(copy(a), copy(b), t, (v) => v, 0, 4, 0));
}
// Production feature-aware sampling: moving corners, reversal, closed curves, pressure.
vm.runInContext(toolsSource.slice(toolsSource.indexOf('function widthAtFrac('), toolsSource.indexOf('// Paper.js', toolsSource.indexOf('function widthAtFrac('))), context);
const curveA = stroke([[0, 0], [60, 30], [40, 80]], { closed: true });
curveA.segments[0].handleIn = [-20, 0];
curveA.segments[2].handleOut = [-30, 0];
call('resample', { name: 'closing-handles', stroke: curveA, count: 6 }, () => context.resamplePJS(copy(curveA), 6));
for (const n of [6, 24, 90]) {
  const cornerA = stroke([[0, 0], [50, 0], [50, 60], [100, 60]]);
  const cornerB = stroke([[10, 20], [100, 20], [100, 60], [130, 100]]);
  const vbA = { ...copy(cornerA), isVectorBrush: true, centerSegments: cornerA.segments.map((s, i) => ({ ...s, width: i + 3 })), widthProfile: [{t:0,width:2},{t:0.3,width:8},{t:1,width:1}] };
  const vbB = { ...copy(cornerB), isVectorBrush: true, centerSegments: cornerB.segments.map((s, i) => ({ ...s, width: 7 - i })).reverse(), widthProfile: [{t:0,width:3},{t:0.7,width:9},{t:1,width:2}] };
  for (const [name, a, b, vb] of [['corners', cornerA, cornerB, false], ['reversed', cornerA, {...cornerB, segments: copy(cornerB.segments).reverse()}, false], ['closed-curves', curveA, {...curveA, segments: [curveA.segments[1],curveA.segments[2],curveA.segments[0]]},false], ['pressure',vbA,vbB,true]]) {
    call('pair', {name, a,b,n,vb}, () => context.resamplePairFeatureAware(copy(a),copy(b),n,vb));
  }
}
process.stdout.write(JSON.stringify({ baseline, sourceSha256: crypto.createHash('sha256').update(source).digest('hex'), cases }, null, 2) + '\n');
