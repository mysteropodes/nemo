'use strict';
// #2373: Safety zones, Clip to canvas and the alpha checker on a native object project are pure view overlays: drawn from the
// native presentation view, no engine command, no revision change, never in an export, baseline geometry at every zoom.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Aids = require('../src/js/adapters/native-view-aids-ui.js');

function el(tag) {
  return { tag, attrs: {}, style: {}, children: [], removed: false,
    setAttribute(k, v) { this.attrs[k] = String(v); },
    appendChild(c) { this.children.push(c); c.parentNode = this; return c; },
    remove() { this.removed = true; if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((c) => c !== this); } };
}
function harness(transform = [1, 0, 0, 1, 0, 0], native = true) {
  const body = el('body'), frames = [], calls = [];
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 7, documentSnapshotId: 'native-object:d:7', canvasW: 1920, canvasH: 1080, layers: [] };
  let presentation = { ...pin, owner: 'native', status: 'presented', contextId: 'scene-root', frame: 0 };
  const rect = { left: 10, top: 20, width: 960, height: 540 };
  // Any engine route (a command, a write, a read of the document) is recorded: a view aid must use none of them.
  const session = new Proxy({ info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ ...pin, presentation }) },
    { get(t, k) { if (k in t) return t[k]; return (...a) => { calls.push(String(k)); }; } });
  const classes = new Set();
  const button = { classList: { toggle: (c, on) => { if (on) classes.add(c); else classes.delete(c); } } };
  const root = { state: { canvasClip: false, safetyZones: false, previewAlphaBg: false },
    document: { body, getElementById: () => ({ getBoundingClientRect: () => rect }), createElementNS: (_n, tag) => el(tag) },
    requestAnimationFrame: (fn) => frames.push(fn),
    NemoNativeObjectProject: { desktop: () => (native ? session : null) },
    NemoNativeViewportUnderlay: { canvasMapping: { transform: () => transform } },
    NemoSelectCanvasIntent: Intent, NemoNativeViewAids: Aids };
  return { root, body, rect, button, classes, calls, pin,
    setView: (t) => { transform = t; presentation = { ...presentation }; },
    tick: () => frames.splice(0).forEach((fn) => fn()),
    svgs: () => body.children.filter((c) => c.tag === 'svg') };
}
const nums = (s) => s.split(/[ ,]+/).map(Number);

test('toggles send no engine command and leave the content revision untouched', () => {
  const h = harness();
  for (const kind of ['safety', 'clip', 'alpha', 'safety', 'clip', 'alpha']) assert.equal(Aids.toggle(h.root, kind, h.button), true);
  assert.deepEqual(h.calls, []);
  assert.equal(h.pin.contentRevision, 7);
  assert.equal(h.svgs().length, 0); // all three turned off again
  assert.deepEqual([h.root.state.canvasClip, h.root.state.safetyZones, h.root.state.previewAlphaBg], [false, false, false]);
});

test('the button mirrors the flag and the legacy path keeps the click when no native object project is presented', () => {
  const h = harness();
  Aids.toggle(h.root, 'safety', h.button);
  assert.ok(h.classes.has('active'));
  Aids.toggle(h.root, 'safety', h.button);
  assert.ok(!h.classes.has('active'));
  const legacy = harness([1, 0, 0, 1, 0, 0], false);
  assert.equal(Aids.toggle(legacy.root, 'safety', legacy.button), false);
  assert.equal(legacy.root.state.safetyZones, false);
  assert.equal(legacy.svgs().length, 0);
});

