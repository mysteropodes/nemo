'use strict';
// #2108 (G24 G18 G19 F07 F08 F15): groups, boolean and eraser joins, layer lifecycle, selection and clipboard remap.
const P = require('../payloads.cjs');
const K = require('../kit.cjs');
const probe = require('../png-probe.cjs');

const RED = [0.9, 0.2, 0.2, 1], BLUE = [0, 0.27, 0.8, 1], GREEN = [0.1, 0.6, 0.2, 1];
const rect = (layerUid, x0, y0, x1, y1, fill) => ({ layerUid, segments: P.rectSegments(x0, y0, x1, y1), fill });
async function stroke(ctx, label, spec) {
  const made = await K.apply(ctx, label, P.vectorCreate(P.vectorRecord(spec)));
  ctx.ok('reply', label + ': stroke id allocated', typeof made.result.strokeId === 'string' && made.result.strokeId.length > 0, made.result.strokeId);
  return made.result.strokeId;
}
function collect(value, key, out = []) {
  if (Array.isArray(value)) value.forEach(item => collect(item, key, out));
  else if (value && typeof value === 'object') for (const [k, v] of Object.entries(value)) { if (k === key && typeof v === 'string') out.push(v); collect(v, key, out); }
  return out;
}
// Flatten, observed: the saved project before and after. The members and the group id are consumed and at least one result
// record replaces them, everything else is untouched. (A revision and a history entry alone do not show a flatten.)
async function flattenGroup(ctx, layerUid, groupId, members) {
  const before = await K.saveAs(ctx, 'pre-flatten'), idsBefore = K.objectStrokeIds(before.document);
  ctx.ok('file', 'before the flatten the group and its members exist', members.every(id => idsBefore.includes(id)) && collect(before.document, 'groupId').includes(groupId), { idsBefore, groups: collect(before.document, 'groupId') });
  await K.apply(ctx, 'flatten the group', P.booleanFlatten(layerUid, groupId));
  const after = await K.saveAs(ctx, 'post-flatten'), idsAfter = K.objectStrokeIds(after.document);
  ctx.eq('file', 'flatten consumed the members', members.filter(id => idsAfter.includes(id)), []);
  ctx.eq('file', 'flatten consumed the group id', collect(after.document, 'groupId').includes(groupId), false);
  const results = idsAfter.filter(id => !idsBefore.includes(id));
  ctx.ok('file', 'at least one flattened result replaces the members', results.length >= 1, results, '>=1');
  ctx.eq('file', 'no other record changed', idsAfter.filter(id => !results.includes(id)).sort(), idsBefore.filter(id => !members.includes(id)).sort());
  return { results };
}
// Ink is distance from the opaque canvas background (see png-probe.cjs): inked is clearly off-background, clear is background.
const inked = (image, x, y) => probe.distanceAt(image, x, y) > 64;
const clear = (image, x, y) => probe.distanceAt(image, x, y) < 16;

