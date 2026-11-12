'use strict';
// D18-B (#2376): the "Expression controls…" entry of the native Motion inspector. The real rows module runs against a fake native owner
// that applies the control declaration edits with the engine's rules; the legacy writers and window.prompt THROW if they are ever reached.
const test = require('node:test');
const assert = require('node:assert/strict');
const Rows = require('../src/js/adapters/native-motion-control-rows.js');
const Contract = require('../src/js/adapters/native-property-contract.js');

const settle = async () => { for (let i = 0; i < 12; i += 1) await new Promise((resolve) => setImmediate(resolve)); };
function element(tagName) {
  const classes = new Set(), listeners = {}, kids = [];
  const el = { tagName, style: {}, dataset: {}, textContent: '', type: '', value: '', title: '', parentNode: null,
    classList: { add: (x) => classes.add(x), remove: (x) => classes.delete(x), contains: (x) => classes.has(x) },
    addEventListener(k, f) { listeners[k] = f; }, removeEventListener() {}, setAttribute() {}, focus() { el.focused = true; },
    replaceChildren(...next) { kids.splice(0).forEach((k) => { k.parentNode = null; }); next.forEach((k) => this.appendChild(k)); },
    appendChild(k) { if (k.parentNode) k.parentNode.removeChild(k); kids.push(k); k.parentNode = this; return k; },
    removeChild(k) { kids.splice(kids.indexOf(k), 1); k.parentNode = null; },
    getBoundingClientRect: () => ({ left: 10, top: 20, width: 9, height: 9, bottom: 29 }),
    fire(type, extra) { return listeners[type] && listeners[type](Object.assign({ target: this, preventDefault() {}, stopPropagation() {} }, extra)); } };
  Object.defineProperty(el, 'className', { get: () => [...classes].join(' '), set(v) { classes.clear(); String(v).split(/\s+/).filter(Boolean).forEach((x) => classes.add(x)); } });
  Object.defineProperty(el, 'children', { get: () => kids.slice() });
  return el;
}
const walk = (n) => [n, ...n.children.flatMap(walk)];
const named = (n, cls) => walk(n).find((x) => x.className.split(' ').includes(cls));
const norm = (name) => name.trim().toLowerCase();

const TARGET = { contextId: 'scene-root', layerUid: 'L-a' };
const DEFAULTS = { number: [0], checkbox: [0], angle: [0], point: [0, 0], color: [255, 255, 255, 255] };
const DIM = { number: 1, checkbox: 1, angle: 1, point: 2, color: 4 };

