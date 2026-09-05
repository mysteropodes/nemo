'use strict';
// Regressions for the R03 surface-inventory generator (scripts/nemo/inventory.cjs).
// Included in `npm test` / `verify` through tests/nemo-inventory.test.cjs.
// Run directly:  node --test scripts/nemo/inventory.test.cjs
//
// Two sources are analysed: the real src/ (schema, explicit-unmapped rule,
// binding shapes the shipped code uses, the staleness gate against a scratch
// directory and `--check` on the committed tree) and a SYNTHETIC tree written
// to a temp directory, which holds the counterexamples the review of PR #944
// reproduced (a comment-only listener and a lookup-only control both came back
// `inventoried` with `click`) next to every binding shape that must keep
// binding. The synthetic tree is the executable definition of "bound".
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const inventory = require('./inventory.cjs');
const ROOT = path.resolve(__dirname, '..', '..');

let built = null;
function inv() { if (!built) built = inventory.build(); return built; }
function rows() { return inv().rows; }
function byId(id) { const r = rows().find((x) => x.id === id); assert.ok(r, 'row ' + id + ' exists'); return r; }

const REQUIRED = ['id', 'kind', 'area', 'surface', 'capability', 'handler', 'events', 'exposure', 'mcp', 'sdk', 'consumers', 'platforms', 'status', 'nextGate', 'source'];

// ---------------------------------------------------------------------------
// the shipped source
// ---------------------------------------------------------------------------

test('every row carries the full schema and ids are unique', () => {
  const d = inv();
  assert.equal(d.schema, 'nemo.inventory/1');
  assert.match(d.inputsDigest, /^[0-9a-f]{64}$/);
  assert.ok(d.rows.length > 500, 'expected hundreds of rows, got ' + d.rows.length);
  const seen = new Set();
  for (const r of d.rows) {
    for (const k of REQUIRED) assert.ok(k in r, r.id + ' lacks ' + k);
    assert.ok(Array.isArray(r.handler) && Array.isArray(r.events) && Array.isArray(r.exposure), r.id);
    assert.ok(d.states.includes(r.status), r.id + ' has unknown status ' + r.status);
    assert.ok(!seen.has(r.id), 'duplicate row id ' + r.id);
    seen.add(r.id);
  }
  for (const k of ['dom', 'shortcut', 'menu', 'labs', 'sdk']) assert.ok(d.counts.byKind[k] > 0, 'no rows of kind ' + k);
});

test('a row without a handler is explicit: unmapped or unavailable-with-reason, never inventoried', () => {
  for (const r of rows()) {
    if (r.status === 'inventoried') assert.ok(r.handler.length > 0, r.id + ' is inventoried without a handler');
    if (r.status === 'unmapped') assert.equal(r.handler.length, 0, r.id + ' is unmapped but has a handler');
    if (r.status === 'unavailable-with-reason') assert.ok(r.reason && r.reason.length > 10, r.id + ' lacks a reason');
  }
});

test('an inventoried dom row names an event registration, never a bare lookup', () => {
  for (const r of rows()) {
    if (r.kind !== 'dom' || r.status !== 'inventoried') continue;
    assert.ok(r.events.length > 0, r.id + ' is inventoried without an event (handler ' + r.handler + ')');
    for (const ev of r.events) assert.match(ev, /^[a-z]+$|^\(dynamic\)$/, r.id + ' event ' + ev);
  }
});

test('unmapped rows are explicit: no handler, no events, and the lookups that merely read or write them are recorded', () => {
  const unmapped = rows().filter((r) => r.status === 'unmapped');
  const md = inventory.renderOutputs(inv())['SURFACES.md'];
  for (const r of unmapped) {
    assert.deepEqual(r.handler, [], r.id);
    assert.deepEqual(r.events, [], r.id);
    assert.match(r.nextGate, /^R03 follow-up/, r.id);
    assert.ok(md.includes('| `' + r.id + '` |'), r.id + ' is listed in SURFACES.md');
    if (r.kind === 'dom') {
      assert.ok(Array.isArray(r.meta.references), r.id + ' carries meta.references');
      // In the shipped source every unmapped control is a field that another
      // surface's handler reads or writes (a form field read on submit, a
      // colour well repainted from its picker); a control nobody touches at
      // all would be a new finding, so it is asserted here.
      assert.ok(r.meta.references.length > 0, r.id + ' is unmapped and never looked up by any handler');
      for (const ref of r.meta.references) assert.match(ref, /^src\/js\/[\w./-]+:\d+( [\w$]+\(\))?$/, r.id + ' reference ' + ref);
    }
  }
});