const rows = [
  { id: '2108-group-combine-flatten', title: 'group, combine mode, rename, flatten consumes the group with one undo, ungroup', tasks: ['G24'],
    limitations: ['One flat group on Animation 2D records: nested groups, group reorder and Motion flatten are listed under coverage.notCovered.'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx);
      const a = await stroke(ctx, 'create outer rectangle', rect(layerUid, 100, 60, 300, 240, RED));
      const b = await stroke(ctx, 'create inner rectangle', rect(layerUid, 160, 100, 240, 200, BLUE));
      const ra = await K.readObject(ctx, P.target(layerUid, a)), rb = await K.readObject(ctx, P.target(layerUid, b));
      const e0 = await K.exportPng(ctx, 'plain');
      ctx.near('pixel', 'inner rectangle paints over the outer one', probe.over(e0.image, 200, 150), [0, 69, 204], 10);
      await K.apply(ctx, 'group the two rectangles', P.groupCreate(layerUid, 'grp_evidence_a', 'Frame', [a, b]));
      const grouped = await K.saveAs(ctx, 'grouped');
      ctx.ok('file', 'saved project records the group id', collect(grouped.document, 'groupId').includes('grp_evidence_a'), collect(grouped.document, 'groupId'));
      await K.apply(ctx, 'rename the group', P.groupRename(layerUid, 'grp_evidence_a', 'Frame renamed'));
      ctx.ok('file', 'saved project carries the new group name', JSON.stringify((await K.saveAs(ctx, 'renamed')).document).includes('Frame renamed'), 'name present');
      await K.apply(ctx, 'combine by subtract', P.groupCombine(layerUid, 'grp_evidence_a', 'subtract', [a, b]));
      const e1 = await K.exportPng(ctx, 'subtract');
      ctx.ok('pixel', 'the inner area is a hole', clear(e1.image, 200, 150), probe.distanceAt(e1.image, 200, 150), '<16');
      ctx.near('pixel', 'the ring keeps the outer colour', probe.over(e1.image, 120, 80), [230, 51, 51], 8);
      ctx.ok('pixel', 'combine changes the export', e1.digest !== e0.digest, [e0.digest, e1.digest]);
      await K.apply(ctx, 'combine mode back to none', P.groupMode(layerUid, 'grp_evidence_a', 'none'));
      ctx.eq('pixel', 'mode none restores the plain export', (await K.exportPng(ctx, 'none')).digest, e0.digest);
      await K.apply(ctx, 'combine mode subtract again', P.groupMode(layerUid, 'grp_evidence_a', 'subtract'));
      const e2 = await K.exportPng(ctx, 'subtract-again');
      ctx.eq('pixel', 'subtract mode is deterministic', e2.digest, e1.digest);
      await flattenGroup(ctx, layerUid, 'grp_evidence_a', [a, b]);
      const e3 = await K.exportPng(ctx, 'flattened');
      ctx.eq('pixel', 'flatten keeps the combined pixels', e3.digest, e1.digest);
      await K.step(ctx, 'one undo of flatten', 'undo');
      const back = await Promise.all([K.readObject(ctx, P.target(layerUid, a)), K.readObject(ctx, P.target(layerUid, b))]);
      ctx.eq('read', 'one undo restores both original records', back.map(r => r.ok && r.object), [ra.object, rb.object]);
      ctx.eq('pixel', 'one undo restores the combined export', (await K.exportPng(ctx, 'unflattened')).digest, e1.digest);
      await K.apply(ctx, 'ungroup', P.groupUngroup(layerUid, [a, b]));
      ctx.eq('pixel', 'ungrouped shapes paint as plain shapes again', (await K.exportPng(ctx, 'ungrouped')).digest, e0.digest);
      await K.apply(ctx, 'group rename on a missing group', P.groupRename(layerUid, 'grp_missing', 'x'), { refused: true });
    } },

  { id: '2108-boolean-apply-erase', title: 'boolean subtract keeps hole and every disjoint island; eraser splits a shape; undo restores', tasks: ['G18', 'G19'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx);
      const a = await stroke(ctx, 'create outer rectangle', rect(layerUid, 100, 60, 300, 240, RED));
      const b = await stroke(ctx, 'create inner rectangle', rect(layerUid, 160, 100, 240, 200, BLUE));
      const e0 = await K.exportPng(ctx, 'before');
      await K.apply(ctx, 'subtract inner from outer', P.booleanApply(layerUid, 'subtract', [a, b]));
      const e1 = await K.exportPng(ctx, 'hole');
      ctx.ok('pixel', 'the hole is empty', clear(e1.image, 200, 150), probe.distanceAt(e1.image, 200, 150), '<16');
      ctx.ok('pixel', 'the ring is inked on all four sides', [[130, 150], [270, 150], [200, 80], [200, 220]].every(([x, y]) => inked(e1.image, x, y)), 'four probes');
      ctx.near('pixel', 'outer bounds unchanged', probe.inkBounds(e1.image).box, [100, 60, 300, 240], 2);
      await K.step(ctx, 'one undo restores both operands', 'undo');
      ctx.eq('pixel', 'undo restores the exact original export', (await K.exportPng(ctx, 'after-undo')).digest, e0.digest);
      await K.step(ctx, 'redo the boolean', 'redo');
      ctx.eq('pixel', 'redo reapplies the hole', (await K.exportPng(ctx, 'after-redo')).digest, e1.digest);
      await K.step(ctx, 'undo again', 'undo');
      // Eraser: a footprint across the middle of a stroke splits it in two.
      await K.apply(ctx, 'erase a vertical band', P.booleanErase(layerUid, a, [[190, 40], [210, 40], [210, 260], [190, 260]]));
      const e3 = await K.exportPng(ctx, 'erased');
      ctx.ok('pixel', 'both halves survive the erase', inked(e3.image, 130, 150) && inked(e3.image, 270, 150), [probe.distanceAt(e3.image, 130, 150), probe.distanceAt(e3.image, 270, 150)], '>64');
      ctx.ok('pixel', 'the erased band is empty where only the erased shape was', clear(e3.image, 200, 80), probe.distanceAt(e3.image, 200, 80), '<16');
      await K.step(ctx, 'undo the erase', 'undo');
      const e4 = await K.exportPng(ctx, 'erase-undone');
      ctx.ok('pixel', 'undo of the erase restores the band', inked(e4.image, 200, 80), probe.distanceAt(e4.image, 200, 80), '>64');
      // Disjoint islands: a bar crossing the whole rectangle leaves two pieces, both must survive.
      const bar = await stroke(ctx, 'create a crossing bar', rect(layerUid, 180, 30, 220, 270, GREEN));
      await K.apply(ctx, 'subtract the bar', P.booleanApply(layerUid, 'subtract', [a, bar]));
      const e2 = await K.exportPng(ctx, 'islands');
      ctx.ok('pixel', 'left island kept', inked(e2.image, 130, 150), probe.distanceAt(e2.image, 130, 150), '>64');
      ctx.ok('pixel', 'right island kept', inked(e2.image, 270, 150), probe.distanceAt(e2.image, 270, 150), '>64');
      ctx.ok('pixel', 'the cut is empty above the inner rectangle', clear(e2.image, 200, 80), probe.distanceAt(e2.image, 200, 80), '<16');
      await K.step(ctx, 'undo the island cut', 'undo');
      await K.apply(ctx, 'boolean on a stale operand', P.booleanApply(layerUid, 'unite', [a, 'native-stroke:99999:9']), { refused: true });
      await K.apply(ctx, 'erase a stale stroke', P.booleanErase(layerUid, 'native-stroke:99999:9', [[0, 0], [10, 0], [10, 10]]), { refused: true });
    } },

  { id: '2108-layer-lifecycle', title: 'layer create, rename, duplicate, visibility, reorder, split, delete: each one revision, one undo', tasks: ['F07'],
    async run(ctx) {
      const { layerUid: l1 } = await K.fresh(ctx);
      await stroke(ctx, 'create artwork on layer 1', rect(l1, 40, 40, 140, 140, RED));
      await stroke(ctx, 'create a second shape on layer 1', rect(l1, 200, 40, 300, 140, BLUE));
      const base = await K.exportPng(ctx, 'base');
      const order0 = await K.layerOrder(ctx);
      const created = await K.apply(ctx, 'create a layer', P.layerCreate('Second'));
      const l2 = created.result.layerUid;
      ctx.eq('read', 'layer order lists the new layer', (await K.layerOrder(ctx)).length, order0.length + 1);
      await K.apply(ctx, 'rename the layer', P.layerRename(l2, 'Renamed Evidence'));
      ctx.ok('file', 'saved project carries the new layer name', JSON.stringify((await K.saveAs(ctx, 'named')).document).includes('Renamed Evidence'), 'name present');
      await K.apply(ctx, 'duplicate layer 1', P.layerDuplicate(l1));
      const order1 = await K.layerOrder(ctx);
      const dup = order1.find(id => !order0.includes(id) && id !== l2);
      ctx.ok('read', 'a new layer id appears for the duplicate', typeof dup === 'string', { order0, order1 });
      ctx.eq('pixel', 'a duplicate on top of identical artwork exports the same pixels', (await K.exportPng(ctx, 'dup')).digest, base.digest);
      await K.apply(ctx, 'hide layer 1', P.layerVisibility(l1, false));
      ctx.eq('pixel', 'the duplicate still draws when the original is hidden', (await K.exportPng(ctx, 'hide-one')).digest, base.digest);
      await K.apply(ctx, 'hide the duplicate too', P.layerVisibility(dup, false));
      ctx.eq('pixel', 'both hidden: no ink', probe.inkBounds((await K.exportPng(ctx, 'hide-both')).image), null);
      await K.step(ctx, 'undo hiding the duplicate', 'undo');
      ctx.eq('pixel', 'undo shows the duplicate again', (await K.exportPng(ctx, 'unhide')).digest, base.digest);
      await K.apply(ctx, 'show layer 1', P.layerVisibility(l1, true));
      await K.apply(ctx, 'reorder the duplicate to the first gap', P.layerReorder([dup], 0, dup));
      const order2 = await K.layerOrder(ctx);
      ctx.ok('read', 'layer order changed', JSON.stringify(order2) !== JSON.stringify(order1), { order1, order2 });
      ctx.eq('read', 'reorder only permutes the layers', [...order2].sort(), [...order1].sort());
      await K.step(ctx, 'undo the reorder', 'undo');
      ctx.eq('read', 'undo restores the previous order', await K.layerOrder(ctx), order1);
      await K.apply(ctx, 'split layer 1 into shape layers', P.layerSplit(l1));
      ctx.ok('read', 'split adds layers', (await K.layerOrder(ctx)).length > order1.length, (await K.layerOrder(ctx)).length, '>' + order1.length);
      ctx.eq('pixel', 'split keeps the pixels', (await K.exportPng(ctx, 'split')).digest, base.digest);
      await K.step(ctx, 'undo the split', 'undo');
      ctx.eq('read', 'undo of the split restores the layer list', await K.layerOrder(ctx), order1);
      await K.apply(ctx, 'delete the duplicate', P.layerDelete(dup));
      ctx.eq('read', 'the duplicate is gone', (await K.layerOrder(ctx)).includes(dup), false);
      ctx.eq('pixel', 'artwork unchanged after deleting the duplicate', (await K.exportPng(ctx, 'deleted')).digest, base.digest);
      await K.step(ctx, 'undo the delete', 'undo');
      ctx.ok('read', 'undo brings the duplicate back', (await K.layerOrder(ctx)).includes(dup), 'dup restored');
      await K.apply(ctx, 'rename a missing layer', P.layerRename('layer-missing', 'x'), { refused: true });
      await K.apply(ctx, 'delete a missing layer', P.layerDelete('layer-missing'), { refused: true });
    } },

  { id: '2108-selection-clipboard-remap', title: 'selection duplicate and delete, copy and paste remap group and stroke ids atomically, cut, locked layer refuses', tasks: ['F08', 'F15'],
    async run(ctx) {
      const { layerUid: l1 } = await K.fresh(ctx);
      const a = await stroke(ctx, 'create rectangle A', rect(l1, 40, 40, 140, 140, RED));
      const b = await stroke(ctx, 'create rectangle B', rect(l1, 200, 40, 300, 140, BLUE));
      const known = new Set([a, b]);
      const base = await K.exportPng(ctx, 'base');
      await K.apply(ctx, 'duplicate A with an offset', P.selectionDuplicate(l1, [a], [0, 200]));
      const dupSave = await K.saveAs(ctx, 'dup');
      const fresh = K.objectStrokeIds(dupSave.document).filter(id => !known.has(id));
      ctx.eq('file', 'exactly one new stroke exists', fresh.length, 1);
      const dupExport = await K.exportPng(ctx, 'dup');
      ctx.near('pixel', 'the copy paints at the offset', probe.over(dupExport.image, 90, 290), [230, 51, 51], 8);
      await K.step(ctx, 'undo the duplicate', 'undo');
      ctx.eq('pixel', 'undo removes the copy', (await K.exportPng(ctx, 'dup-undone')).digest, base.digest);
      await K.step(ctx, 'redo the duplicate', 'redo');
      await K.apply(ctx, 'delete the copy', P.selectionDelete(l1, fresh));
      ctx.eq('pixel', 'deleting the copy restores the base export', (await K.exportPng(ctx, 'copy-deleted')).digest, base.digest);
      await K.step(ctx, 'undo the delete', 'undo');
      await K.apply(ctx, 'delete the copy again for a clean scene', P.selectionDelete(l1, fresh));
      // Group, copy and paste: ids must be remapped, as one transaction.
      await K.apply(ctx, 'group A and B', P.groupCreate(l1, 'grp_clip_src', 'Pair', [a, b]));
      const copy = await K.apply(ctx, 'copy the group members', P.clipboardCopy(l1, [a, b]), { noop: true });
      const clip = copy.result.clipboard;
      ctx.ok('reply', 'the clipboard payload carries both records', clip && Array.isArray(clip.records) && clip.records.length === 2, clip && clip.records && clip.records.length, 2);
      const layer2 = (await K.apply(ctx, 'create a destination layer', P.layerCreate('Paste target'))).result.layerUid;
      const before = await K.saveAs(ctx, 'pre-paste');
      const idsBefore = K.objectStrokeIds(before.document), groupsBefore = collect(before.document, 'groupId');
      await K.apply(ctx, 'paste into the destination layer', P.clipboardPaste(layer2, clip));
      const after = await K.saveAs(ctx, 'post-paste');
      const idsAfter = K.objectStrokeIds(after.document), groupsAfter = collect(after.document, 'groupId');
      ctx.eq('file', 'paste adds exactly two strokes with fresh ids', [idsAfter.length, new Set(idsAfter).size], [idsBefore.length + 2, idsBefore.length + 2]);
      ctx.ok('file', 'the pasted strokes are new ids, none reused', idsBefore.every(id => idsAfter.includes(id)) && idsAfter.filter(id => !idsBefore.includes(id)).length === 2, idsAfter);
      ctx.ok('file', 'the pasted group has its own id, not the source group id', new Set(groupsAfter).size > new Set(groupsBefore).size && groupsAfter.includes('grp_clip_src'), { groupsBefore, groupsAfter });
      await K.step(ctx, 'one undo removes the whole paste', 'undo');
      const undone = await K.saveAs(ctx, 'undone-paste');
      ctx.eq('file', 'undo removes every pasted stroke and group together', [K.objectStrokeIds(undone.document), collect(undone.document, 'groupId').length], [idsBefore, groupsBefore.length]);
      await K.apply(ctx, 'cut A', P.clipboardCut(l1, [a]));
      ctx.eq('file', 'cut removes the stroke from the document', K.objectStrokeIds((await K.saveAs(ctx, 'cut')).document).includes(a), false);
      await K.step(ctx, 'undo the cut', 'undo');
      ctx.eq('file', 'undo restores the cut stroke', K.objectStrokeIds((await K.saveAs(ctx, 'uncut')).document).includes(a), true);
      await K.apply(ctx, 'lock layer 1', P.layerPolicy(l1, 'locked', true));
      await K.apply(ctx, 'delete a selection on a locked layer', P.selectionDelete(l1, [a]), { refused: true });
      await K.apply(ctx, 'paste a stale clip target layer', P.clipboardPaste('layer-missing', clip), { refused: true });
    } },
];

module.exports = { lane: '2108', issue: 2108, rows, helpers: { flattenGroup } };
