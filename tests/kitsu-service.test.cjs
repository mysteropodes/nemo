'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const Service = require('../src/js/adapters/kitsu-service.js');
const source = fs.readFileSync(path.join(__dirname, '../src/js/kitsu.js'), 'utf8');
const session = { serverUrl: 'https://kitsu.invalid', accessToken: 'fixture-token' };
const response = (value, options = {}) => ({ ok: true, status: 200,
  json: async () => value, text: async () => '', ...options });
function fixture(options = {}) {
  const calls = [], saved = new Map(options.saved ? [['sm-kitsu-session', options.saved]] : []);
  const state = { layers: [{ frames: [{ strokes: [] }] }] };
  const root = { console, state, URLSearchParams, NemoKitsuService: Service,
    document: { readyState: 'loading', addEventListener() {} },
    sessionStorage: { getItem: key => saved.get(key), setItem: (key, value) => saved.set(key, value),
      removeItem: key => saved.delete(key) },
    localStorage: { setItem() { throw Error('unexpected persistent credential write'); } },
    createUserLayer(name) { calls.push(['layer', name]); state.layers.push({ name, frames: [] }); },
    activateUL(n) { calls.push(['activate', n]); }, loadFrame(n) { calls.push(['frame', n]); },
    renderOS() {}, renderArcs() {}, updateUI() {},
    SMProject: { async newProject(cfg) { calls.push(['newProject', cfg]);
      if (options.admit) return options.admit(cfg);
      state.layers = [{ frames: [{ strokes: [] }] }];
    } }
  };
  if (!options.browser) root.__TAURI__ = { http: { async fetch(url, request) {
    calls.push(['http', url, request]);
    return options.fetch ? options.fetch(url, request) : response([]);
  } } };
  root.window = root;
  vm.runInNewContext(source, root, { filename: 'src/js/kitsu.js' });
  return { root, state, calls, saved, api: root.SMKitsu };
}
test('construction makes no request and desktop availability stays dynamic', async () => {
  let available = false, calls = 0;
  const service = Service.create({ available: () => available, fetch: () => { calls++; } });
  assert.equal(service.available(), false);
  await assert.rejects(service.login(session.serverUrl, 'fixture-user', 'fixture-password'), /desktop/);
  assert.equal(calls, 0); available = true; assert.equal(service.available(), true);
});
test('desktop forwards the original HTTP receiver without binding a stale provider', async () => {
  const root = { __TAURI__: { http: { async fetch() { assert.equal(this, root.__TAURI__.http); return response(7); } } } };
  const service = Service.desktop(root);
  assert.equal(await service.request(session, '/projects/open'), 7);
  delete root.__TAURI__; assert.equal(service.available(), false);
});
test('login retains form encoding, slash normalization and session-only identity fields', async () => {
  const f = fixture({ fetch: async () => response({ access_token: 'fixture-token', user: { id: 'person', first_name: 'Test', last_name: 'User' } }) });
  const result = await f.api.login(session.serverUrl + '///', 'fixture+user', 'fixture password');
  assert.equal(result.serverUrl, session.serverUrl); assert.equal(result.personName, 'Test User');
  assert.equal(result.personId, 'person');
  const request = f.calls[0]; assert.equal(request[1], session.serverUrl + '/api/auth/login');
  assert.equal(request[2].method, 'POST'); assert.equal(new URLSearchParams(request[2].body).get('email'), 'fixture+user');
  assert.equal(JSON.parse(f.saved.get('sm-kitsu-session')).accessToken, 'fixture-token');
});
test('login HTTP failure and missing token preserve the previous session', async () => {
  for (const reply of [response({}, { ok: false, status: 401 }), response({})]) {
    const f = fixture({ saved: JSON.stringify(session), fetch: async () => reply });
    await assert.rejects(f.api.login(session.serverUrl, 'fixture-user', 'fixture-password'));
    assert.equal(f.api.getSession().accessToken, session.accessToken);
    assert.equal(f.saved.get('sm-kitsu-session'), JSON.stringify(session));
    assert.equal(f.calls.some(c => c[0] === 'newProject'), false);
  }
});
test('request retains bearer precedence, explicit body and default method', async () => {
  const calls = [];
  const service = Service.create({ available: () => true, fetch: async (url, opts) => { calls.push([url, opts]); return response({ count: 3 }); } });
  assert.deepEqual(await service.request(session, '/projects/open'), { count: 3 });
  await service.request(session, '/actions/task', { method: 'POST', headers: { Authorization: 'ignored', Other: 'header' }, body: 'payload' });
  assert.equal(calls[0][1].method, 'GET'); assert.equal(calls[1][1].headers.Authorization, 'Bearer fixture-token');
  assert.equal(calls[1][1].headers.Other, 'header'); assert.equal(calls[1][1].body, 'payload');
});
test('service error text and unreadable-body status fallback remain separate from admission', async () => {
  for (const [reply, expected] of [[response(null, { ok: false, status: 503, text: async () => 'offline' }), /Kitsu \(503\) offline/],
    [response(null, { ok: false, status: 502, statusText: 'Gateway', text: async () => { throw Error('body unavailable'); } }), /Kitsu \(502\) Gateway/]]) {
    const f = fixture({ fetch: async () => reply });
    await assert.rejects(f.api.listProjects(session), expected);
    assert.equal(f.calls.some(c => c[0] === 'newProject'), false);
  }
});
test('browser login is unavailable without a request or document side effect', async () => {
  const f = fixture({ browser: true }); assert.equal(f.api.isAvailable(), false);
  await assert.rejects(f.api.login(session.serverUrl, 'fixture-user', 'fixture-password'), /desktop/);
  assert.deepEqual(f.calls, []);
});
test('invalid stored JSON degrades to no session and logout removes only its session key', () => {
  const f = fixture({ saved: '{' }); assert.equal(f.api.getSession(), null);
  f.saved.set('other', 'preserve'); f.api.logout(); assert.equal(f.saved.has('sm-kitsu-session'), false);
  assert.equal(f.saved.get('other'), 'preserve'); assert.deepEqual(f.calls, []);
});
test('service list methods retain their exact endpoints without document admission', async () => {
  const f = fixture();
  await f.api.listProjects(session); await f.api.listSequences(session, 'project'); await f.api.listShots(session, 'project');
  await f.api.listShotsForSequence(session, 'sequence'); await f.api.getShot(session, 'shot');
  assert.deepEqual(f.calls.map(c => c[1]), ['/projects/open', '/projects/project/sequences', '/projects/project/shots', '/sequences/sequence/shots', '/shots/shot'].map(p => session.serverUrl + '/api' + p));
});
test('native replacement rejection prevents layer writes, follow-up requests and shot publication', async () => {
  const f = fixture({ admit: async () => { throw Error('native admission unavailable'); } });
  const layers = JSON.stringify(f.state.layers);
  await assert.rejects(f.api.openShot(session, { id: 'project', name: 'Project' }, null, { id: 'shot', name: 'Shot' }), /native admission unavailable/);
  assert.equal(JSON.stringify(f.state.layers), layers); assert.equal(f.state.kitsuShot, null);
  assert.deepEqual(f.calls.map(c => c[0]), ['newProject']);
});
test('pending replacement holds all continuation effects until accepted legacy fixture resolves', async () => {
  let resolve;
  const pending = new Promise(r => { resolve = r; });
  const f = fixture({ admit: () => pending, fetch: async url => url.endsWith('/tasks') ? response([{ id: 'task', task_type_name: 'Animation' }]) : response([], { ok: false, status: 404 }) });
  const opening = f.api.openShot(session, { id: 'project', name: 'Project' }, null, { id: 'shot', name: 'Shot' });
  assert.deepEqual(f.calls.map(c => c[0]), ['newProject']); assert.equal(f.state.kitsuShot, null);
  resolve(); const result = await opening;
  assert.equal(result.meta.fps, 24); assert.equal(result.meta.frameCount, 120);
  assert.equal(result.meta.w, 1920); assert.equal(result.meta.h, 1080);
  assert.equal(f.state.kitsuShot.taskId, 'task'); assert.equal(f.state.layers[0].name, 'Rough');
  assert.equal(f.state.layers[0].frames.length, 120); assert.deepEqual(Array.from(result.previews), []);
});
test('publishing without a shot remains refused without export or service I/O', async () => {
  const f = fixture(); await assert.rejects(f.api.publish(), /ouvert depuis Kitsu/); assert.deepEqual(f.calls, []);
});