// The owner: the same declaration rules as native-engine/src/object_property_control_edit.rs, validated by the real public contract.
function harness(options = {}) {
  const controls = (options.controls || []).map((c) => ({ ...c }));
  const sent = [], toasts = [], menus = [], queries = [];
  const pin = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 'snap-4' };
  const context = { pin, frame: 0, viewGeneration: 1, presented: { available: true, ...pin, presentation: { ...pin, owner: 'native', status: 'presented', frame: 0 } } };
  const validate = Contract.create({ exact: (v, req, opt) => { for (const k of req) if (!(k in v)) throw new TypeError('missing ' + k); for (const k of Object.keys(v)) if (!req.includes(k) && !(opt || []).includes(k)) throw new TypeError('extra ' + k); },
    revision() {}, objectUid() {}, frame() {}, id() {}, plain: (v) => !!v && typeof v === 'object' && !Array.isArray(v), has: (v, k) => Object.prototype.hasOwnProperty.call(v, k) });
  const descriptor = (c) => ({ property: c.key, dimension: DIM[c.type], unit: '', defaultValue: DEFAULTS[c.type], animatable: true, controlType: c.type, controlName: c.name,
    authored: { availability: 'available', keyCount: 0, hasStaticOverride: false, expressionEnabled: false },
    availability: { metadata: 'available', applicability: 'available', propertyWrite: 'unavailable', reason: 'read-only' } });
  const builtin = { property: 'opacity', dimension: 1, unit: '%', defaultValue: [100], animatable: true, controlType: null, controlName: null,
    authored: { availability: 'available', keyCount: 0, hasStaticOverride: false, expressionEnabled: false },
    availability: { metadata: 'available', applicability: 'available', propertyWrite: 'unavailable', reason: 'read-only' } };
  const ports = {
    readContext: () => context,
    async query(selector) {
      queries.push(selector);
      const all = [builtin, ...controls.map(descriptor)], { offset, limit } = selector.view;
      const page = (items) => ({ instanceId: pin.instanceId, documentId: pin.documentId, atRevision: pin.contentRevision, documentSnapshotId: pin.documentSnapshotId,
        kind: 'properties', target: selector.view.target, items, nextOffset: offset + items.length < all.length ? offset + items.length : null,
        unavailableCategories: [{ category: 'propertyWrites', state: 'unavailable', reason: 'separate' }] });
      // Like the native page owner: a page stops early rather than exceed the 3072-byte inner response bound.
      let items = all.slice(offset, offset + limit);
      while (JSON.stringify(page(items)).length > 3000) items = items.slice(0, -1);
      return page(items);
    },
    async editProperty(payload, expectedPin, isCurrent) {
      assert.equal(isCurrent(), true);
      assert.deepEqual(expectedPin, pin);
      validate.editPayload(payload);
      sent.push(structuredClone(payload));
      if (options.refuse) throw new Error(options.refuse);
      const a = payload.action, at = controls.findIndex((c) => c.key === payload.property);
      let applied = true;
      if (a.kind === 'addControl') {
        if (at >= 0 || controls.some((c) => norm(c.name) === norm(a.name))) throw new Error('invalid_request');
        controls.push({ key: payload.property, name: a.name.trim(), type: a.controlType });
      } else if (at < 0) throw new Error('not_found');
      else if (a.kind === 'renameControl') controls[at].name = a.name.trim();
      else if (a.kind === 'removeControl') { if ((options.bound || []).includes(payload.property)) throw new Error('unavailable'); controls.splice(at, 1); }
      else { const to = at + a.direction; if (to < 0 || to >= controls.length) applied = false; else [controls[at], controls[to]] = [controls[to], controls[at]]; }
      return { applied, historyEntriesAdded: applied ? 1 : 0 };
    },
  };
  const throws = (name) => () => { throw new Error('LEGACY ' + name); };
  const SM = { t: (key) => ({ ctxExprControlsEllipsis: 'Expression controls…', ctrlMenuAddEllipsis: 'Add a control…', ctrlType_point: 'Point', ctrlType_number: 'Number',
    ctrlMenuExistingColon: 'Existing controls:', ctrlMenuRenameEllipsis: 'Rename…', ctrlMenuMoveUp: 'Move up', ctrlMenuMoveDown: 'Move down', ctrlMenuDelete: 'Delete',
    toastControlNameTaken: 'name taken', toastControlAddedPrefix: 'Control “', toastControlAddedSuffix: '” added', toastControlDeletedPrefix: 'Control “', toastControlDeletedSuffix: '” deleted' }[key] || key) };
  const host = { document: { createElement: element }, SM, showToast: (m) => toasts.push(m), showContextMenu: (x, y, items) => menus.push({ x, y, items }),
    prompt: throws('prompt'), addExprControl: throws('addExprControl'), renameExprControl: throws('renameExprControl'),
    removeExprControl: throws('removeExprControl'), moveExprControl: throws('moveExprControl'), pushUndo: throws('pushUndo') };
  return { host, ports, sent, toasts, menus, queries, controls };
}
const labels = (menu) => menu.items.filter((i) => !i.sep).map((i) => i.label);
async function open(h, built) { named(built.node, 'native-motion-controls-button').fire('click'); await settle(); return h.menus[h.menus.length - 1]; }

