'use strict';
// #2371 / #2372 D27: the in/out bar context menu over the native document. The REAL layer-inout handlers, the real range owner,
// the real project session, rig command and contract run in the fixture of native-layer-range-desktop.test.cjs; only native I/O
// and the DOM are modeled. Every item is exactly one command.document.object.layer.range.arrange, selection items write nothing.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const fixture = fs.readFileSync(path.join(__dirname, 'native-layer-range-desktop.test.cjs'), 'utf8');
const begin = fixture.indexOf('// BEGIN REUSABLE REAL RANGE FIXTURE'), end = fixture.indexOf('// END REUSABLE REAL RANGE FIXTURE', begin);
assert.ok(begin >= 0 && end > begin);
const { harness, turn } = require('node:vm').compileFunction(fixture.slice(begin, end) + '\nreturn {harness,turn};', ['require', '__dirname'])(require, __dirname);
const ARRANGE = 'command.document.object.layer.range.arrange';
const plain = (value) => JSON.parse(JSON.stringify(value));
const sent = (h) => h.calls.filter((c) => c.q && c.q.operation === ARRANGE).map((c) => c.q);
const labels = (menu) => menu.items.map((item) => (item.sep ? '-' : item.label));
const item = (menu, label) => menu.items.find((entry) => entry.label === label);
async function open(h, li = 0) { h.bar(li).fire('contextmenu', { clientX: 40, clientY: 60 }); await turn(); return h.root.menus[h.root.menus.length - 1]; }
const both = (...lis) => lis.map((li) => ({ li, part: 'both' }));

test('right-click on a bar lists the baseline items: reset and Parent in Time, plus the batch items only for two or more bars', async () => {
  const h = await harness();
  const single = await open(h);
  assert.deepEqual(labels(single), ['Réinitialiser (pleine durée)', 'Parent in Time — lier le temps à…']);
  assert.equal(single.x, 40);
  assert.equal(h.root.notices.length, 0, 'no more "range menus are unavailable" toast');
  h.setBars(both(0));
  assert.deepEqual(labels(await open(h)), ['Réinitialiser (pleine durée)', 'Parent in Time — lier le temps à…', 'Inverser la sélection']);
  h.setBars(both(0, 1));
  assert.deepEqual(labels(await open(h)), ['Réinitialiser (pleine durée)', 'Parent in Time — lier le temps à…', '-', 'Aligner à gauche (in)', 'Aligner au centre', 'Aligner à droite (out)',
    'Distribuer uniformément', 'Échelonner (stagger)…', 'Inverser l’ordre (flip)', 'Sélectionner 1 sur 2', 'Inverser la sélection']);
  assert.equal(h.legacy(), 0);
  assert.equal(sent(h).length, 0, 'opening a menu writes nothing');
});

test('Reset is one native command for the right-clicked layer only, whatever else is selected', async () => {
  const h = await harness();
  h.setBars(both(0, 1));
  const menu = await open(h, 1);
  item(menu, 'Réinitialiser (pleine durée)').action();
  await h.settle();
  const requests = sent(h);
  assert.equal(requests.length, 1);
  assert.deepEqual(plain(requests[0].payload), { command: 'object.layer.range.arrange', contextId: 'scene-root', layerUids: ['beta'], action: 'reset', frame: 2 });
  assert.equal(requests[0].expectedRevision, 0);
  assert.equal(h.s.info().contentRevision, 1);
  assert.equal(h.legacy(), 0);
});

test('each batch item sends the selected layers in selection order as one command', async () => {
  const cases = [['Aligner à gauche (in)', 'alignLeft'], ['Aligner au centre', 'alignCenter'], ['Aligner à droite (out)', 'alignRight'],
    ['Distribuer uniformément', 'distribute'], ['Inverser l’ordre (flip)', 'flip']];
  for (const [label, action] of cases) {
    const h = await harness();
    h.setBars([{ li: 1, part: 'both' }, { li: 0, part: 'in' }]);
    item(await open(h), label).action();
    await h.settle();
    const requests = sent(h);
    assert.equal(requests.length, 1, label);
    assert.deepEqual(plain(requests[0].payload), { command: 'object.layer.range.arrange', contextId: 'scene-root', layerUids: ['beta', 'alpha'], action, frame: 2 }, label);
    assert.equal(h.s.info().contentRevision, 1, label);
    assert.equal(h.legacy(), 0, label);
  }
});

