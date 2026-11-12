'use strict';
// CS8-B09 U32 (#2374): the Labs 3D reference viewer over a counting fake WebGL context. A known OBJ gives its
// triangle count and view controls; closing or replacing the viewer releases every GL object and window listener;
// opening writes no document state; fetch/parse failures keep the original toasts and results.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '..', 'src/js/labs/reference-3d.js'), 'utf8');
function element(tag, live) {
  const listeners = {};
  const el = { tagName: tag, style: {}, children: [], textContent: '', width: 0, height: 0, parentNode: null,
    appendChild(child) { child.parentNode = el; el.children.push(child); return child; },
    remove() { if (el.parentNode) el.parentNode.children = el.parentNode.children.filter((c) => c !== el); el.parentNode = null; live.delete(el); },
    addEventListener(kind, fn) { (listeners[kind] = listeners[kind] || []).push(fn); },
    fire(kind, event) { for (const fn of listeners[kind] || []) fn(Object.assign({ stopPropagation() {}, preventDefault() {} }, event)); },
    getContext() { return live.gl; } };
  live.add(el);
  return el;
}
function fakeGL() {
  const made = new Set(), stats = { created: 0, deleted: 0, lost: 0 };
  const make = () => { const o = { id: ++stats.created }; made.add(o); return o; };
  const drop = (o) => { if (made.delete(o)) stats.deleted++; };
  const gl = new Proxy({ made, stats,
    createBuffer: make, createTexture: make, createProgram: make, createShader: () => ({}),
    deleteBuffer: drop, deleteTexture: drop, deleteProgram: drop,
    getShaderParameter: () => true, getProgramParameter: () => true,
    getExtension: (name) => (name === 'WEBGL_lose_context' ? { loseContext() { stats.lost++; } } : null) },
  { get: (t, k) => (k in t ? t[k] : typeof k === 'string' && /^[A-Z_0-9]+$/.test(k) ? 1 : function () { return 0; }) });
  return gl;
}
function harness(options = {}) {
  const live = new Set(), toasts = [], windowListeners = new Map(), frames = new Set();
  live.gl = options.noGL ? null : fakeGL();
  const body = element('body', live);
  const state = { layers: [{ frames: [] }], contentRevision: 4 };
  const root = { state, SM: { t: (k) => k }, showToast: (t) => toasts.push(t), console: { error() {}, log() {} },
    document: { body, createElement: (tag) => element(tag, live) },
    addEventListener(kind, fn) { (windowListeners.get(kind) || windowListeners.set(kind, new Set()).get(kind)).add(fn); },
    removeEventListener(kind, fn) { if (windowListeners.has(kind)) windowListeners.get(kind).delete(fn); },
    requestAnimationFrame() { const id = frames.size + 1; frames.add(id); return id; }, cancelAnimationFrame(id) { frames.delete(id); },
    URL: { revokeObjectURL() {} }, Image: function () {},
    fetch: options.fetch || (async () => ({ ok: false, status: 404 })),
    NemoNativeObjectProject: { desktop() { throw Error('a reference must not touch the native document'); } },
    exportJSON() { throw Error('a reference must not serialize the project'); },
    SMLabs: { register() {} } };
  root.window = root;
  vm.createContext(root);
  vm.runInContext(source, root, { filename: 'reference-3d.js' });
  const listenerCount = () => [...windowListeners.values()].reduce((n, s) => n + s.size, 0);
  const panel = () => body.children.find((c) => c.id === 'labs-3dref');
  return { root, labs: root.SMLabs, gl: live.gl, toasts, panel, listenerCount, frames, state };
}
const CUBE = ['v 0 0 0', 'v 1 0 0', 'v 1 1 0', 'v 0 1 0', 'v 0 0 1', 'v 1 0 1', 'v 1 1 1', 'v 0 1 1',
  'f 1 2 3 4', 'f 5 8 7 6', 'f 1 5 6 2', 'f 2 6 7 3', 'f 3 7 8 4', 'f 5 1 4 8'].join('\n');

test('a known OBJ opens with its triangle count and the view buttons set the original angles', () => {
  const h = harness();
  assert.deepEqual({ ...h.labs.open3DReference(CUBE) }, { triCount: 12 });
  assert.equal(h.labs.get3DState().triCount, 12);
  const views = h.panel().children[3].children;
  views[1].fire('pointerdown', {});
  assert.deepEqual([h.labs.get3DState().yaw, h.labs.get3DState().pitch], [Math.PI / 2, 0]);
  views[2].fire('pointerdown', {});
  assert.equal(h.labs.get3DState().yaw, Math.PI);
  assert.equal(h.listenerCount(), 2, 'one pointermove and one pointerup');
});

test('replacing or closing the viewer releases every GL object, the context and both window listeners', () => {
  const h = harness();
  h.labs.open3DReference(CUBE);
  const firstMade = h.gl.stats.created;
  h.labs.open3DReference(CUBE);
  assert.equal(h.listenerCount(), 2, 'the replaced viewer left no listener behind');
  assert.equal(h.gl.stats.deleted, firstMade, 'the replaced viewer released all its buffers, textures and program');
  h.labs.close3DReference();
  assert.equal(h.listenerCount(), 0);
  assert.equal(h.gl.made.size, 0, 'nothing allocated is still alive');
  assert.equal(h.gl.stats.lost, 2, 'each context was released');
  assert.equal(h.panel(), undefined); assert.equal(h.frames.size, 0);
  assert.equal(h.labs.get3DState(), null);
});

test('opening a reference writes no document state, and failures keep the original outcomes', async () => {
  const h = harness();
  const before = JSON.stringify(h.state);
  h.labs.open3DReference(CUBE); h.labs.openHandReference('fist'); h.labs.close3DReference();
  assert.equal(JSON.stringify(h.state), before);
  assert.equal(h.labs.open3DReference('# nothing here'), false);
  assert.deepEqual(h.toasts, ['labsToastObjInvalidOrEmpty']);
  assert.equal(await h.labs.openCC0AvatarReference(), false);
  assert.equal(h.toasts[1], 'labsToastAvatarCC0UnavailablePrefixasset absent (404)');
  const parse = harness({ fetch: async () => ({ ok: true, text: async () => 'garbage' }) });
  assert.equal(await parse.labs.openCC0HandReference(), false);
  assert.deepEqual(parse.toasts, ['labsToastHandsCC0UnavailablePrefixOBJ invalide']);
  const noGL = harness({ noGL: true });
  assert.equal(noGL.labs.open3DReference(CUBE), false);
  assert.deepEqual(noGL.toasts, ['labsToastWebglUnavailable']);
  assert.equal(noGL.listenerCount(), 0);
});
