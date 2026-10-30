'use strict';
// #2376 G26: the History panel on a native object project. The real history-panel.js and native-object-history-ui.js run in one vm context
// against a recording DOM and a session double. The tests observe the rows (labels, cursor marker, dimmed future), that a click makes ONE
// session jump with the baseline's step count, that the panel follows the revision, that a late answer is dropped, and that no legacy
// stack or writer is ever reached.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');

const read = (file) => fs.readFileSync(path.join(__dirname, '..', 'src', 'js', file), 'utf8');
const LABELS = ['Draw', 'Property: Opacity', 'Rename Layer', 'Insert Frame', 'Move Layer Range'];

class Node {
  constructor(tag) { this.tag = tag; this.children = []; this.handlers = {}; this.attrs = {}; this.style = {}; this.className = ''; this._text = ''; this.parent = null; }
  get firstChild() { return this.children[0] || null; }
  set innerHTML(v) { this.children = []; this._text = ''; }
  get textContent() { return this._text + this.children.map((c) => c.textContent).join(''); }
  set textContent(v) { this._text = String(v); this.children = []; }
  get classList() { const n = this; return { contains: (c) => n.className.split(/\s+/).includes(c), add: (c) => { if (!n.classList.contains(c)) n.className = (n.className + ' ' + c).trim(); },
    remove: (c) => { n.className = n.className.split(/\s+/).filter((x) => x && x !== c).join(' '); } }; }
  setAttribute(k, v) { this.attrs[k] = v; }
  appendChild(c) { c.parent = this; this.children.push(c); return c; }
  removeChild(c) { this.children.splice(this.children.indexOf(c), 1); }
  addEventListener(type, fn) { (this.handlers[type] = this.handlers[type] || []).push(fn); }
  click() { (this.handlers.click || []).forEach((fn) => fn({ stopPropagation() {}, target: this })); }
  scrollIntoView() { this.scrolled = true; }
  getBoundingClientRect() { return { bottom: 20, right: 100 }; }
  find(cls) { return this.children.filter((c) => c.className.split(/\s+/).includes(cls)); }
}

function harness(options) {
  options = options || {};
  const legacy = [], jumps = [], reads = [], timers = [], frames = [], held = [];
  const pop = new Node('div');
  const sandbox = { console, setTimeout: (fn, ms) => { timers.push({ fn, ms }); return timers.length; }, clearTimeout() {}, innerWidth: 1000,
    requestAnimationFrame: (fn) => { frames.push(fn); return frames.length; } };
  sandbox.window = sandbox; sandbox.globalThis = sandbox;
  sandbox.document = { readyState: 'complete', getElementById: (id) => (id === 'history-pop' ? pop : id === 'history-btn' ? new Node('button') : null),
    createElement: (tag) => new Node(tag), createTextNode: (text) => { const n = new Node('#text'); n._text = text; return n; }, addEventListener() {} };
  sandbox.state = { undoStack: [], undoLabels: [], redoStack: [], redoLabels: [], layers: [], activeLayerIdx: 0, currentFrame: 0 };
  sandbox.SM = { t: (key) => (key === 'hsNoActionYet' ? 'Aucune action pour le moment' : key) };
  sandbox.showToast = () => {};
  for (const name of ['undo', 'redo']) Object.defineProperty(sandbox, name, { configurable: true, get() { legacy.push(name); throw new Error('legacy ' + name + ' reached'); } });
  sandbox.NemoNativeOpacityCutover = { blocksLegacy: () => true };
  sandbox.n20AllowLegacyWrite = () => false;
  let revision = 7, undoDepth = 3, redoDepth = 2;
  const info = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 'native-object:d:' + revision });
  const session = {
    blocksLegacy: () => true, isActive: () => true, info,
    async readHistory(offset) {
      const asked = info();
      reads.push({ offset, revision });
      if (options.hold) await new Promise((resolve) => held.push(resolve));
      if (options.fail) throw Object.assign(new Error('refused'), { code: options.fail, reason: 'history_stale' });
      const total = undoDepth + redoDepth, from = offset === undefined ? Math.max(0, undoDepth - 20) : offset, to = Math.min(total, from + 32);
      const entries = []; for (let i = from; i < to; i++) entries.push({ index: i, label: options.label ? options.label(i) : LABELS[i % LABELS.length] });
      return Object.freeze({ info: asked, undoDepth, redoDepth, offset: from, entries });
    },
    async jumpHistory(action, steps) {
      jumps.push([action, steps]);
      if (options.jumpFails) throw new Error('refused');
      if (action === 'undo') { undoDepth -= steps; redoDepth += steps; } else { undoDepth += steps; redoDepth -= steps; }
      revision += 1;
    },
  };
  if (!options.noSession) sandbox.NemoNativeObjectProject = { desktop: () => session };
  vm.createContext(sandbox);
  vm.runInContext(read('adapters/native-object-history-ui.js'), sandbox, { filename: 'native-object-history-ui.js' });
  // The adapter is a classic script: its global is a vm-realm object, so read it from the context.
  vm.runInContext(read('history-panel.js'), sandbox, { filename: 'history-panel.js' });
  const flush = async () => { for (let n = 0; n < 6; n++) await new Promise((resolve) => setImmediate(resolve)); };
  const rows = () => pop.children.filter((c) => c.className.split(/\s+/).includes('hist-item'));
  const labels = () => rows().map((r) => r.children[1].children[0].textContent);
  return { sandbox, pop, session, legacy, jumps, reads, timers, frames, held, flush, rows, labels,
    open: async () => { vm.runInContext('toggleHistoryPopover()', sandbox); await flush(); },
    tick: async () => { const t = timers.splice(0); t.forEach((x) => x.fn()); await flush(); },
    bump: () => { revision += 1; }, set: (u, r) => { undoDepth = u; redoDepth = r; } };
}