for (const [name, V] of [['fit', [1, 0, 0, 1, 0, 0]], ['zoom 2 panned', [2, 0, 0, 2, -300, 120]], ['zoom 0.25', [0.25, 0, 0, 0.25, 700, 400]]]) {
  test(`safety zones at ${name} sit at the baseline 5% and 10% insets, 1 screen px, 200/255 alpha`, () => {
    const h = harness(V);
    Aids.toggle(h.root, 'safety', h.button);
    const [svg] = h.svgs();
    assert.equal(svg.attrs.viewBox, '0 0 1920 1080');
    assert.equal(svg.style.pointerEvents, 'none');
    assert.equal(svg.style.position, 'fixed');
    assert.deepEqual([svg.style.left, svg.style.top, svg.style.width, svg.style.height], ['10px', '20px', '960px', '540px']);
    const [action, title] = svg.children;
    const map = (x, y) => [V[0] * x + V[2] * y + V[4], V[1] * x + V[3] * y + V[5]];
    const exp = (p) => [map(1920 * p, 1080 * p), map(1920 * (1 - p), 1080 * p), map(1920 * (1 - p), 1080 * (1 - p)), map(1920 * p, 1080 * (1 - p))];
    assert.deepEqual(action.attrs.points.split(' ').map(nums), exp(0.05));
    assert.deepEqual(title.attrs.points.split(' ').map(nums), exp(0.1));
    assert.equal(action.attrs.stroke, 'rgb(255,210,0)');
    assert.equal(title.attrs.stroke, 'rgb(255,90,90)');
    assert.equal(Number(action.attrs['stroke-opacity']), 200 / 255);
    assert.equal(action.attrs['vector-effect'], 'non-scaling-stroke');
    assert.equal(action.attrs['stroke-width'], '1');
    assert.equal(action.attrs.fill, 'none');
  });
  test(`clip at ${name} covers everything outside the artboard in the baseline colour`, () => {
    const h = harness(V);
    Aids.toggle(h.root, 'clip', h.button);
    const [svg] = h.svgs(), band = svg.children[0];
    assert.equal(band.attrs.fill, 'rgb(35,35,48)');
    assert.equal(band.attrs['fill-rule'], 'evenodd');
    const [outer, inner] = band.attrs.d.split('Z').filter(Boolean).map((s) => s.replace(/[ML]/g, ' ').trim().split(/\s+/).map(Number));
    assert.deepEqual(outer, [-100000, -100000, 100000, -100000, 100000, 100000, -100000, 100000]);
    const map = (x, y) => [V[0] * x + V[2] * y + V[4], V[1] * x + V[3] * y + V[5]];
    assert.deepEqual(inner, [].concat(map(0, 0), map(1920, 0), map(1920, 1080), map(0, 1080)));
    assert.equal(Number(svg.style.zIndex) < 20, true); // under the Select overlay, like the baseline layer order
  });
  test(`alpha checker at ${name} follows the baseline grid and darkens alternate tiles by 255 -> 222`, () => {
    const h = harness(V);
    Aids.toggle(h.root, 'alpha', h.button);
    const [svg] = h.svgs(), group = svg.children[0], tiles = group.children[0];
    assert.equal(group.attrs.transform, `matrix(${V.join(' ')})`);
    // baseline engine-bridge: world tile = 8 / view.zoom, view.zoom = screen px per world unit, 4000-tile cap
    const zoom = Math.hypot(V[0], V[1]) * 960 / 1920;
    let tile = 8 / zoom;
    if ((1920 / tile) * (1080 / tile) > 4000) tile *= Math.sqrt(((1920 / tile) * (1080 / tile)) / 4000);
    const cols = Math.max(1, Math.round(1920 / tile)), rows = Math.max(1, Math.round(1080 / tile));
    const cells = tiles.attrs.d.split('Z').filter((s) => s.trim());
    let even = 0;
    for (let y = 0; y < rows; y++) for (let x = 0; x < cols; x++) if ((x + y) % 2 === 0) even++;
    assert.equal(cells.length, even);
    assert.match(cells[0], new RegExp(`^M 0 0 h ${1920 / cols} v ${1080 / rows} h ${-1920 / cols}`));
    assert.equal(Number(tiles.attrs['fill-opacity']), (255 - 222) / 255);
    assert.ok(cols * rows <= 4000 * 1.2);
  });
}

test('an aid follows the view: a pan/zoom presentation redraws in place on the next frame, and is dropped without a presentation', () => {
  const h = harness();
  Aids.toggle(h.root, 'safety', h.button);
  const first = h.svgs()[0];
  h.setView([2, 0, 0, 2, 10, 10]);
  Aids.notify(h.root);
  assert.equal(h.svgs()[0], first); // not yet: the redraw is queued
  h.tick();
  assert.equal(first.removed, true);
  assert.equal(h.svgs().length, 1);
  assert.deepEqual(nums(h.svgs()[0].children[0].attrs.points.split(' ')[0]), [2 * 96 + 10, 2 * 54 + 10]);
  h.root.NemoNativeObjectProject = { desktop: () => null };
  Aids.notify(h.root); h.tick();
  assert.equal(h.svgs().length, 0);
});

test('aids on from a loaded project draw when the presentation arrives and stack in baseline order', () => {
  const h = harness();
  Object.assign(h.root.state, { canvasClip: true, safetyZones: true, previewAlphaBg: true });
  Aids.notify(h.root); h.tick();
  const [under, over] = h.svgs();
  assert.deepEqual(under.children.map((c) => c.tag), ['g', 'path']); // checker below the clip mask
  assert.equal(over.children.length, 2);
  assert.ok(Number(over.style.zIndex) > 20); // safety above the selection handles, as in the baseline
});

test('export stays unaffected: the module is DOM-only, outside every export path, and registered once', () => {
  const src = fs.readFileSync(path.join(__dirname, '../src/js/adapters/native-view-aids-ui.js'), 'utf8');
  assert.ok(!/invoke|tauri|\bcommand\(|submit|writeDocument|exportFrame/i.test(src.replace(/\/\*[\s\S]*?\*\//, '')));
  for (const f of ['export.js', 'rive-export.js']) {
    const p = path.join(__dirname, '../src/js', f);
    if (fs.existsSync(p)) assert.ok(!/NemoNativeViewAids/.test(fs.readFileSync(p, 'utf8')), f);
  }
  const html = fs.readFileSync(path.join(__dirname, '../src/index.html'), 'utf8');
  assert.equal(html.split('js/adapters/native-view-aids-ui.js').length - 1, 1);
});

test('the three toolbar buttons delegate to the native aids before the legacy flag-flip, and the repaint paths notify them', () => {
  const read = (f) => fs.readFileSync(path.join(__dirname, '../src/js', f), 'utf8');
  const tl = read('timeline.js');
  for (const [id, kind, legacy] of [['btn-clip', 'clip', 'setCanvasClip'], ['btn-safety', 'safety', 'setSafetyZones']]) {
    const line = tl.split('\n').find((l) => l.includes(`getElementById('${id}').addEventListener`));
    assert.ok(line.indexOf(`NemoNativeViewAids.toggle(window,'${kind}'`) > 0 && line.indexOf(`NemoNativeViewAids.toggle(window,'${kind}'`) < line.indexOf(legacy), id);
  }
  const at = tl.indexOf("_btnAlphaBg.addEventListener('click'");
  assert.ok(tl.indexOf("toggle(window,'alpha'", at) > at && tl.indexOf("toggle(window,'alpha'", at) < tl.indexOf('state.previewAlphaBg=!state.previewAlphaBg', at));
  assert.match(read('adapters/native-viewport-underlay.js'), /NemoNativeViewAids/);
  assert.match(read('adapters/native-object-project.js'), /NemoNativeViewAids/);
});
