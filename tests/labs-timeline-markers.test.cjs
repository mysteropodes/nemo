'use strict';
// CS8-B09 U31 (#2374): Labs timeline markers are view-only. Their storage is keyed by project, their flags are
// re-applied after the timeline header is rebuilt, disabling removes every flag and the header observer, and no
// marker action touches the document.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '..', 'src/js/labs/timeline-markers.js'), 'utf8');
function node(className) {
  const el = { className: className || '', children: [], style: {}, title: '', parentNode: null,
    classList: { contains: (c) => (el.className || '').split(/\s+/).includes(c) },
    appendChild(child) { child.parentNode = el; el.children.push(child); return child; },
    remove() { if (el.parentNode) el.parentNode.children = el.parentNode.children.filter((c) => c !== el); el.parentNode = null; },
    querySelectorAll(sel) {
      const cls = sel.replace(/^\./, ''), out = [];
      (function walk(n) { for (const c of n.children) { if (c.classList.contains(cls)) out.push(c); walk(c); } })(el);
      return out;
    } };
  return el;
}
function harness() {
  const storage = new Map(), observers = [];
  let project = 'project-a', on = true, def = null;
  const hdr = node('frame-hdr');
  for (let i = 0; i < 6; i++) hdr.appendChild(node('fhc'));
  const state = { layers: [{ frames: [{}, {}] }], totalFrames: 6, contentRevision: 3 };
  const root = { state, console,
    SMProject: { getProjectKey: () => project },
    localStorage: { getItem: (k) => (storage.has(k) ? storage.get(k) : null), setItem: (k, v) => storage.set(k, String(v)) },
    getComputedStyle: () => ({ position: 'static' }),
    MutationObserver: function (callback) { const o = { callback, target: null, connected: false,
      observe(target) { o.target = target; o.connected = true; }, disconnect() { o.connected = false; } }; observers.push(o); return o; },
    document: { getElementById: (id) => (id === 'frame-hdr' ? hdr : null), createElement: () => node(),
      querySelectorAll: (sel) => hdr.querySelectorAll(sel) },
    SMLabs: { isOn: () => on, register: (name, d) => { def = d; } } };
  root.window = root;
  vm.createContext(root);
  vm.runInContext(source, root, { filename: 'timeline-markers.js' });
  const flags = () => hdr.children.map((cell, i) => (cell.querySelectorAll('labs-marker-flag').length ? i : null)).filter((i) => i !== null);
  return { root, labs: root.SMLabs, hdr, storage, observers, flags, state,
    project: (p) => { project = p; }, disable: () => { on = false; def.onDisable(); }, enable: () => { on = true; def.onEnable(); },
    rebuild() { hdr.children = []; for (let i = 0; i < 6; i++) hdr.appendChild(node('fhc')); for (const o of observers) if (o.connected) o.callback([{ addedNodes: hdr.children }]); } };
}

test('markers are stored per project and never leak between projects', () => {
  const h = harness();
  h.labs.addMarker(2, 'key pose', '#ff0000');
  assert.deepEqual(Object.keys(h.labs.listMarkers()), ['2']);
  h.project('project-b');
  assert.deepEqual({ ...h.labs.listMarkers() }, {});
  h.labs.addMarker(4, 'b');
  h.project('project-a');
  assert.deepEqual(Object.keys(h.labs.listMarkers()), ['2']);
  assert.deepEqual([...h.storage.keys()].sort(), ['nemo-labs-markers-project-a', 'nemo-labs-markers-project-b']);
});

test('decoration survives a header rebuild; disable removes every flag and the observer; the document is untouched', () => {
  const h = harness(), before = JSON.stringify(h.state);
  h.labs.addMarker(2, 'pose'); h.labs.addMarker(5, 'end');
  assert.deepEqual(h.flags(), [2, 5]);
  h.rebuild();
  assert.deepEqual(h.flags(), [2, 5], 'renderTimeline rebuilt the cells; the flags came back');
  h.labs.removeMarker(5);
  assert.deepEqual(h.flags(), [2]);
  h.disable();
  assert.deepEqual(h.flags(), []);
  assert.equal(h.observers.filter((o) => o.connected).length, 0, 'no header observer remains attached');
  h.rebuild();
  assert.deepEqual(h.flags(), [], 'a disabled aid never redecorates');
  h.enable();
  assert.deepEqual(h.flags(), [2]);
  assert.equal(h.observers.filter((o) => o.connected).length, 1, 'enabling attaches exactly one observer');
  assert.equal(JSON.stringify(h.state), before);
});