test('the open panel lists the entry labels, the current marker at the cursor, and dims the redoable rows', async () => {
  const h = harness();
  await h.open();
  assert.ok(h.pop.classList.contains('open'));
  assert.deepEqual(h.labels(), ['Draw', 'Property: Opacity', 'Rename Layer', 'État actuel', 'Insert Frame', 'Move Layer Range']);
  assert.deepEqual(h.rows().map((r) => r.className), ['hist-item', 'hist-item', 'hist-item', 'hist-item current', 'hist-item future', 'hist-item future']);
  assert.equal(h.pop.find('hist-header')[0].textContent, '6 étatsEffacer');
  assert.deepEqual(h.reads.map((r) => r.offset), [undefined], 'one read, the default window');
  assert.deepEqual(h.legacy, []);
});

test('Effacer is shown disabled with its reason and does nothing', async () => {
  const h = harness();
  await h.open();
  const clear = h.pop.find('hist-header')[0].children.find((c) => c.className.includes('hist-clear'));
  assert.ok(clear.className.includes('disabled'));
  assert.equal(clear.attrs['aria-disabled'], 'true');
  assert.match(clear.title, /cannot be cleared/);
  clear.click();
  await h.flush();
  assert.deepEqual(h.jumps, []);
  assert.equal(h.reads.length, 1);
});

test('a click on a done row is one undo of (done - index) steps, on a redoable row one redo through it, and never a legacy undo', async () => {
  const h = harness();
  await h.open();
  h.rows()[0].click();                       // index 0 of 3 done: three steps back
  await h.flush();
  assert.deepEqual(h.jumps, [['undo', 3]]);
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.rows().map((r) => r.className).slice(0, 2), ['hist-item current', 'hist-item future'], 'the marker is at the top after the jump');
  h.rows()[3].click();                       // index 2 (third redoable row, nearest first): redo through it
  await h.flush();
  assert.deepEqual(h.jumps, [['undo', 3], ['redo', 3]]);
  assert.equal(h.jumps.length, 2, 'one native call per click');
  assert.deepEqual(h.legacy, []);
});

test('the nearest entries are one step away in both directions', async () => {
  const h = harness();
  await h.open();
  h.rows()[2].click(); await h.flush();      // last done entry
  h.rows()[3].click(); await h.flush();      // first redoable entry (after the marker at index 2)
  assert.deepEqual(h.jumps, [['undo', 1], ['redo', 1]]);
});

