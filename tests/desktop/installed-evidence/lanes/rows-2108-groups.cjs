'use strict';
// #2108 (G24 G18 G19): nested groups as one operand, reorder inside nested groups, linked fills that follow their walls, Flatten consuming element Motion.
// The structure of groups is read from the hit-test listing (groupPath, outermost first, with the stored combine mode: native-object-subselect.json),
// the paint from the transparent export (alpha), the document from the saved project. Expected geometry follows the lane text:
// "nested combined children remain one operand ... Flatten consumes current element Motion and one Undo restores originals".
const P = require('../payloads.cjs');
const K = require('../kit.cjs');
const probe = require('../png-probe.cjs');

const RED = [0.9, 0.2, 0.2, 1], BLUE = [0, 0.27, 0.8, 1], GREEN = [0.1, 0.6, 0.2, 1], BLACK = [0, 0, 0, 1];
async function stroke(ctx, label, layerUid, [x0, y0, x1, y1], fill, extra = {}) {
  const made = await K.apply(ctx, label, P.vectorCreate(P.vectorRecord({ layerUid, segments: P.rectSegments(x0, y0, x1, y1), fill, ...extra })));
  ctx.ok('reply', label + ': stroke id allocated', typeof made.result.strokeId === 'string' && made.result.strokeId.length > 0, made.result.strokeId);
  return made.result.strokeId;
}
const alpha = (e, x, y) => probe.alphaAt(e.image, x, y);
const paths = (items, ids) => ids.map(id => (K.itemOf(items, id) ? (K.itemOf(items, id).groupPath || []).map(g => [g.id, g.mode, g.name]) : null));
const order = items => items.map(i => i.stableTarget.strokeId);
// a (red) holds b (blue) and c (green) in an inner group, itself inside an outer group with a.
async function nested(ctx, layerUid) {
  const a = await stroke(ctx, 'create the big rectangle', layerUid, [100, 60, 400, 260], RED);
  const b = await stroke(ctx, 'create the left bar', layerUid, [130, 100, 180, 220], BLUE);
  const c = await stroke(ctx, 'create the right bar', layerUid, [320, 100, 370, 220], GREEN);
  await K.apply(ctx, 'group the two bars as Inner', P.groupCreate(layerUid, 'g_inner', 'Inner', [b, c]));
  await K.apply(ctx, 'group the big rectangle with the inner group as Outer', P.groupCreate(layerUid, 'g_outer', 'Outer', [a, b, c]));
  return { a, b, c };
}

