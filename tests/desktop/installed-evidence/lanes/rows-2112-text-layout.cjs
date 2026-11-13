'use strict';
// #2112 (G38b G27): the layout box of a retained text block, as far as a public read shows it. The hit-test listing carries one bounds box per text
// block (role text, "the item's stored rect or glyph box in its own frame", matrix = the map to the painted box: native-object-subselect.json). It is
// compared with the ink of the transparent export (alpha), follows size, letter spacing, wrapping, line height and alignment, and is identical after
// undo, redo and reopen.
// What no public read returns: per-glyph identity and advances. Needed: a `layout` view of query.document.object.text (glyph ids, advances, line boxes).
const P = require('../payloads.cjs');
const K = require('../kit.cjs');
const probe = require('../png-probe.cjs');

const BOX_TOL = 2;
const rows = [
  { id: '2112-text-layout-box', title: 'text layout box: the listed glyph box encloses the ink and follows the anchor, size, spacing, wrapping, line height and alignment; identical after undo, redo and reopen', tasks: ['G38b', 'G27'],
    limitations: ['One box per text block: no read returns glyph ids, advances or line boxes (needed: a layout view of query.document.object.text), so glyph identity is not observed.'],
    async run(ctx) {
      const { layerUid } = await K.fresh(ctx, { transparent: true });
      const created = await K.apply(ctx, 'create a text block', P.textCreate(layerUid, P.textBlock('HELLO', { size: 72, color: '#cc2200', anchorTopLeft: { x: 20, y: 30 } })));
      const id = created.result.strokeId, tgt = P.target(layerUid, id);
      const boxOf = async () => { const item = K.itemOf(await K.hitTest(ctx, layerUid), id); return item; };
      const it0 = await boxOf(), e0 = await K.exportPng(ctx, 'box0'), ink0 = probe.inkBounds(e0.image);
      ctx.eq('read', 'the listing names the text block with role text, no anchors and the identity map', [it0.role, it0.count, it0.matrix], ['text', 0, [1, 0, 0, 1, 0, 0]]);
      const [x0, y0, x1, y1] = it0.bounds;
      ctx.near('pixel', 'the glyph box encloses the ink to within 2 px on every side (ink bounds are inclusive pixels)', [ink0.box[0] - x0, ink0.box[1] - y0, x1 - (ink0.box[2] + 1), y1 - (ink0.box[3] + 1)], [0, 0, 0, 0], BOX_TOL);
      ctx.ok('read', 'the box starts at or after the anchor (20, 30)', x0 >= 19.5 && y0 >= 29.5, [x0, y0], '>= (20, 30)');
      ctx.within('read', 'five glyphs at 72 px: the box is 150..270 px wide', x1 - x0, 150, 270);
      ctx.within('read', 'and 40..70 px high', y1 - y0, 40, 70);
      ctx.eq('read', 'a second listing is identical: the layout is deterministic', (await boxOf()).bounds, it0.bounds);
      await K.apply(ctx, 'edit the size to 36', P.textEdit(tgt, { size: 36 }));
      const it1 = await boxOf(), e1 = await K.exportPng(ctx, 'box1'), ink1 = probe.inkBounds(e1.image);
      ctx.near('read', 'half the size gives half the box in both extents', [(it1.bounds[2] - it1.bounds[0]) / (x1 - x0), (it1.bounds[3] - it1.bounds[1]) / (y1 - y0)], [0.5, 0.5], 0.05);
      ctx.near('pixel', 'the half-size box still encloses the half-size ink', [ink1.box[0] - it1.bounds[0], ink1.box[1] - it1.bounds[1], it1.bounds[2] - (ink1.box[2] + 1), it1.bounds[3] - (ink1.box[3] + 1)], [0, 0, 0, 0], BOX_TOL);
      await K.step(ctx, 'undo the size edit', 'undo');
      ctx.eq('read', 'undo restores the exact box', (await boxOf()).bounds, it0.bounds);
      await K.step(ctx, 'redo the size edit', 'redo');
      ctx.eq('read', 'redo restores the half-size box', (await boxOf()).bounds, it1.bounds);
      await K.apply(ctx, 'size back to 72 with letter spacing 10', P.textEdit(tgt, { size: 72, letterSpacing: 10 }));
      const it2 = await boxOf(), e2 = await K.exportPng(ctx, 'box2'), ink2 = probe.inkBounds(e2.image);
      ctx.within('read', 'four gaps of 10 px widen the box by about 40', (it2.bounds[2] - it2.bounds[0]) - (x1 - x0), 30, 60);
      ctx.near('read', 'the height does not change with letter spacing', it2.bounds[3] - it2.bounds[1], y1 - y0, 1);
      ctx.near('pixel', 'the widened box still encloses the ink', [ink2.box[0] - it2.bounds[0], ink2.box[1] - it2.bounds[1], it2.bounds[2] - (ink2.box[2] + 1), it2.bounds[3] - (ink2.box[3] + 1)], [0, 0, 0, 0], BOX_TOL);
      // Wrapping: two words in a fixed width of 300 break into two lines, the box follows the wrapped text (a glyph box, not the frame).
      const enclose = (box, ink) => [ink.box[0] - box[0], ink.box[1] - box[1], box[2] - (ink.box[2] + 1), box[3] - (ink.box[3] + 1)];
      await K.apply(ctx, 'two words in a fixed width of 300', P.textEdit(tgt, { text: 'HELLO WORLD', letterSpacing: 0, fixedWidth: 300 }));
      const it3 = await boxOf(), e3 = await K.exportPng(ctx, 'box3'), ink3 = probe.inkBounds(e3.image);
      ctx.ok('read', 'the wrapped box is no wider than the fixed width', it3.bounds[2] - it3.bounds[0] <= 300 + BOX_TOL, it3.bounds[2] - it3.bounds[0], '<=302');
      ctx.ok('read', 'and at least 1.8 times as high as one line', (it3.bounds[3] - it3.bounds[1]) >= 1.8 * (y1 - y0), [it3.bounds[3] - it3.bounds[1], y1 - y0], '>= 1.8 lines');
      ctx.near('pixel', 'the wrapped box encloses the wrapped ink', enclose(it3.bounds, ink3), [0, 0, 0, 0], BOX_TOL);
      await K.apply(ctx, 'line height times 2', P.textEdit(tgt, { lineHeightMult: 2 }));
      const it4 = await boxOf(), ink4 = probe.inkBounds((await K.exportPng(ctx, 'box4a')).image);
      ctx.ok('read', 'a taller line pitch makes the box higher and leaves its width', it4.bounds[3] - it4.bounds[1] > it3.bounds[3] - it3.bounds[1] + 20 && Math.abs(it4.bounds[2] - it4.bounds[0] - (it3.bounds[2] - it3.bounds[0])) < 1, [it3.bounds, it4.bounds]);
      ctx.near('pixel', 'the taller box still encloses the ink', enclose(it4.bounds, ink4), [0, 0, 0, 0], BOX_TOL);
      await K.apply(ctx, 'align right', P.textEdit(tgt, { align: 'right' }));
      const it5 = await boxOf(), ink5 = probe.inkBounds((await K.exportPng(ctx, 'box5a')).image);
      ctx.ok('read', 'right alignment moves the box right, to within 8 px of the right edge of the frame (20 + 300)', it5.bounds[0] > it4.bounds[0] + 20 && Math.abs(it5.bounds[2] - 320) <= 8, [it4.bounds, it5.bounds]);
      ctx.near('pixel', 'the right aligned box encloses the ink', enclose(it5.bounds, ink5), [0, 0, 0, 0], BOX_TOL);
      const e4 = await K.exportPng(ctx, 'box4');
      await K.roundTrip(ctx, 'text-box', e4, async () => {
        const again = await boxOf();
        ctx.eq('read', 'after save and reopen the listing gives the same box and role', [again && again.role, again && again.bounds], ['text', it5.bounds]);
      });
    } },
];

module.exports = { rows };
