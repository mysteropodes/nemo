// Oracle for native-engine/src/motion_parent_chain.rs.
// usage: node oracle-gen/parent_chain.mjs <repo> > native-engine/tests/fixtures/motion-parent-chain/oracle.txt
import { motionSource, extract, hx, hl, rng, PIN } from './common.mjs';

const src = motionSource(process.argv[2] || '.');
let trig = false;
const M = Object.create(Math);
M.cos = (x) => { if (x !== 0) trig = true; return Math.cos(x); };
M.sin = (x) => { if (x !== 0) trig = true; return Math.sin(x); };
M.atan2 = (y, x) => { trig = true; return Math.atan2(y, x); };

function sandbox(names, prelude) {
  const body = names.map((n) => extract(src, n)).join('\n');
  return new Function('Math', 'state', 'userLayers', 'hooks',
    `${prelude}\n${body}\nreturn { ${names.join(', ')} };`);
}
const state = { layers: [], canvasW: 1920, canvasH: 1080 };
const userLayers = [];
const hooks = {};
const GEOM = ['rotPt', 'rotVec', 'transformSegments', 'transformImageRect', 'lerpAngleDeg'];
const pure = sandbox([...GEOM, 'computeMotionMat', 'invertVectorThroughParentChain',
  'applyParentChainToSegments', 'applyParentChainToImageRect'], `
  function parentChainMats() { return hooks.chain; }
  function valueAtFrame(h, p) { return h._v[p]; }
  function hasExpr() { return false; }`)(M, state, userLayers, hooks);
const graph = sandbox([...GEOM, 'legacyParentChainMats', 'layerParentUidAt', 'findLayerIndexByUid',
  'composeChainTransform'], `
  function computeMotionMat(h) { return h._mat; }`)(M, state, userLayers, hooks);
const blend = sandbox(['lerpAngleDeg', 'blendWeighted', 'blendedAncestorMat', 'weightedParentsOf',
  'parentWeightKeyFor'], `
  function composeChainTransform(uid) { return hooks.poses[uid] || null; }
  function valueAtFrame(h, p) { return h._v[p] || [0]; }`)(M, state, userLayers, hooks);

const r = rng(0x18f2);
const SPECIAL = [0, -0, 1, -1, 0.5, 100, NaN, Infinity, -Infinity, 1e-300, 1e300, 45, 90, 180, -30, 359.9, 720, 0.1];
const pick = (a) => a[Math.floor(r() * a.length)];
const num = (scale = 500) => (r() < 0.2 ? pick(SPECIAL) : (r() - 0.5) * 2 * scale);
const deg = () => (r() < 0.3 ? pick([0, -0, NaN, 90, 180, -45, 360, 1e6, Infinity]) : (r() - 0.5) * 720);
const scl = () => (r() < 0.25 ? pick([0, -0, 1, -1, NaN, 2, 0.5, Infinity]) : (r() - 0.3) * 3);
const mat = () => ({ dx: num(), dy: num(), rot: deg(), sx: scl(), sy: scl(), op: r(), ax: num(50), ay: num(50) });
const MK = ['dx', 'dy', 'rot', 'sx', 'sy', 'op', 'ax', 'ay'];
const hmat = (m) => (m ? hl(MK.map((k) => m[k])) : '-');
const pose = () => ({ dx: num(), dy: num(), rot: deg(), sx: scl(), sy: scl() });
const PK = ['dx', 'dy', 'rot', 'sx', 'sy'];
const hpose = (p) => (p ? hl(PK.map((k) => p[k])) : '-');
const seg = () => ({
  point: [num(), num()],
  handleIn: r() < 0.7 ? [num(50), num(50)] : null,
  handleOut: r() < 0.7 ? [num(50), num(50)] : null,
});
const hseg = (s) => [hl(s.point), s.handleIn ? hl(s.handleIn) : '-', s.handleOut ? hl(s.handleOut) : '-'].join(' ');
const hchain = (c) => (c.length ? c.map((e) => `${hmat(e.mat)}/${hl([e.pivot.x, e.pivot.y])}`).join(';') : '-');
const chain = (n) => Array.from({ length: n }, () => ({ mat: mat(), pivot: { x: num(), y: num() } }));

const rows = [];
function emit(op, fields) { rows.push(`${op} ${trig ? 't' : 'x'} ${fields.join(' ')}`); trig = false; }

