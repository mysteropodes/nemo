'use strict';
// #2113 (D32 F16 G03a I16a G06 G17): one pinned scene for every producer, style and compositing, image mesh, bitmap-brush
// resources, and evaluate versus export.
const P = require('../payloads.cjs');
const K = require('../kit.cjs');
const probe = require('../png-probe.cjs');

const RED = [0.9, 0.2, 0.2, 1], GREEN = [0.1, 0.7, 0.3, 1], TEAL = [40, 160, 200];
const QUAD = [[200, 40, 40], [40, 160, 60], [40, 60, 200], [220, 200, 40]];
const MESH = { outline: [[0, 0], [0.5, 0], [1, 0], [1, 0.5], [1, 1], [0.5, 1], [0, 1], [0, 0.5]], verts: [[0, 0], [0.5, 0], [1, 0], [1, 0.5], [1, 1], [0.5, 1], [0, 1], [0, 0.5], [0.5, 0.5]],
  tris: [8, 0, 1, 8, 1, 2, 8, 2, 3, 8, 3, 4, 8, 4, 5, 8, 5, 6, 8, 6, 7, 8, 7, 0], offsets: [[0, 0], [0, 0], [0, 0], [0, 0], [0, 0], [0, 0], [0, 0], [0, 0], [0, 0]], cols: 2, rows: 2 };
const typed = (res, codes) => res.ok === false && res.frontDoor !== true && codes.includes(res.error.code);
const STALE = ['stale_revision', 'not_found', 'invalid_request'];
const same = (image, other, points) => points.filter(([x, y]) => probe.over(image, x, y).join() !== probe.over(other, x, y).join());
const attach = (ctx, label, file, name) => K.apply(ctx, label, () => K.pinned(ctx, ({ pin, lifecycleGeneration }) => P.assetAttach({ pin, name, path: file, lifecycleGeneration })));
async function opacityBuilder(ctx, layerUid, fraction) {
  const { revision, pin } = await K.pinNow(ctx);
  const read = P.propertyMetadata(revision, pin, layerUid);
  const meta = await ctx.wire.query(read.operation, read.payload);
  ctx.ok('reply', 'property metadata read', meta.ok, meta.ok ? 'ok' : meta.error);
  const item = (meta.result.items || []).find(i => i.property === 'opacity');
  ctx.ok('read', 'the layer exposes an opacity property', !!item, Object.keys(meta.result));
  const base = item.defaultValue;
  return P.propertyOpacity(layerUid, Array.isArray(base) ? base.map(v => v * fraction) : base * fraction);
}

