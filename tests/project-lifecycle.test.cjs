'use strict';
// #2510: the MCP v1 project.* operations drive the app's own lifecycle flows through injected ports.
const test = require('node:test');
const assert = require('node:assert/strict');
const Lifecycle = require('../src/js/application/project-lifecycle.js');

function app(options = {}) {
  const calls = [];
  let active = options.active !== false, dirty = !!options.dirty, held = options.held || null, doc = 1;
  const info = () => ({ instanceId: 'instance-A', documentId: 'doc-' + doc, contentRevision: 2, canvasW: options.canvasW || 320,
    canvasH: options.canvasH || 180, fps: 24, totalFrames: 21,
    layers: [{ layerUid: 'layer-1', name: 'Ink', visible: true, locked: false, shy: false, color: '#fff' }] });
  const objectProject = { isActive: () => active, isDirty: () => dirty,
    saveAs: async (path) => { calls.push(['saveAs', path]); return options.saveResult || { status: 'saved', acknowledged: true }; } };
  const tabs = {
    held: () => held,
    openPath: async (path) => { calls.push(['tabs.openPath', path]); held = { name: 'prepared' }; return { status: 'prepared', meta: held }; },
    activateClick: async (meta) => { calls.push(['activate', meta.name]); doc++; held = { name: 'previous' }; return { status: 'activated', meta: held }; },
    close: async (meta) => { calls.push(['close', meta.name]); held = null; return options.closeResult || { status: 'closed' }; },
  };
  const ports = {
    available: () => options.available !== false,
    objectProject: () => objectProject,
    info,
    tabs: () => tabs,
    openPath: async (path) => { calls.push(['openPath', path]); active = true; },
    newProject: async (config, isCurrent, onFailure) => {
      calls.push(['newProject', config, isCurrent()]); active = true; doc++;
      if (options.newFails) { onFailure(new Error('admission refused')); return false; }
      return true;
    },
    exporters: {
      png: async (dir, range) => { calls.push(['png', dir, range]); return { ok: true, dir: dir + '/nemo-native-1' }; },
      svg: async (dir, range) => { calls.push(['svg', dir, range]); return { ok: true }; },
      lottie: async (path, range) => { calls.push(['lottie', path, range]);
        return { ok: false, unavailable: true, error: 'Lottie export is not available for native projects yet.' }; },
    },
  };
  return { calls, lifecycle: Lifecycle.create(ports), setDirty: (value) => { dirty = value; } };
}
const request = (operation, payload, requestId = 'r-' + operation) =>
  ({ apiVersion: 1, requestId, instanceId: 'instance-A', operation, payload });

test('open with no project runs the ordinary Open path and answers the opened document', async () => {
  const { calls, lifecycle } = app({ active: false });
  const response = await lifecycle.handle(request('project.open', { path: '/tmp/A.json' }));
  assert.deepEqual(calls, [['openPath', '/tmp/A.json']]);
  assert.equal(response.ok, true);
  assert.deepEqual([response.documentId, response.revision, response.result.layers[0]],
    ['doc-1', 2, { layerUid: 'layer-1', name: 'Ink', visible: true, locked: false, shy: false }]);
});

test('open over a clean project prepares, switches to, then closes the previous tab, as the UI does', async () => {
  const { calls, lifecycle } = app();
  const response = await lifecycle.handle(request('project.open', { path: '/tmp/A-copy.json' }));
  assert.deepEqual(calls, [['tabs.openPath', '/tmp/A-copy.json'], ['activate', 'prepared'], ['close', 'previous']]);
  assert.equal(response.ok, true); assert.equal(response.documentId, 'doc-2');
});

test('a dirty or tab-holding project is never replaced, and nothing runs', async () => {
  for (const options of [{ dirty: true }, { held: { name: 'retained' } }]) {
    const { calls, lifecycle } = app(options);
    for (const [operation, payload] of [['project.open', { path: '/tmp/B.json' }], ['project.new', { canvasW: 320, canvasH: 180, fps: 24 }]]) {
      const response = await lifecycle.handle(request(operation, payload));
      assert.equal(response.ok, false);
      assert.equal(response.error.code, options.dirty ? 'unsaved_changes' : 'unavailable');
    }
    assert.deepEqual(calls, []);
  }
});

test('an open that cannot close the previous tab reports indeterminate', async () => {
  const { lifecycle } = app({ closeResult: { status: 'refused' } });
  const response = await lifecycle.handle(request('project.open', { path: '/tmp/A-copy.json' }));
  assert.deepEqual([response.ok, response.error.code], [false, 'indeterminate']);
});