const rows = [
  { id: '2108-nested-group-structure', title: 'nested groups: path outermost first, independent combine modes, rename, member removal dissolves a group below two, undo, refusals', tasks: ['G24'],
    limitations: ['Structure is read from the hit-test listing; the combined result of a nested group is observed by 2108-nested-group-operand-flatten.'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx, { transparent: true });
      const { a, b, c } = await nested(ctx, layerUid);
      const l0 = await K.hitTest(ctx, layerUid);
      ctx.eq('read', 'the big rectangle is in Outer; each bar is in Outer then Inner (outermost first)', paths(l0, [a, b, c]).map(p => p.map(g => g[0])), [['g_outer'], ['g_outer', 'g_inner'], ['g_outer', 'g_inner']]);
      ctx.eq('read', 'the group names are the ones sent and every mode starts at none', paths(l0, [b]), [[['g_outer', 'none', 'Outer'], ['g_inner', 'none', 'Inner']]]);
      await K.apply(ctx, 'combine Inner by unite', P.groupCombine(layerUid, 'g_inner', 'unite', [b, c]));
      await K.apply(ctx, 'combine Outer by intersect', P.groupCombine(layerUid, 'g_outer', 'intersect', [a, b, c]));
      const l1 = await K.hitTest(ctx, layerUid);
      ctx.eq('read', 'each group keeps its own combine mode: Outer intersect, Inner unite', paths(l1, [a, b]).map(p => p.map(g => g[1])), [['intersect'], ['intersect', 'unite']]);
      await K.apply(ctx, 'rename Inner', P.groupRename(layerUid, 'g_inner', 'Inner renamed'));
      ctx.eq('read', 'the rename reaches every member of the group and not the outer group', paths(await K.hitTest(ctx, layerUid), [b, c]).map(p => p.map(g => g[2])), [['Outer', 'Inner renamed'], ['Outer', 'Inner renamed']]);
      const before = JSON.stringify(paths(await K.hitTest(ctx, layerUid), [a, b, c]));
      await K.apply(ctx, 'remove the right bar from its group', P.groupMemberRemove(layerUid, c));
      const l2 = await K.hitTest(ctx, layerUid);
      ctx.ok('read', 'the right bar is no longer in Inner', !K.groupIds(K.itemOf(l2, c)).includes('g_inner'), K.groupIds(K.itemOf(l2, c)));
      ctx.eq('read', 'Inner is left with one entry and dissolves in the same revision; Outer still holds the big rectangle and the left bar', [K.groupIds(K.itemOf(l2, b)), K.groupIds(K.itemOf(l2, a))], [['g_outer'], ['g_outer']]);
      await K.step(ctx, 'undo the removal', 'undo');
      ctx.eq('read', 'one undo restores every path and mode', JSON.stringify(paths(await K.hitTest(ctx, layerUid), [a, b, c])), before);
      await K.apply(ctx, 'group a single stroke', P.groupCreate(layerUid, 'g_single', 'Single', [a]), { refused: true });
      await K.apply(ctx, 'group a stroke that does not exist', P.groupCreate(layerUid, 'g_ghost', 'Ghost', [a, 'native-stroke:99999:9']), { refused: true });
      ctx.eq('read', 'the refusals left every path as it was', JSON.stringify(paths(await K.hitTest(ctx, layerUid), [a, b, c])), before);
    } },

  { id: '2108-group-stack-reorder', title: 'order inside nested groups: sending the member to the front changes the stack, the export and the listing order, group paths stay, no-op, undo, redo', tasks: ['G24'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx, { transparent: true });
      const { a, b, c } = await nested(ctx, layerUid);
      const l0 = await K.hitTest(ctx, layerUid), e0 = await K.exportPng(ctx, 'stack0');
      ctx.eq('read', 'the stack starts as created: big rectangle, left bar, right bar', order(l0), [a, b, c]);
      ctx.near('pixel', 'the left bar paints over the big rectangle', probe.over(e0.image, 150, 150), [0, 69, 204], 10);
      await K.apply(ctx, 'send the big rectangle to the front', P.vectorOrder(P.target(layerUid, a), 'front'));
      const l1 = await K.hitTest(ctx, layerUid), e1 = await K.exportPng(ctx, 'stack1');
      ctx.eq('read', 'the big rectangle is now last in paint order and the bars keep their order', order(l1), [b, c, a]);
      ctx.eq('read', 'group membership is untouched by the reorder', JSON.stringify(paths(l1, [a, b, c])), JSON.stringify(paths(l0, [a, b, c])));
      ctx.near('pixel', 'the big rectangle now covers the bars', [probe.over(e1.image, 150, 150), probe.over(e1.image, 345, 150)].flat(), [230, 51, 51, 230, 51, 51], 10);
      await K.apply(ctx, 'send the big rectangle to the front again', P.vectorOrder(P.target(layerUid, a), 'front'), { noop: true });
      await K.step(ctx, 'undo the reorder', 'undo');
      ctx.eq('read', 'undo restores the stack', order(await K.hitTest(ctx, layerUid)), [a, b, c]);
      ctx.eq('pixel', 'undo restores the export', (await K.exportPng(ctx, 'stack-undone')).digest, e0.digest);
      await K.step(ctx, 'redo the reorder', 'redo');
      ctx.eq('pixel', 'redo reapplies the export', (await K.exportPng(ctx, 'stack-redone')).digest, e1.digest);
      await K.apply(ctx, 'send a stroke that does not exist to the back', P.vectorOrder(P.target(layerUid, 'native-stroke:99999:9'), 'back'), { refused: true });
    } },

  { id: '2108-nested-group-operand-flatten', title: 'nested combined children are one operand: Outer intersect with an Inner union flattens to the two bars, one undo restores', tasks: ['G24', 'G19'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx, { transparent: true });
      const { a, b, c } = await nested(ctx, layerUid);
      await K.apply(ctx, 'combine Inner by unite', P.groupCombine(layerUid, 'g_inner', 'unite', [b, c]));
      await K.apply(ctx, 'combine Outer by intersect', P.groupCombine(layerUid, 'g_outer', 'intersect', [a, b, c]));
      const l0 = await K.hitTest(ctx, layerUid), e0 = await K.exportPng(ctx, 'before-flatten');
      // Outer = big rectangle INTERSECT (left bar UNION right bar): if the inner group were flattened into the outer list the intersection of the three would be empty.
      await K.apply(ctx, 'flatten Outer', P.booleanFlatten(layerUid, 'g_outer'));
      const l1 = await K.hitTest(ctx, layerUid), e1 = await K.exportPng(ctx, 'flattened');
      ctx.eq('pixel', 'both bars are painted: the inner union is one operand', [alpha(e1, 150, 150), alpha(e1, 345, 150)], [255, 255]);
      ctx.eq('pixel', 'the gap between the bars and the rest of the big rectangle are clear', [alpha(e1, 250, 150), alpha(e1, 110, 70), alpha(e1, 390, 250)], [0, 0, 0]);
      ctx.near('pixel', 'the ink is the two bars: 130..370 by 100..220', probe.inkBounds(e1.image).box, [130, 100, 369, 219], 2);
      ctx.eq('read', 'flatten leaves no group: every record is loose', l1.map(i => (i.groupPath || []).length), l1.map(() => 0));
      await K.step(ctx, 'one undo of the flatten', 'undo');
      ctx.eq('read', 'one undo restores the three records and both groups', JSON.stringify(paths(await K.hitTest(ctx, layerUid), [a, b, c])), JSON.stringify(paths(l0, [a, b, c])));
      ctx.eq('pixel', 'one undo restores the export', (await K.exportPng(ctx, 'unflattened')).digest, e0.digest);
    } },

  { id: '2108-linked-fill-join', title: 'a traced fill follows its walls: one revision to fill, one to move a wall (the fill regenerates in it), undo restores both, an unenclosed point is a no-op', tasks: ['G18', 'G19'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx, { transparent: true });
      const wall = await K.apply(ctx, 'create the wall: an unfilled stroked rectangle', P.vectorCreate(P.vectorRecord({ layerUid, segments: P.rectSegments(40, 40, 240, 200), stroke: BLACK, width: 6 })));
      const wallId = wall.result.strokeId;
      const e0 = await K.exportPng(ctx, 'wall-only');
      ctx.eq('pixel', 'the wall alone leaves the inside clear', alpha(e0, 100, 100), 0);
      await K.apply(ctx, 'fill the enclosed area at (100, 100)', P.fillTrace(layerUid, [100, 100], '#2a9d3fff'));
      const l1 = await K.hitTest(ctx, layerUid), e1 = await K.exportPng(ctx, 'filled');
      ctx.eq('pixel', 'inside the wall is the fill colour and opaque; outside it is clear', [alpha(e1, 100, 100), alpha(e1, 280, 100)], [255, 0]);
      ctx.near('pixel', 'the fill colour is the one traced', probe.over(e1.image, 100, 100), [42, 157, 63], 4);
      const filled = l1.filter(i => i.filled === true);
      ctx.eq('read', 'one filled record appeared, bounded by the wall', [l1.length, filled.length, filled.length === 1 ? filled[0].bounds.map(Math.round) : null], [2, 1, [40, 40, 240, 200]]);
      await K.apply(ctx, 'fill a point no wall encloses', P.fillTrace(layerUid, [400, 300], '#2a9d3fff'), { noop: true });
      ctx.eq('pixel', 'a no-op fill leaves the export as it was', (await K.exportPng(ctx, 'after-noop')).digest, e1.digest);
      // The wall edit is ONE revision and ONE history entry (K.apply): the linked fill is regenerated inside it, never left stale and never a second entry.
      const next = P.vectorRecord({ layerUid, strokeId: wallId, segments: P.rectSegments(40, 40, 340, 200), stroke: BLACK, width: 6 });
      await K.apply(ctx, 'move the right wall to x = 340', P.vectorReplace(next));
      const l2 = await K.hitTest(ctx, layerUid), e2 = await K.exportPng(ctx, 'wall-moved');
      ctx.eq('pixel', 'the fill now reaches the moved wall: x = 280 is painted with the fill colour', [alpha(e2, 280, 100), probe.over(e2.image, 280, 100)], [255, [42, 157, 63]]);
      ctx.eq('read', 'the filled record follows the wall', l2.filter(i => i.filled === true).map(i => i.bounds.map(Math.round)), [[40, 40, 340, 200]]);
      await K.step(ctx, 'undo the wall move', 'undo');
      ctx.eq('pixel', 'one undo restores the wall and the fill together', (await K.exportPng(ctx, 'wall-undone')).digest, e1.digest);
      await K.step(ctx, 'redo the wall move', 'redo');
      ctx.eq('pixel', 'redo reapplies both', (await K.exportPng(ctx, 'wall-redone')).digest, e2.digest);
      await K.roundTrip(ctx, 'linked-fill', e2);
    } },

  { id: '2108-flatten-element-motion', title: 'Flatten bakes the current element Motion: the hole is where the moved member is, the result carries no Motion, one undo restores the members and their Motion', tasks: ['G24', 'G19'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx, { transparent: true });
      const a = await stroke(ctx, 'create the big rectangle', layerUid, [100, 60, 300, 260], RED);
      const b = await stroke(ctx, 'create the small rectangle', layerUid, [120, 100, 160, 140], BLUE);
      await K.apply(ctx, 'move the small rectangle by +100 in x with element Motion at frame 0', P.elementValue(layerUid, b, 'position', [100, 0]));
      const motion = async id => { const { revision, pin } = await K.pinNow(ctx), q = P.elementRead(revision, pin, layerUid, id, 'position'), r = await ctx.wire.query(q.operation, q.payload); return r.ok ? r.result.value : r.error; };
      ctx.eq('read', 'the small rectangle carries the element Motion position (100, 0) at frame 0', await motion(b), [100, 0]);
      const moved = await K.exportPng(ctx, 'moved');
      ctx.eq('pixel', 'the export draws the small rectangle at its moved place and not at its stored place', [probe.over(moved.image, 240, 120), probe.over(moved.image, 140, 120)], [[0, 69, 204], [230, 51, 51]]);
      await K.apply(ctx, 'group the two rectangles', P.groupCreate(layerUid, 'g_motion', 'Motion pair', [a, b]));
      await K.apply(ctx, 'combine by subtract', P.groupCombine(layerUid, 'g_motion', 'subtract', [a, b]));
      const before = await K.hitTest(ctx, layerUid), pre = await K.exportPng(ctx, 'pre-flatten');
      await K.apply(ctx, 'flatten the group', P.booleanFlatten(layerUid, 'g_motion'));
      const after = await K.hitTest(ctx, layerUid), flat = await K.exportPng(ctx, 'flattened');
      ctx.eq('read', 'one result record replaces the two members, bounded by the big rectangle', [after.length, after.length === 1 ? after[0].bounds.map(Math.round) : null], [1, [100, 60, 300, 260]]);
      ctx.eq('pixel', 'the hole is at the MOVED place (baked Motion), the stored place of the small rectangle is painted', [alpha(flat, 240, 120), alpha(flat, 140, 120), alpha(flat, 110, 70)], [0, 255, 255]);
      const resultId = after[0].stableTarget.strokeId;
      ctx.ok('read', 'the result is a new record, neither member', resultId !== a && resultId !== b, resultId);
      ctx.eq('read', 'the result carries no element Motion: its position is (0, 0)', await motion(resultId), [0, 0]);
      await K.step(ctx, 'one undo of the flatten', 'undo');
      ctx.eq('read', 'one undo restores both members', order(await K.hitTest(ctx, layerUid)).sort(), before.map(i => i.stableTarget.strokeId).sort());
      ctx.eq('read', 'and the Motion of the small rectangle', await motion(b), [100, 0]);
      ctx.eq('pixel', 'and the export as it was right before the flatten', (await K.exportPng(ctx, 'unflattened')).digest, pre.digest);
    } },
];

module.exports = { rows };
