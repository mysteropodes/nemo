'use strict';
// I41 (#2357): the Lottie preview takes an explicit pinned export result, and closing disposes the player, so a result
// that arrives after the preview was closed or replaced cannot update it. The baseline in-memory open(json) still works.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function harness() {
  const drawn = [], timers = new Set(), log = { cleared: 0 };
  const el = (id) => ({ id, style: {}, value: '0', width: 0, height: 0, listeners: {}, innerHTML: '', textContent: '',
    addEventListener(kind, fn) { (this.listeners[kind] = this.listeners[kind] || []).push(fn); } });
  const els = Object.fromEntries(['lottie-preview-canvas', 'lottie-preview-modal', 'lottie-preview-close', 'lottie-preview-scrub',
    'lottie-preview-play', 'lottie-preview-frame'].map((id) => [id, el(id)]));
  function Path() { this.segments = []; drawn.push(this); }
  Path.prototype.add = function (s) { this.segments.push(s); };
  Path.Rectangle = function (o) { drawn.push(o); };
  const value = function (...args) { this.args = args; };
  const preview = { activate() {}, clear() { log.cleared++; }, view: { update() {} } };
  const root = { console, Math, JSON, Path, Segment: value, Point: value, Color: value, Size: value,
    paper: { project: { activate() {} }, Project: function () { return preview; } },
    document: { getElementById: (id) => els[id] || null },
    setInterval: (fn) => { const id = { fn }; timers.add(id); return id; }, clearInterval: (id) => timers.delete(id) };
  root.window = root;
  vm.createContext(root);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'src/js/lottie-preview.js'), 'utf8'), root, { filename: 'lottie-preview.js' });
  const click = (id, target) => els[id].listeners.click.forEach((fn) => fn({ target: target || els[id] }));
  return { api: root.SMLottiePreview, els, drawn, timers, log, click };
}
const doc = (w) => ({ v: '5.9.0', fr: 24, ip: 0, op: 2, w, h: 10, layers: [{ ty: 1, ip: 0, op: 2, sc: '#fff', sw: w, sh: 10 }] });
const pin = (revision) => ({ instanceId: 'instance-A', documentId: 'document-A', contentRevision: revision, snapshotId: 's' + revision });
const plain = (v) => JSON.parse(JSON.stringify(v));

test('a pinned result opens with its pin; the baseline open(json) has none; anything unpinned shows nothing', () => {
  const h = harness();
  assert.equal(h.api.openPinned({ json: doc(20), pin: pin(4) }) > 0, true);
  assert.deepEqual(plain(h.api.current().pin), pin(4));
  assert.deepEqual([h.els['lottie-preview-modal'].style.display, h.els['lottie-preview-canvas'].width], ['flex', 20]);
  h.api.open(doc(30));
  assert.equal(h.api.current().pin, null);
  const g = h.api.current().generation;
  for (const bad of [null, { json: doc(5) }, { json: doc(5), pin: { documentId: 'd' } }, { pin: pin(1) }, { json: {}, pin: pin(1) }]) {
    assert.equal(h.api.openPinned(bad), false);
  }
  assert.equal(h.api.current().generation, g, 'a refused result changes nothing');
});

test('a result that arrives after close or after a replacement is dropped', () => {
  const h = harness();
  const late = h.api.expect();
  h.api.openPinned({ json: doc(20), pin: pin(4) });
  h.api.close();
  assert.equal(h.api.present(late, { json: doc(40), pin: pin(5) }), false);
  assert.deepEqual([h.api.current().open, h.els['lottie-preview-modal'].style.display], [false, 'none']);
  const ticket = h.api.expect();
  assert.equal(h.api.present(ticket, { json: doc(50), pin: pin(6) }), true, 'an untouched preview takes its result');
  const stale = h.api.expect();
  h.api.openPinned({ json: doc(60), pin: pin(7) });
  assert.equal(h.api.present(stale, { json: doc(70), pin: pin(8) }), false);
  assert.equal(h.api.current().pin.contentRevision, 7, 'the replacement is still shown');
});

test('closing disposes the player: playback stops and the preview project is cleared', () => {
  const h = harness();
  h.api.openPinned({ json: doc(20), pin: pin(4) });
  h.click('lottie-preview-play');
  assert.equal(h.timers.size, 1);
  h.click('lottie-preview-close');
  assert.deepEqual([h.timers.size, h.api.current().open, h.api.current().pin], [0, false, null]);
  assert.ok(h.log.cleared >= 1);
  h.api.openPinned({ json: doc(20), pin: pin(4) });
  h.click('lottie-preview-modal');
  assert.equal(h.api.current().open, false, 'a backdrop click closes the same way');
});