test('the button opens the baseline menu with the existing controls read from the native metadata, page by page', async () => {
  const many = Array.from({ length: 10 }, (_, i) => ({ key: 'xc_k' + i, name: 'C' + i, type: i % 2 ? 'point' : 'number' }));
  const h = harness({ controls: many });
  const built = Rows.build(h.host, h.ports, TARGET);
  assert.equal(named(built.node, 'native-motion-controls-button').textContent, 'Expression controls…');
  const menu = await open(h, built);
  assert.deepEqual(labels(menu).slice(0, 2), ['Add a control…', 'Existing controls:']);
  assert.equal(labels(menu).length, 2 + 10, 'the builtin property is not a control');
  assert.equal(labels(menu)[2], 'C0  ·  Number');
  assert.equal(labels(menu)[3], 'C1  ·  Point');
  assert.equal(menu.items[1].sep, true);
  assert.equal(menu.items[2].disabled, true);
  assert.ok(h.queries.length >= 2 && h.queries[0].view.offset === 0 && h.queries.every((q, i) => i === 0 || q.view.offset > h.queries[i - 1].view.offset));
  assert.deepEqual(h.sent, []);
});

test('add: type, inline name, Enter sends one addControl with a fresh xc_ key and the baseline toast; the legacy writers are never reached', async () => {
  const h = harness();
  const built = Rows.build(h.host, h.ports, TARGET);
  const menu = await open(h, built);
  menu.items[0].action();
  const types = h.menus[h.menus.length - 1];
  assert.deepEqual(labels(types), ['Number', 'checkbox', 'angle', 'Point', 'color'].map((t) => t));
  types.items[3].action();
  const editor = named(built.node, 'native-motion-control-editor'), input = walk(editor).find((n) => n.tagName === 'input');
  assert.equal(editor.style.display, '');
  assert.equal(input.value, 'Point');
  input.value = '  Where  ';
  input.fire('keydown', { key: 'Enter' });
  await settle();
  assert.equal(h.sent.length, 1);
  const { action, property, command, target } = h.sent[0];
  assert.deepEqual(action, { kind: 'addControl', controlType: 'point', name: 'Where' });
  assert.match(property, /^xc_[a-z0-9]+_[a-z0-9]{1,4}$/);
  assert.equal(command, 'object.property.edit');
  assert.deepEqual(target, TARGET);
  assert.deepEqual(h.toasts, ['Control “Where” added']);
  assert.deepEqual(h.controls.map((c) => c.name), ['Where']);
  assert.equal(editor.style.display, 'none');
});

test('add: an empty name, Escape and a case-insensitive clash send nothing (the clash shows the baseline toast)', async () => {
  const h = harness({ controls: [{ key: 'xc_a', name: 'Speed', type: 'number' }] });
  const built = Rows.build(h.host, h.ports, TARGET);
  const input = () => walk(named(built.node, 'native-motion-control-editor')).find((n) => n.tagName === 'input');
  const start = async () => { const menu = await open(h, built); menu.items[0].action(); h.menus[h.menus.length - 1].items[0].action(); };
  await start(); input().value = '   '; input().fire('keydown', { key: 'Enter' }); await settle();
  await start(); input().value = 'Other'; input().fire('keydown', { key: 'Escape' }); await settle();
  assert.deepEqual(h.sent, []);
  await start(); input().value = '  sPEED '; input().fire('keydown', { key: 'Enter' }); await settle();
  assert.deepEqual(h.sent, []);
  assert.deepEqual(h.toasts, ['name taken']);
});

