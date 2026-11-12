'use strict';
// #2107 (G14 G22 G13): native open/closed vector records, parametric shapes, ordered path effects, through nemo-mcp.
const P = require('../payloads.cjs');
const K = require('../kit.cjs');
const probe = require('../png-probe.cjs');

const RED = [0.9, 0.2, 0.2, 1], BLUE = [0, 0.27, 0.8, 1], GREEN_HALF = [0.1, 0.6, 0.2, 0.5];
const pts = object => object.geometry.segments.map(s => [s.point.x, s.point.y]);
const around = (ctx, kind, id, bounds, expected, tol) => ctx.near(kind, id, bounds && bounds.box, expected, tol);

async function createRecord(ctx, label, layerUid, spec, extra) {
  const made = await K.apply(ctx, label, P.vectorCreate(P.vectorRecord({ layerUid, ...spec }), extra));
  ctx.ok('reply', label + ': owner allocated a stroke id', typeof made.result.strokeId === 'string' && made.result.strokeId.length > 0, made.result.strokeId);
  return made.result.strokeId;
}

const rows = [
  { id: '2107-vector-create-export-reopen', title: 'open and closed vector records with paint, stroke and alpha: records, export pixels, save and reopen', tasks: ['G14'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx);
      const blank = await K.exportPng(ctx, 'blank');
      ctx.eq('pixel', 'empty document exports no ink', probe.inkBounds(blank.image), null);
      const a = await createRecord(ctx, 'create closed filled rectangle', layerUid, { segments: P.rectSegments(20, 20, 160, 140), fill: RED });
      const b = await createRecord(ctx, 'create open stroked path', layerUid, { closed: false, segments: [P.seg(200, 60), P.seg(420, 160)], stroke: BLUE, width: 8, cap: 'butt' });
      const c = await createRecord(ctx, 'create half-alpha closed fill', layerUid, { segments: P.rectSegments(450, 40, 600, 140), fill: GREEN_HALF });
      ctx.eq('reply', 'three distinct stroke ids', new Set([a, b, c]).size, 3);
      const ra = await K.readObject(ctx, P.target(layerUid, a)), rb = await K.readObject(ctx, P.target(layerUid, b)), rc = await K.readObject(ctx, P.target(layerUid, c));
      ctx.ok('read', 'all three records read back', ra.ok && rb.ok && rc.ok, [ra.ok, rb.ok, rc.ok]);
      ctx.eq('read', 'closed rectangle: closed flag and anchors as sent', [ra.object.geometry.closed, pts(ra.object)], [true, [[20, 20], [160, 20], [160, 140], [20, 140]]]);
      ctx.eq('read', 'closed rectangle: solid red fill, no real stroke', [ra.object.fill, ra.object.vector.hasRealStroke], [P.solid(RED), false]);
      ctx.eq('read', 'open path: independent closure, anchors as sent', [rb.object.geometry.closed, pts(rb.object)], [false, [[200, 60], [420, 160]]]);
      ctx.eq('read', 'open path: real stroke, width, cap, null fill', [rb.object.vector.hasRealStroke, rb.object.vector.strokeWidth, rb.object.vector.strokeCap, rb.object.fill, rb.object.vector.stroke], [true, 8, 'butt', null, P.solid(BLUE)]);
      ctx.eq('read', 'half-alpha fill keeps its alpha', rc.object.fill, P.solid(GREEN_HALF));
      const scene = await K.exportPng(ctx, 'scene');
      ctx.eq('pixel', 'export has the canvas size', [scene.width, scene.height], [640, 360]);
      const ink = probe.inkBounds(scene.image);
      ctx.ok('pixel', 'export has ink', ink !== null, ink);
      ctx.near('pixel', 'ink left/top/right edge match the artwork', [ink.box[0], ink.box[1], ink.box[2]], [20, 20, 600], 4);
      ctx.within('pixel', 'ink bottom edge is the open path end, stroke included', ink.box[3], 160, 168);
      ctx.near('pixel', 'rectangle interior is the fill colour', probe.over(scene.image, 90, 80), [230, 51, 51], 8);
      ctx.near('pixel', 'open path centre is the stroke colour', probe.over(scene.image, 310, 110), [0, 69, 204], 14);
      ctx.near('pixel', 'half-alpha fill composites to half coverage', probe.over(scene.image, 525, 90), [140, 204, 153], 12);
      ctx.near('pixel', 'half-alpha fill alpha', probe.alphaAt(scene.image, 525, 90), 128, 6);
      ctx.eq('pixel', 'outside the artwork is transparent', [probe.alphaAt(scene.image, 5, 5), probe.alphaAt(scene.image, 300, 300)], [0, 0]);
      const trip = await K.roundTrip(ctx, 'vector', scene);
      const saved = K.objectStrokeIds(trip.first.document);
      ctx.ok('file', 'saved project holds every created stroke', [a, b, c].every(id => saved.includes(id)), saved, [a, b, c]);
      const after = await Promise.all([a, b, c].map(id => K.readObject(ctx, P.target(layerUid, id))));
      ctx.eq('read', 'records after reopen equal the records before', after.map(r => r.ok && r.object), [ra.object, rb.object, rc.object]);
    } },

  { id: '2107-vector-replace-delete-undo-redo', title: 'replace, delete, no-op, undo and redo restore exact pixels; stale target, stale revision and locked layer refuse', tasks: ['G14'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx);
      const id = await createRecord(ctx, 'create rectangle', layerUid, { segments: P.rectSegments(20, 20, 160, 140), fill: RED });
      const tgt = P.target(layerUid, id);
      const e0 = await K.exportPng(ctx, 'original'), r0 = await K.readObject(ctx, tgt);
      const next = P.vectorRecord({ layerUid, strokeId: id, segments: P.rectSegments(20, 20, 220, 160), fill: BLUE });
      await K.apply(ctx, 'replace in place', P.vectorReplace(next));
      const r1 = await K.readObject(ctx, tgt), e1 = await K.exportPng(ctx, 'replaced');
      ctx.eq('read', 'replaced record holds the new anchors and fill', [pts(r1.object), r1.object.fill], [[[20, 20], [220, 20], [220, 160], [20, 160]], P.solid(BLUE)]);
      ctx.ok('pixel', 'replacement changes the export', e1.digest !== e0.digest, [e0.digest, e1.digest]);
      ctx.near('pixel', 'new interior is blue', probe.over(e1.image, 190, 140), [0, 69, 204], 8);
      await K.step(ctx, 'undo replace', 'undo');
      const r2 = await K.readObject(ctx, tgt), e2 = await K.exportPng(ctx, 'undone');
      ctx.eq('read', 'undo restores the original record', r2.object, r0.object);
      ctx.eq('pixel', 'undo restores the original export', e2.digest, e0.digest);
      await K.step(ctx, 'redo replace', 'redo');
      const e3 = await K.exportPng(ctx, 'redone');
      ctx.eq('pixel', 'redo reapplies the replacement export', e3.digest, e1.digest);
      await K.apply(ctx, 'replace with identical record', P.vectorReplace(next), { noop: true });
      await K.apply(ctx, 'delete', P.vectorDelete(tgt));
      const gone = await K.readObject(ctx, tgt), e4 = await K.exportPng(ctx, 'deleted');
      ctx.eq('read', 'deleted stroke no longer reads', gone.ok, false);
      ctx.eq('pixel', 'deleted artwork leaves no ink', probe.inkBounds(e4.image), null);
      await K.step(ctx, 'undo delete', 'undo');
      const e5 = await K.exportPng(ctx, 'delete-undone');
      ctx.eq('pixel', 'undo restores the replaced export', e5.digest, e1.digest);
      // Negative controls: nothing moves, redo (none here) and history stay intact.
      await K.apply(ctx, 'replace a stale target', P.vectorReplace(P.vectorRecord({ layerUid, strokeId: 'native-stroke:99999:9', segments: P.rectSegments(1, 1, 9, 9), fill: RED })), { refused: true });
      const now = await ctx.wire.discover();
      await K.apply(ctx, 'delete with a stale expected revision', P.vectorDelete(tgt), { refused: true, codes: ['stale_revision'], send: { expectedRevision: now.contentRevision - 1 } });
      await K.apply(ctx, 'lock the layer', P.layerPolicy(layerUid, 'locked', true));
      await K.apply(ctx, 'create on a locked layer', P.vectorCreate(P.vectorRecord({ layerUid, segments: P.rectSegments(300, 200, 360, 260), fill: RED })), { refused: true });
      const e6 = await K.exportPng(ctx, 'after-refusals');
      ctx.eq('pixel', 'refusals leave the export untouched', e6.digest, e1.digest);
    } },

  { id: '2107-parametric-set-translate-scale', title: 'parametric rectangle: exact translate and scale arithmetic, corner radius, undo, persistence', tasks: ['G22'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx);
      const ext = { paramShape: { kind: 'rect', tl: 0, tr: 0, br: 0, bl: 0, box: { x1: 100, y1: 100, x2: 300, y2: 200 } } };
      const id = await createRecord(ctx, 'create parametric rectangle', layerUid, { segments: P.rectSegments(100, 100, 300, 200), fill: RED, ext });
      const tgt = P.target(layerUid, id);
      const r0 = await K.readObject(ctx, tgt), e0 = await K.exportPng(ctx, 'p0');
      ctx.eq('read', 'parameters persisted as created', [r0.object.vectorExt.paramShape.kind, r0.object.vectorExt.paramShape.box], ['rect', ext.paramShape.box]);
      around(ctx, 'pixel', 'created shape ink bounds', probe.inkBounds(e0.image), [100, 100, 300, 200], 2);
      await K.apply(ctx, 'translate by (40, 30)', P.parametricTranslate(tgt, [40, 30]));
      const r1 = await K.readObject(ctx, tgt), e1 = await K.exportPng(ctx, 'p1');
      ctx.eq('read', 'translated anchors are exact', pts(r1.object), [[140, 130], [340, 130], [340, 230], [140, 230]]);
      ctx.eq('read', 'translated parameter box moved with the shape', r1.object.vectorExt.paramShape.box, { x1: 140, y1: 130, x2: 340, y2: 230 });
      around(ctx, 'pixel', 'translated ink bounds', probe.inkBounds(e1.image), [140, 130, 340, 230], 2);
      await K.apply(ctx, 'scale (2, 0.5) about (140, 130)', P.parametricScale(tgt, [2, 0.5], [140, 130]));
      const r2 = await K.readObject(ctx, tgt), e2 = await K.exportPng(ctx, 'p2');
      ctx.eq('read', 'scaled anchors are exact', pts(r2.object), [[140, 130], [540, 130], [540, 180], [140, 180]]);
      ctx.eq('read', 'scaled parameter box', r2.object.vectorExt.paramShape.box, { x1: 140, y1: 130, x2: 540, y2: 180 });
      around(ctx, 'pixel', 'scaled ink bounds', probe.inkBounds(e2.image), [140, 130, 540, 180], 2);
      await K.apply(ctx, 'round the corners to radius 20', P.parametricSet(tgt, { kind: 'rect', tl: 20, tr: 20, br: 20, bl: 20 }));
      const r3 = await K.readObject(ctx, tgt), e3 = await K.exportPng(ctx, 'p3');
      ctx.eq('read', 'corner radius is stored', [r3.object.vectorExt.paramShape.tl, r3.object.vectorExt.paramShape.br], [20, 20]);
      ctx.ok('read', 'rounded outline has more anchors than the rectangle', r3.object.geometry.segments.length > 4, r3.object.geometry.segments.length, '>4');
      ctx.ok('pixel', 'the corner pixel is no longer inked', probe.alphaAt(e3.image, 141, 131) < 40, probe.alphaAt(e3.image, 141, 131), '<40');
      ctx.ok('pixel', 'the middle edge is still inked', probe.alphaAt(e3.image, 340, 131) > 200, probe.alphaAt(e3.image, 340, 131), '>200');
      around(ctx, 'pixel', 'rounded shape keeps its bounds', probe.inkBounds(e3.image), [140, 130, 540, 180], 2);
      await K.step(ctx, 'undo the radius', 'undo');
      const r4 = await K.readObject(ctx, tgt), e4 = await K.exportPng(ctx, 'p4');
      ctx.eq('read', 'undo restores the scaled record', r4.object, r2.object);
      ctx.eq('pixel', 'undo restores the scaled export', e4.digest, e2.digest);
      await K.step(ctx, 'redo the radius', 'redo');
      const e5 = await K.exportPng(ctx, 'p5');
      await K.roundTrip(ctx, 'parametric', e5);
      const r5 = await K.readObject(ctx, tgt);
      ctx.eq('read', 'parameters persist after reopen', r5.ok && r5.object.vectorExt.paramShape, r3.object.vectorExt.paramShape);
      await K.apply(ctx, 'translate a stale target', P.parametricTranslate(P.target(layerUid, 'native-stroke:99999:9'), [1, 1]), { refused: true });
    } },

  { id: '2107-path-effects-stack', title: 'ordered zigzag and roughen stack: source geometry immutable, order, disable, no-op, removal, refusal while active', tasks: ['G13'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx);
      const id = await createRecord(ctx, 'create a straight open path', layerUid, { closed: false, segments: [P.seg(60, 180), P.seg(580, 180)], stroke: BLUE, width: 6, cap: 'butt' });
      const tgt = P.target(layerUid, id);
      const r0 = await K.readObject(ctx, tgt), e0 = await K.exportPng(ctx, 'x0');
      const ink0 = probe.inkBounds(e0.image);
      const zig = { type: 'zigzag', enabled: true, p1: 12, p2: 6, p3: 0 }, rough = { type: 'roughen', enabled: true, p1: 8, p2: 6 };
      await K.apply(ctx, 'set [zigzag]', P.pathEffects(layerUid, [zig]));
      const e1 = await K.exportPng(ctx, 'x1'), ink1 = probe.inkBounds(e1.image);
      ctx.ok('pixel', 'zigzag changes the export', e1.digest !== e0.digest, [e0.digest, e1.digest]);
      ctx.ok('pixel', 'zigzag spreads the ink vertically', ink1.box[3] - ink1.box[1] > ink0.box[3] - ink0.box[1] + 4, [ink0.box, ink1.box]);
      ctx.eq('read', 'source geometry is unchanged by the effect', (await K.readObject(ctx, tgt)).object, r0.object);
      await K.apply(ctx, 'set [zigzag, roughen]', P.pathEffects(layerUid, [zig, rough]));
      const e2 = await K.exportPng(ctx, 'x2');
      ctx.ok('pixel', 'adding roughen changes the export', e2.digest !== e1.digest, [e1.digest, e2.digest]);
      await K.apply(ctx, 'reorder to [roughen, zigzag]', P.pathEffects(layerUid, [rough, zig]));
      const e3 = await K.exportPng(ctx, 'x3'), e3b = await K.exportPng(ctx, 'x3-again');
      ctx.eq('pixel', 'evaluation is deterministic: the same state exports the same pixels', e3b.digest, e3.digest);
      ctx.ok('pixel', 'stack order is part of the result', e3.digest !== e2.digest, [e2.digest, e3.digest]);
      await K.apply(ctx, 'disable roughen', P.pathEffects(layerUid, [zig, { ...rough, enabled: false }]));
      const e4 = await K.exportPng(ctx, 'x4');
      ctx.eq('pixel', 'a disabled effect contributes nothing: equals the zigzag-only export', e4.digest, e1.digest);
      await K.apply(ctx, 'set the same stack again', P.pathEffects(layerUid, [zig, { ...rough, enabled: false }]), { noop: true });
      await K.apply(ctx, 'vector create on a layer with an active effect', P.vectorCreate(P.vectorRecord({ layerUid, segments: P.rectSegments(10, 10, 40, 40), fill: RED })), { refused: true });
      const trip = await K.roundTrip(ctx, 'effects', e4);
      ctx.ok('file', 'saved project keeps the ordered stack', JSON.stringify(trip.first.document).includes('"roughen"') && JSON.stringify(trip.first.document).includes('"zigzag"'), 'pathFx present');
      await K.apply(ctx, 'remove all effects', P.pathEffects(layerUid, []));
      const e5 = await K.exportPng(ctx, 'x5');
      ctx.eq('pixel', 'removing the stack restores the pre-effect export exactly', e5.digest, e0.digest);
      ctx.eq('read', 'source geometry still unchanged after removal', (await K.readObject(ctx, tgt)).object, r0.object);
      await K.step(ctx, 'undo the removal', 'undo');
      const e6 = await K.exportPng(ctx, 'x6');
      ctx.eq('pixel', 'undo brings the effect back', e6.digest, e4.digest);
    } },
];

module.exports = { lane: '2107', issue: 2107, rows };
