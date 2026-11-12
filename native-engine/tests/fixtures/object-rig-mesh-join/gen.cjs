'use strict';
// L03-P oracle generator: rig pose of a mesh-bound image -> offsets -> scene payload (#2252, #2230 mesh part).
// Usage: node gen.cjs > oracle.txt   (run from anywhere; reads the baseline JS of this checkout).
//
// Cut VERBATIM from the shipped sources and executed in a Node vm: applyRigDeform + _boneSegsToPath (app.js),
// worldOf / normalizedOf / worldVerts / get / store / scenePayloadFor and the R2/R5 rounders (image-mesh.js),
// rasterImageRect (engine-bridge.js). Only the live Raster and the layer are stubs (a Raster needs a canvas):
// the stub carries exactly the members rasterImageRect and applyRigDeform read.
// One tab-separated row per case: `name  input-json  offsets-json  payload-json`.
//  - input:   {rig, mesh, rect (the raster's own rect), finalRect (the rect scenePayloadFor draws into)}
//  - offsets: mesh.offsets after applyRigDeform (the whole vector that feeds scenePayloadFor, full precision)
//  - payload: scenePayloadFor(meshId, finalRect, null) -> {verts, uvs, tris, outline} or null
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ROOT = path.join(__dirname, '..', '..', '..', '..');
const read = (rel) => fs.readFileSync(path.join(ROOT, rel), 'utf8');
function cut(text, name) {
  const at = text.search(new RegExp(`(^|\\n)\\s*function ${name}\\(`));
  if (at < 0) throw new Error(`function ${name} not found`);
  const start = text.indexOf('function ' + name + '(', at);
  let depth = 0;
  for (let i = text.indexOf('{', start); i < text.length; i++) {
    if (text[i] === '{') depth++;
    else if (text[i] === '}' && --depth === 0) return text.slice(start, i + 1);
  }
  throw new Error(`function ${name} is not closed`);
}
const line = (text, re) => { const m = text.match(re); if (!m) throw new Error(`line ${re} not found`); return m[0]; };
const app = read('src/js/app.js');
const mesh = read('src/js/image-mesh.js');
const bridge = read('src/js/engine-bridge.js');
const source = [
  cut(app, '_boneSegsToPath'), cut(app, '_rigNormalizeAngle'), cut(app, '_rigChordAngle'), cut(app, 'applyRigDeform'),
  line(mesh, /var R2 = function[^\n]*/), line(mesh, /var R5 = function[^\n]*/),
  cut(mesh, 'store'), cut(mesh, 'get'), cut(mesh, 'worldOf'), cut(mesh, 'normalizedOf'), cut(mesh, 'scenePayloadFor'),
  cut(bridge, 'rasterImageRect'),
].join('\n');

