'use strict';
// L03-O oracle generator for the MESH half of the rig edit transaction (#2251).
// Usage: node gen.cjs > oracle.txt   (run from anywhere; reads the baseline JS of this checkout).
//
// The baseline functions are CUT VERBATIM from the shipped sources and executed in a Node vm with the
// shipped Paper.js: rigBindMesh / rigAutoAssignLayer / rigWeighOnePointByControl / ensureLayerRig /
// _boneSegsToPath / isSelectablePathChild from src/js/app.js, worldOf / worldVerts from src/js/image-mesh.js
// and rasterImageRect from src/js/engine-bridge.js. Only the live Raster and the layer are stubs (a Raster
// needs a canvas): the stub carries exactly the members rasterImageRect and rigBindMesh read.
// One tab-separated row per case: `mesh  name  scenario  action  ret  before  after`.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ROOT = path.join(__dirname, '..', '..', '..', '..');
const paper = require(path.join(ROOT, 'tests', 'helpers', 'native-import-paper.cjs')).load();

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
const app = read('src/js/app.js');
const mesh = read('src/js/image-mesh.js');
const bridge = read('src/js/engine-bridge.js');
const source = [
  ...['ensureLayerRig', '_boneSegsToPath', 'isSelectablePathChild', 'rigWeighOnePoint', 'rigWeighOnePointByControl',
    'rigBindStroke', 'rigBindMesh', 'rigAutoAssignLayer'].map((n) => cut(app, n)),
  cut(mesh, 'worldOf'), cut(mesh, 'worldVerts'), cut(bridge, 'rasterImageRect'),
].join('\n');

class Raster {
  constructor(spec) {
    const { x, y, w, h, rot } = spec.rect;
    this.data = { meshId: spec.meshId };
    this.bounds = { x, y, width: w, height: h };
    this.matrix = { rotation: rot || 0 };
    this.scaling = { x: 1, y: 1 };
    this.width = w; this.height = h;
    this.position = { x: x + w / 2, y: y + h / 2 };
  }
}
function layerOf(children) {
  return { children };
}
function exportRig(rig) {
  return JSON.stringify({
    bones: rig.bones, ikChains: rig.ikChains, nextId: rig.nextId,
    binds: rig.binds.map((b) => ({ strokeId: b.strokeId, meshId: b.meshId, rest: b.rest, restHandles: b.restHandles, weights: b.weights, rotate: b.rotate })),
  });
}
function run(c) {
  const meshes = JSON.parse(JSON.stringify(c.meshes));
  const sb = {
    Path: paper.Path, Point: paper.Point, Segment: paper.Segment, CompoundPath: paper.CompoundPath, Raster, Math,
    console: { warn() {} }, Object, JSON,
    ensureStrokeId: (p) => p.data.strokeId, // the paths of these cases already carry their stable id
  };
  sb.window = { SMImageMesh: { get: (id) => meshes[id] || null }, SMEngineBridge: {} };
  sb.SMImageMesh = sb.window.SMImageMesh;
  sb.window.SMImageMesh.worldVerts = null;
  const ctx = vm.createContext(sb);
  vm.runInContext(source + '\nthis.fns={rigBindMesh,rigAutoAssignLayer,worldVerts,rasterImageRect};', ctx);
  sb.window.SMImageMesh.worldVerts = sb.fns.worldVerts;
  sb.window.SMEngineBridge.rasterImageRect = sb.fns.rasterImageRect;
  sb.SMEngineBridge = sb.window.SMEngineBridge;
  const children = c.children.map((k) => {
    if (k.t === 'raster') return new Raster(k);
    const p = new paper.Path({ insert: false });
    k.pts.forEach((pt, i) => p.add(new paper.Segment(new paper.Point(pt[0], pt[1]), new paper.Point(0, 0), new paper.Point(0, 0))));
    p.closed = !!k.closed;
    p.data = { strokeId: k.id };
    return p;
  });
  // Paper's own Path must pass `instanceof Path` in the vm; Raster is the stub class above.
  const ld = { rig: JSON.parse(c.rig) };
  const layer = layerOf(children);
  const a = c.action;
  let ret;
  if (a.kind === 'bindMesh') {
    const raster = children.find((k) => k instanceof Raster && k.data.meshId === a.meshId);
    const ids = a.boneIds || Object.keys(ld.rig.bones);
    ret = sb.fns.rigBindMesh(ld, raster, ids, a.radius, a.rotate, a.softness);
  } else {
    ret = sb.fns.rigAutoAssignLayer(ld, layer, a.defaultRadius, a.rotate, a.softness, { paths: a.paths, meshes: a.meshes });
  }
  const after = exportRig(ld.rig);
  return { ret: JSON.stringify(ret === undefined ? null : ret), after };
}

