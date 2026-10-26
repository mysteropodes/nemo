'use strict';
// CS8-B09 U31 (#2374): the Labs x-sheet under native authority reads every column through the session's whole-row
// cells read (the native timing query) and never the legacy frames; a refused column is shown unavailable, not
// blank; the legacy path keeps its original glyphs; disable removes the panel, the observer and any pending read.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '..', 'src/js/labs/xsheet-panel.js'), 'utf8');
const turn = () => new Promise(setImmediate);
function element(tag, id) {
  const el = { tagName: tag, id: id || '', style: {}, children: [], parentNode: null, html: '', rows: [],
    set innerHTML(v) {
      el.html = v;
      el.rows = [...v.matchAll(/<tr data-frame="(\d+)"/g)].map((m) => {
        const row = { dataset: { frame: m[1] }, listeners: [], addEventListener(kind, fn) { row.listeners.push(fn); }, scrollIntoView() {} };
        return row;
      });
    },
    get innerHTML() { return el.html; },
    appendChild(child) { child.parentNode = el; el.children.push(child); return child; },
    remove() { if (el.parentNode) el.parentNode.children = el.parentNode.children.filter((c) => c !== el); el.parentNode = null; },
    querySelectorAll: () => el.rows,
    querySelector(sel) { const f = /data-frame="(\d+)"/.exec(sel); return el.rows.find((r) => r.dataset.frame === (f && f[1])) || null; } };
  return el;
}
function harness(native) {
  const on = new Set(), features = {}, observers = [], jumps = [], reads = [];
  const body = element('body'), grid = element('div', 'frame-grid');
  const layers = [{ name: 'Ink', frames: [{ isKeyframe: true, strokes: [1] }, { strokes: [] }, { isInterpolated: true }] }];
  if (native) for (const l of layers) Object.defineProperty(l, 'frames', { get() { throw Error('legacy frame read'); } });
  const state = { layers, totalFrames: 3, fps: 24, currentFrame: 1, contentRevision: 5 };
  const pin = native && { documentId: 'doc', documentSnapshotId: 'snap-1', totalFrames: 6, fps: 2,
    layers: [{ layerUid: 'ink', name: 'Ink' }, { layerUid: 'ref', name: 'Ref' }] };
  const session = native && { blocksLegacy: () => true, info: () => pin,
    async readLayerCells(uid, p) {
      reads.push([uid, p.documentSnapshotId]);
      if (uid === 'ref') return null;
      return [[0, 1, 'key', 0, true, false], [1, 2, 'held', 0, true, false], [3, 1, 'interpolated', 3, false, false],
        [4, 1, 'key', 4, false, false], [5, 1, 'held', 4, false, false]];
    } };
  const root = { state, SM: { t: (k) => k }, goToFrame: (f) => jumps.push(f),
    document: { body, createElement: (tag) => element(tag), getElementById: (id) => (id === 'frame-grid' ? grid : null) },
    MutationObserver: function (fn) { this.fn = fn; this.live = false; this.observe = () => { this.live = true; }; this.disconnect = () => { this.live = false; }; observers.push(this); },
    NemoNativeObjectProject: native ? { desktop: () => session } : undefined,
    SMLabs: { isOn: (name) => on.has(name), register(name, spec) { features[name] = spec; } } };
  root.window = root;
  vm.createContext(root);
  vm.runInContext(source, root, { filename: 'xsheet-panel.js' });
  const panel = () => body.children.find((c) => c.id === 'labs-xsheet');
  // Glyph text of every cell, frame by frame: [frame number, column 1, column 2...].
  const cells = () => panel().html.split('<tr data-frame=').slice(1)
    .map((row) => [...row.matchAll(/<td[^>]*>([^<]*)<\/td>/g)].map((m) => m[1]));
  return { root, state, pin, reads, jumps, observers, panel, cells, grid,
    enable() { on.add('xsheet'); features.xsheet.onEnable(); }, disable() { on.delete('xsheet'); features.xsheet.onDisable(); } };
}

test('native authority: every column comes from the cells read; a refused column is unavailable, not blank', async () => {
  const h = harness(true);
  const before = JSON.stringify({ c: h.state.contentRevision, f: h.state.currentFrame });
  h.enable();
  await turn(); await turn();
  assert.deepEqual(h.reads, [['ink', 'snap-1'], ['ref', 'snap-1']]);
  assert.deepEqual(h.cells(), [['1', '●', ''], ['2', '│', ''], ['3', '│', ''], ['4', '◆', ''], ['5', '●', ''], ['6', '', '']],
    'key, held with content, interpolated, an empty key and its empty hold; the refused Ref column stays empty');
  assert.match(h.panel().html, /title="Native timing is unavailable\."[^>]*>Ref —<\/th>/);
  h.observers[0].fn();
  await turn();
  assert.equal(h.reads.length, 2, 'a rebuild on the same snapshot reuses the read');
  h.panel().rows[4].listeners[0]();
  assert.deepEqual(h.jumps, [4]);
  assert.equal(JSON.stringify({ c: h.state.contentRevision, f: h.state.currentFrame }), before);
  h.pin.documentSnapshotId = 'snap-2';
  h.observers[0].fn();
  await turn(); await turn();
  assert.deepEqual(h.reads.slice(2), [['ink', 'snap-2'], ['ref', 'snap-2']], 'a new snapshot reads again');
});

test('legacy path keeps the original glyphs from the stored frames', () => {
  const h = harness(false);
  h.enable();
  assert.deepEqual(h.cells(), [['1', '●'], ['2', ''], ['3', '◆']]);
});

test('disable removes the panel and the grid observer and drops a pending native read', async () => {
  const h = harness(true);
  h.enable();
  h.disable();
  await turn(); await turn();
  assert.equal(h.panel(), undefined, 'the read finishing after disable builds nothing');
  assert.equal(h.observers[0].live, false);
  h.enable();
  await turn(); await turn();
  assert.equal(h.observers.length, 2, 'enable observes again');
  assert.ok(h.panel());
  h.disable();
  assert.equal(h.panel(), undefined);
  assert.equal(h.observers[1].live, false);
});
