'use strict';
// CS8-D08 I43 (#2381): the original Kitsu shot metadata defaults (kitsu.js shotMetadata via openShot), pinned over
// exact source fixtures. Service behaviour only: no live server, no native admission claim (that is I44).
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const Service = require('../src/js/adapters/kitsu-service.js');

const source = fs.readFileSync(path.join(__dirname, '../src/js/kitsu.js'), 'utf8');
const session = { serverUrl: 'https://kitsu.invalid', accessToken: 'fixture-token' };
const response = (value) => ({ ok: true, status: 200, json: async () => value, text: async () => '' });
function open(project, shot) {
  const created = [], saved = new Map();
  const state = { layers: [{ frames: [{ strokes: [] }] }] };
  const root = { console, state, URLSearchParams, NemoKitsuService: Service,
    document: { readyState: 'loading', addEventListener() {} },
    sessionStorage: { getItem: (k) => saved.get(k), setItem: (k, v) => saved.set(k, v), removeItem: (k) => saved.delete(k) },
    localStorage: { setItem() { throw Error('unexpected persistent credential write'); } },
    createUserLayer(name) { state.layers.push({ name, frames: [] }); return state.layers.length - 1; },
    activateUL() {}, loadFrame() {}, renderOS() {}, renderArcs() {}, updateUI() {},
    SMProject: { async newProject(cfg) { created.push(cfg); state.layers = [{ frames: [{ strokes: [] }] }]; } },
    __TAURI__: { http: { fetch: async () => response([]) } } };
  root.window = root;
  vm.runInNewContext(source, root, { filename: 'src/js/kitsu.js' });
  return root.SMKitsu.openShot(session, project, null, shot).then((result) => ({ meta: result.meta, created, state, saved }));
}

test('fps, frame count, resolution and deadline follow the original lookup order and defaults', async () => {
  const cases = [
    // [project, shot, expected {fps, frameCount, w, h, deadline}]
    [{}, { name: 'S', data: {} }, { fps: 24, frameCount: 120, w: 1920, h: 1080, deadline: null }],
    [{}, { name: 'S', data: { fps: '25', frame_in: 10, frame_out: 109 } }, { fps: 25, frameCount: 100, w: 1920, h: 1080, deadline: null }],
    [{}, { name: 'S', data: { frame_rate: 30 }, nb_frames: 48 }, { fps: 30, frameCount: 48, w: 1920, h: 1080, deadline: null }],
    [{}, { name: 'S', fps: 12, data: { nb_frames: 7 } }, { fps: 12, frameCount: 7, w: 1920, h: 1080, deadline: null }],
    [{ fps: 23.976 }, { name: 'S' }, { fps: 23.976, frameCount: 120, w: 1920, h: 1080, deadline: null }],
    [{ data: { fps: 50 } }, { name: 'S', data: { fps: '' } }, { fps: 50, frameCount: 250, w: 1920, h: 1080, deadline: null }],
    [{ resolution: '1280x720' }, { name: 'S', data: { resolution: '2048X858', deadline: '2026-11-01' } }, { fps: 24, frameCount: 120, w: 2048, h: 858, deadline: '2026-11-01' }],
    [{ resolution: '1280x720' }, { name: 'S', data: { end_date: '2026-12-24' } }, { fps: 24, frameCount: 120, w: 1280, h: 720, deadline: '2026-12-24' }],
    [{}, { name: 'S', data: { resolution: '4K' } }, { fps: 24, frameCount: 120, w: 1920, h: 1080, deadline: null }],
  ];
  for (const [project, shot, expected] of cases) {
    const { meta, created, state } = await open(project, shot);
    assert.deepEqual({ ...meta }, expected, JSON.stringify(shot));
    assert.deepEqual(created.map((c) => [c.w, c.h, c.fps]), [[expected.w, expected.h, expected.fps]]);
    assert.equal(state.totalFrames, expected.frameCount);
    assert.equal(state.layers[0].frames.length, expected.frameCount);
    assert.deepEqual(state.layers.map((l) => l.name), ['Rough', 'Clean', 'Color']);
  }
});

test('an unreadable fps is a characterized service defect: fps falls back to 24 but the frame count is NaN', async () => {
  // Kept explicit, not repaired here: frameCount derives from Number(fps) before the `|| 24` fallback, so a
  // non-numeric studio value yields NaN frames and an empty Rough layer.
  const { meta, state } = await open({}, { name: 'S', data: { fps: 'abc' } });
  assert.equal(meta.fps, 24);
  assert.ok(Number.isNaN(meta.frameCount));
  assert.equal(state.layers[0].frames.length, 0);
});

