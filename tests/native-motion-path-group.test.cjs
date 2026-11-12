'use strict';
// C-motion-canvas-4 (#2520/#2525): an element's "Path" group in the native Motion panel (baseline motion.js@1065e9d8 renderPathVertexGroup): a collapsed
// header while the element has vertices, the `vtxN` rows (after the standard ones, labelled Vertex N) only while it is open, one open group at a time.
const test = require('node:test'), assert = require('node:assert/strict');
const Panel = require('../src/js/adapters/native-motion-property-panel.js');

class Node {
  constructor(doc, tag) { this.ownerDocument = doc; this.tagName = tag.toUpperCase(); this.children = []; this.parentNode = null; this.style = {}; this.dataset = {}; this.attrs = {}; this.listeners = new Map(); this._text = ''; this.className = ''; this.hidden = false; }
  set textContent(v) { this.replaceChildren(); this._text = String(v); } get textContent() { return this._text + this.children.map((c) => c.textContent).join(''); }
  set innerHTML(v) { this._html = v; } get innerHTML() { return this._html || ''; }
  appendChild(n) { if (n.parentNode) n.parentNode.removeChild(n); n.parentNode = this; this.children.push(n); return n; }
  removeChild(n) { this.children.splice(this.children.indexOf(n), 1); n.parentNode = null; return n; }
  replaceChildren(...nodes) { this.children.forEach((n) => { n.parentNode = null; }); this.children = []; this._text = ''; nodes.forEach((n) => this.appendChild(n)); }
  setAttribute(k, v) { this.attrs[k] = String(v); } contains(n) { for (; n; n = n.parentNode) if (n === this) return true; return false; }
  addEventListener(t, fn) { if (!this.listeners.has(t)) this.listeners.set(t, []); this.listeners.get(t).push(fn); }
  removeEventListener(t, fn) { this.listeners.set(t, (this.listeners.get(t) || []).filter((f) => f !== fn)); }
  remove() { if (this.parentNode) this.parentNode.removeChild(this); }
  async fire(t) { for (const fn of this.listeners.get(t) || []) await fn.call(this, { type: t, target: this, preventDefault() {}, stopPropagation() {} }); }
}
const walk = (n) => [n, ...n.children.flatMap(walk)];
const doc = { createElement: (t) => new Node(doc, t) };
const tick = () => new Promise((r) => setImmediate(r));

const PIN = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 's' };
const T = { contextId: 'scene-root', layerUid: 'L', strokeId: 'S' };
const row = (property, dimension) => ({ property, dimension, unit: 'px', defaultValue: new Array(dimension).fill(0), animatable: true, controlType: null, controlName: null,
  authored: { availability: 'available', keyCount: 0, hasStaticOverride: false, expressionEnabled: false }, availability: { metadata: 'available', applicability: 'available', propertyWrite: 'unavailable', reason: 'read-only' } });
function host(vertexCount) {
  const seen = [], opened = [];
  const ctx = { pin: PIN, frame: 0, viewGeneration: 1, presented: { ...PIN, available: true, presentation: { ...PIN, owner: 'native', status: 'presented', frame: 0 } } };
  const base = [row('position', 2), row('rotation', 1)], vtx = Array.from({ length: vertexCount }, (_, i) => row('vtx' + i, 2));
  const query = async (q) => {
    seen.push(q.view); const v = q.view, categories = [{ category: 'propertyWrites', state: 'unavailable', reason: 'x' }];
    const out = { instanceId: 'i', documentId: 'd', atRevision: 4, documentSnapshotId: 's', kind: v.kind, target: v.kind === 'targets' ? null : { ...v.target }, unavailableCategories: categories };
    if (v.kind === 'targets') return { ...out, items: [{ kind: 'context', contextId: 'scene-root' }, { kind: 'layer', target: { contextId: 'scene-root', layerUid: 'L' } }, { kind: 'element', target: { ...T } }], nextOffset: null };
    const all = v.includeVertices ? base.concat(vtx) : base, items = all.slice(v.offset, v.offset + Math.min(v.limit, 3)), next = v.offset + items.length;
    return { ...out, items: JSON.parse(JSON.stringify(items)), nextOffset: next < all.length ? next : null };
  };
  const container = doc.createElement('div');
  const panel = Panel.create(container, { document: doc, query, readContext: () => ctx, readSelection: () => ({ ...T }), readSelections: () => [{ ...T }], readContextId: () => 'scene-root',
    pathOpened: (t) => opened.push(t), translate: (k) => k, isMotion: () => true });
  return { panel, container, seen, opened, rows: () => walk(container).filter((n) => /motion-prop-row/.test(n.className)).map((n) => n.dataset.property),
    header: () => walk(container).find((n) => /motion-path-group/.test(n.className)) };
}

test('an element with vertices shows a collapsed Path header and no vertex rows; opening it lists the rows after the standard ones, labelled Vertex N', async () => {
  const h = host(5);
  await h.panel.refresh(true); await tick();
  assert.deepEqual(h.rows(), ['position', 'rotation'], 'the standard rows only while the group is closed');
  assert.ok(h.header(), 'the Path header is there'); assert.match(h.header().textContent, /^▸Path$/);
  assert.ok(h.seen.some((v) => v.kind === 'properties' && v.includeVertices === true && v.offset === 2 && v.limit === 8), 'a one-page probe past the standard rows discovers the vertices');
  await h.header().fire('click'); await tick(); await tick();
  assert.deepEqual(h.rows(), ['position', 'rotation', 'vtx0', 'vtx1', 'vtx2', 'vtx3', 'vtx4']);
  assert.match(h.header().textContent, /^▾Path$/);
  assert.deepEqual(h.opened, [T], 'the canvas dots are told which element opened its group');
  const names = walk(h.container).filter((n) => /motion-prop-name/.test(n.className)).map((n) => n.textContent);
  assert.deepEqual(names.slice(-2), ['Vertex 4', 'Vertex 5']);
  await h.header().fire('click'); await tick(); await tick();
  assert.deepEqual(h.rows(), ['position', 'rotation']); assert.deepEqual(h.opened, [T, null], 'closing tells the canvas too');
});

test('an element without vertices has no Path group', async () => {
  const h = host(0);
  await h.panel.refresh(true); await tick();
  assert.equal(h.header(), undefined); assert.deepEqual(h.rows(), ['position', 'rotation']);
});