test('consumer reachability records the depth at which each consumer is reached', () => {
  let withConsumers = 0;
  for (const r of rows()) {
    for (const [name, depth] of Object.entries(r.consumers)) {
      assert.ok(inv().consumers.includes(name), r.id + ' names unknown consumer ' + name);
      assert.ok(Number.isInteger(depth) && depth >= 1 && depth <= 3, r.id + ' consumer depth ' + depth);
    }
    if (Object.keys(r.consumers).length) withConsumers++;
  }
  assert.ok(withConsumers > rows().length / 2, 'most surfaces reach at least one document consumer');
});

// --- binding shapes the shipped source uses (each was an unmapped row once) ---

test('ids looked up through a table (getElementById(IDS[key])) are bound', () => {
  for (const id of ['btn-combine-unite', 'btn-combine-subtract', 'btn-combine-intersect', 'btn-combine-exclude', 'btn-rig-mode-draw', 'btn-rig-mode-assign', 'btn-rig-mode-move']) {
    const r = byId('dom:#' + id);
    assert.equal(r.status, 'inventoried', id);
    assert.ok(r.handler.some((h) => h.startsWith('src/js/timeline.js:')), id + ' handler ' + r.handler);
    assert.ok(r.events.includes('click'), id + ' events ' + r.events);
  }
});

test('ids iterated from an array literal (["a","b"].forEach(id => getElementById(id))) are bound', () => {
  const r = byId('dom:#btn-open-tutorial-topbar');
  assert.equal(r.status, 'inventoried');
  assert.ok(r.handler.some((h) => h.startsWith('src/js/tutorial.js:')), r.handler);
  assert.ok(r.events.includes('click'));
});

test('compound descendant selectors (#align-toolbar .align-btn[data-align]) bind their buttons', () => {
  const btns = rows().filter((r) => r.kind === 'dom' && r.meta.container === 'align-toolbar' && r.surface.endsWith('button'));
  assert.equal(btns.length, 8, 'six align + two distribute buttons');
  for (const r of btns) {
    assert.equal(r.status, 'inventoried', r.id);
    assert.ok(r.handler.some((h) => h.startsWith('src/js/timeline.js:')), r.id);
    assert.ok(r.events.includes('click'), r.id);
  }
  const distribute = btns.filter((r) => r.meta.data.distribute);
  assert.equal(distribute.length, 2);
});

test('markup a container rebuilds at runtime is a placeholder, not an unmapped control', () => {
  const tabs = rows().filter((r) => r.kind === 'dom' && r.meta.container === 'symbol-tabs');
  assert.ok(tabs.length >= 1);
  for (const r of tabs) {
    assert.equal(r.status, 'unavailable-with-reason', r.id);
    assert.match(r.reason, /rebuilt at runtime by src\/js\/timeline\.js:\d+ renderSymbolTabs\(\)/);
    assert.equal(r.handler.length, 1);
    assert.equal(r.meta.via, 'container rebuild #symbol-tabs');
  }
});

