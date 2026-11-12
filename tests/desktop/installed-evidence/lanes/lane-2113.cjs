'use strict';
// #2113 (D32 F16 G03a I16a G06 G17): one pinned scene for every producer, style and compositing, bitmap-brush resources, and
// evaluate versus export.
const P = require('../payloads.cjs');
const K = require('../kit.cjs');
const probe = require('../png-probe.cjs');

const RED = [0.9, 0.2, 0.2, 1], GREEN = [0.1, 0.7, 0.3, 1], TEAL = [40, 160, 200];
const MESH = { outline: [[0, 0], [0.5, 0], [1, 0], [1, 0.5], [1, 1], [0.5, 1], [0, 1], [0, 0.5]], verts: [[0, 0], [0.5, 0], [1, 0], [1, 0.5], [1, 1], [0.5, 1], [0, 1], [0, 0.5], [0.5, 0.5]],
  tris: [8, 0, 1, 8, 1, 2, 8, 2, 3, 8, 3, 4, 8, 4, 5, 8, 5, 6, 8, 6, 7, 8, 7, 0], offsets: [[0, 0], [0, 0], [0, 0], [0, 0], [0, 0], [0, 0], [0, 0], [0, 0], [0, 0]], cols: 2, rows: 2 };
const same = (image, other, points) => points.filter(([x, y]) => probe.over(image, x, y).join() !== probe.over(other, x, y).join());
const attach = (ctx, label, file, name) => K.apply(ctx, label, () => K.pinned(ctx, ({ pin, lifecycleGeneration }) => P.assetAttach({ pin, name, path: file, lifecycleGeneration })));
async function opacityBuilder(ctx, layerUid, fraction) {
  const { revision, pin } = await K.pinNow(ctx);
  const view = { view: { kind: 'properties', target: { contextId: P.ROOT, layerUid }, offset: 0, limit: 8 } };
  const meta = await ctx.wire.query('query.document.object.property.metadata', view);
  ctx.ok('reply', 'property metadata read', meta.ok, meta.ok ? 'ok' : meta.error, { revision, pin: typeof pin });
  const item = (meta.result.items || []).find(i => i.property === 'opacity');
  ctx.ok('read', 'the layer exposes an opacity property', !!item, Object.keys(meta.result));
  const base = item.defaultValue;
  return P.propertyOpacity(layerUid, Array.isArray(base) ? base.map(v => v * fraction) : base * fraction);
}