const rows = [
  { id: '2113-mixed-scene-one-export', title: 'vector with gradient and stroke, text and a raster in one export: families, z-order by reorder, layer opacity once, reopen', tasks: ['D32', 'F16', 'G03a'],
    limitations: ['Vector, gradient, text and raster producers only; no component or rig producer is built.'],
    async run(ctx) {
      const { layerUid: l1 } = await K.fresh(ctx);
      await K.apply(ctx, 'create a red rectangle', P.vectorCreate(P.vectorRecord({ layerUid: l1, segments: P.rectSegments(20, 20, 160, 140), fill: RED })));
      const gradient = { fillGradient: { kind: 'linear', from: [20, 0], to: [200, 0], stops: [{ offset: 0, color: '#ff0000' }, { offset: 1, color: '#0000ff' }] } };
      await K.apply(ctx, 'create a gradient-filled, stroked rectangle', P.vectorCreate(P.vectorRecord({ layerUid: l1, segments: P.rectSegments(20, 200, 200, 280), fill: RED, stroke: [0, 0, 0, 1], width: 6, ext: gradient })));
      await K.apply(ctx, 'create text', P.textCreate(l1, P.textBlock('MIX', { size: 72, color: '#0044cc', anchorTopLeft: { x: 440, y: 20 } })));
      const image = K.stage(ctx, 'teal.png', K.solidPng(160, 100, [...TEAL, 255]));
      const att = await attach(ctx, 'attach a raster on a new layer', image, 'mix-image');
      const l2 = att.result.target.layerUid;
      const e0 = await K.exportPng(ctx, 'mix0');
      const red = probe.countColor(e0.image, [230, 51, 51], { box: [20, 20, 160, 140], tol: 40 });
      const blue = probe.countColor(e0.image, [0, 68, 204], { box: [420, 0, 639, 140], tol: 40 });
      const teal = probe.countColor(e0.image, TEAL, { box: [170, 150, 439, 359], tol: 40 });
      ctx.ok('pixel', 'vector family is in the export', red > 1000, red, '>1000');
      ctx.ok('pixel', 'text family is in the export', blue > 20, blue, '>20');
      ctx.ok('pixel', 'raster family is in the export', teal > 5000, teal, '>5000');
      const left = probe.over(e0.image, 25, 240), right = probe.over(e0.image, 195, 240);
      ctx.ok('pixel', 'the gradient runs from red at the start to blue at the end', left[0] > right[0] + 80 && right[2] > left[2] + 80, { left, right }, 'r falls, b rises');
      ctx.near('pixel', 'the stroke paints at the rectangle edge', probe.over(e0.image, 110, 200), [0, 0, 0], 40);
      // z-order: overlap the raster with a vector and let reorder decide who is on top.
      const tealBox = probe.inkBounds(e0.image, { box: [170, 150, 439, 359] });
      ctx.ok('pixel', 'the raster has its own region', tealBox !== null && tealBox.box[2] - tealBox.box[0] >= 140, tealBox);
      const cx = Math.round((tealBox.box[0] + tealBox.box[2]) / 2), cy = Math.round((tealBox.box[1] + tealBox.box[3]) / 2);
      await K.apply(ctx, 'create a vector over the raster centre', P.vectorCreate(P.vectorRecord({ layerUid: l1, segments: P.rectSegments(cx - 20, cy - 20, cx + 20, cy + 20), fill: GREEN })));
      const over0 = probe.over((await K.exportPng(ctx, 'z0')).image, cx, cy);
      const isGreen = c => Math.abs(c[0] - 26) < 20 && Math.abs(c[1] - 179) < 20 && Math.abs(c[2] - 77) < 20, isTeal = c => probe.near(c, TEAL, 20);
      ctx.ok('pixel', 'the overlap shows exactly one of the two layers', isGreen(over0) !== isTeal(over0), over0);
      const order = await K.layerOrder(ctx), mover = order[0] === l1 ? l2 : l1;
      await K.apply(ctx, 'move the other layer to the first gap', P.layerReorder([mover], 0, mover));
      const over1 = probe.over((await K.exportPng(ctx, 'z1')).image, cx, cy);
      ctx.ok('pixel', 'reordering flips which family is on top', isGreen(over1) !== isGreen(over0) && (isGreen(over1) || isTeal(over1)), { before: over0, after: over1 });
      await K.step(ctx, 'undo the reorder', 'undo');
      const e1 = await K.exportPng(ctx, 'z-undone');
      ctx.eq('pixel', 'undo restores the original order', probe.over(e1.image, cx, cy), over0);
      // Layer opacity applied once to the raster layer only.
      const sample = [[40, 40], [120, 100], [500, 60], [30, 240], [190, 270]];
      await K.apply(ctx, 'raster layer opacity to half', () => opacityBuilder(ctx, l2, 0.5));
      const e2 = await K.exportPng(ctx, 'op');
      ctx.near('pixel', 'opacity is applied once to the raster', probe.coverageRatio(e1, e2, tealBox.box), 0.5, 0.08);
      ctx.eq('pixel', 'vector, gradient and text pixels are untouched by the raster layer opacity', same(e1.image, e2.image, sample), []);
      await K.step(ctx, 'undo the opacity', 'undo');
      ctx.eq('pixel', 'undo restores the export', (await K.exportPng(ctx, 'op-undone')).digest, e1.digest);
      await K.roundTrip(ctx, 'mixed', e1);
    } },

  { id: '2113-image-mesh-sculpt', title: 'raster bound to an image mesh: bind, read the mesh and its offsets back, sculpt one outline vertex, the pixels move, undo, redo, reopen', tasks: ['G03a'],
    limitations: ['The mesh id and offsets come from the imageMesh read view; image.mesh.edit answers only its receipt.'],
    async run(ctx) {
      await K.fresh(ctx);
      // A raster with four distinct quadrants, so a deformation has content to move (a flat image cannot show one).
      const image = K.stage(ctx, 'quadrants.png', K.quadrantPng(160, 100, QUAD));
      const att = await attach(ctx, 'attach the quadrant raster on a new layer', image, 'mesh-image');
      const layer = att.result.target.layerUid;
      const e0 = await K.exportPng(ctx, 'plain'), box = probe.inkBounds(e0.image);
      ctx.ok('pixel', 'the raster is on the canvas', box !== null && box.box[2] - box.box[0] >= 140, box);
      const [x0, y0, x1, y1] = box.box, w = x1 - x0, h = y1 - y0;
      const at = (u, v) => [Math.round(x0 + u * w), Math.round(y0 + v * h)];
      const colorAt = (img, u, v) => probe.over(img, ...at(u, v));
      ctx.eq('pixel', 'each quadrant of the raster reaches the export with its own colour', [[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]].map(([u, v]) => QUAD.findIndex(c => probe.near(colorAt(e0.image, u, v), c, 12))), [0, 1, 2, 3]);
      ctx.eq('read', 'before binding, the raster has no mesh', (await K.meshState(ctx, layer)).mesh, null);
      const bound = await K.apply(ctx, 'bind a mesh to the raster', P.meshAttach(att.result.target, MESH));
      ctx.eq('reply', 'the mesh attach reply is only its receipt (the mesh id is read, not returned)', Object.keys(bound.res.result).sort(), ['applied', 'historyEntriesAdded']);
      const rest = await ctx.behavior('the mesh is bound', async () => {
        const state = await K.meshState(ctx, layer);
        ctx.ok('read', 'a mesh is bound with an id', state.mesh !== null && typeof state.mesh.meshId === 'string' && state.mesh.meshId.length > 0, state.mesh);
        ctx.eq('read', 'the mesh counts are the ones sent', [state.mesh.outlineCount, state.mesh.vertexCount, state.mesh.triangleIndexCount], [8, 9, 24]);
        ctx.eq('read', 'the rest sculpt is all zero', state.offsets === null ? 'absent' : state.offsets.every(([du, dv]) => du === 0 && dv === 0), true);
        return state;
      });
      const meshId = rest.mesh.meshId;
      const e1 = await K.exportPng(ctx, 'rest-mesh');
      ctx.ok('pixel', 'a rest mesh draws the same image (seam noise only)', probe.differing(e0.image, e1.image) <= 0.02 * box.count, probe.differing(e0.image, e1.image), '<=' + Math.round(0.02 * box.count));
      // Sculpt the top-right OUTLINE vertex (index 2) toward the centre: the outline is the mask, so the corner must disappear.
      const corner = at(0.97, 0.03), keep = at(0.05, 0.95);
      ctx.ok('pixel', 'the corner probe is inked before the sculpt', probe.distanceAt(e1.image, ...corner) > 64, probe.distanceAt(e1.image, ...corner), '>64');
      await K.apply(ctx, 'sculpt the top-right outline vertex', P.meshSetOffset(meshId, 2, -0.4, 0.4));
      const sculpted = await ctx.behavior('the sculpt is stored and moves pixels', async () => {
        const state = await K.meshState(ctx, layer);
        ctx.near('read', 'the offset of vertex 2 is the one sent', state.offsets[2], [-0.4, 0.4], 1e-6);
        ctx.eq('read', 'no other vertex moved', state.offsets.filter((_, i) => i !== 2).every(([du, dv]) => du === 0 && dv === 0), true);
        const e2 = await K.exportPng(ctx, 'sculpt');
        ctx.ok('pixel', 'the corner is cut away by the moved outline vertex', probe.distanceAt(e2.image, ...corner) < 16, probe.distanceAt(e2.image, ...corner), '<16');
        ctx.eq('pixel', 'the opposite corner still shows the image', probe.near(probe.over(e2.image, ...keep), probe.over(e1.image, ...keep), 12), true);
        const after = probe.inkBounds(e2.image);
        ctx.near('pixel', 'the left and bottom edges of the raster did not move', [after.box[0], after.box[3]], [x0, y1], 2);
        return e2;
      });
      await K.step(ctx, 'undo the sculpt', 'undo');
      ctx.eq('read', 'undo restores every offset to zero', (await K.meshState(ctx, layer)).offsets.every(([du, dv]) => du === 0 && dv === 0), true);
      ctx.eq('pixel', 'undo restores the rest-mesh export', (await K.exportPng(ctx, 'sculpt-undone')).digest, e1.digest);
      await K.step(ctx, 'redo the sculpt', 'redo');
      ctx.near('read', 'redo reapplies the offset', (await K.meshState(ctx, layer)).offsets[2], [-0.4, 0.4], 1e-6);
      const e3 = await K.exportPng(ctx, 'sculpt-redone');
      ctx.eq('pixel', 'redo reapplies the sculpt export', e3.digest, sculpted.digest);
      // Negative controls: the owner refuses, nothing moves.
      await K.apply(ctx, 'sculpt a vertex that does not exist', P.meshSetOffset(meshId, 99, 0.1, 0.1), { refused: true });
      await K.apply(ctx, 'sculpt a mesh that does not exist', P.meshSetOffset('im_missing', 0, 0.1, 0.1), { refused: true });
      await K.roundTrip(ctx, 'mesh', e3, async () => {
        const state = await K.meshState(ctx, layer);
        ctx.ok('read', 'the mesh is bound after reopen', state.mesh !== null && state.mesh.vertexCount === 9, state.mesh);
        ctx.near('read', 'the sculpt offset survived save and reopen', state.offsets[2], [-0.4, 0.4], 1e-6);
      });
    } },

  { id: '2113-bitmap-brush-resources', title: 'bitmap brush publication, accounting, replacement, refusal, and a baked stroke in one revision', tasks: ['G06', 'G17'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx);
      const small = K.stage(ctx, 'tip8.png', K.solidPng(8, 8, [10, 10, 10, 255])), large = K.stage(ctx, 'tip16.png', K.solidPng(16, 16, [200, 40, 40, 255]));
      const a1 = await attach(ctx, 'attach the first tip image', small, 'tip-small'), a2 = await attach(ctx, 'attach the second tip image', large, 'tip-large');
      const inspect = async brushId => {
        const { revision, pin } = await K.pinNow(ctx), b = P.brushInspect(revision, pin, brushId);
        const res = await ctx.wire.query(b.operation, b.payload);
        ctx.ok('reply', 'brush inspect answers for ' + brushId, res.ok, res.ok ? 'ok' : res.error);
        return res.result.brush;
      };
      const publish = (label, assetId, brushId, w, h, opts) => K.resource(ctx, label, () => K.pinned(ctx, ({ pin }) => P.brushPublish(pin, assetId, brushId, w, h)), opts);
      const first = await publish('first publication', a1.result.assetId, 'evidence-brush', 8, 8, { applied: true });
      ctx.eq('reply', 'publication reports the published size', [first.result.brushId, first.result.width, first.result.height], ['evidence-brush', 8, 8]);
      const row1 = await inspect('evidence-brush');
      ctx.eq('read', 'inspect shows the brush resident with the reported accounting', [row1.resident, row1.version, row1.width, row1.height, row1.bytes], [true, first.result.version, 8, 8, first.result.bytes]);
      ctx.within('read', 'resident bytes are the decoded size of an 8x8 RGBA tip (plus bounded overhead)', row1.bytes, 256, 4096);
      ctx.eq('read', 'an unpublished brush is not resident', (await inspect('never-published')).resident, false);
      const again = await publish('publish the same asset again', a1.result.assetId, 'evidence-brush', 8, 8, { applied: false });
      ctx.eq('read', 'an identical republication reuses the retained bitmap: same version and bytes', [(await inspect('evidence-brush')).version, again.result.version], [row1.version, row1.version]);
      const second = await publish('replacement by another asset', a2.result.assetId, 'evidence-brush', 16, 16, { applied: true });
      const row2 = await inspect('evidence-brush');
      ctx.eq('read', 'replacement keeps one brush with the new size and accounting', [row2.resident, row2.width, row2.height, row2.bytes], [true, 16, 16, second.result.bytes]);
      ctx.ok('read', 'replacement advances the version and the bytes grow with the pixels', row2.version !== row1.version && row2.bytes > row1.bytes, { v1: row1.version, v2: row2.version, b1: row1.bytes, b2: row2.bytes });
      // Refusals are the OWNER's: sizes the transport admits (<= 4096 a side) that the owner refuses because they are not the
      // asset's own size, and an asset the project does not hold. They publish nothing and change nothing.
      await publish('publish a 16x16 asset as 32x32 (wrong size)', a2.result.assetId, 'evidence-brush', 32, 32, { refused: true });
      await publish('publish an asset the project does not hold', 'asset-that-does-not-exist', 'other-brush', 8, 8, { refused: true, codes: ['not_found'] });
      ctx.eq('read', 'refusals leave the resident brush as it was', await inspect('evidence-brush'), row2);
      ctx.eq('read', 'a refused publication created no brush', (await inspect('other-brush')).resident, false);
      // A baked stroke: anchor, companion and PNG land as one revision and one undo entry.
      const tip = K.stage(ctx, 'baked.png', K.solidPng(58, 18, [...TEAL, 255]));
      const e0 = await K.exportPng(ctx, 'before-stroke');
      await K.apply(ctx, 'bitmap-brush stroke', () => K.pinned(ctx, ({ pin, lifecycleGeneration }) => P.brushStroke({ pin, lifecycleGeneration, layerUid, groupId: 'bmb_evidence_1', path: tip,
        companion: { x: 35, y: 215, width: 58, height: 18, opacity: 1 }, segments: [P.seg(10, 210), P.seg(60, 220)] })));
      const e1 = await K.exportPng(ctx, 'after-stroke');
      ctx.near('pixel', 'the baked companion paints its image at its rectangle', probe.over(e1.image, 64, 224), TEAL, 14);
      ctx.ok('pixel', 'the stroke changed the export', e1.digest !== e0.digest, [e0.digest, e1.digest]);
      await K.step(ctx, 'one undo removes the whole stroke', 'undo');
      ctx.eq('pixel', 'undo restores the pre-stroke export', (await K.exportPng(ctx, 'stroke-undone')).digest, e0.digest);
      await K.step(ctx, 'redo the stroke', 'redo');
      const e2 = await K.exportPng(ctx, 'stroke-redone');
      ctx.eq('pixel', 'redo reapplies the stroke export', e2.digest, e1.digest);
      await K.roundTrip(ctx, 'brush', e2);
    } },

  { id: '2113-evaluate-vs-export-pin', title: 'the evaluated layer value agrees with the export; an edit after a pin cannot change what the pinned revision reports', tasks: ['D32', 'I16a'],
    limitations: ['The document evaluate query is an opacity-document operation (authority NativeOpacityHistory) that the object owner refuses; the evaluated value is the layer opacity sample of the object property read (rawSample view).'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx);
      await K.apply(ctx, 'create a rectangle', P.vectorCreate(P.vectorRecord({ layerUid, segments: P.rectSegments(40, 40, 200, 160), fill: RED })));
      const e0 = await K.exportPng(ctx, 'full');
      const valueAt = async (revision, pin) => {
        const b = P.propertyRead(revision, pin, layerUid, 'opacity'), res = await ctx.wire.query(b.operation, b.payload);
        return res.ok ? { ok: true, value: res.result.value[0] } : { ok: false, error: res.error, frontDoor: res.frontDoor === true };
      };
      const valueNow = async () => { const { revision, pin } = await K.pinNow(ctx); return valueAt(revision, pin); };
      const ev0 = await valueNow();
      ctx.eq('read', 'evaluate reports full opacity for the layer', [ev0.ok, ev0.value], [true, 100]);
      await K.apply(ctx, 'layer opacity to half', () => opacityBuilder(ctx, layerUid, 0.5));
      const e1 = await K.exportPng(ctx, 'half');
      const ev1 = await valueNow();
      ctx.near('read', 'evaluate reports half opacity', ev1.value, 50, 0.5);
      const ratio = probe.coverageRatio(e0, e1, probe.inkBounds(e0.image).box);
      ctx.near('pixel', 'the export coverage agrees with the evaluated value', ratio, ev1.value / 100, 0.08);
      const { revision: r1, pin: p1 } = await K.pinNow(ctx);
      await K.apply(ctx, 'edit after the pin: opacity to a quarter', () => opacityBuilder(ctx, layerUid, 0.25));
      const e2 = await K.exportPng(ctx, 'quarter');
      ctx.ok('pixel', 'the later export shows the edit', e2.digest !== e1.digest, [e1.digest, e2.digest]);
      K.exportIntact(ctx, 'the earlier export file is byte-identical on disk after the later edit and export', e1);
      const old = await valueAt(r1, p1);
      ctx.ok('read', 'the pinned revision never reports the later value (a typed stale refusal, or the old value)', old.ok ? Math.abs(old.value - 50) < 0.5 : !old.frontDoor && STALE.includes(old.error.code), old);
      const stalePin = P.historyRead(r1, p1), staleRead = await ctx.wire.query(stalePin.operation, stalePin.payload);
      ctx.ok('reply', 'a history read with the pre-edit pin is a typed stale refusal from the owner', typed(staleRead, STALE), staleRead.ok ? 'accepted' : { code: staleRead.error.code, frontDoor: staleRead.frontDoor === true }, STALE);
      const ev2 = await valueNow();
      ctx.near('read', 'the current revision evaluates to a quarter', ev2.value, 25, 0.5);
      await K.step(ctx, 'undo the edit', 'undo');
      ctx.eq('pixel', 'undo returns the export to the pinned state', (await K.exportPng(ctx, 'back')).digest, e1.digest);
      await K.roundTrip(ctx, 'evaluate', e1);
    } },
];

module.exports = { lane: '2113', issue: 2113, rows };