test('shipped bindings reached through a variable, a helper parameter, an id-taking helper, a returning helper or delegation stay inventoried', () => {
  const expect = {
    'dom:#btn-bool-unite': ['click', /^helper wireBoolBtn\(\)/],          // wireBoolBtn(id, op) looks the id up and binds it
    'dom:#p-widget-w': ['change', /^helper bindWidgetField\(\)/],          // bindWidgetField(id, apply) → el(id) → addEventListener
    'dom:#mac-update-btn': ['click', /^helper macBtn\(\)/],                // function macBtn() { return getElementById(...) }
    'dom:#assets-tab-media': ['click', /^variable btn/],                   // tabs.forEach(t => { var btn = getElementById(t.btn); btn.addEventListener })
    'dom:#exp-w': ['input', /^variable /],                                 // var wInput = getElementById('exp-w'); … wInput.addEventListener
    'dom:#p-opacity': ['input', /^chain$/],                                // getElementById('p-opacity').addEventListener
    'dom:#exp-alpha': ['click', /^container delegation #export-modal$/],
  };
  for (const [id, [event, via]] of Object.entries(expect)) {
    const r = byId(id);
    assert.equal(r.status, 'inventoried', id);
    assert.ok(r.events.includes(event), id + ' events ' + r.events);
    assert.match(r.meta.via || '', via, id + ' via ' + r.meta.via);
  }
});

// ---------------------------------------------------------------------------
// synthetic source: the review counterexamples and every binding shape
// ---------------------------------------------------------------------------
// synthetic-source:start
const SYNTHETIC_HTML = `<!DOCTYPE html><html><body>
<div id="toolbar">
  <button id="btn-real">Real</button>
  <button id="inventory-review-comment">Comment only</button>
  <button id="inventory-review-lookup">Lookup only</button>
  <button id="btn-string-only">String only</button>
  <button id="btn-neighbor">Neighbor</button>
  <button id="btn-var">Var</button>
  <button id="btn-var-later">Var later</button>
  <button id="btn-onclick">Onclick</button>
  <button id="btn-el-helper">El helper</button>
  <button id="btn-wire-helper">Wire helper</button>
  <button id="btn-nested-helper">Nested helper</button>
  <button id="btn-arg-helper">Arg helper</button>
  <button id="btn-cross-file">Cross file</button>
  <input id="sl-range" type="range">
  <button id="btn-returner">Returner</button>
  <button id="btn-reassigned">Reassigned</button>
  <button id="btn-reassign-target">Reassign target</button>
  <button id="btn-shadow-a">Shadow A</button>
  <button id="btn-shadow-b">Shadow B</button>
  <button id="btn-table-a">Table A</button><button id="btn-table-b">Table B</button>
  <button id="btn-tab-a">Tab A</button><button id="btn-tab-b">Tab B</button>
  <button id="btn-arr-a">Arr A</button><button id="btn-arr-b">Arr B</button>
  <button id="btn-untouched">Untouched</button>
</div>
<div id="cards"><button class="card" data-key="a">A</button><button class="card" data-key="b">B</button></div>
<div id="bar"><button class="k" data-key="x">X</button></div>
<div id="scoped"><button class="sc">S</button></div>
<div id="other"><button class="sc">Other</button></div>
<div id="rows"><button class="r">R</button></div>
<a id="lnk-docs" href="https://example.org/docs">Docs</a>
</body></html>
`;
const SYNTHETIC_JS = `'use strict';
function saveActiveLayerFrame() {}
function renderNow() {}
function fakeHandler() { saveActiveLayerFrame(); }
var re = /'[/]/g; var div = 10 / 2 / 5; var re2 = /\\/\\/ not a comment/;
function inventoryProbe(){ document.getElementById('inventory-review-lookup').textContent='status'; }
// document.getElementById('inventory-review-comment').addEventListener('click', fakeHandler);
/* document.getElementById('inventory-review-comment').addEventListener('click', fakeHandler); */
var note = "document.getElementById('btn-string-only').addEventListener('click', fakeHandler)";
var tpl = \`document.getElementById('btn-string-only').addEventListener('click', \${'fakeHandler'})\`;
function wireReal() {
  document.getElementById('btn-real').addEventListener('click', function () { saveActiveLayerFrame(); });
}
function wireNeighbor() {
  document.getElementById('inventory-review-lookup').disabled = true; document.getElementById('btn-neighbor').addEventListener('click', function () { renderNow(); });
}
var later = document.getElementById('btn-var-later');
function wireVar() {
  var b = document.getElementById('btn-var');
  if (b) b.addEventListener('pointerdown', function (e) { saveActiveLayerFrame(); });
  b.addEventListener('dblclick', fakeHandler);
  later.addEventListener('click', function () { renderNow(); });
  document.getElementById('btn-onclick').onclick = function () { renderNow(); };
}
function el(id) { return document.getElementById(id); }
function wireEl() { el('btn-el-helper').addEventListener('input', function () { renderNow(); }); }
function wire(id, op) { var b = document.getElementById(id); b.addEventListener('click', function () { saveActiveLayerFrame(op); }); }
wire('btn-wire-helper', 'x');
function wireField(id, apply) { var fld = el(id); if (!fld) return; fld.addEventListener('change', function () { apply(this.value); renderNow(); }); }
wireField('btn-nested-helper', function () {});
function bindSlider(input, cb) { input.addEventListener('input', function () { cb(input.value); renderNow(); }); }
bindSlider(document.getElementById('sl-range'), function () {});
var argBtn = document.getElementById('btn-arg-helper');
bindSlider(argBtn, fakeHandler);
bindToggle(document.getElementById('btn-cross-file'), 'k');
function mainBtn() { return document.getElementById('btn-returner'); }
function wireReturner() { var b = mainBtn(); if (!b) return; b.addEventListener('click', fakeHandler); }
function wireReassign() {
  var t = document.getElementById('btn-reassigned');
  t.textContent = 'x';
  t = document.getElementById('btn-reassign-target');
  t.addEventListener('click', fakeHandler);
}
var s = document.getElementById('btn-shadow-a');
s.textContent = 'a';
function wireShadow() { var s = document.getElementById('btn-shadow-b'); s.addEventListener('click', fakeHandler); }
var IDS = { a: 'btn-table-a', b: 'btn-table-b' };
function wireTable() { ['a', 'b'].forEach(function (k) { document.getElementById(IDS[k]).addEventListener('click', function () { renderNow(); }); }); }
var tabs = [{ btn: 'btn-tab-a', view: 'view-a' }, { btn: 'btn-tab-b', view: 'view-b' }];
tabs.forEach(function (t) { var btn = document.getElementById(t.btn); if (btn) btn.addEventListener('click', function () { renderNow(); }); });
['btn-arr-a', 'btn-arr-b'].forEach(function (id) { document.getElementById(id).addEventListener('change', fakeHandler); });
document.querySelectorAll('.card').forEach(function (c) { c.addEventListener('pointerdown', function () { saveActiveLayerFrame(); }); });
document.getElementById('bar').addEventListener('click', function (e) { var t = e.target.closest('.k'); if (t) renderNow(); });
document.getElementById('scoped').querySelectorAll('.sc').forEach(function (b) { b.addEventListener('click', fakeHandler); });
Array.prototype.forEach.call(document.getElementById('rows').querySelectorAll('.r'), function (r) { r.addEventListener('click', fakeHandler); });
`;
const SYNTHETIC_WIRE_JS = `'use strict';
function bindToggle(btn, key) { btn.addEventListener('click', function () { renderNow(key); }); }
`;
// synthetic-source:end

function writeSyntheticTree(dir) {
  fs.mkdirSync(path.join(dir, 'src', 'js'), { recursive: true });
  fs.writeFileSync(path.join(dir, 'src', 'index.html'), SYNTHETIC_HTML);
  fs.writeFileSync(path.join(dir, 'src', 'js', 'app.js'), SYNTHETIC_JS);
  fs.writeFileSync(path.join(dir, 'src', 'js', 'wire.js'), SYNTHETIC_WIRE_JS);
}
let synth = null;
function synthetic() {
  if (synth) return synth;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-inventory-synthetic-'));
  try { writeSyntheticTree(dir); synth = inventory.build({ root: dir }); } finally { fs.rmSync(dir, { recursive: true, force: true }); }
  return synth;
}
function srow(id) { const r = synthetic().rows.find((x) => x.id === id); assert.ok(r, 'synthetic row ' + id); return r; }
function srowsIn(container) { return synthetic().rows.filter((r) => r.kind === 'dom' && r.meta.container === container); }
const unmappedShape = (r, refs) => { assert.equal(r.status, 'unmapped', r.id); assert.deepEqual(r.events, [], r.id); assert.deepEqual(r.handler, [], r.id); assert.deepEqual(r.meta.references, refs, r.id + ' references'); };
const bound = (r, events, via) => { assert.equal(r.status, 'inventoried', r.id + ' (' + JSON.stringify(r.meta.references) + ')'); assert.deepEqual(r.events, events, r.id + ' events'); if (via) assert.match(r.meta.via, via, r.id + ' via ' + r.meta.via); assert.ok(r.handler.length >= 1, r.id + ' handler'); };

test('lexRegions: comments are blanked, strings/templates/regex are literals, template expressions are code', () => {
  const src = "a(); // getElementById('x')\nvar re = /'[/]/g; b(); /* c('y') */ var s = 'd(\"e\")'; var t = `f ${g('h')} i`; j();";
  const { code, kind } = inventory.lexRegions(src);
  assert.equal(code.length, src.length);
  assert.ok(!code.includes('getElementById'), 'line comment blanked');
  assert.equal(code.indexOf('\n'), src.indexOf('\n'), 'newlines kept');
  const at = (needle) => src.indexOf(needle);
  assert.equal(kind[at("'[/]")], 2, 'regex literal');
  assert.equal(kind[at('b();')], 0, 'code resumes after a regex containing a quote and a slash');
  assert.equal(kind[at("c('y')")], 1, 'block comment');
  assert.equal(kind[at('d("e")')], 2, 'string');
  assert.equal(kind[at('f ${')], 2, 'template text');
  assert.equal(kind[at("g('h')")], 0, 'template expression is code');
  assert.equal(kind[at(' i`')], 2, 'template text resumes after the expression');
  assert.equal(kind[at('j();')], 0);
});

test('review counterexample: a lookup that only writes the element is unmapped, with the lookup recorded as a reference', () => {
  const r = srow('dom:#inventory-review-lookup');
  unmappedShape(r, ['src/js/app.js:6 inventoryProbe()', 'src/js/app.js:15 wireNeighbor()']);
  assert.match(r.nextGate, /referenced without an event registration at src\/js\/app\.js:6 inventoryProbe\(\)/);
});

test('review counterexample: a listener in a line comment, a block comment, a string or a template literal is not a binding', () => {
  unmappedShape(srow('dom:#inventory-review-comment'), []);
  unmappedShape(srow('dom:#btn-string-only'), []);
});

test('an unrelated listener registered on the next statement is not attributed to a lookup-only control', () => {
  // wireNeighbor(): `getElementById('inventory-review-lookup').disabled = true;` immediately
  // followed by `getElementById('btn-neighbor').addEventListener('click', …)` on the same line.
  assert.deepEqual(srow('dom:#inventory-review-lookup').events, []);
  const n = srow('dom:#btn-neighbor');
  bound(n, ['click'], /^chain$/);
  assert.equal(n.consumers.render, 1);
  assert.equal(n.consumers.persist, undefined, 'the neighbour does not inherit the persisting handler of another control');
});

test('a control nobody looks up is unmapped with no references', () => {
  unmappedShape(srow('dom:#btn-untouched'), []);
});

test('bindings on the element expression itself: addEventListener chain, on<event> assignment', () => {
  const real = srow('dom:#btn-real');
  bound(real, ['click'], /^chain$/);
  assert.equal(real.consumers.persist, 1, 'callback body reaches persistence at depth 1');
  assert.match(real.handler[0], /^src\/js\/app\.js:12 wireReal\(\)$/);
  bound(srow('dom:#btn-onclick'), ['click'], /^chain$/);
  assert.equal(srow('dom:#btn-onclick').consumers.render, 1);
});

test('bindings through the variable the element is assigned to, within its function and from a later function', () => {
  const v = srow('dom:#btn-var');
  bound(v, ['pointerdown', 'dblclick'], /^variable b$/);
  assert.equal(v.consumers.persist, 1);
  const later = srow('dom:#btn-var-later');
  bound(later, ['click'], /^variable later$/);
  assert.match(later.handler[0], /^src\/js\/app\.js:22 wireVar\(\)$/, 'handler is the registration site, not the top-level lookup');
});

test('bindings through helpers: a transparent id helper, an id-taking helper, a helper forwarding to one, an element parameter, a cross-file helper, a returning helper', () => {
  bound(srow('dom:#btn-el-helper'), ['input'], /^helper el\(\)|^chain$/);
  const w = srow('dom:#btn-wire-helper');
  bound(w, ['click'], /^helper wire\(\)$/);
  assert.match(w.handler[0], / wire\(\)$/, 'handler is the registration inside wire()');
  assert.equal(w.consumers.persist, 1);
  bound(srow('dom:#btn-nested-helper'), ['change'], /^helper wireField\(\)$/);
  bound(srow('dom:#sl-range'), ['input'], /^helper bindSlider\(\)$/);
  bound(srow('dom:#btn-arg-helper'), ['input'], /^helper bindSlider\(\)$/);
  const x = srow('dom:#btn-cross-file');
  bound(x, ['click'], /^helper bindToggle\(\)$/);
  assert.match(x.handler[0], /^src\/js\/wire\.js:2 bindToggle\(\)$/);
  bound(srow('dom:#btn-returner'), ['click'], /^helper mainBtn\(\)$/);
});

test('a reassigned variable binds the element it holds at the registration; a shadowing declaration does not bind the outer element', () => {
  unmappedShape(srow('dom:#btn-reassigned'), ['src/js/app.js:39 wireReassign()']);
  bound(srow('dom:#btn-reassign-target'), ['click'], /^variable t$/);
  unmappedShape(srow('dom:#btn-shadow-a'), ['src/js/app.js:44']);
  bound(srow('dom:#btn-shadow-b'), ['click'], /^variable s$/);
});

test('id tables (object, array of objects) and id arrays bind every id they list at the lookup site', () => {
  for (const id of ['btn-table-a', 'btn-table-b']) bound(srow('dom:#' + id), ['click'], /^chain$/);
  for (const id of ['btn-tab-a', 'btn-tab-b']) bound(srow('dom:#' + id), ['click'], /^variable btn$/);
  for (const id of ['btn-arr-a', 'btn-arr-b']) bound(srow('dom:#' + id), ['change'], /^callback id$/);
});

test('class selectors, forEach callbacks, scoped selectors and ancestor delegation bind anonymous rows without leaking across containers', () => {
  const cards = srowsIn('cards');
  assert.equal(cards.length, 2);
  for (const c of cards) { bound(c, ['pointerdown'], /^callback c$/); assert.equal(c.consumers.persist, 1); }
  const k = srowsIn('bar');
  assert.equal(k.length, 1);
  bound(k[0], ['click'], /^container delegation #bar$/);
  const sc = srowsIn('scoped');
  assert.equal(sc.length, 1);
  bound(sc[0], ['click'], /^callback b$/);
  const other = srowsIn('other');
  assert.equal(other.length, 1);
  unmappedShape(other[0], []);
  const r = srowsIn('rows');
  assert.equal(r.length, 1);
  bound(r[0], ['click'], /^callback r$/);
  bound(srow('dom:#lnk-docs'), ['click'], /^href$/);
});

// ---------------------------------------------------------------------------
// outputs and the staleness gate
// ---------------------------------------------------------------------------

test('rendered outputs list unmapped rows (with their references) and placeholders explicitly', () => {
  const out = inventory.renderOutputs(inv());
  assert.deepEqual(Object.keys(out).sort(), ['SURFACES.md', 'surfaces.csv', 'surfaces.json']);
  const n = inv().counts.byStatus.unmapped || 0;
  const p = inv().counts.byStatus['unavailable-with-reason'] || 0;
  assert.ok(out['SURFACES.md'].includes('## Unmapped surfaces (' + n + ')'));
  assert.ok(out['SURFACES.md'].includes('| Row | Area | Capability | Source | Referenced at |'));
  assert.ok(out['SURFACES.md'].includes('## Static placeholders (' + p + ')'));
  const header = out['surfaces.csv'].split('\n')[0];
  assert.equal(header, 'id,kind,area,surface,capability,handler,events,exposure,sdk,mcp,consumers,fixtures,platforms,status,nextGate,source,reason');
  assert.equal(out['surfaces.csv'].trim().split('\n').length, rows().length + 1);
  assert.equal(JSON.parse(out['surfaces.json']).rows.length, rows().length);
});

test('staleOutputs ignores the source stamp but reports missing and modified files', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-inventory-test-'));
  try {
    const out = inventory.renderOutputs(inv());
    for (const [name, content] of Object.entries(out)) fs.writeFileSync(path.join(dir, name), content);
    assert.deepEqual(inventory.staleOutputs(out, dir), []);
    // a different commit stamp alone is not staleness
    const restamped = {
      'surfaces.json': out['surfaces.json'].replace(/"head": "[0-9a-f]*"/, '"head": "0000000000000000000000000000000000000000"').replace(/"describe": "[^"]*"/, '"describe": "v0.0.0-0-g0000000"'),
      'surfaces.csv': out['surfaces.csv'],
      'SURFACES.md': out['SURFACES.md'].replace(/source `[0-9a-f]*` \([^)]*\)/, 'source `000000000000` (v0.0.0-0-g0000000)'),
    };
    assert.deepEqual(inventory.staleOutputs(restamped, dir), []);
    fs.appendFileSync(path.join(dir, 'surfaces.csv'), '"extra"\n');
    fs.rmSync(path.join(dir, 'SURFACES.md'));
    assert.deepEqual(inventory.staleOutputs(out, dir), ['surfaces.csv', 'SURFACES.md (missing)']);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('the committed inventory is up to date (`inventory.cjs --check`)', () => {
  const r = spawnSync(process.execPath, [path.join(__dirname, 'inventory.cjs'), '--check'], { cwd: ROOT, encoding: 'utf8' });
  assert.equal(r.status, 0, 'stale inventory: ' + r.stderr + ' — run `npm run inventory` and commit engineering/inventory');
  assert.match(r.stdout, /^inventory up to date \(\d+ rows, digest [0-9a-f]{12}\)/);
});