for (let i = 0; i < 40; i++) {
  trig = false;
  const a = [num(), num(), num(), num(), deg()];
  emit('rotPt', [hl(a), hl(pure.rotPt(...a))]);
  const b = [num(), num(), deg()];
  emit('rotVec', [hl(b), hl(pure.rotVec(...b))]);
}
for (let i = 0; i < 80; i++) {
  trig = false;
  const s = seg(); const p = { x: num(), y: num() }; const m = mat();
  const [o] = pure.transformSegments([s], p, m);
  emit('seg', [hl([p.x, p.y]), hmat(m), hseg(s), hseg({ point: o.point, handleIn: o.handleIn || null, handleOut: o.handleOut || null })]);
}
for (let i = 0; i < 40; i++) {
  trig = false;
  const rb = { x: num(), y: num(), width: num(), height: num() };
  if (r() < 0.6) rb.rotation = deg();
  const p = { x: num(), y: num() }; const m = mat();
  const o = pure.transformImageRect(rb, p, m);
  emit('rect', [hl([rb.x, rb.y, rb.width, rb.height]), 'rotation' in rb ? hx(rb.rotation) : '-',
    hl([p.x, p.y]), hmat(m), hl([o.x, o.y, o.width, o.height, o.rotation])]);
}
for (let i = 0; i < 30; i++) {
  trig = false;
  const a = [deg(), deg(), r() < 0.2 ? pick(SPECIAL) : r()];
  emit('lerp', [hl(a), hx(pure.lerpAngleDeg(...a))]);
}
for (let i = 0; i < 40; i++) {
  trig = false;
  const vals = { position: [num(), num()], anchor: [num(), num()], rotation: [deg()], scale: [scl() * 100, scl() * 100], opacity: [num(150)] };
  if (i < 12) { // identity-ish rows exercise the null return and its falsy tests
    vals.position = [pick([0, -0, NaN]), pick([0, -0, NaN])]; vals.anchor = [pick([0, -0, NaN]), pick([0, -0])];
    vals.rotation = [pick([0, -0, NaN])]; vals.scale = [100, i % 5 ? 100 : 100.0000001]; vals.opacity = [i % 4 ? 100 : -0];
  }
  const m = pure.computeMotionMat({ motion: {}, _v: vals }, 0);
  emit('mm', [hl(vals.position), hl(vals.anchor), hx(vals.rotation[0]), hl(vals.scale), hx(vals.opacity[0]), hmat(m)]);
}
for (let i = 0; i < 40; i++) {
  trig = false;
  hooks.chain = chain(Math.floor(r() * 4));
  const v = [num(), num()];
  emit('inv', [hchain(hooks.chain), hl(v), hl(pure.invertVectorThroughParentChain(0, 0, v[0], v[1]))]);
  trig = false;
  const s = seg();
  const [o] = pure.applyParentChainToSegments([s], 0, 0);
  emit('chainSeg', [hchain(hooks.chain), hseg(s), hseg({ point: o.point, handleIn: o.handleIn || null, handleOut: o.handleOut || null })]);
  trig = false;
  const rb = { x: num(), y: num(), width: num(), height: num() };
  const res = pure.applyParentChainToImageRect(rb, 0, 0);
  const q = res.rect;
  emit('chainRect', [hchain(hooks.chain), hl([rb.x, rb.y, rb.width, rb.height]), hl([q.x, q.y, q.width, q.height, q.rotation, res.opacityMul])]);
}
hooks.chain = [];

