'use strict';
// The scene rows of a native project show the baseline frame cells (key dot, held span, tween mark): native-component-grid.js paintScene,
// called by the range grid (native-layer-range-desktop.js) on its own rows. Only the DOM and the session read are simulated.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const Grid = require('../src/js/adapters/native-component-grid.js');
const turn = () => new Promise(setImmediate);
class El {
  constructor(tag) { this.tagName = tag; this.children = []; this.dataset = {}; this.className = ''; this.textContent = ''; this.hidden = false; this.ownerDocument = doc;
    const props = {}; this.style = { setProperty(k, v) { props[k] = v; }, props };
    const self = this; this.classList = { add(c) { if (!self.className.split(' ').includes(c)) self.className = (self.className + ' ' + c).trim(); },
      remove(c) { self.className = self.className.split(' ').filter((x) => x && x !== c).join(' '); }, contains(c) { return self.className.split(' ').includes(c); } }; }
  get firstChild() { return this.children[0] || null; }
  appendChild(x) { this.children.push(x); x.parentNode = this; return x; }
  removeChild(x) { this.children.splice(this.children.indexOf(x), 1); x.parentNode = null; return x; }
  insertBefore(x, ref) { const at = ref ? this.children.indexOf(ref) : this.children.length; this.children.splice(at < 0 ? this.children.length : at, 0, x); x.parentNode = this; return x; }
  querySelector(sel) { const cls = sel.slice(1); return this.children.find((c) => c.className.split(' ').includes(cls)) || null; }
}
const doc = { createElement: (tag) => new El(tag) };
function bench(answers) {
  const info = { contentRevision: 3, totalFrames: 6, layers: [{ layerUid: 'A', color: '#5FA875' }, { layerUid: 'B' }] };
  const grid = new El('div'), status = new El('div'); status.className = 'native-object-timing-unavailable'; status.textContent = 'Key — Source frame 1 — Content'; grid.appendChild(status);
  const rows = info.layers.map(() => { const row = new El('div'), bar = new El('div'); bar.className = 'layer-inout-bar'; row.appendChild(bar); return row; });
  const asked = [];
  const session = { readPresentedObjectSession: () => ({}), readLayerCells: async (uid, pin) => { asked.push(uid); assert.equal(pin, info); const a = answers[uid]; return typeof a === 'function' ? a() : a; } };
  const host = { document: doc, FC: 10, state: { appMode: 'anim2d', currentFrame: 1, waIn: 0, waOut: 4 } };
  return { host, view: { info, rows, grid }, session, asked, status };
}
const KEY_HELD = [[0, 1, 'key', 0, true, false], [1, 3, 'held', 0, true, false], [4, 1, 'interpolated', 0, true, false], [5, 1, 'key', 5, false, false]];
test('each scene row gets one display-only baseline cell per frame, under the bar, with the runs of its own layer', async () => {
  const b = bench({ A: KEY_HELD, B: [[0, 6, 'unexposed', null, false, false]] });
  assert.equal(Grid.paintScene(b.host, b.view, b.session), true); await turn();
  const [a, other] = b.view.rows;
  assert.equal(a.children.length, 7); assert.equal(a.children[6].className, 'layer-inout-bar', 'the bar stays on top of the cells');
  const cells = a.nativeCells; assert.equal(cells.length, 6);
  assert.deepEqual(cells.map((c) => c.className), ['fc kf-full', 'fc cur span-full', 'fc span-full', 'fc span-full span-end', 'fc tw', 'fc outside-wa kf-empty']);
  assert.equal(cells[0].children[0].className, 'km fl'); assert.equal(cells[5].children[0].className, 'km hl'); assert.equal(cells[4].children[0].className, 'km td');
  for (const [i, c] of cells.entries()) { assert.equal(c.style.pointerEvents, 'none', 'no gesture is taken from the header, the bars or the keys'); assert.equal(c.style.left, i * 10 + 'px'); assert.equal(c.dataset.frame, String(i)); assert.equal(c.dataset.layer, '0'); }
  assert.equal(cells[0].style.props['--dot-color'], '#5FA875');
  assert.deepEqual(other.nativeCells.map((c) => c.className), ['fc', 'fc cur', 'fc', 'fc', 'fc', 'fc outside-wa'], 'an unexposed row stays blank');
  assert.deepEqual(b.asked, ['A', 'B']);
});
test('a frame-only repaint moves the current cell and starts no read', async () => {
  const b = bench({ A: KEY_HELD, B: [] }); Grid.paintScene(b.host, b.view, b.session); await turn();
  const cells = b.view.rows[0].nativeCells; b.host.state.currentFrame = 4; Grid.paintScene(b.host, b.view, b.session); await turn();
  assert.equal(cells[1].className, 'fc span-full'); assert.equal(cells[4].className, 'fc tw cur'); assert.equal(b.view.rows[0].nativeCells, cells); assert.deepEqual(b.asked, ['A', 'B']);
});
test('a refused read leaves the row blank and is asked again by the next paint; a thrown read is contained', async () => {
  let n = 0; const b = bench({ A: () => (n++ ? KEY_HELD : null), B: () => { throw new Error('refused'); } });
  Grid.paintScene(b.host, b.view, b.session); await turn();
  assert.equal(b.view.rows[0].nativeCells[0].className, 'fc'); assert.equal(b.status.style.position, '', 'the text stays while a row has no cells');
  Grid.paintScene(b.host, b.view, b.session); await turn();
  assert.equal(b.view.rows[0].nativeCells[0].className, 'fc kf-full'); assert.deepEqual(b.asked, ['A', 'B', 'A', 'B']);
});
test('a still timeline asks a refused row again on its own, a bounded number of times', async () => {
  let n = 0; const b = bench({ A: () => (++n < 3 ? null : KEY_HELD), B: [] }), timers = []; b.host.setTimeout = (fn, ms) => { assert.equal(ms, 250); timers.push(fn); };
  Grid.paintScene(b.host, b.view, b.session); await turn(); assert.equal(timers.length, 1); timers.shift()(); await turn(); timers.shift()(); await turn();
  assert.equal(b.view.rows[0].nativeCells[0].className, 'fc kf-full'); assert.equal(timers.length, 0); assert.equal(b.status.style.position, 'absolute');
  const never = bench({ A: null, B: [] }), again = []; never.host.setTimeout = (fn) => again.push(fn); Grid.paintScene(never.host, never.view, never.session); await turn();
  for (let i = 0; i < 20 && again.length; i++) { again.shift()(); await turn(); } assert.equal(never.asked.filter((u) => u === 'A').length, 9, 'the first ask and eight more');
});
test('an answer for rows that were replaced meanwhile is dropped', async () => {
  let release; const gate = new Promise((r) => { release = r; }); const b = bench({ A: () => gate.then(() => KEY_HELD), B: [] });
  Grid.paintScene(b.host, b.view, b.session); const cells = b.view.rows[0].nativeCells; b.view.rows = b.view.rows.slice(); release(); await turn(); await turn();
  assert.equal(cells[0].className, 'fc');
});
test('the timing text leaves the layout once every shown row has its cells and never comes back on a text change (no row shift per frame)', async () => {
  const b = bench({ A: KEY_HELD, B: [] }); Grid.paintScene(b.host, b.view, b.session); await turn();
  assert.equal(b.status.style.position, 'absolute'); assert.equal(b.status.style.width, '1px'); assert.equal(b.status.style.display, undefined, 'kept for assistive technology, not display:none');
  b.status.textContent = 'Native timing is unavailable.'; b.host.state.currentFrame = 2; Grid.paintScene(b.host, b.view, b.session); await turn();
  assert.equal(b.status.style.position, 'absolute', 'the per-frame blanking of the text does not bring the line back');
});
test('Motion rows carry no frame cells: none are painted there, and those painted before the switch are removed and come back in a drawing mode', async () => {
  const b = bench({ A: KEY_HELD, B: [] }); b.host.state.appMode = 'motion'; assert.equal(Grid.paintScene(b.host, b.view, b.session), true); await turn();
  assert.equal(b.view.rows[0].children.length, 1); assert.deepEqual(b.asked, []);
  b.host.state.appMode = 'anim2d'; Grid.paintScene(b.host, b.view, b.session); await turn(); assert.equal(b.view.rows[0].children.length, 7); assert.equal(b.status.style.position, 'absolute');
  b.host.state.appMode = 'motion'; Grid.paintScene(b.host, b.view, b.session); await turn();
  assert.equal(b.view.rows[0].children.length, 1); assert.equal(b.view.rows[0].children[0].className, 'layer-inout-bar'); assert.equal(b.status.style.position, '', 'the text is the display again');
  b.host.state.appMode = 'anim2d'; Grid.paintScene(b.host, b.view, b.session); await turn(); assert.equal(b.view.rows[0].nativeCells[0].className, 'fc kf-full');
});
test('a busy native queue is waited for through the shared presentation wait before the read', async () => {
  const b = bench({ A: KEY_HELD, B: [] }); let shown = null, waits = 0; b.session.readPresentedObjectSession = () => shown;
  b.host.NemoNativePresentationWait = { await: async (_h, o) => { waits++; assert.equal(o.read(), null); await null; shown = {}; } };
  Grid.paintScene(b.host, b.view, b.session); assert.deepEqual(b.asked, [], 'not read while the queue is busy'); await turn(); await turn();
  assert.equal(waits, 2); assert.deepEqual(b.asked, ['A', 'B']); assert.equal(b.view.rows[0].nativeCells[0].className, 'fc kf-full');
});
test('the range grid paints the cells on a rebuild and on a frame-only reuse, and a failure there never breaks the bars', () => {
  const source = fs.readFileSync(path.join(__dirname, '../src/js/adapters/native-layer-range-desktop.js'), 'utf8');
  assert.equal(source.split('sceneCells(root,w,s);').length - 1, 2); assert.match(source, /function sceneCells\(root,w,s\)\{try\{[^\n]*paintScene\(root,w,s\);\}catch/);
  assert.equal(Grid.paintScene({ state: {} }, null, null), false);
});
