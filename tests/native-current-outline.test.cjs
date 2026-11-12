'use strict';
// #2372: "Current frame outline" (#p-current-outline) on a native object project is a pure view FLAG of the preview request: the
// checkbox flips state.currentFrameOutline, asks the native session for one more preview at the playhead and sends no command. A
// refusal of that request unchecks the box and toasts once. The engine draws the outline (Rust tests); an export never has it.
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
  const box = { checked: false, classList: undefined };
  const root = { state: { currentFrameOutline: false, currentFrame: 5, canvasClip: false, safetyZones: false, previewAlphaBg: false },
    document: { getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 960, height: 540 }) }) },
    requestAnimationFrame: () => {},
    NemoNativeObjectProject: { desktop: () => (native ? session : null) },
    NemoSelectCanvasIntent: Intent, NemoNativeViewAids: Aids, showToast: (m) => toasts.push(m) };
  return { root, box, calls, presented, toasts, pin };
}
const settle = () => new Promise((r) => setImmediate(r));

test('checking the box sets the view flag and asks for one more preview at the playhead, with no engine command', async () => {
  const h = harness();
  h.box.checked = true;
  assert.equal(Aids.toggle(h.root, 'outline', h.box), true);
  assert.equal(h.root.state.currentFrameOutline, true);
  assert.deepEqual(h.presented, [['present', 5, true]]);
  assert.deepEqual(h.calls, []);
  assert.equal(h.pin.contentRevision, 7);
  await settle();
  assert.deepEqual(h.toasts, []);
  assert.equal(h.box.checked, true);
  h.box.checked = false;
  Aids.toggle(h.root, 'outline', h.box);
  assert.equal(h.root.state.currentFrameOutline, false, 'unchecking sets the flag to the box state, it does not just flip it');
  assert.equal(h.presented.length, 2);
});

test('a refused request unchecks the box, clears the flag and toasts once', async () => {
  const h = harness({ refuse: true });
  h.box.checked = true;
  Aids.toggle(h.root, 'outline', h.box);
  await settle();
  assert.equal(h.root.state.currentFrameOutline, false);
  assert.equal(h.box.checked, false);
  assert.equal(h.toasts.length, 1);
  // Switching it off again asks for a preview without the flag and never toasts a refusal.
  Aids.toggle(h.root, 'outline', h.box);
  await settle();
  assert.equal(h.toasts.length, 1);
});

test('a session without presentPreview still re-presents (the generic notice reports a failure) and the legacy path keeps the click', async () => {
  const h = harness({ strict: false });
  h.box.checked = true;
  assert.equal(Aids.toggle(h.root, 'outline', h.box), true);
  assert.deepEqual(h.presented, [['render', 5]]);
  const legacy = harness({ native: false });
  legacy.box.checked = true;
  assert.equal(Aids.toggle(legacy.root, 'outline', legacy.box), false);
  assert.equal(legacy.root.state.currentFrameOutline, false, 'the legacy branch of the caller owns the flag');
  assert.deepEqual(legacy.presented, []);
});

test('the checkbox hands the native route first and the desktop port reads the same state flag', () => {
  const timeline = fs.readFileSync(path.join(__dirname, '../src/js/timeline.js'), 'utf8');
  const at = timeline.indexOf("getElementById('p-current-outline')");
  const handler = timeline.slice(at, at + 700);
  assert.ok(handler.indexOf("NemoNativeViewAids.toggle(window,'outline',this))return;") > 0);
  assert.ok(handler.indexOf("toggle(window,'outline'") < handler.indexOf('state.currentFrameOutline=this.checked'), 'native route before the legacy writer');
  const desktop = fs.readFileSync(path.join(__dirname, '../src/js/adapters/native-object-project-desktop.js'), 'utf8');
  assert.match(desktop, /currentOutline:function\(\)\{return !!\(root\.state&&root\.state\.currentFrameOutline===true\);\}/);
});

test('the view flag is never exported: the export request builders do not read it', () => {
  for (const file of ['native-object-project.js', 'native-object-project-desktop.js', 'native-object-open-exposure.js']) {
    const text = fs.readFileSync(path.join(__dirname, '../src/js/adapters', file), 'utf8');
    for (const m of text.matchAll(/function (?:exportPng|exportSvg)[\s\S]{0,1200}/g)) assert.doesNotMatch(m[0], /currentOutline|currentFrameOutline/, file);
  }
});