test('Stagger asks for the offset in an inline field, since window.prompt is ignored by the desktop webview, and a zero or blank offset sends nothing', async () => {
  const h = await harness();
  h.setBars(both(0, 1));
  item(await open(h), 'Échelonner (stagger)…').action();
  const box = h.doc.body.children.find((node) => node.className.includes('native-range-step'));
  assert.ok(box, 'an inline field opens at the menu position');
  assert.deepEqual([box.style.left, box.style.top], ['40px', '60px']);
  const [, input, ok] = box.children;
  assert.equal(input.className, 'pi scrub');
  assert.equal(input.value, '2', 'the baseline default');
  input.value = '-3';
  ok.fire('click');
  await h.settle();
  assert.equal(h.doc.body.children.some((node) => node.className.includes('native-range-step')), false);
  assert.deepEqual(plain(sent(h).map((q) => q.payload)), [{ command: 'object.layer.range.arrange', contextId: 'scene-root', layerUids: ['alpha', 'beta'], action: 'stagger', step: -3, frame: 2 }]);
  const zero = await harness();
  zero.setBars(both(0, 1));
  item(await open(zero), 'Échelonner (stagger)…').action();
  const field = zero.doc.body.children.find((node) => node.className.includes('native-range-step'));
  field.children[1].value = '0';
  field.children[2].fire('click');
  await zero.settle();
  assert.equal(sent(zero).length, 0);
  assert.equal(zero.doc.body.children.some((node) => node.className.includes('native-range-step')), false);
});

test('Sélectionner 1 sur 2 and Inverser la sélection only change the bar selection, never the document', async () => {
  const h = await harness();
  h.setBars([{ li: 1, part: 'out' }, { li: 0, part: 'in' }]);
  item(await open(h), 'Sélectionner 1 sur 2').action();
  // both bars read the same raw in point: the stable sort keeps the selection order, so the first stays
  assert.deepEqual(h.root.SMLayerInOut.getBarSelection(), [{ li: 1, part: 'out' }]);
  assert.match(h.root.notices[h.root.notices.length - 1], /^1/);
  item(await open(h), 'Inverser la sélection').action();
  assert.deepEqual(h.root.SMLayerInOut.getBarSelection(), [{ li: 0, part: 'both' }]);
  h.setBars(both(0, 1));
  item(await open(h), 'Inverser la sélection').action();
  assert.deepEqual(h.root.SMLayerInOut.getBarSelection(), []);
  assert.equal(sent(h).length, 0);
  assert.equal(h.s.info().contentRevision, 0);
});

test('Parent in Time opens the native Link time picker, whose pick is the existing timeLink.set command', async () => {
  const h = await harness();
  const menu = await open(h, 0);
  item(menu, 'Parent in Time — lier le temps à…').action();
  const picker = h.root.menus[h.root.menus.length - 1];
  assert.deepEqual([picker.x, picker.y], [48, 68]);
  assert.equal(picker.items[0].label, 'Time: None (unlink, position kept)');
  assert.equal(picker.items[0].disabled, true, 'nothing is linked yet');
  const target = picker.items.find((entry) => /Link time to: /.test(entry.label));
  assert.ok(target && target.disabled !== true);
  target.action();
  await h.settle();
  const link = h.calls.filter((c) => c.q && c.q.operation === 'command.document.object.layer.timeLink.set').map((c) => plain(c.q.payload));
  assert.deepEqual(link, [{ command: 'object.layer.timeLink.set', contextId: 'scene-root', layerUid: 'alpha', sourceLayerUid: 'beta', frame: 2, mode: 'both' }]);
  assert.equal(sent(h).length, 0);
});

test('a right-click mousedown starts no bar drag and no history entry', async () => {
  const h = await harness();
  h.bar(0).fire('mousedown', { button: 2 });
  h.up(100);
  await h.settle();
  assert.equal(h.commands().length, 0, 'no empty range.drag checkpoint');
  await open(h);
  assert.equal(h.s.info().contentRevision, 0);
});

test('a stale menu, a refusal and an unavailable owner each say so and write nothing more', async () => {
  const stale = await harness();
  stale.setBars(both(0, 1));
  const menu = await open(stale);
  item(menu, 'Réinitialiser (pleine durée)').action();
  await stale.settle();
  assert.equal(sent(stale).length, 1);
  item(menu, 'Aligner au centre').action(); // the menu was built at revision 0, the document is now at 1
  await stale.settle();
  assert.equal(sent(stale).length, 1, 'nothing more is sent from a stale menu');
  assert.match(stale.root.notices[stale.root.notices.length - 1], /unavailable or stale/);

  const refused = await harness({ arrangeRefusal: 'unavailable' });
  refused.setBars(both(0, 1));
  item(await open(refused), 'Distribuer uniformément').action();
  await refused.settle();
  assert.equal(sent(refused).length, 1);
  assert.match(refused.root.notices[refused.root.notices.length - 1], /edge cannot be moved exactly/);
  assert.equal(refused.s.info().contentRevision, 0);

  const missing = await harness({ arrangeRefusal: 'not_found' });
  item(await open(missing), 'Réinitialiser (pleine durée)').action();
  await missing.settle();
  assert.match(missing.root.notices[missing.root.notices.length - 1], /no longer in the project/);

  const single = await harness();
  single.setBars(both(0));
  const one = await open(single);
  assert.equal(item(one, 'Aligner au centre'), undefined, 'a group action needs a group');
  const no = await harness();
  no.root.NemoNativeLayerRangeMenu = undefined;
  no.bar(0).fire('contextmenu', { clientX: 1, clientY: 2 });
  assert.equal(no.root.menus.length, 0);
  assert.ok(no.root.notices.length, 'without the menu module the bar says so rather than falling back');
});
