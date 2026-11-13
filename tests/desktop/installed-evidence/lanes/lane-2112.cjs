'use strict';
// #2112 (G01 G02 G27 G38a G38b): raster and text assets, immutable identity, dedup, refusals without partial publication,
// retained text and glyph layout. Font fetching and font bytes are another lane's provider; here an absent font is
// asserted as a refusal, never a fallback.
const fs = require('node:fs');
const P = require('../payloads.cjs');
const K = require('../kit.cjs');
const probe = require('../png-probe.cjs');

const TEAL = [40, 160, 200], CC2200 = [0xcc, 0x22, 0x00];
const attach = (ctx, label, file, name, opts) => K.apply(ctx, label, () => K.pinned(ctx, ({ pin, lifecycleGeneration }) => P.assetAttach({ pin, name, path: file, lifecycleGeneration })), opts);
const width = bounds => bounds.box[2] - bounds.box[0] + 1, height = bounds => bounds.box[3] - bounds.box[1] + 1;
async function savedRaster(ctx, tag) {
  const saved = await K.saveAs(ctx, tag), list = (saved.document.drawing && saved.document.drawing.items && saved.document.drawing.items.raster) || [];
  return { saved, items: list };
}

const rows = [
  { id: '2112-asset-attach-dedup-refusals', title: 'PNG asset attach, stable identity, dedup, corrupt, oversize and missing sources refused without a revision', tasks: ['G01', 'G02'],
    async run(ctx) {
      await K.fresh(ctx, { transparent: true });
      const file = K.stage(ctx, 'teal.png', K.solidPng(160, 100, [...TEAL, 255]));
      const first = await attach(ctx, 'attach a PNG as a new layer', file, 'evidence-image');
      const r = first.result;
      ctx.eq('reply', 'natural size is the file size', [r.naturalWidth, r.naturalHeight], [160, 100]);
      ctx.ok('reply', 'an immutable asset id and a placement are reported', typeof r.assetId === 'string' && r.assetId.length > 0 && !!r.target && r.newLayer === true, { assetId: r.assetId, newLayer: r.newLayer });
      const e0 = await K.exportPng(ctx, 'attached'), ink = probe.inkBounds(e0.image);
      ctx.ok('pixel', 'the raster is on the canvas', ink !== null, ink);
      ctx.near('pixel', 'its ink size is the reported display size', [width(ink), height(ink)], [r.displayWidth, r.displayHeight], 3);
      ctx.ok('pixel', 'its colour is the file colour', probe.countColor(e0.image, TEAL, { box: ink.box, tol: 10 }) > 0.9 * width(ink) * height(ink), probe.countColor(e0.image, TEAL, { box: ink.box, tol: 10 }));
      const second = await attach(ctx, 'attach the same bytes again under another name', file, 'evidence-image-copy');
      ctx.eq('reply', 'identical bytes resolve to the same asset id', second.result.assetId, r.assetId);
      const { saved, items } = await savedRaster(ctx, 'two-placements');
      ctx.eq('file', 'the catalog holds one asset for two placements', [Array.isArray(saved.document.assets) ? saved.document.assets.length : null, items.length], [1, 2]);
      // Negative controls: no revision, no history, no partial publication. Each is a typed refusal by the native owner.
      await attach(ctx, 'attach a corrupt PNG', K.stage(ctx, 'corrupt.png', K.corruptPng()), 'corrupt', { refused: true });
      // A VALID PNG one pixel wider than the admitted 8192: only a size cap refuses it (it is not a decode failure). The owner answers
      // `unavailable` for it (native-engine/src/object_raster_asset_prepare.rs:86, Refusal::ImageTooLarge); a corrupt PNG is `invalid_request` (:84).
      await attach(ctx, 'attach a decodable PNG wider than the admitted dimension', K.stage(ctx, 'oversize.png', K.oversizePng()), 'oversize', { refused: true, codes: ['unavailable'] });
      // A missing source is the host's typed "source unavailable", not dispatched: the attach operation itself answered above.
      await attach(ctx, 'attach a missing file', ctx.work + '/does-not-exist.png', 'missing', { refused: true, codes: ['unavailable'],
        details: error => !!error.details && error.details.disposition === 'notDispatched' && error.details.committed === false && /image source/.test(String(error.message)) });
      // Removing an asset the catalog does not hold is a no-op by contract (not a refusal): no revision, no history.
      await K.apply(ctx, 'remove an unknown asset', () => K.pinned(ctx, ({ pin }) => P.assetRemove(pin, 'asset-that-does-not-exist')), { noop: true });
      const after = await savedRaster(ctx, 'after-refusals');
      ctx.eq('file', 'refusals left the catalog and placements as they were', [after.saved.document.assets.length, after.items.length], [1, 2]);
      ctx.eq('pixel', 'refusals left the export untouched', (await K.exportPng(ctx, 'after-refusals')).digest, e0.digest);
    } },

  { id: '2112-asset-remove-lifetime', title: 'asset removal and resource lifetime: an asset in use is refused, an unreferenced one is removed in one revision, undo and redo restore it, an absent one is a no-op', tasks: ['G01'],
    async run(ctx) {
      await K.fresh(ctx, { transparent: true });
      const file = K.stage(ctx, 'teal.png', K.solidPng(160, 100, [...TEAL, 255]));
      const made = await attach(ctx, 'attach a PNG as a new layer', file, 'evidence-image');
      const { assetId } = made.result, layer = made.result.target.layerUid;
      const removeIt = (label, opts) => K.apply(ctx, label, () => K.pinned(ctx, ({ pin }) => P.assetRemove(pin, assetId)), opts);
      const catalog = async tag => (await K.saveAs(ctx, tag)).document.assets.map(a => a.id || a.assetId || a.name);
      const e0 = await K.exportPng(ctx, 'attached');
      ctx.eq('file', 'the catalog holds the asset', (await catalog('held')).length, 1);
      await removeIt('remove the asset while a placement uses it', { refused: true });
      ctx.eq('file', 'a refused removal left the asset in the catalog', (await catalog('in-use')).length, 1);
      await K.apply(ctx, 'delete the placement', P.rasterDelete(made.result.target));
      ctx.eq('pixel', 'the deleted placement leaves no ink', probe.inkBounds((await K.exportPng(ctx, 'no-placement')).image), null);
      await removeIt('remove the now unreferenced asset');
      ctx.eq('file', 'the removed asset is gone from the catalog', (await catalog('removed')).length, 0);
      await K.step(ctx, 'undo the removal', 'undo');
      ctx.eq('file', 'undo puts the asset back in the catalog', (await catalog('restored')).length, 1);
      await K.step(ctx, 'redo the removal', 'redo');
      ctx.eq('file', 'redo removes it again', (await catalog('removed-again')).length, 0);
      await K.step(ctx, 'undo the removal again', 'undo');
      await K.step(ctx, 'undo the placement delete', 'undo');
      ctx.eq('pixel', 'the restored placement paints the original export', (await K.exportPng(ctx, 'restored')).digest, e0.digest);
      await removeIt('remove the asset while the restored placement uses it', { refused: true });
      await K.apply(ctx, 'remove an asset id the catalog never held', () => K.pinned(ctx, ({ pin }) => P.assetRemove(pin, 'asset-never-held')), { noop: true });
      await K.roundTrip(ctx, 'lifetime', e0);
    } },

  // The layer that holds a raster placement. Split out of the lifetime row because the installed owner refuses it: layer.delete answers
  // invalid_request "Layer lifecycle payload is invalid or target is absent." for a layer holding a raster item, while a vector layer deletes.
  { id: '2112-raster-layer-delete', title: 'deleting the layer that holds a raster placement removes the placement and one undo restores it', tasks: ['G02'],
    async run(ctx) {
      await K.fresh(ctx, { transparent: true });
      const file = K.stage(ctx, 'teal.png', K.solidPng(160, 100, [...TEAL, 255]));
      const made = await attach(ctx, 'attach a PNG as a new layer', file, 'evidence-image');
      const layer = made.result.target.layerUid;
      const e0 = await K.exportPng(ctx, 'with-placement');
      ctx.ok('pixel', 'the placement is on the canvas', probe.inkBounds(e0.image) !== null, probe.inkBounds(e0.image));
      await K.apply(ctx, 'delete the layer that holds the placement', P.layerDelete(layer));
      ctx.eq('pixel', 'the deleted layer leaves no ink', probe.inkBounds((await K.exportPng(ctx, 'no-layer')).image), null);
      await K.step(ctx, 'undo the layer delete', 'undo');
      ctx.eq('pixel', 'undo restores the placement export', (await K.exportPng(ctx, 'restored')).digest, e0.digest);
    } },

  { id: '2112-raster-edit-save-reopen', title: 'raster move, resize, rotate, opacity measured in pixels; asset bytes survive save and reopen without the staging file', tasks: ['G02'],
    async run(ctx) {
      await K.fresh(ctx, { transparent: true });
      const file = K.stage(ctx, 'teal.png', K.solidPng(160, 100, [...TEAL, 255]));
      const made = await attach(ctx, 'attach a PNG', file, 'evidence-image');
      const tgt = made.result.target;
      const e0 = await K.exportPng(ctx, 'r0'), b0 = probe.inkBounds(e0.image);
      const { items } = await savedRaster(ctx, 'r0');
      const item = items[0].item;
      ctx.ok('file', 'the saved raster item carries its placement', ['x', 'y', 'width', 'height'].every(key => typeof item[key] === 'number'), item);
      await K.apply(ctx, 'move by (50, -10)', P.rasterMove(tgt, item.x + 50, item.y - 10));
      const e1 = await K.exportPng(ctx, 'r1'), b1 = probe.inkBounds(e1.image);
      ctx.near('pixel', 'the ink moved by the same amount', [b1.box[0] - b0.box[0], b1.box[1] - b0.box[1]], [50, -10], 2);
      ctx.near('pixel', 'its size did not change', [width(b1), height(b1)], [width(b0), height(b0)], 1);
      await K.apply(ctx, 'resize to twice the width', P.rasterResize(tgt, item.width * 2, item.height));
      const e2 = await K.exportPng(ctx, 'r2'), b2 = probe.inkBounds(e2.image);
      ctx.near('pixel', 'ink width doubled, height kept', [width(b2), height(b2)], [width(b0) * 2, height(b0)], 3);
      await K.apply(ctx, 'rotate by 90 degrees', P.rasterRotate(tgt, 90));
      const e3 = await K.exportPng(ctx, 'r3'), b3 = probe.inkBounds(e3.image);
      ctx.near('pixel', 'the rotated ink has swapped extents', [width(b3), height(b3)], [height(b2), width(b2)], 6);
      await K.step(ctx, 'undo the rotation', 'undo');
      ctx.eq('pixel', 'undo restores the resized export', (await K.exportPng(ctx, 'r3-undone')).digest, e2.digest);
      await K.apply(ctx, 'set opacity to 0.5', P.rasterOpacity(tgt, 0.5));
      const e4 = await K.exportPng(ctx, 'r4');
      const ratio = probe.coverageRatio(e2.image, e4.image, probe.inkBounds(e2.image).box);
      ctx.near('pixel', 'opacity is applied once: half the coverage', ratio, 0.5, 0.08);
      await K.apply(ctx, 'raster move on a stale target', P.rasterMove(P.target(tgt.layerUid, 'native-stroke:99999:9'), 1, 1), { refused: true });
      fs.rmSync(file);
      const trip = await K.roundTrip(ctx, 'raster', e4);
      const kept = (trip.first.document.assets || []).length;
      ctx.eq('file', 'the saved project owns the asset: one catalog entry without the staging file', kept, 1);
      ctx.eq('pixel', 'opacity and geometry persist: the reopened export is pixel-equal', trip.exported.digest, e4.digest);
    } },

  { id: '2112-text-layout-edit-reopen', title: 'retained text: stored block, text read, ink bounds, size and spacing, undo, delete, absent font refused, reopen', tasks: ['G27', 'G38a'],
    limitations: ['No public read returns glyph or layout boxes; ink bounds are compared with ranges derived from the declared size, not with a layout box.'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx, { w: 320, h: 180, transparent: true });
      const block = P.textBlock('HELLO', { size: 72, color: '#cc2200', anchorTopLeft: { x: 20, y: 30 } });
      const created = await K.apply(ctx, 'create a text block', P.textCreate(layerUid, block));
      const textTarget = P.target(layerUid, created.result.strokeId);
      const { revision, pin } = await K.pinNow(ctx);
      const list = await ctx.wire.query(P.textTargets(revision, pin, layerUid).operation, P.textTargets(revision, pin, layerUid).payload);
      ctx.ok('read', 'the text read lists one target', list.ok && list.result.items.length === 1 && list.result.total === 1, list.ok ? list.result.total : list.error);
      const listed = list.result.items[0].target;
      const inspect = await ctx.wire.query(P.textInspect(revision, pin, listed).operation, P.textInspect(revision, pin, listed).payload);
      ctx.ok('read', 'inspect answers', inspect.ok, inspect.ok ? 'ok' : inspect.error);
      const meta = inspect.result.block;
      ctx.eq('read', 'stored block fields are the ones sent', [meta.font, meta.size, meta.color, meta.align, meta.anchorTopLeft], [block.font, 72, '#cc2200', 'left', { x: 20, y: 30 }]);
      const body = await ctx.wire.query(P.textBody(revision, pin, listed).operation, P.textBody(revision, pin, listed).payload);
      ctx.eq('read', 'the text body is the text sent', body.ok && body.result.text, 'HELLO');
      const e0 = await K.exportPng(ctx, 't0'), b0 = probe.inkBounds(e0.image);
      ctx.ok('pixel', 'the text paints', b0 !== null, b0);
      ctx.ok('pixel', 'ink starts at the anchor', b0.box[0] >= 14 && b0.box[0] <= 40 && b0.box[1] >= 24 && b0.box[1] <= 70, b0.box, 'x 14..40, y 24..70');
      ctx.within('pixel', 'five glyphs at 72 px are 150..270 px wide', width(b0), 150, 270);
      ctx.within('pixel', 'cap height at 72 px is 40..70 px', height(b0), 40, 70);
      ctx.ok('pixel', 'the ink has the text colour', probe.countColor(e0.image, CC2200, { tol: 40 }) > 200, probe.countColor(e0.image, CC2200, { tol: 40 }));
      ctx.eq('pixel', 'layout is deterministic: a second export is pixel-equal', (await K.exportPng(ctx, 't0-again')).digest, e0.digest);
      await K.apply(ctx, 'edit the size to 36', P.textEdit(textTarget, { size: 36 }));
      const b1 = probe.inkBounds((await K.exportPng(ctx, 't1')).image);
      ctx.near('pixel', 'half the size gives half the ink extent', [width(b1) / width(b0), height(b1) / height(b0)], [0.5, 0.5], 0.1);
      await K.step(ctx, 'undo the size edit', 'undo');
      ctx.eq('pixel', 'undo restores the original export', (await K.exportPng(ctx, 't1-undone')).digest, e0.digest);
      await K.step(ctx, 'redo the size edit', 'redo');
      await K.apply(ctx, 'edit the size back and add letter spacing', P.textEdit(textTarget, { size: 72, letterSpacing: 10 }));
      const b2 = probe.inkBounds((await K.exportPng(ctx, 't2')).image);
      ctx.within('pixel', 'four gaps of 10 px widen the ink by about 40', width(b2) - width(b0), 30, 60);
      const e2 = await K.exportPng(ctx, 't2-final');
      const trip = await K.roundTrip(ctx, 'text', e2);
      const again = await ctx.wire.discover();
      const pin2 = await K.pinNow(ctx);
      const relisted = await ctx.wire.query(P.textTargets(pin2.revision, pin2.pin, layerUid).operation, P.textTargets(pin2.revision, pin2.pin, layerUid).payload);
      ctx.ok('read', 'the text target is listed after reopen', relisted.ok && relisted.result.total === 1, relisted.ok ? relisted.result.total : relisted.error, { revisionAfterOpen: again.contentRevision });
      ctx.ok('file', 'the saved project holds the text', JSON.stringify(trip.first.document).includes('HELLO'), 'text present');
      // Absent font: refused, nothing written, nothing falls back. This project holds no asset catalog, and the owner answers `unavailable`
      // for that (native-engine/src/object_raster_asset_record.rs:151-152 NoInventory, mapped in object_raster_asset_commands.rs:189);
      // `not_found` (MissingFont, :190) is only the answer when a catalog exists and lacks the font.
      await K.apply(ctx, 'create text with an absent asset font', P.textCreate(layerUid, P.textBlock('X', { font: { kind: 'asset', assetId: 'font-that-is-not-in-the-project' } })), { refused: true, codes: ['unavailable'] });
      await K.apply(ctx, 'delete the text', P.textDelete(P.target(layerUid, relisted.result.items[0].target.strokeId)));
      ctx.eq('pixel', 'deleting the text leaves no ink', probe.inkBounds((await K.exportPng(ctx, 't3')).image), null);
      await K.step(ctx, 'undo the delete', 'undo');
      ctx.eq('pixel', 'undo brings the text back exactly', (await K.exportPng(ctx, 't3-undone')).digest, e2.digest);
    } },
];

module.exports = { lane: '2112', issue: 2112, rows };
