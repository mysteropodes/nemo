'use strict';
// #2107 (G22 G14): parametric ellipse, polygon and star, a rotated rectangle, and the alpha / opaque canvas contract of the export.
// Expected anchors come from the baseline generators, written here as independent oracles (not copied from the product):
//   git show 1065e9d8:src/js/tools.js  buildArcEllipsePath (6770), buildStarPolygonPath (6828), syncParamShapeBoxOnScale (6887),
//   reapplyParamShapeAngle (6937); engine counterpart native-engine/src/object_parametric_geometry*.rs.
// Every row runs on a transparent canvas (K.fresh transparent): paint is alpha, so "clear" means nothing is painted.
const P = require('../payloads.cjs');
const K = require('../kit.cjs');
const probe = require('../png-probe.cjs');

const RED = [0.9, 0.2, 0.2, 1], GREEN_HALF = [0.1, 0.6, 0.2, 0.5], WHITE = [1, 1, 1, 1];
const flat = anchors => anchors.flat();
const pts = object => object.geometry.segments.map(s => [s.point.x, s.point.y]);
const rad = deg => (deg * Math.PI) / 180;
const polar = (cx, cy, rx, ry, deg) => [cx + rx * Math.cos(rad(deg)), cy + ry * Math.sin(rad(deg))];
// buildArcEllipsePath: n = max(8, ceil(sweep/4)) steps, outer arc, then the inner arc walked back (donut) or the centre (pie).
function arcAnchors(cx, cy, rx, ry, start, sweep, inner = 0) {
  const n = Math.max(8, Math.ceil(sweep / 4)), out = [];
  for (let i = 0; i <= n; i++) out.push(polar(cx, cy, rx, ry, start + (sweep * i) / n));
  if (inner > 0) for (let j = n; j >= 0; j--) out.push(polar(cx, cy, rx * inner, ry * inner, start + (sweep * j) / n));
  else out.push([cx, cy]);
  return out;
}
// Paper.js Matrix.rotate about a centre: x' = cx + dx cos - dy sin, y' = cy + dx sin + dy cos (clockwise on a y-down canvas).
const rotate = (points, deg, [cx, cy]) => points.map(([x, y]) => [cx + (x - cx) * Math.cos(rad(deg)) - (y - cy) * Math.sin(rad(deg)), cy + (x - cx) * Math.sin(rad(deg)) + (y - cy) * Math.cos(rad(deg))]);
const corners = ({ x1, y1, x2, y2 }) => [[x1, y1], [x2, y1], [x2, y2], [x1, y2]];

async function create(ctx, label, layerUid, ext, segments, fill = RED) {
  const made = await K.apply(ctx, label, P.vectorCreate(P.vectorRecord({ layerUid, segments, fill, ext })));
  ctx.ok('reply', label + ': owner allocated a stroke id', typeof made.result.strokeId === 'string' && made.result.strokeId.length > 0, made.result.strokeId);
  return made.result.strokeId;
}
const alpha = (scene, x, y) => probe.alphaAt(scene.image, x, y);