// ---- graphs: walk (legacyParentChainMats), compose (composeChainTransform)
const uidTok = (u) => (u === undefined ? '-' : u === '' ? '~' : u);
function graphRow(layers) {
  state.layers = layers.map((l) => {
    const ld = { _mat: l.mat };
    if (l.uid !== undefined) ld.layerUid = l.uid;
    if (l.parent !== undefined) ld.parentLayerUid = l.parent;
    if (l.kind === 'n') { ld.isNullLayer = true; if (l.base) ld.nullPos = l.base; }
    if (l.keys) ld.parentKeys = l.keys.map(([frame, uid]) => ({ frame, uid }));
    return ld;
  });
  userLayers.length = 0;
  layers.forEach((l, i) => {
    userLayers[i] = l.kind === 'x' ? null
      : { bounds: { center: l.kind === 'n' ? { x: 0, y: 0 } : { x: l.base[0], y: l.base[1] } } };
  });
  return layers.map((l) => [uidTok(l.uid), uidTok(l.parent), l.kind, l.base ? hl(l.base) : '-', hmat(l.mat),
    l.keys ? l.keys.map(([f, u]) => `${hx(f)}:${uidTok(u)}`).join('+') : '-'].join('/')).join(';');
}
function entriesTok(mats) {
  if (!mats.length) return '-';
  return mats.map((e) => `${state.layers.findIndex((l) => l._mat === e.mat)}/${hl([e.pivot.x, e.pivot.y])}`).join(';');
}
function randLayers(n, cyc) {
  const ls = [];
  for (let i = 0; i < n; i++) {
    const kind = r() < 0.2 ? 'n' : r() < 0.1 ? 'x' : 'c';
    const l = { uid: r() < 0.05 ? undefined : `L${i}`, kind, mat: r() < 0.8 ? mat() : null };
    l.base = kind === 'n' ? (r() < 0.5 ? [num(), num()] : null) : [num(), num()];
    const p = i + 1 < n ? `L${i + 1}` : cyc ? 'L0' : undefined;
    l.parent = r() < 0.05 ? pick(['', 'nope', undefined]) : p;
    if (r() < 0.15) l.keys = [[pick([0, 5, 10]), p], [pick([5, 10, 20]), r() < 0.5 ? undefined : `L${Math.floor(r() * n)}`]];
    ls.push(l);
  }
  return ls;
}
function walkRows(layers, start, frames) {
  const g = graphRow(layers);
  const walks = frames.map((f) => `${hx(f)}=${entriesTok(graph.legacyParentChainMats(start, f))}`);
  trig = false;
  const comp = [0, start].map((s) => hpose(graph.composeChainTransform(layers[s].uid, 0)));
  emit('graph', [g, String(start), walks.join('|'), comp.join('|')]);
}
// Deep linear chains are rebuilt by the Rust test from (n, cyclic, start):
// layer i is uid D<i>, content centre (i, -i), mat dx=1 identity otherwise,
// parent D<i+1> (the last one parents D0 when cyclic).
function deepRow(n, cyc, start) {
  const layers = Array.from({ length: n }, (_, i) => ({
    uid: `D${i}`, kind: 'c', base: [i, -i], mat: { dx: 1, dy: 0, rot: 0, sx: 1, sy: 1, op: 1, ax: 0, ay: 0 },
    parent: i + 1 < n ? `D${i + 1}` : cyc ? 'D0' : undefined,
  }));
  graphRow(layers);
  const mats = graph.legacyParentChainMats(start, 0);
  trig = false;
  const comp = hpose(graph.composeChainTransform(layers[start].uid, 0));
  emit('deep', [String(n), cyc ? '1' : '0', String(start), entriesTok(mats), comp]);
}
deepRow(66, false, 0); // 65 ancestors: the 64 limit bites
deepRow(65, false, 0); // exactly 64 ancestors
deepRow(64, false, 0);
deepRow(66, true, 0);
deepRow(5, true, 0); // cycle back to the start
deepRow(5, true, 2);
deepRow(1, true, 0); // self parent
walkRows([{ uid: 'A', kind: 'c', base: [1, 2], mat: mat(), parent: 'A2' },
  { uid: 'A2', kind: 'c', base: [3, 4], mat: mat(), parent: 'A' },
  { uid: 'A', kind: 'n', base: [5, 6], mat: mat() }], 0, [0]); // duplicate uid: first wins
for (let i = 0; i < 30; i++) walkRows(randLayers(2 + Math.floor(r() * 6), r() < 0.3), Math.floor(r() * 2), [0, 7, 12, NaN]);

// ---- blends (blendedAncestorMat with composeChainTransform stubbed per uid)
for (let i = 0; i < 40; i++) {
  trig = false;
  const A = r() < 0.8 ? pose() : null; const B = r() < 0.8 ? pose() : null;
  const pb = r() < 0.3 ? pick([0, -0, NaN, 100, 50, -50, 150, Infinity]) : r() * 100;
  hooks.poses = { A, B };
  state.layers = [{ parentLayerUid: 'A', parentLayerUidB: 'B', _v: { parentBlend: [pb] } }];
  emit('blend2', [hpose(A), hpose(B), hx(pb), hmat(blend.blendedAncestorMat(0, 0))]);
}
for (let i = 0; i < 40; i++) {
  trig = false;
  const n = 1 + Math.floor(r() * 3);
  const pb = r() < 0.3 ? pick([0, NaN, 100, -0]) : r() * 100;
  const ws = Array.from({ length: n }, () => (r() < 0.4 ? pick([0, -0, NaN, 100, -50, 1e-12]) : r() * 100));
  if (i % 7 === 0) { ws.fill(0); }
  const uids = ['A', 'B', ...ws.map((_, k) => `E${k}`)];
  hooks.poses = {};
  uids.forEach((u) => { hooks.poses[u] = r() < 0.75 ? pose() : null; });
  const v = { parentBlend: [i % 7 === 0 ? 0 : pb] };
  ws.forEach((w, k) => { v[`parentWeight${k}`] = [w]; });
  state.layers = [{ parentLayerUid: 'A', parentLayerUidB: 'B', parentsMore: ws.map((_, k) => ({ uid: `E${k}` })), _v: v }];
  const parts = uids.map((u) => hpose(hooks.poses[u])).join(';');
  emit('blendW', [hx(v.parentBlend[0]), hl(ws), parts, hmat(blend.blendedAncestorMat(0, 0))]);
}
process.stdout.write(`${rows.join('\n')}\n`);
