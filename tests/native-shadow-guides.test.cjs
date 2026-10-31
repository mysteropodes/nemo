'use strict';
// #2373: "Shadow guides" (#btn-shadow-guides) on a native object project is a pure view FLAG of the preview request, the inverse of
// the other aids: state.showShadowGuides is true by default (guides shown, nothing sent) and only when the button switches it off
// does the request carry hideShadowGuides. The button flips the flag, mirrors its lit state and asks the native session for one
// more preview, with no command. A refused hide lights the button again and toasts once. The engine leaves the shadow-tagged items
// out of the viewport (Rust tests); an export keeps its own rule.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Aids = require('../src/js/adapters/native-view-aids-ui.js');

function harness({ native = true, refuse = false, strict = true } = {}) {
  const calls = [], presented = [], toasts = [];
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 7, documentSnapshotId: 'native-object:d:7', canvasW: 1920, canvasH: 1080, layers: [] };
  const base = { info: () => pin, isActive: () => true, blocksLegacy: () => true,
    renderPreview: (frame) => { presented.push(['render', frame]); return Promise.resolve(null); } };
  if (strict) base.presentPreview = (frame, superseded) => { presented.push(['present', frame, superseded]); return refuse ? Promise.reject(new Error('refused')) : Promise.resolve({}); };
  const session = new Proxy(base, { get(t, k) { if (k in t) return t[k]; if (k === 'presentPreview') return undefined; return (...a) => { calls.push(String(k)); }; } });
  const lit = new Set(['active']);
  const button = { classList: { toggle: (n, on) => { if (on) lit.add(n); else lit.delete(n); }, add: (n) => lit.add(n) } };
  const root = { state: { showShadowGuides: true, currentFrame: 5, canvasClip: false, safetyZones: false, previewAlphaBg: false },
    document: { getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 960, height: 540 }) }) },
    requestAnimationFrame: () => {},
    NemoNativeObjectProject: { desktop: () => (native ? session : null) },
    NemoSelectCanvasIntent: Intent, NemoNativeViewAids: Aids, showToast: (m) => toasts.push(m) };
  return { root, button, lit, calls, presented, toasts, pin };
}
const settle = () => new Promise((r) => setImmediate(r));

test('clicking hides the guides, mirrors the button and asks for one more preview at the playhead, with no engine command', async () => {
  const h = harness();
  assert.equal(Aids.toggle(h.root, 'shadow', h.button), true);
  assert.equal(h.root.state.showShadowGuides, false);
  assert.equal(h.lit.has('active'), false, 'the button is dark while the guides are hidden');
  assert.deepEqual(h.presented, [['present', 5, true]]);
  assert.deepEqual(h.calls, []);
  assert.equal(h.pin.contentRevision, 7);
  await settle();
  assert.deepEqual(h.toasts, []);
  Aids.toggle(h.root, 'shadow', h.button);
  assert.equal(h.root.state.showShadowGuides, true);
  assert.equal(h.lit.has('active'), true);
  assert.deepEqual(h.presented[1], ['render', 5], 'showing the guides again never needs the strict presenting call');
  await settle();
  assert.deepEqual(h.toasts, []);
});

test('a refused hide lights the button again, restores the flag and toasts once', async () => {
  const h = harness({ refuse: true });
  Aids.toggle(h.root, 'shadow', h.button);
  await settle();
  assert.equal(h.root.state.showShadowGuides, true);
  assert.equal(h.lit.has('active'), true);
  assert.equal(h.toasts.length, 1);
  // Switching it on is never a hide, so it never reports a refusal.
  Aids.toggle(h.root, 'shadow', h.button);
  Aids.toggle(h.root, 'shadow', h.button);
  await settle();
  assert.equal(h.toasts.length, 2, 'only the second hide was refused again');
});

test('a session without presentPreview still re-presents and the legacy path keeps the click', async () => {
  const h = harness({ strict: false });
  assert.equal(Aids.toggle(h.root, 'shadow', h.button), true);
  assert.deepEqual(h.presented, [['render', 5]]);
  const legacy = harness({ native: false });
  assert.equal(Aids.toggle(legacy.root, 'shadow', legacy.button), false);
  assert.equal(legacy.root.state.showShadowGuides, true, 'the legacy branch of the caller owns the flag');
  assert.deepEqual(legacy.presented, []);
});

test('the button hands the native route first, and the desktop port reads the inverse of the same state flag', () => {
  const timeline = fs.readFileSync(path.join(__dirname, '../src/js/timeline.js'), 'utf8');
  const at = timeline.indexOf("getElementById('btn-shadow-guides').addEventListener");
  const handler = timeline.slice(at, at + 500);
  assert.ok(handler.indexOf("NemoNativeViewAids.toggle(window,'shadow',this))return;") > 0);
  assert.ok(handler.indexOf("toggle(window,'shadow'") < handler.indexOf('SM.toggleShadowGuides()'), 'native route before the legacy writer');
  const desktop = fs.readFileSync(path.join(__dirname, '../src/js/adapters/native-object-project-desktop.js'), 'utf8');
  assert.match(desktop, /hideShadowGuides:function\(\)\{return !!\(root\.state&&root\.state\.showShadowGuides===false\);\}/);
});

test('the view flag is never exported: the export request builders do not read it', () => {
  for (const file of ['native-object-project.js', 'native-object-project-desktop.js', 'native-object-open-exposure.js']) {
    const text = fs.readFileSync(path.join(__dirname, '../src/js/adapters', file), 'utf8');
    for (const m of text.matchAll(/function (?:exportPng|exportSvg)[\s\S]{0,1200}/g)) assert.doesNotMatch(m[0], /hideShadowGuides|showShadowGuides/, file);
  }
});