const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const bone = (pts) => ({ segments: pts.map((p) => seg(p[0], p[1])), restSegments: pts.map((p) => seg(p[0], p[1])), closed: false });
const rigOf = (bones, binds) => JSON.stringify({ bones, ikChains: {}, nextId: Object.keys(bones).length + 1, binds: binds || [] });
const grid = (n) => {
  // The outline is the prefix of `verts` (the store's own rule): the four corners first, then the rest.
  const outline = [[0, 0], [1, 0], [1, 1], [0, 1]];
  const verts = outline.slice();
  for (let j = 0; j <= n; j++) {
    for (let i = 0; i <= n; i++) {
      if ((i === 0 || i === n) && (j === 0 || j === n)) continue;
      verts.push([i / n, j / n]);
    }
  }
  return { outline, verts, tris: [0, 1, 2], offsets: verts.map(() => [0, 0]) };
};
const two = rigOf({ bone1: bone([[0, 0], [100, 0]]), bone2: bone([[100, 0], [180, 60]]) });
const radiusBone = rigOf({ bone1: Object.assign(bone([[0, 0], [100, 0]]), { radius: 30 }), bone2: bone([[100, 0], [180, 60]]) });
const rect = { x: 20, y: 10, w: 300, h: 200 };
const sculpted = Object.assign(grid(2), { offsets: grid(2).verts.map((_, i) => [0.1 * (i % 3), -0.05 * i]) });
const old = rigOf({ bone1: bone([[0, 0], [100, 0]]) }, [{ meshId: 'im_1', rest: [[0, 0]], weights: [[{ boneId: 'bone1', ptIndex: 0, w: 1 }]], rotate: false }]);
const cases = [
  { name: 'bind_mesh_two_bones', rig: two, meshes: { im_1: grid(2) }, children: [{ t: 'raster', meshId: 'im_1', rect }], action: { kind: 'bindMesh', meshId: 'im_1', radius: 200, rotate: false, softness: 0 } },
  { name: 'bind_mesh_soft_rotate', rig: two, meshes: { im_1: grid(2) }, children: [{ t: 'raster', meshId: 'im_1', rect }], action: { kind: 'bindMesh', meshId: 'im_1', radius: 150, rotate: true, softness: 0.5 } },
  { name: 'bind_mesh_one_bone_of_two', rig: two, meshes: { im_1: grid(2) }, children: [{ t: 'raster', meshId: 'im_1', rect }], action: { kind: 'bindMesh', meshId: 'im_1', boneIds: ['bone2'], radius: 220, rotate: false, softness: 0 } },
  { name: 'bind_mesh_bone_radius_wins', rig: radiusBone, meshes: { im_1: grid(2) }, children: [{ t: 'raster', meshId: 'im_1', rect }], action: { kind: 'bindMesh', meshId: 'im_1', radius: 200, rotate: false, softness: 0 } },
  { name: 'bind_mesh_rotated_raster', rig: two, meshes: { im_1: grid(2) }, children: [{ t: 'raster', meshId: 'im_1', rect: { x: 20, y: 10, w: 300, h: 200, rot: 90 } }], action: { kind: 'bindMesh', meshId: 'im_1', radius: 250, rotate: false, softness: 0 } },
  { name: 'bind_mesh_ignores_the_sculpt', rig: two, meshes: { im_1: sculpted }, children: [{ t: 'raster', meshId: 'im_1', rect }], action: { kind: 'bindMesh', meshId: 'im_1', radius: 200, rotate: false, softness: 0 } },
  { name: 'rebind_mesh_replaces', rig: old, meshes: { im_1: grid(1) }, children: [{ t: 'raster', meshId: 'im_1', rect }], action: { kind: 'bindMesh', meshId: 'im_1', radius: 200, rotate: true, softness: 0 } },
  { name: 'auto_mesh_default_radius_grows', rig: two, meshes: { im_1: grid(2) }, children: [{ t: 'raster', meshId: 'im_1', rect: { x: 0, y: -300, w: 900, h: 600 } }], action: { kind: 'autoAssign', defaultRadius: 200, rotate: false, softness: 0, paths: true, meshes: true } },
  { name: 'auto_mesh_and_path_meshes_first', rig: two, meshes: { im_1: grid(2) }, children: [{ t: 'path', id: 's1', closed: false, pts: [[0, 40], [120, 90]] }, { t: 'raster', meshId: 'im_1', rect }], action: { kind: 'autoAssign', defaultRadius: 100, rotate: true, softness: 0.25, paths: true, meshes: true } },
  { name: 'auto_meshes_off_still_scans_them', rig: two, meshes: { im_1: grid(2) }, children: [{ t: 'path', id: 's1', closed: false, pts: [[0, 40], [120, 90]] }, { t: 'raster', meshId: 'im_1', rect: { x: 0, y: -300, w: 900, h: 600 } }], action: { kind: 'autoAssign', defaultRadius: 100, rotate: false, softness: 0, paths: true, meshes: false } },
  { name: 'auto_paths_off_binds_only_meshes', rig: two, meshes: { im_1: grid(2) }, children: [{ t: 'path', id: 's1', closed: false, pts: [[0, 40], [120, 90]] }, { t: 'raster', meshId: 'im_1', rect }], action: { kind: 'autoAssign', defaultRadius: 150, rotate: false, softness: 0, paths: false, meshes: true } },
  { name: 'auto_explicit_bone_radius_is_not_scanned', rig: radiusBone, meshes: { im_1: grid(2) }, children: [{ t: 'raster', meshId: 'im_1', rect: { x: 0, y: -300, w: 900, h: 600 } }], action: { kind: 'autoAssign', defaultRadius: 200, rotate: false, softness: 0, paths: true, meshes: true } },
  { name: 'auto_dangling_mesh_is_skipped', rig: two, meshes: {}, children: [{ t: 'raster', meshId: 'im_9', rect }], action: { kind: 'autoAssign', defaultRadius: 200, rotate: false, softness: 0, paths: true, meshes: true } },
];
for (const c of cases) {
  const before = c.rig;
  const out = run(c);
  const scenario = JSON.stringify({ children: c.children, meshes: c.meshes });
  process.stdout.write(['mesh', c.name, scenario, JSON.stringify(c.action), out.ret, before.length ? JSON.stringify(JSON.parse(before)) : 'null', out.after].join('\t') + '\n');
}
