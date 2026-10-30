'use strict';
// Frozen operational-baseline Motion functions and Paper cubic bounds, never the native renderer.
// Regenerate: node native-engine/tests/fixtures/layer-transform-keys/gen.cjs > native-engine/tests/fixtures/layer-transform-keys/oracle.json
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { execFileSync } = require('node:child_process'), { createHash } = require('node:crypto');
const root = path.resolve(__dirname, '../../../..'), baseline = '1065e9d8';
const motion = execFileSync('git', ['show', baseline + ':src/js/motion.js'], { cwd: root, encoding: 'utf8', maxBuffer: 8e7 });
const paper = require(path.join(root, 'tests/helpers/native-import-paper.cjs')).load();
const ctx = vm.createContext({ Math });
for (const name of ['rotPt', 'rotVec', 'transformSegments']) {
  const at = motion.indexOf('  function ' + name + '('), end = motion.indexOf('\n  function ', at + 1);
  vm.runInContext(motion.slice(at, end), ctx);
}
const seg = (p, hi = [0, 0], ho = [0, 0]) => ({ point: p, handleIn: hi, handleOut: ho });
const rect = (x, y, w, h) => [seg([x, y]), seg([x + w, y]), seg([x + w, y + h]), seg([x, y + h])];
const records = {
  a: { segments: [seg([10, 10], [0, -5], [8, 3]), seg([60, 20], [-6, 5], [4, 9]), seg([30, 50], [-5, 2], [3, -6])], strokeWidth: 3 },
  b: { segments: rect(80, 10, 40, 30), gradient: { from: [80, 10], to: [120, 40] } },
};
// A raster item shares the layer: its display rect joins the pivot union. x/y is the item's CENTRE (a Paper Raster position).
const imageRect = { x: 130, y: 5, width: 40, height: 30 };
const parentRecord = { segments: rect(300, 0, 40, 20) };
const makePath = s => new paper.Path({ insert: false, closed: true, segments: s.map(s => new paper.Segment(s.point, s.handleIn, s.handleOut)) });
function unionCentre(list) {
  let b = null;
  for (const s of list) { const p = makePath(s); b = b ? b.unite(p.bounds) : p.bounds.clone(); p.remove(); }
  return { x: b.center.x, y: b.center.y };
}
const lerp = (a, b, y) => a.map((v, d) => v + (b[d] - v) * y);
// rawValueAtFrame with the passthrough curve (`curvePoints: []`, y = t): clamp outside the keyed range.
function sample(prop, frame) {
  const defaults = { position: [0, 0], anchor: [0, 0], rotation: [0], scale: [100, 100] };
  if (prop.track) {
    const ks = prop.track.keys;
    if (frame <= ks[0].frame) return ks[0].v.slice();
    if (frame >= ks[ks.length - 1].frame) return ks[ks.length - 1].v.slice();
    let i = 0; while (frame >= ks[i + 1].frame) i++;
    return lerp(ks[i].v, ks[i + 1].v, (frame - ks[i].frame) / (ks[i + 1].frame - ks[i].frame));
  }
  return prop.static.slice();
}
function matAt(props, frame) {
  const pos = sample(props.position || { static: [0, 0] }, frame), anc = sample(props.anchor || { static: [0, 0] }, frame);
  const rot = sample(props.rotation || { static: [0] }, frame)[0], scl = sample(props.scale || { static: [100, 100] }, frame);
  if (!pos[0] && !pos[1] && !anc[0] && !anc[1] && !rot && scl[0] === 100 && scl[1] === 100) return null;
  return { dx: pos[0], dy: pos[1], rot, sx: scl[0] / 100, sy: scl[1] / 100, ax: anc[0], ay: anc[1] };
}
const meanScale = m => m ? (Math.abs(m.sx) + Math.abs(m.sy)) / 2 : 1;
const key = (frame, v) => ({ frame, v, curvePoints: [] });
const parentPose = { position: { static: [-4, 7] }, anchor: { static: [3, 2] }, rotation: { static: [61] }, scale: { static: [80, 160] } };
const cases = [
  { name: 'rotation', layer: { rotation: { static: [30] } }, frames: [0] },
  { name: 'scale-nonuniform', layer: { scale: { static: [150, 50] } }, frames: [0] },
  { name: 'scale-uniform', layer: { rotation: { static: [20] }, scale: { static: [150, 150] } }, frames: [0] },
  { name: 'scale-mirrored', layer: { scale: { static: [-80, 120] } }, frames: [0] },
  { name: 'anchor-rotation', layer: { anchor: { static: [12, -8] }, rotation: { static: [30] } }, frames: [0] },
  { name: 'combined', layer: { position: { static: [7, -3] }, anchor: { static: [10, 5] }, rotation: { static: [-40] }, scale: { static: [120, 80] } }, frames: [0] },
  { name: 'keyed', layer: { anchor: { static: [6, -4] }, rotation: { track: { keys: [key(0, [0]), key(8, [60])] } }, scale: { track: { keys: [key(0, [100, 100]), key(8, [200, 50])] } } }, frames: [0, 4, 8, 12] },
  { name: 'mixed-image', layer: { position: { static: [7, -3] }, anchor: { static: [10, 5] }, rotation: { static: [-40] }, scale: { static: [120, 80] } }, raster: true, frames: [0] },
  { name: 'parented', layer: { position: { static: [7, -3] }, anchor: { static: [10, 5] }, rotation: { static: [-40] }, scale: { static: [120, 80] } }, parent: parentPose, frames: [0] },
];
const out = cases.map(c => ({
  name: c.name, layer: c.layer, parent: c.parent || null, raster: c.raster ? imageRect : null,
  frames: c.frames.map(frame => {
    const layerMat = matAt(c.layer, frame), parentMat = c.parent ? matAt(c.parent, frame) : null;
    const own = Object.values(records).map(r => r.segments);
    if (c.raster) own.push(rect(imageRect.x - imageRect.width / 2, imageRect.y - imageRect.height / 2, imageRect.width, imageRect.height));
    const lp = layerMat ? { x: unionCentre(own).x + layerMat.ax, y: unionCentre(own).y + layerMat.ay } : null;
    const pc = parentMat ? unionCentre([parentRecord.segments]) : null;
    const pp = parentMat ? { x: pc.x + parentMat.ax, y: pc.y + parentMat.ay } : null;
    const pose = s => {
      let r = s;
      if (layerMat) r = ctx.transformSegments(r, lp, layerMat);
      if (parentMat) r = ctx.transformSegments(r, pp, parentMat);
      return r;
    };
    const point = p => pose([seg(p)])[0].point;
    const stroke = meanScale(layerMat) * meanScale(parentMat);
    return {
      frame,
      segments: Object.fromEntries(Object.entries(records).map(([id, r]) => [id, pose(r.segments)])),
      strokeWidth: { a: records.a.strokeWidth * stroke },
      gradient: { from: point(records.b.gradient.from), to: point(records.b.gradient.to) },
    };
  }),
}));
process.stdout.write(JSON.stringify({
  baseline, sourceHash: createHash('sha256').update(motion).digest('hex'),
  records, parentRecord, cases: out,
}) + '\n');