const rows = [
  { id: '2107-canvas-alpha-and-opaque-export', title: 'export alpha and colour: the default canvas is opaque white, a transparent canvas exports paint as alpha (white paint included), undo and reopen keep both', tasks: ['G14'],
    limitations: ['project.export has no alpha option: a transparent export is a project whose canvas background is #00000000 (project settings edit setCanvasBackground).'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx);
      const blank = await K.exportPng(ctx, 'opaque-blank');
      ctx.eq('pixel', 'a new project exports opaque white (alpha 255 at both corners, colour 255,255,255 at the centre)', [probe.alphaAt(blank.image, 0, 0), probe.alphaAt(blank.image, 639, 359), probe.over(blank.image, 320, 180)], [255, 255, [255, 255, 255]]);
      const red = await create(ctx, 'create a red rectangle', layerUid, undefined, P.rectSegments(20, 20, 160, 140));
      const white = await create(ctx, 'create a white rectangle', layerUid, undefined, P.rectSegments(200, 20, 300, 100), WHITE);
      const half = await create(ctx, 'create a half-alpha green rectangle', layerUid, undefined, P.rectSegments(450, 40, 600, 140), GREEN_HALF);
      ctx.eq('reply', 'three distinct stroke ids', new Set([red, white, half]).size, 3);
      const opaque = await K.exportPng(ctx, 'opaque');
      ctx.near('pixel', 'opaque canvas: the red rectangle keeps its colour', probe.over(opaque.image, 90, 80), [230, 51, 51], 8);
      ctx.near('pixel', 'opaque canvas: the half-alpha fill composites over the white canvas', probe.over(opaque.image, 525, 90), [140, 204, 153], 12);
      ctx.eq('pixel', 'opaque canvas: every probed pixel is alpha 255, painted or not', [[90, 80], [250, 60], [525, 90], [5, 5]].map(([x, y]) => probe.alphaAt(opaque.image, x, y)), [255, 255, 255, 255]);
      ctx.eq('pixel', 'opaque canvas: the white rectangle is colour distance 0 from the canvas (colour alone can not see it)', probe.distanceAt(opaque.image, 250, 60), 0);
      await K.transparentCanvas(ctx);
      const clear = await K.exportPng(ctx, 'transparent');
      ctx.eq('pixel', 'transparent canvas: unpainted pixels are alpha 0', [[5, 5], [300, 300], [630, 350]].map(([x, y]) => probe.alphaAt(clear.image, x, y)), [0, 0, 0]);
      ctx.eq('pixel', 'transparent canvas: the red and the white rectangle are alpha 255', [probe.alphaAt(clear.image, 90, 80), probe.alphaAt(clear.image, 250, 60)], [255, 255]);
      ctx.eq('pixel', 'transparent canvas: the white rectangle is still colour white, so only alpha tells it from nothing', probe.over(clear.image, 250, 60), [255, 255, 255]);
      ctx.near('pixel', 'transparent canvas: the half-alpha fill is alpha 128', probe.alphaAt(clear.image, 525, 90), 128, 3);
      ctx.near('pixel', 'transparent canvas: the paint colour composites to the same colour as on the opaque canvas', [probe.over(clear.image, 90, 80), probe.over(clear.image, 525, 90)].flat(), [probe.over(opaque.image, 90, 80), probe.over(opaque.image, 525, 90)].flat(), 3);
      ctx.near('pixel', 'transparent canvas: the alpha ink bounds are the artwork bounds', probe.inkBounds(clear.image).box, [20, 20, 599, 139], 2);
      await K.step(ctx, 'undo the canvas background', 'undo');
      ctx.canvas = 'opaque';
      ctx.eq('pixel', 'undo restores the opaque export exactly', (await K.exportPng(ctx, 'opaque-again')).digest, opaque.digest);
      await K.step(ctx, 'redo the canvas background', 'redo');
      ctx.canvas = 'transparent';
      const again = await K.exportPng(ctx, 'transparent-again');
      ctx.eq('pixel', 'redo restores the transparent export exactly', again.digest, clear.digest);
      const trip = await K.roundTrip(ctx, 'canvas', again);
      ctx.eq('file', 'the saved project keeps the transparent canvas background', trip.first.document.canvasBg, '#00000000');
      ctx.eq('pixel', 'after reopen the corner is still alpha 0 and the white rectangle alpha 255', [probe.alphaAt(trip.exported.image, 5, 5), probe.alphaAt(trip.exported.image, 250, 60)], [0, 255]);
    } },

  { id: '2107-parametric-ellipse-polygon-star', title: 'parametric ellipse arc and donut, regular polygon and star: anchors equal the baseline generators, stored intent, pixels, undo, no-op, save and reopen', tasks: ['G22'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx, { transparent: true });
      const E = { x1: 40, y1: 60, x2: 240, y2: 160 }, S = { x1: 340, y1: 40, x2: 540, y2: 160 };
      const ell = await create(ctx, 'create a parametric ellipse record', layerUid, { paramShape: { kind: 'ellipse', box: E } }, P.rectSegments(E.x1, E.y1, E.x2, E.y2));
      const star = await create(ctx, 'create a parametric star record', layerUid, { paramShape: { kind: 'star', box: S } }, P.rectSegments(S.x1, S.y1, S.x2, S.y2));
      const te = P.target(layerUid, ell), ts = P.target(layerUid, star);
      // Ellipse centre (140, 110), radii (100, 50): a 90 degree pie from angle 0 is the lower right quarter (y grows downward).
      await K.apply(ctx, 'ellipse arc: start 0, sweep 90', P.parametricSet(te, { kind: 'ellipse', startAngle: 0, sweep: 90 }));
      const pie = await K.readObject(ctx, te);
      ctx.eq('read', 'the stored intent is the ellipse arc and the generator box is untouched', [pie.object.vectorExt.paramShape.kind, pie.object.vectorExt.paramShape.startAngle, pie.object.vectorExt.paramShape.sweep, pie.object.vectorExt.paramShape.box], ['ellipse', 0, 90, E]);
      ctx.eq('read', 'the pie has 25 anchors (24 arc points and the centre)', pie.object.geometry.segments.length, 25);
      ctx.near('read', 'the pie anchors are the baseline arc polyline', flat(pts(pie.object)), flat(arcAnchors(140, 110, 100, 50, 0, 90)), 1e-6);
      const ePie = await K.exportPng(ctx, 'pie');
      ctx.eq('pixel', 'inside the quarter is painted, left of the centre and above it are not', [alpha(ePie, 190, 130), alpha(ePie, 90, 130), alpha(ePie, 190, 80)], [255, 0, 0]);
      ctx.near('pixel', 'the pie ink bounds are the quarter of the box', probe.inkBounds(ePie.image, { box: [0, 0, 300, 359] }).box, [140, 110, 239, 159], 2);
      await K.apply(ctx, 'ellipse donut: inner radius 0.5 over almost a full turn', P.parametricSet(te, { kind: 'ellipse', startAngle: 0, sweep: 359.9, innerRadius: 0.5 }));
      const donut = await K.readObject(ctx, te);
      ctx.near('read', 'the donut anchors are the baseline outer arc followed by the inner arc walked back', flat(pts(donut.object)), flat(arcAnchors(140, 110, 100, 50, 0, 359.9, 0.5)), 1e-6);
      const eDonut = await K.exportPng(ctx, 'donut');
      ctx.eq('pixel', 'the donut hole and the area outside are clear, the ring is painted', [alpha(eDonut, 140, 110), alpha(eDonut, 140, 120), alpha(eDonut, 140, 150), alpha(eDonut, 140, 165)], [0, 0, 255, 0]);
      await K.step(ctx, 'undo the donut', 'undo');
      ctx.eq('read', 'undo restores the pie record exactly', (await K.readObject(ctx, te)).object, pie.object);
      await K.step(ctx, 'redo the donut', 'redo');
      // Star box (340,40)-(540,160): outer radius min(w,h)/2 = 60 about (440, 100).
      await K.apply(ctx, 'polygon: six points, inner ratio 1', P.parametricSet(ts, { kind: 'star', pointCount: 6, innerRatio: 1 }));
      const hex = await K.readObject(ctx, ts);
      ctx.eq('read', 'the stored intent is the polygon', [hex.object.vectorExt.paramShape.pointCount, hex.object.vectorExt.paramShape.innerRatio], [6, 1]);
      ctx.eq('read', 'a polygon has one anchor per point', hex.object.geometry.segments.length, 6);
      ctx.near('read', 'the hexagon vertices are on the radius-60 circle, the first at the top, then clockwise every 60 degrees', flat(pts(hex.object)), flat([-90, -30, 30, 90, 150, 210].map(deg => polar(440, 100, 60, 60, deg))), 1e-6);
      const eHex = await K.exportPng(ctx, 'hexagon');
      const hexInk = probe.inkBounds(eHex.image, { box: [300, 0, 639, 359] });
      ctx.near('pixel', 'the hexagon ink spans 388..492 by 40..160', hexInk.box, [388, 40, 491, 159], 2);
      ctx.eq('pixel', 'the hexagon centre is painted and the corner of its box is not', [alpha(eHex, 440, 100), alpha(eHex, 345, 45)], [255, 0]);
      await K.apply(ctx, 'star: five points, inner ratio 0.5', P.parametricSet(ts, { kind: 'star', pointCount: 5, innerRatio: 0.5 }));
      const five = await K.readObject(ctx, ts);
      ctx.eq('read', 'a five point star has ten anchors alternating outer and inner radius', five.object.geometry.segments.length, 10);
      ctx.near('read', 'the star anchors alternate radius 60 and 30 starting at the top, clockwise', flat(pts(five.object)), flat(Array.from({ length: 10 }, (_, i) => polar(440, 100, i % 2 === 0 ? 60 : 30, i % 2 === 0 ? 60 : 30, -90 + 36 * i))), 1e-6);
      const eStar = await K.exportPng(ctx, 'star');
      ctx.eq('pixel', 'a probe inside the top arm and one inside the right arm are painted, the notch between them is not', [alpha(eStar, 440, 50), alpha(eStar, 488, 85), alpha(eStar, 469, 60)], [255, 255, 0]);
      await K.apply(ctx, 'the same star again', P.parametricSet(ts, { kind: 'star', pointCount: 5, innerRatio: 0.5 }), { noop: true });
      await K.apply(ctx, 'parameters of a stale target', P.parametricSet(P.target(layerUid, 'native-stroke:99999:9'), { kind: 'star', pointCount: 5, innerRatio: 0.5 }), { refused: true });
      await K.roundTrip(ctx, 'shapes', eStar, async () => {
        const [again, star2] = [await K.readObject(ctx, te), await K.readObject(ctx, ts)];
        ctx.eq('read', 'the donut and the star survive save and reopen with their intent and anchors', [again.ok && again.object, star2.ok && star2.object], [donut.object, five.object]);
      });
    } },

  { id: '2107-parametric-rotated-box', title: 'rotated parametric rectangle: box angle stored, exact translate, scale rebuilds in the rotated frame, corner radius, undo, pixels, save and reopen', tasks: ['G22'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx, { transparent: true });
      const box = { x1: 100, y1: 100, x2: 300, y2: 200 }, centre = [200, 150];
      const ext = { paramShape: { kind: 'rect', tl: 0, tr: 0, br: 0, bl: 0, box }, boxAngle: 30 };
      const id = await create(ctx, 'create a rectangle turned by 30 degrees', layerUid, ext, rotate(corners(box), 30, centre).map(([x, y]) => P.seg(x, y)));
      const tgt = P.target(layerUid, id);
      const r0 = await K.readObject(ctx, tgt);
      ctx.eq('read', 'the box angle and the un-rotated generator box are stored', [r0.object.vectorExt.boxAngle, r0.object.vectorExt.paramShape.box], [30, box]);
      const listing0 = await K.hitTest(ctx, layerUid), item0 = K.itemOf(listing0, id);
      ctx.eq('read', 'the hit-test listing reports the dynamic rectangle and its box angle', [item0.paramShape.kind, item0.boxAngle], ['rect', 30]);
      // Rotated about (200,150): half-extents 100 x 50 turn into a 223.2 x 186.6 bounding box.
      const e0 = await K.exportPng(ctx, 'r0');
      ctx.near('pixel', 'the ink bounds are those of the turned rectangle (223.2 x 186.6)', probe.inkBounds(e0.image).box, [88, 57, 311, 243], 2);
      ctx.eq('pixel', 'the centre is painted; the corner of the un-rotated box is not', [alpha(e0, 200, 150), alpha(e0, 102, 102)], [255, 0]);
      const inward = (p, toward, d) => { const v = [toward[0] - p[0], toward[1] - p[1]], n = Math.hypot(...v); return [p[0] + (v[0] / n) * d, p[1] + (v[1] / n) * d].map(Math.round); };
      const turned = rotate(corners(box), 30, centre);
      ctx.eq('pixel', 'a point just inside each turned corner is painted', turned.map(c => { const q = inward(c, centre, 6); return alpha(e0, q[0], q[1]); }), [255, 255, 255, 255]);
      await K.apply(ctx, 'translate by (40, 30)', P.parametricTranslate(tgt, [40, 30]));
      const r1 = await K.readObject(ctx, tgt);
      ctx.near('read', 'every anchor moved by exactly (40, 30)', flat(pts(r1.object)), flat(pts(r0.object).map(([x, y]) => [x + 40, y + 30])), 1e-9);
      ctx.eq('read', 'the box moved with the shape and the box angle is unchanged', [r1.object.vectorExt.paramShape.box, r1.object.vectorExt.boxAngle], [{ x1: 140, y1: 130, x2: 340, y2: 230 }, 30]);
      const e1 = await K.exportPng(ctx, 'r1'), b0 = probe.inkBounds(e0.image).box;
      ctx.near('pixel', 'the ink moved by (40, 30)', probe.inkBounds(e1.image).box.map((v, i) => v - b0[i]), [40, 30, 40, 30], 2);
      // Scale (2, 1) about (140, 130): the box becomes (140,130)-(540,230) and the rectangle is REBUILT from it in the rotated frame about the new box centre
      // (syncParamShapeBoxOnScale, tools.js 6887: "a rect is rebuilt from the new box with its radii and box angle"), not sheared.
      await K.apply(ctx, 'scale (2, 1) about (140, 130)', P.parametricScale(tgt, [2, 1], [140, 130]));
      const r2 = await K.readObject(ctx, tgt), wide = { x1: 140, y1: 130, x2: 540, y2: 230 };
      ctx.eq('read', 'the box is scaled about the anchor and the angle is kept', [r2.object.vectorExt.paramShape.box, r2.object.vectorExt.boxAngle], [wide, 30]);
      ctx.near('read', 'the anchors are the scaled box turned by 30 degrees about its own centre', flat(pts(r2.object)), flat(rotate(corners(wide), 30, [340, 180])), 1e-6);
      const side = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]), p2 = pts(r2.object);
      ctx.near('read', 'the sides stay perpendicular rectangle sides 400 and 100 (a shear would not)', [side(p2[0], p2[1]), side(p2[1], p2[2])], [400, 100], 1e-6);
      const e2 = await K.exportPng(ctx, 'r2');
      ctx.near('pixel', 'the ink bounds are those of a 400 x 100 rectangle turned by 30 degrees (396.4 x 286.6)', probe.inkBounds(e2.image).box, [142, 37, 538, 323], 2);
      await K.apply(ctx, 'round the corners to radius 20', P.parametricSet(tgt, { kind: 'rect', tl: 20, tr: 20, br: 20, bl: 20 }));
      const r3 = await K.readObject(ctx, tgt);
      ctx.eq('read', 'eight anchors and the box angle and box are kept', [r3.object.geometry.segments.length, r3.object.vectorExt.boxAngle, r3.object.vectorExt.paramShape.box], [8, 30, wide]);
      const e3 = await K.exportPng(ctx, 'r3'), corner = rotate(corners(wide), 30, [340, 180])[0];
      const along = d => inward(corner, [340, 180], d);
      ctx.eq('pixel', 'the rounded corner is cut away 3 px in towards the centre and painted 18 px in', [alpha(e3, ...along(3)), alpha(e3, ...along(18))], [0, 255]);
      await K.step(ctx, 'undo the radius', 'undo');
      ctx.eq('read', 'undo restores the scaled record exactly', (await K.readObject(ctx, tgt)).object, r2.object);
      ctx.eq('pixel', 'undo restores the scaled export', (await K.exportPng(ctx, 'r3-undone')).digest, e2.digest);
      await K.step(ctx, 'redo the radius', 'redo');
      await K.roundTrip(ctx, 'rotated', e3, async () => {
        const back = await K.readObject(ctx, tgt);
        ctx.eq('read', 'the rotated shape survives save and reopen with its box angle', [back.ok && back.object.vectorExt.boxAngle, back.ok && back.object.vectorExt.paramShape.box], [30, wide]);
      });
    } },
];

module.exports = { rows };