test('a refused jump leaves the panel on the current native state and does not stay busy', async () => {
  const h = harness({ jumpFails: true });
  await h.open();
  h.rows()[0].click(); await h.flush();
  h.rows()[1].click(); await h.flush();
  assert.equal(h.jumps.length, 2, 'the second click is not swallowed by a stuck busy flag');
  assert.equal(h.labels().length, 6);
});

test('the panel follows the revision while open, and a closed panel reads nothing', async () => {
  const h = harness();
  await h.open();
  assert.equal(h.reads.length, 1);
  await h.tick();
  assert.equal(h.reads.length, 1, 'an unchanged pin costs a comparison, not a read');
  h.set(4, 0); h.bump();
  await h.tick();
  assert.equal(h.reads.length, 2);
  assert.deepEqual(h.labels(), ['Draw', 'Property: Opacity', 'Rename Layer', 'Insert Frame', 'État actuel']);
  h.pop.classList.remove('open');
  h.bump();
  await h.tick();
  assert.equal(h.reads.length, 2, 'closing the popover stops the watch');
  assert.equal(h.timers.length, 0);
});

test('an answer that lands after the revision moved is dropped and the panel asks again', async () => {
  const h = harness({ hold: true });
  await h.open();
  assert.deepEqual(h.rows(), [], 'nothing is painted before the first answer');
  h.bump(); h.set(5, 0);
  h.held.splice(0).forEach((resolve) => resolve());
  await h.flush();
  assert.deepEqual(h.rows(), [], 'the stale page is never shown');
  await h.tick();
  h.held.splice(0).forEach((resolve) => resolve());
  await h.flush();
  assert.equal(h.labels().length, 6, 'the current page: 5 done entries and the marker');
  assert.equal(h.labels()[5], 'État actuel');
});

test('an empty history says so, and a read failure is said in the panel, not as a legacy list', async () => {
  const h = harness();
  h.set(0, 0);
  await h.open();
  assert.deepEqual(h.rows(), []);
  assert.equal(h.pop.find('hist-empty')[0].textContent, 'Aucune action pour le moment');
  const f = harness({ fail: 'unavailable' });
  await f.open();
  assert.match(f.pop.find('hist-empty')[0].textContent, /could not be read \(history_stale\)/);
  assert.deepEqual(f.legacy, []);
});

test('a long history is paged: rows above and below the window move the window, the marker only shows when the cursor is inside it', async () => {
  const h = harness();
  h.set(60, 40);
  await h.open();
  const [first, ...rest] = h.pop.children.filter((c) => c.className.includes('hist-empty') || c.className.includes('hist-item'));
  assert.match(first.textContent, /^↑ 40/, 'the default window starts 20 entries before the cursor');
  assert.equal(h.labels().filter((l) => l === 'État actuel').length, 1);
  assert.equal(h.rows().length, 32 + 1, '32 entries plus the marker');
  assert.match(rest[rest.length - 1].textContent, /^↓ 28/);
  first.click(); await h.flush();
  assert.deepEqual(h.reads.map((r) => r.offset), [undefined, 8]);
  assert.equal(h.labels().includes('État actuel'), false, 'cursor 60 is outside entries 8..40: no marker is invented');
  h.pop.children.filter((c) => c.className.includes('hist-empty')).pop().click(); await h.flush();
  assert.equal(h.reads[2].offset, 40, 'the next window starts where this one ended');
});

test('without a native object session the panel keeps the explicit unavailable state and reads nothing', async () => {
  const h = harness({ noSession: true });
  await h.open();
  assert.equal(h.pop.children[0].textContent, 'Native history panel is unavailable.');
  assert.deepEqual(h.reads, []);
  assert.deepEqual(h.legacy, []);
});

test('a jump from a page the project has left reloads the panel and sends nothing', async () => {
  const h = harness();
  await h.open();
  const before = h.reads.length;
  h.bump();                                  // a commit lands before the watch refreshed the panel
  h.rows()[0].click(); await h.flush();
  assert.deepEqual(h.jumps, [], 'a stale click is not dispatched');
  assert.deepEqual(h.legacy, []);
  assert.ok(h.reads.length > before, 'the panel read the new state instead');
  h.rows()[0].click(); await h.flush();      // the fresh page jumps normally
  assert.deepEqual(h.jumps, [['undo', 3]]);
});