test('rename, move up/down (disabled at the ends) and delete each send one control edit on the control key', async () => {
  const h = harness({ controls: [{ key: 'xc_a', name: 'A', type: 'number' }, { key: 'xc_b', name: 'B', type: 'angle' }, { key: 'xc_c', name: 'C', type: 'number' }] });
  const built = Rows.build(h.host, h.ports, TARGET);
  const sub = async (index) => { const menu = await open(h, built); menu.items[3 + index].action(); return h.menus[h.menus.length - 1]; };
  let second = await sub(0);
  assert.deepEqual(second.items.map((i) => [i.label, !!i.disabled]), [['Rename…', false], ['Move up', true], ['Move down', false], ['Delete', false]]);
  second = await sub(2);
  assert.deepEqual(second.items.map((i) => !!i.disabled), [false, false, true, false]);
  second = await sub(1);
  second.items[0].action();
  const input = walk(named(built.node, 'native-motion-control-editor')).find((n) => n.tagName === 'input');
  assert.equal(input.value, 'B');
  input.value = ' Bee '; input.fire('keydown', { key: 'Enter' }); await settle();
  second.items[1].action(); await settle();
  second.items[2].action(); await settle();
  second.items[3].action(); await settle();
  assert.deepEqual(h.sent.map((p) => [p.property, p.action.kind, p.action.name || p.action.direction]),
    [['xc_b', 'renameControl', 'Bee'], ['xc_b', 'moveControl', -1], ['xc_b', 'moveControl', 1], ['xc_b', 'removeControl', undefined]]);
  assert.deepEqual(h.controls.map((c) => c.name), ['A', 'C']);
  assert.deepEqual(h.toasts, ['Control “B” deleted'], 'the toast carries the name the menu showed');
});

test('rename onto another control name is refused locally; a refused owner edit shows the refusal and leaves the list unchanged', async () => {
  const h = harness({ controls: [{ key: 'xc_a', name: 'A', type: 'number' }, { key: 'xc_b', name: 'B', type: 'number' }], bound: ['xc_a'] });
  const built = Rows.build(h.host, h.ports, TARGET);
  const menu = await open(h, built);
  menu.items[3].action();
  const sub = h.menus[h.menus.length - 1];
  sub.items[0].action();
  const input = walk(named(built.node, 'native-motion-control-editor')).find((n) => n.tagName === 'input');
  input.value = ' b '; input.fire('keydown', { key: 'Enter' }); await settle();
  assert.deepEqual(h.sent, []);
  assert.deepEqual(h.toasts, ['name taken']);
  sub.items[3].action(); await settle();
  assert.equal(h.sent.length, 1);
  assert.match(h.toasts[1], /Control edit refused: unavailable/);
  assert.deepEqual(h.controls.map((c) => c.name), ['A', 'B']);
});

test('an unreadable control list shows the failure and opens no menu; dispose closes the editor and blocks the button', async () => {
  const h = harness();
  h.ports.query = async () => { throw new Error('offline'); };
  const built = Rows.build(h.host, h.ports, TARGET);
  named(built.node, 'native-motion-controls-button').fire('click'); await settle();
  assert.equal(h.menus.length, 0);
  assert.match(h.toasts[0], /Controls are unavailable/);
  built.dispose();
  named(built.node, 'native-motion-controls-button');
  assert.equal(built.refresh(), false);
});

test('only a layer target builds rows; compose merges a link rows object and the control rows into one refresh/dispose', () => {
  const h = harness();
  assert.equal(Rows.build(h.host, h.ports, { contextId: 'scene-root', layerUid: 'L', strokeId: 'S' }), null);
  assert.equal(Rows.build(h.host, { readContext() {} }, TARGET), null);
  const log = [];
  const link = { node: element('div'), refresh: () => { log.push('link-refresh'); return true; }, dispose: () => log.push('link-dispose') };
  const ctl = Rows.build(h.host, h.ports, TARGET);
  assert.equal(Rows.compose(h.host, null, null), null);
  assert.equal(Rows.compose(h.host, link, null), link);
  assert.equal(Rows.compose(h.host, null, ctl), ctl);
  const both = Rows.compose(h.host, link, ctl);
  assert.deepEqual(both.node.children, [link.node, ctl.node]);
  assert.equal(both.refresh(), true);
  both.dispose();
  assert.deepEqual(log, ['link-refresh', 'link-dispose']);
  assert.equal(ctl.refresh(), false);
});