const rows = [
  { id: '2113-mixed-scene-one-export', title: 'vector with gradient and stroke, text, mesh-bound raster in one export: families, z-order by reorder, layer opacity once, mesh edit, reopen', tasks: ['D32', 'F16', 'G03a'],
    async run(ctx) {
      const { layerUid: l1 } = await K.fresh(ctx);
      await K.apply(ctx, 'create a red rectangle', P.vectorCreate(P.vectorRecord({ layerUid: l1, segments: P.rectSegments(20, 20, 160, 140), fill: RED })));
      const gradient = { fillGradient: { kind: 'linear', from: [20, 0], to: [200, 0], stops: [{ offset: 0, color: '#ff0000' }, { offset: 1, color: '#0000ff' }] } };
      await K.apply(ctx, 'create a gradient-filled, stroked rectangle', P.vectorCreate(P.vectorRecord({ layerUid: l1, segments: P.rectSegments(20, 200, 200, 280), fill: RED, stroke: [0, 0, 0, 1], width: 6, ext: gradient })));
      await K.apply(ctx, 'create text', P.textCreate(l1, P.textBlock('MIX', { size: 72, color: '#0044cc', anchorTopLeft: { x: 440, y: 20 } })));
      const image = K.stage(ctx, 'teal.png', K.solidPng(160, 100, [...TEAL, 255]));
      const att = await attach(ctx, 'attach a raster on a new layer', image, 'mix-image');
      const l2 = att.result.target.layerUid;
      const meshed = await K.apply(ctx, 'bind a mesh to the raster', P.meshAttach(att.result.target, MESH));
      const meshId = findKey(meshed.result, 'meshId');
      ctx.ok('reply', 'the mesh attach names its mesh id', typeof meshId === 'string' && meshId.length > 0, meshId);
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
      // Mesh sculpt deforms only the raster.
      await K.apply(ctx, 'sculpt one mesh vertex', P.meshSetOffset(meshId, 8, 0.3, 0.25));
      const e3 = await K.exportPng(ctx, 'sculpt');
      ctx.ok('pixel', 'the sculpt changes the export', e3.digest !== e1.digest, [e1.digest, e3.digest]);
      ctx.eq('pixel', 'the sculpt moves no vector or text pixel', same(e1.image, e3.image, sample), []);
      await K.step(ctx, 'undo the sculpt', 'undo');
      const e4 = await K.exportPng(ctx, 'sculpt-undone');
      ctx.eq('pixel', 'undo restores the pre-sculpt export', e4.digest, e1.digest);
      await K.roundTrip(ctx, 'mixed', e4);
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
      const publish = async (assetId, brushId, w, h) => {
        const built = await K.pinned(ctx, ({ pin }) => P.brushPublish(pin, assetId, brushId, w, h));
        return ctx.wire.command(built.operation, built.payload);
      };
      const before = await ctx.wire.discover();
      const first = await publish(a1.result.assetId, 'evidence-brush', 8, 8);
      ctx.ok('reply', 'first publication accepted', first.ok === true, first.ok ? 'ok' : first.error);
      ctx.eq('reply', 'publication reports the published size', [first.result.brushId, first.result.width, first.result.height], ['evidence-brush', 8, 8]);
      const row1 = await inspect('evidence-brush');
      ctx.eq('read', 'inspect shows the brush resident with the reported accounting', [row1.resident, row1.version, row1.width, row1.height, row1.bytes], [true, first.result.version, 8, 8, first.result.bytes]);
      ctx.within('read', 'resident bytes are the decoded size of an 8x8 RGBA tip (plus bounded overhead)', row1.bytes, 256, 4096);
      ctx.eq('read', 'an unpublished brush is not resident', (await inspect('never-published')).resident, false);
      const second = await publish(a2.result.assetId, 'evidence-brush', 16, 16);
      ctx.ok('reply', 'replacement accepted', second.ok === true, second.ok ? 'ok' : second.error);
      const row2 = await inspect('evidence-brush');
      ctx.eq('read', 'replacement keeps one brush with the new size and accounting', [row2.resident, row2.width, row2.height, row2.bytes], [true, 16, 16, second.result.bytes]);
      ctx.ok('read', 'replacement advances the version and the bytes grow with the pixels', row2.version !== row1.version && row2.bytes > row1.bytes, { v1: row1.version, v2: row2.version, b1: row1.bytes, b2: row2.bytes });
      // Refusals publish nothing and change nothing.
      const mark = await ctx.wire.discover();
      const refusedSize = await publish(a2.result.assetId, 'evidence-brush', 5000, 5000);
      ctx.eq('reply', 'an oversize publication is refused', refusedSize.ok, false);
      const refusedAsset = await publish('asset-that-does-not-exist', 'other-brush', 8, 8);
      ctx.eq('reply', 'an unknown asset is refused', refusedAsset.ok, false);
      const after = await ctx.wire.discover();
      ctx.eq('read', 'refusals leave the revision as it was', after.contentRevision, mark.contentRevision);
      ctx.eq('read', 'refusals leave the resident brush as it was', await inspect('evidence-brush'), row2);
      ctx.eq('read', 'a refused publication created no brush', (await inspect('other-brush')).resident, false);
      ctx.ok('read', 'publication is a resource, not an edit', before.contentRevision <= mark.contentRevision, [before.contentRevision, mark.contentRevision]);
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

  { id: '2113-evaluate-vs-export-pin', title: 'evaluate agrees with the export; an edit after a pin cannot change what the pinned revision reports', tasks: ['D32', 'I16a'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx);
      await K.apply(ctx, 'create a rectangle', P.vectorCreate(P.vectorRecord({ layerUid, segments: P.rectSegments(40, 40, 200, 160), fill: RED })));
      const e0 = await K.exportPng(ctx, 'full');
      const valueAt = async (revision) => {
        const b = P.evaluate(revision), res = await ctx.wire.query(b.operation, b.payload);
        return res.ok ? { ok: true, value: (res.result.layers.find(l => l.layerUid === layerUid) || {}).value, contentRevision: res.result.contentRevision } : { ok: false, error: res.error };
      };
      const now0 = await ctx.wire.discover(), ev0 = await valueAt(now0.contentRevision);
      ctx.eq('read', 'evaluate reports full opacity for the layer', [ev0.ok, ev0.value, ev0.contentRevision], [true, 100, now0.contentRevision]);
      await K.apply(ctx, 'layer opacity to half', () => opacityBuilder(ctx, layerUid, 0.5));
      const e1 = await K.exportPng(ctx, 'half');
      const pinned = await ctx.wire.discover(), ev1 = await valueAt(pinned.contentRevision);
      ctx.near('read', 'evaluate reports half opacity', ev1.value, 50, 0.5);
      const ratio = probe.coverageRatio(e0, e1, probe.inkBounds(e0.image).box);
      ctx.near('pixel', 'the export coverage agrees with the evaluated value', ratio, ev1.value / 100, 0.08);
      const { revision: r1, pin: p1 } = await K.pinNow(ctx);
      await K.apply(ctx, 'edit after the pin: opacity to a quarter', () => opacityBuilder(ctx, layerUid, 0.25));
      const e2 = await K.exportPng(ctx, 'quarter');
      ctx.ok('pixel', 'the later export shows the edit, the earlier export bytes are untouched', e2.digest !== e1.digest && probe.pixels(e1.image) === e1.digest, [e1.digest, e2.digest]);
      const old = await valueAt(r1);
      ctx.ok('read', 'the pinned revision never reports the later value (refused or the old value)', old.ok ? Math.abs(old.value - 50) < 0.5 : typeof old.error.code === 'string', old);
      const stalePin = P.historyRead(r1, p1), staleRead = await ctx.wire.query(stalePin.operation, stalePin.payload);
      ctx.eq('reply', 'a history read with the pre-edit pin is refused', staleRead.ok, false);
      const current = await ctx.wire.discover(), ev2 = await valueAt(current.contentRevision);
      ctx.near('read', 'the current revision evaluates to a quarter', ev2.value, 25, 0.5);
      await K.step(ctx, 'undo the edit', 'undo');
      ctx.eq('pixel', 'undo returns the export to the pinned state', (await K.exportPng(ctx, 'back')).digest, e1.digest);
      await K.roundTrip(ctx, 'evaluate', e1);
    } },
];

function findKey(value, key) {
  if (Array.isArray(value)) { for (const item of value) { const found = findKey(item, key); if (found !== undefined) return found; } return undefined; }
  if (value && typeof value === 'object') {
    if (typeof value[key] === 'string') return value[key];
    for (const item of Object.values(value)) { const found = findKey(item, key); if (found !== undefined) return found; }
  }
  return undefined;
}

module.exports = { lane: '2113', issue: 2113, rows };