test('new creates through the ordinary New Project route and checks the created canvas', async () => {
  const { calls, lifecycle } = app({ canvasW: 1920, canvasH: 1080 });
  const response = await lifecycle.handle(request('project.new', { canvasW: 1920, canvasH: 1080, fps: 24 }));
  assert.deepEqual(calls, [['newProject', { w: 1920, h: 1080, fps: 24, name: 'Untitled' }, true]]);
  assert.equal(response.ok, true); assert.equal(response.result.canvasW, 1920);
  const failed = await app({ canvasW: 1920, canvasH: 1080, newFails: true }).lifecycle
    .handle(request('project.new', { canvasW: 1920, canvasH: 1080, fps: 24 }));
  assert.deepEqual([failed.ok, failed.error.code, failed.error.message], [false, 'unavailable', 'The new project was not created: admission refused']);
});

test('saveAs runs the Save As workflow at the given path and succeeds only when acknowledged', async () => {
  const saved = app();
  const response = await saved.lifecycle.handle(request('project.saveAs', { path: '/tmp/A-copy.json' }));
  assert.deepEqual(saved.calls, [['saveAs', '/tmp/A-copy.json']]);
  assert.deepEqual([response.ok, response.result.path], [true, '/tmp/A-copy.json']);
  const refused = await app({ saveResult: { status: 'refused' } }).lifecycle.handle(request('project.saveAs', { path: '/tmp/x.json' }));
  assert.deepEqual([refused.ok, refused.error.code], [false, 'unavailable']);
});

test('export uses the path exporters over the whole timeline by default; native Lottie stays a typed refusal', async () => {
  const { calls, lifecycle } = app();
  const png = await lifecycle.handle(request('project.export', { format: 'png', destination: '/tmp/out' }, 'r-png'));
  assert.deepEqual([png.ok, png.result.output, png.result.end], [true, '/tmp/out/nemo-native-1', 20]);
  const svg = await lifecycle.handle(request('project.export', { format: 'svg', destination: '/tmp/svg', start: 2, end: 5 }, 'r-svg'));
  assert.equal(svg.ok, true);
  const lottie = await lifecycle.handle(request('project.export', { format: 'lottie', destination: '/tmp/a.json' }, 'r-lottie'));
  assert.deepEqual([lottie.ok, lottie.error.code, lottie.error.message],
    [false, 'unavailable', 'Lottie export is not available for native projects yet.']);
  const outside = await lifecycle.handle(request('project.export', { format: 'png', destination: '/tmp/out', end: 21 }, 'r-out'));
  assert.deepEqual([outside.ok, outside.error.code], [false, 'invalid_request']);
  assert.deepEqual(calls, [['png', '/tmp/out', { start: 0, end: 20 }], ['svg', '/tmp/svg', { start: 2, end: 5 }],
    ['lottie', '/tmp/a.json', { start: 0, end: 20 }]]);
});

test('malformed payloads refuse before any flow runs', async () => {
  const { calls, lifecycle } = app();
  for (const [operation, payload] of [
    ['project.open', { path: 'relative.json' }], ['project.open', { path: '/a.json', replaceUnsaved: true }],
    ['project.new', { canvasW: 0, canvasH: 180, fps: 24 }], ['project.new', { canvasW: 320, canvasH: 180 }],
    ['project.saveAs', {}], ['project.export', { format: 'gif', destination: '/tmp' }],
    ['project.export', { format: 'png', destination: '/tmp', start: 5, end: 2 }]]) {
    const response = await lifecycle.handle(request(operation, payload));
    assert.deepEqual([response.ok, response.error.code], [false, 'invalid_request'], operation + ' ' + JSON.stringify(payload));
  }
  assert.deepEqual(calls, []);
});

test('a retried requestId returns the first answer; a changed body and a browser host refuse', async () => {
  const { calls, lifecycle } = app();
  const first = await lifecycle.handle(request('project.saveAs', { path: '/tmp/A-copy.json' }, 'same'));
  const again = await lifecycle.handle(request('project.saveAs', { path: '/tmp/A-copy.json' }, 'same'));
  assert.deepEqual(again, first); assert.equal(calls.length, 1, 'the save ran once');
  const changed = await lifecycle.handle(request('project.saveAs', { path: '/tmp/other.json' }, 'same'));
  assert.deepEqual([changed.ok, changed.error.code], [false, 'invalid_request']);
  const browser = await app({ available: false }).lifecycle.handle(request('project.saveAs', { path: '/tmp/A.json' }));
  assert.deepEqual([browser.ok, browser.error.code], [false, 'unavailable']);
});

test('one project operation runs at a time', async () => {
  const { lifecycle } = app();
  const first = lifecycle.handle(request('project.saveAs', { path: '/tmp/A.json' }, 'one'));
  const second = await lifecycle.handle(request('project.saveAs', { path: '/tmp/B.json' }, 'two'));
  assert.deepEqual([second.ok, second.error.code], [false, 'unavailable']);
  assert.equal((await first).ok, true);
});
