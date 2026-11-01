'use strict';
// D18-C (#2376): the widget axis "+" menus (#p-widget-xadd / #p-widget-yadd) emit exactly the baseline rig-widget.js text and write one
// native object.property.edit setExpression at the pin they were read at; a stale pin refuses with a toast and the legacy writers throw.
const test = require('node:test');
const assert = require('node:assert/strict');
const Link = require('../src/js/adapters/native-object-widget-link.js');
const fixture = require('../engineering/application/examples/native-property-metadata-v1/cases.json');
const clone = structuredClone;
const settle = async () => { for (let i = 0; i < 12; i += 1) await new Promise((resolve) => setImmediate(resolve)); };

function harness() {
  const sent = [], toasts = [], menus = [], state = { revision: 2 };
  const pinNow = () => ({ instanceId: 'x-instance', documentId: 'x-document', lifecycleGeneration: 1, contentRevision: state.revision, documentSnapshotId: 'snap-' + state.revision });
  const rows = ['opacity', 'rotation'].map((p) => clone(fixture.propertyPageOracle.find((r) => r.property === p)));
  const categories = clone(fixture.cases.find((c) => c.expectedResult).expectedResult.unavailableCategories);
  const link = Link.create({
    contextId: 'scene-root',
    readContext: () => { const pin = pinNow(); return { pin, frame: 3, viewGeneration: 1, presented: { ...pin, available: true, presentation: { ...pin, owner: 'native', status: 'presented', frame: 3 } } }; },
    async query(q) { return { instanceId: q.instanceId, documentId: q.documentId, atRevision: q.atRevision, documentSnapshotId: q.documentSnapshotId, kind: q.view.kind, target: q.view.target, items: clone(rows), nextOffset: null, unavailableCategories: categories }; },
    layers: () => [{ layerUid: 'W', name: 'Rig' }, { layerUid: 'A', name: 'Arm' }],
    async editProperty(payload, pin, isCurrent) { assert.equal(isCurrent(), true); sent.push(clone(payload)); if (JSON.stringify(pin) !== JSON.stringify(pinNow())) throw new Error('stale_revision'); state.revision += 1; return { applied: true, historyEntriesAdded: 1 }; },
    showContextMenu: (x, y, items) => menus.push(items), showToast: (m) => toasts.push(m), translate: (k) => k,
  });
  return { link, sent, toasts, menus, state };
}
const source = { layerUid: 'W', name: 'Turn', key: 'xc_turn' };

test('refText is exactly the baseline layerControl text', () => {
  assert.equal(Link.refText('ly_x9', 'Turn'), 'layerControl("ly_x9", "Turn")');
});

test('Link a property and Drive a pose write layerControl / self.at(layerControl) through one setExpression each', async () => {
  const h = harness();
  assert.equal(h.link.openMenu(5, 6, source), true);
  const entry = h.menus.pop();
  assert.deepEqual(entry.map((i) => i.label), ['Link a property…', 'Drive a pose…']);
  entry[0].action(); await settle();
  const props = h.menus.pop();
  assert.ok(props.length >= 4, 'every property of every layer');
  assert.ok(props.some((i) => /^Arm · /.test(i.label)));
  await props.find((i) => /^Arm · /.test(i.label)).action();
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].command, 'object.property.edit');
  assert.equal(h.sent[0].target.layerUid, 'A');
  assert.deepEqual(h.sent[0].action, { kind: 'setExpression', code: 'layerControl("W", "Turn")' });
  entry[1].action(); await settle();
  await h.menus.pop().find((i) => /^Arm · /.test(i.label)).action();
  assert.equal(h.sent[1].action.code, 'self.at(layerControl("W", "Turn"))');
});

test('a stale pin refuses with a toast and no change', async () => {
  const h = harness();
  h.link.open(0, 0, source, 'link'); await settle();
  const items = h.menus.pop(); h.state.revision += 1;
  await items[0].action();
  assert.equal(h.toasts.length, 1); assert.match(h.toasts[0], /Edit refused/);
  assert.equal(h.state.revision, 3, 'nothing applied');
});

test('without the native edit port the menu is a typed toast, never a legacy writer', () => {
  const toasts = [], link = Link.create({ readContext() {}, query() {}, layers: () => [], showToast: (m) => toasts.push(m), showContextMenu() {} });
  assert.equal(link.open(0, 0, source, 'link') instanceof Promise, true);
  assert.equal(toasts.length, 1); assert.match(toasts[0], /unavailable/);
});