class Raster {
  constructor(rect) {
    const { x, y, w, h, rot } = rect;
    this.bounds = { x, y, width: w, height: h };
    this.matrix = { rotation: rot || 0 };
    this.scaling = { x: 1, y: 1 };
    this.width = w; this.height = h;
    this.position = { x: x + w / 2, y: y + h / 2 };
  }
}
function run(c) {
  const state = { imageMeshes: { im_1: JSON.parse(JSON.stringify(c.mesh)) } };
  const sb = { Math, Object, JSON, console: { warn() {} }, state };
  sb.window = {};
  const ctx = vm.createContext(sb);
  vm.runInContext(source + '\nthis.fns={applyRigDeform,scenePayloadFor,normalizedOf,get,rasterImageRect};', ctx);
  sb.window.SMImageMesh = { get: sb.fns.get, normalizedOf: sb.fns.normalizedOf };
  sb.window.SMEngineBridge = { rasterImageRect: sb.fns.rasterImageRect };
  sb.SMImageMesh = sb.window.SMImageMesh;
  sb.SMEngineBridge = sb.window.SMEngineBridge;
  const ld = { rig: JSON.parse(JSON.stringify(c.rig)) };
  const raster = new Raster(c.rect);
  ld.rig.binds.forEach((b) => { b._liveRaster = raster; });
  sb.fns.applyRigDeform(ld);
  const f = c.finalRect;
  const payload = sb.fns.scenePayloadFor('im_1', { x: f.x, y: f.y, width: f.w, height: f.h, rotation: f.rot || 0 }, null);
  return { offsets: state.imageMeshes.im_1.offsets, payload };
}
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const bone = (rest, cur) => ({ segments: cur.map((p) => seg(p[0], p[1])), restSegments: rest.map((p) => seg(p[0], p[1])), closed: false });
const rigOf = (bones, binds) => ({ bones, ikChains: {}, nextId: Object.keys(bones).length + 1, binds });
// A 3x3 grid mesh, outline = the 8 border vertices, centre vertex last (the store's own prefix rule).
const verts = [[0, 0], [0.5, 0], [1, 0], [1, 0.5], [1, 1], [0.5, 1], [0, 1], [0, 0.5], [0.5, 0.5]];
const grid = (offsets) => ({
  outline: verts.slice(0, 8), verts, offsets: offsets || verts.map(() => [0, 0]),
  tris: [8, 0, 1, 8, 1, 2, 8, 2, 3, 8, 3, 4, 8, 4, 5, 8, 5, 6, 8, 6, 7, 8, 7, 0],
});
const rect = { x: 100, y: 50, w: 200, h: 100 };
const world = (r) => verts.map((v) => [r.x + v[0] * r.w, r.y + v[1] * r.h]);
// Every vertex anchored on the control point of one bone, weight 1 (the centre column follows bone1's far end).
const bind = (rest, boneId, rotate) => ({ meshId: 'im_1', rest, weights: rest.map((_, i) => [{ boneId, ptIndex: i === 4 || i === 8 ? 1 : 0, w: 1 }]), rotate: !!rotate });
const restWorld = world(rect);
const one = rigOf({ b1: bone([[100, 100], [200, 100]], [[100, 100], [200, 100]]) }, [bind(restWorld, 'b1', false)]);
const moved = (dx, dy) => rigOf({ b1: bone([[100, 100], [200, 100]], [[100 + dx, 100 + dy], [200 + dx, 100 + dy]]) }, [bind(restWorld, 'b1', false)]);
const rotated = rigOf({ b1: bone([[100, 100], [200, 100]], [[100, 100], [200, 160]]) }, [bind(restWorld, 'b1', true)]);
const cases = [
  { name: 'rest_pose_writes_zero_offsets', rig: one, mesh: grid(), rect, finalRect: rect },
  { name: 'translate_all_bound_vertices', rig: moved(30, 20), mesh: grid(), rect, finalRect: rect },
  { name: 'stored_sculpt_is_replaced_by_the_rig', rig: moved(30, 20), mesh: grid(verts.map((_, i) => [0.01 * i, -0.02])), rect, finalRect: rect },
  { name: 'rotate_mode_turns_the_vertices', rig: rotated, mesh: grid(), rect, finalRect: rect },
  { name: 'offsets_ride_a_scaled_final_rect', rig: moved(30, 20), mesh: grid(), rect, finalRect: { x: 40, y: 10, w: 400, h: 300 } },
  { name: 'rotated_raster_rect', rig: moved(30, 20), mesh: grid(), rect: { x: 100, y: 50, w: 200, h: 100, rot: 30 }, finalRect: { x: 100, y: 50, w: 200, h: 100, rot: 30 } },
  { name: 'rest_longer_than_verts_is_clamped', rig: rigOf(one.bones, [bind(restWorld.concat([[1, 1], [2, 2]]), 'b1', false)]), mesh: grid(), rect, finalRect: rect },
];
for (const c of cases) {
  const out = run(c);
  process.stdout.write([c.name, JSON.stringify({ rig: c.rig, mesh: c.mesh, rect: c.rect, finalRect: c.finalRect }), JSON.stringify(out.offsets), JSON.stringify(out.payload)].join('\t') + '\n');
}
