'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const localFactory = require('../src/js/adapters/feedback-local.js');
const remoteFactory = require('../src/js/adapters/feedback-remote.js');
const bridge = fs.readFileSync(path.join(__dirname, '../src/js/feedback-bridge.js'), 'utf8');
const WORKER = 'https://nemo-feedback.mysteropodes-auth.workers.dev';
const plain = value => JSON.parse(JSON.stringify(value));
function response(data, status = 200) {
  return { ok: status >= 200 && status < 300, status, json: async () => data, text: async () => 'refused' };
}
function fixture(hostname = 'nemo.example') {
  const store = new Map(), files = new Map(), io = [], requests = [], warnings = [], listeners = [];
  let key = 'project one', now = 1000, failWrite = false;
  let route = async url => {
    if (url.startsWith(WORKER)) return response({});
    throw Error('Unexpected test transport: ' + url);
  };
  const fetch = async (url, options) => {
    requests.push({ channel: 'plain', url, options: plain(options || {}) });
    return route(url, options);
  };
  const context = {
    NemoFeedbackLocal: localFactory, NemoFeedbackRemote: remoteFactory,
    state: { tool: 'draw', currentFrame: 7, activeLayerIdx: 0, layers: [{ name: 'Ink' }],
      userProfile: { id: 'me', name: 'Me', color: '#abc', role: 'artist', email: 'me@example.test' } },
    document: { title: 'Nemo v1.2.3', addEventListener: (...args) => listeners.push(args) },
    navigator: { userAgent: 'test-agent', onLine: true }, screen: { width: 1440, height: 900 },
    innerWidth: 800, innerHeight: 600, location: { hostname },
    Date: { now: () => now }, Math: { random: () => 0.5, floor: Math.floor }, fetch,
    console: { warn: (...args) => warnings.push(args) },
    localStorage: {
      getItem: key => store.get(key) || null,
      setItem(key, value) { if (failWrite) throw Error('quota'); store.set(key, value); },
    },
    SMProject: { getProjectKey: () => key, getSyncFolder: () => '',
      profileDir: (root, id) => root + '/' + id },
    exportJSON() { throw Error('feedback must never serialize a project'); },
    importJSON() { throw Error('feedback must never mutate a project'); },
  };
  context.window = context;
  vm.runInNewContext(bridge, context, { filename: 'feedback-bridge.js' });
  function desktop() {
    context.__TAURI__ = {
      path: { appDataDir: async () => '/app///' },
      http: { fetch: async (url, options) => {
        requests.push({ channel: 'plugin', url, options: plain(options || {}) });
        return route(url, options);
      } },
      fs: {
        async readDir(dir) {
          io.push(['readDir', dir]);
          const names = new Set();
          for (const name of files.keys()) {
            if (name.startsWith(dir + '/')) names.add(name.slice(dir.length + 1).split('/')[0]);
          }
          return [...names].map(name => ({ name }));
        },
        async readTextFile(name) { if (!files.has(name)) throw Error('missing'); return files.get(name); },
        async mkdir(dir, opts) { io.push(['mkdir', dir, opts]); },
        async writeTextFile(name, text) { io.push(['write', name, text]); files.set(name, text); },
        async remove(name) { io.push(['remove', name]); files.delete(name); },
      },
    };
  }
  return { api: context.SMFeedback, context, store, files, io, requests, warnings, listeners, desktop,
    project: value => { key = value; }, tick: value => { now = value; },
    quota: value => { failWrite = value; }, route: fn => { route = fn; } };
}

test('public feedback API and local trust transitions stay outside project state', async () => {
  const f = fixture(), api = f.api;
  assert.deepEqual(Object.keys(api), ['logAction', 'recentClickTrail', 'actionLogMark', 'clickLogMark',
    'actionTrailSince', 'clickTrailSince', 'submitFeedback', 'readAllLocal', 'checkIncomingFeedback',
    'importIncoming', 'pullAllIncoming', 'approveFeedback', 'resolveFeedback', 'deleteFeedback',
    'githubTriageToken', 'setGithubTriageToken', 'fetchGithubIssues', 'approveGithubIssue',
    'resolveGithubIssue', 'editGithubIssueBody', 'deleteGithubIssue']);
  const entry = await api.submitFeedback({ note: 'Own', pos: { x: 2, y: 3 }, screenshotDataUrl: 'data:image/png;base64,AA==' });
  assert.equal(entry.status, 'approved'); assert.equal(entry.origin, 'local');
  assert.deepEqual(plain(entry.pos), [2, 3]);
  assert.deepEqual(plain(entry.screenshotDataUrls), ['data:image/png;base64,AA==']);
  assert.equal(f.store.has('nemo-feedback-project one'), true);
  const remote = { id: 'fb_remote', projectKey: 'project one', status: 'approved', author: { id: 'them' }, createdAt: 1 };
  const imported = await api.importIncoming(remote);
  assert.equal(imported.status, 'pending'); assert.equal(imported.origin, 'them');
  assert.equal(remote.status, 'approved');
  assert.equal((await api.approveFeedback(remote.id)).status, 'approved');
  assert.equal((await api.resolveFeedback(remote.id, 'fixed')).resolution, 'fixed');
  assert.equal((await api.readAllLocal())[0].resolvedAt, 1000);
  assert.equal(await api.approveFeedback('missing'), null);
  await api.deleteFeedback(remote.id);
  f.project('other'); assert.deepEqual(plain(await api.readAllLocal()), []);
  f.project('project one'); assert.equal((await api.readAllLocal()).length, 1);
  assert.equal(f.context.state.feedback, undefined);
});

test('localStorage preserves insertion/update order, cap, read failure and write failure', async () => {
  const f = fixture();
  const entries = Array.from({ length: 500 }, (_, i) => ({ id: 'fb_' + i, createdAt: i }));
  f.store.set('nemo-feedback-project one', JSON.stringify(entries));
  await f.api.importIncoming({ id: 'fb_10', createdAt: 10 });
  assert.equal((await f.api.readAllLocal())[10].status, 'pending');
  await f.api.importIncoming({ id: 'new', createdAt: 1001 });
  const rows = await f.api.readAllLocal();
  assert.equal(rows.length, 500); assert.equal(rows[0].id, 'new'); assert.equal(rows[499].id, 'fb_498');
  f.store.set('nemo-feedback-project one', '{bad');
  assert.deepEqual(plain(await f.api.readAllLocal()), []);
  f.quota(true);
  await assert.rejects(f.api.submitFeedback({ note: 'must fail' }), /quota/);
  assert.equal(f.requests.length, 0, 'failed durable write must not publish');
  assert.equal(f.warnings.length, 0);
});

test('dev-server probes only local hosts, caches availability and retains exact route bodies', async () => {
  const f = fixture('localhost');
  f.route(async (url, opts) => {
    if (url === '/__feedback/ping') return response({});
    if (url.startsWith('/__feedback/list?')) return response([{ id: 'server' }]);
    if (url === '/__feedback/save' || url === '/__feedback/delete') return response({});
    throw Error(url);
  });
  assert.equal((await f.api.readAllLocal())[0].id, 'server');
  await f.api.importIncoming({ id: 'r', status: 'approved' });
  await f.api.deleteFeedback('r');
  assert.deepEqual(f.requests.map(r => r.url), ['/__feedback/ping',
    '/__feedback/list?projectKey=project%20one', '/__feedback/save', '/__feedback/delete']);
  assert.deepEqual(f.requests[2].options, { method: 'POST', body: '{"id":"r","status":"pending","origin":"remote"}' });
  assert.equal(f.requests[3].options.body, '{"id":"r","projectKey":"project one"}');
  assert.equal(f.store.size, 0);
  const unavailable = fixture('127.0.0.1');
  unavailable.route(async () => response({}, 404));
  await unavailable.api.importIncoming({ id: 'fallback' });
  await unavailable.api.readAllLocal();
  assert.equal(unavailable.requests.length, 1);
  assert.equal(unavailable.store.has('nemo-feedback-project one'), true);
  const deployed = fixture(); await deployed.api.readAllLocal(); assert.equal(deployed.requests.length, 0);
});

test('Tauri persistence keeps file filtering, newest read order and oldest pruning', async () => {
  const f = fixture('localhost'); f.desktop();
  for (let i = 0; i < 501; i++) f.files.set('/app/feedback/project one/fb_' + i + '.json', JSON.stringify({ id: 'fb_' + i, createdAt: i }));
  f.files.set('/app/feedback/project one/broken.json', '{bad');
  f.files.set('/app/feedback/project one/keep.txt', 'not feedback');
  await f.api.importIncoming({ id: 'fb_400', createdAt: 400 });
  const rows = await f.api.readAllLocal();
  assert.equal(rows.length, 500); assert.equal(rows[0].createdAt, 500);
  assert.equal(f.files.has('/app/feedback/project one/fb_0.json'), false);
  assert.equal(f.files.has('/app/feedback/project one/broken.json'), true);
  assert.equal(f.files.has('/app/feedback/project one/keep.txt'), true);
  assert.equal(f.requests.length, 0); assert.equal(f.store.size, 0);
  await f.api.deleteFeedback('fb_400');
  assert.equal(f.files.has('/app/feedback/project one/fb_400.json'), false);
});

test('shared folders exclude own profile, known ids and other projects; pulled approval never transfers', async () => {
  const f = fixture(); f.desktop(); f.context.SMProject.getSyncFolder = () => '/sync';
  const add = (name, data) => f.files.set(name, JSON.stringify(data));
  add('/sync/me/feedback/fb_self.json', { id: 'fb_self', projectKey: 'project one' });
  add('/sync/them/feedback/fb_known.json', { id: 'fb_known', projectKey: 'project one' });
  add('/app/feedback/project one/fb_known.json', { id: 'fb_known', createdAt: 1 });
  add('/sync/them/feedback/fb_wrong.json', { id: 'fb_wrong', projectKey: 'other' });
  add('/sync/them/feedback/fb_new.json', { id: 'fb_new', projectKey: 'project one', status: 'approved', author: { id: 'them' } });
  add('/sync/them/feedback/_profile.json', { id: 'them' });
  const found = await f.api.checkIncomingFeedback(); assert.deepEqual(plain(found.map(e => e.id)), ['fb_new']);
  const imported = await f.api.pullAllIncoming(); assert.equal(imported[0].status, 'pending');
  assert.equal(imported[0].origin, 'them');
  const own = await f.api.submitFeedback({ note: 'shared' });
  assert.equal(f.files.get('/sync/me/feedback/' + own.id + '.json'), JSON.stringify(own));
  assert.equal(f.files.get('/sync/me/feedback/_profile.json'), JSON.stringify(f.context.state.userProfile));
  assert.equal(f.requests.every(r => r.channel === 'plugin'), true);
});

test('action and captured click trails keep coalescing, caps and precise recording', async () => {
  const f = fixture(), api = f.api;
  assert.equal(f.listeners.length, 1); assert.equal(f.listeners[0][0], 'pointerdown'); assert.equal(f.listeners[0][2], true);
  api.logAction(); f.tick(1001); api.logAction();
  assert.equal(api.actionTrailSince(0)[0].count, 2); assert.equal(api.actionTrailSince(0)[0].t, 1001);
  api.logAction('audio:deleteTrack'); assert.equal(api.actionTrailSince(1)[0].tool, 'audio:deleteTrack');
  for (let i = 0; i < 151; i++) { f.context.state.currentFrame = i; api.logAction(); }
  assert.equal(api.actionLogMark(), 150);
  const click = f.listeners[0][1];
  click({ target: { nodeType: 3 } }); assert.equal(api.clickLogMark(), 0);
  const el = { nodeType: 1, tagName: 'BUTTON', id: 'save', className: 'one two three', textContent: 'x'.repeat(60), title: 'Save', dataset: { action: 'save' } };
  click({ target: el, clientX: 3, clientY: 4 }); click({ target: el, clientX: 5, clientY: 6 });
  const captured = api.recentClickTrail()[0];
  assert.equal(captured.count, 2); assert.equal(captured.x, 3);
  assert.equal(captured.el.path, 'button#save'); assert.equal(captured.el.text.length, 40);
  assert.deepEqual(plain(captured.el.data), { action: 'save' });
  for (let i = 0; i < 201; i++) click({ target: { ...el, id: 'b' + i }, clientX: i, clientY: 0 });
  assert.equal(api.clickLogMark(), 200); assert.equal(api.recentClickTrail().length, 20);
  const am = api.actionLogMark(), cm = api.clickLogMark();
  f.context.state.currentFrame = 999; api.logAction('recorded');
  click({ target: { ...el, id: 'last' }, clientX: 1, clientY: 2 });
  // Ring-buffer marks retain original length-based behavior; explicit recordings are still preferred.
  const entry = await api.submitFeedback({ actionTrail: [{ tool: 'precise' }], clickTrail: [] });
  assert.equal(entry.recorded, true); assert.deepEqual(plain(entry.actionTrail), [{ tool: 'precise' }]);
  assert.deepEqual(plain(entry.clickTrail), []); assert.equal(am, 150); assert.equal(cm, 200);
});

test('Worker attachments are sequential, use plugin only on desktop and keep literal issue body bytes', async () => {
  const f = fixture(); f.desktop(); const sequence = [];
  let active = 0;
  f.route(async (url, opts) => {
    assert.equal(active, 0); active++; const data = JSON.parse(opts.body); sequence.push(data);
    await Promise.resolve(); active--;
    if (url.endsWith('/attachment')) return response({ url: 'https://shots/' + data.filename });
    return response({});
  });
  const entry = await f.api.submitFeedback({ note: 'Broken', tags: ['idee', 'bug'], blocking: true,
    actionTrail: [], clickTrail: [], screenshotDataUrls: ['data:image/jpeg;base64,AA==', 'bad', 'data:image/png;base64,BB=='] });
  assert.deepEqual(sequence.slice(0, 2), [
    { filename: entry.id + '-1.jpg', contentBase64: 'AA==' },
    { filename: entry.id + '-3.png', contentBase64: 'BB==' },
  ]);
  assert.deepEqual(f.requests.map(r => r.url), [WORKER + '/attachment', WORKER + '/attachment', WORKER + '/issue']);
  assert.equal(f.requests.every(r => r.channel === 'plugin'), true);
  assert.deepEqual(f.requests[2].options.headers, { 'Content-Type': 'application/json' });
  assert.equal(sequence[2].title, '[BLOQUANT] Broken');
  assert.deepEqual(sequence[2].labels, ['pending', 'idée', 'bug', 'blocking']);
  const expected = '**Note**\nBroken\n\n**Captures d\'écran**\n![capture 1](https://shots/' + entry.id + '-1.jpg)\n' +
    '![capture 2](https://shots/' + entry.id + '-3.png)\n\n**Contexte**\n- Frame: 7\n- Auteur: Me (artist)\n' +
    '- Email: me@example.test\n- Projet: project one\n- Enregistré précisément: oui\n\n**Infos techniques**\n' +
    '- Plateforme: App bureau\n- Version: v1.2.3\n- Navigateur: test-agent\n' +
    '- Fenêtre: 800×600 (écran 1440×900)\n- Connexion: en ligne\n\n**Trail d\'actions**\n```json\n[]\n```\n\n' +
    '**Trail de clics**\n```json\n[]\n```\n\n<!-- sm-feedback-id: ' + entry.id + ' -->';
  assert.equal(sequence[2].body, expected);
});

test('attachment and Worker failures keep best-effort publication and the durable entry', async () => {
  const f = fixture();
  f.route(async url => url.endsWith('/attachment') ? response({ error: 'shot refused' }, 403) : response({ error: 'issue refused' }, 429));
  const entry = await f.api.submitFeedback({ note: 'saved', screenshotDataUrl: 'data:image/png;base64,AA==' });
  assert.equal((await f.api.readAllLocal())[0].id, entry.id);
  assert.match(f.warnings[0][1].message, /shot refused/); assert.match(f.warnings[1][1].message, /issue refused/);
  assert.equal(JSON.parse(f.requests[0].options.body).filename, entry.id + '.png');
  const issue = JSON.parse(f.requests[1].options.body);
  assert.equal(issue.body.includes('**Capture'), false); assert.equal(issue.body.includes('Navigateur web'), true);
  f.tick(2000);
  f.route(async () => ({ ok: false, status: 502, json: async () => { throw Error('not JSON'); } }));
  await f.api.submitFeedback({});
  assert.match(f.warnings.at(-1)[1].message, /feedback worker error 502/);
});

test('GitHub list remains public and token triage retains exact URLs, headers, ordering and errors', async () => {
  const f = fixture(); f.desktop();
  f.route(async url => {
    if (url.includes('/search/issues')) return response({ items: [
      { number: 1, node_id: 'N1', title: 'report', body: '<!-- sm-feedback-id: fb_1 -->', html_url: 'https://issue/1', labels: [{ name: 'pending' }], state: 'open', created_at: 'date' },
      { number: 2, pull_request: {}, body: '' }, { number: 3, body: '' },
    ] });
    return response({ updated: true });
  });
  const issues = await f.api.fetchGithubIssues();
  assert.deepEqual(plain(issues.map(i => [i.number, i.id])), [[1, 'fb_1'], [3, 'gh_3']]);
  assert.equal(f.requests[0].url, 'https://api.github.com/search/issues?q=repo%3Amysteropodes%2Fnemo%20is%3Aissue&per_page=100&sort=created&order=desc');
  assert.deepEqual(f.requests[0].options.headers, { Accept: 'application/vnd.github+json' });
  await assert.rejects(f.api.approveGithubIssue(1, []), /Aucun token GitHub configuré/);
  assert.equal(f.requests.length, 1);
  f.api.setGithubTriageToken('personal'); assert.equal(f.api.githubTriageToken(), 'personal');
  assert.equal(f.store.get('nemo-github-triage-token'), 'personal');
  f.project('other'); assert.equal(f.api.githubTriageToken(), 'personal');
  await f.api.approveGithubIssue(1, ['pending', 'bug']);
  await f.api.resolveGithubIssue(1, ['pending', 'bug'], 'fixed');
  await f.api.editGithubIssueBody(1, 'new body');
  await f.api.deleteGithubIssue('N1');
  assert.equal(f.requests.every(r => r.channel === 'plain'), true);
  assert.deepEqual(f.requests.slice(1).map(r => [r.options.method, r.url.split('github.com')[1]]), [
    ['PATCH', '/repos/mysteropodes/nemo/issues/1'], ['POST', '/repos/mysteropodes/nemo/issues/1/comments'],
    ['PATCH', '/repos/mysteropodes/nemo/issues/1'], ['PATCH', '/repos/mysteropodes/nemo/issues/1'], ['POST', '/graphql'],
  ]);
  assert.deepEqual(f.requests[1].options.headers, { Authorization: 'Bearer personal', Accept: 'application/vnd.github+json', 'Content-Type': 'application/json' });
  assert.equal(f.requests[1].options.body, '{"labels":["bug"]}');
  assert.equal(f.requests[2].options.body, '{"body":"fixed"}');
  assert.equal(f.requests[3].options.body, '{"labels":["bug","resolved"],"state":"closed"}');
  assert.equal(f.requests[4].options.body, '{"body":"new body"}');
  assert.deepEqual(JSON.parse(f.requests[5].options.body), {
    query: 'mutation($id:ID!){deleteIssue(input:{issueId:$id}){clientMutationId}}', variables: { id: 'N1' },
  });
  f.route(async () => response({}, 403));
  await assert.rejects(f.api.fetchGithubIssues(), /GitHub list failed: 403/);
  await assert.rejects(f.api.editGithubIssueBody(1, 'x'), /GitHub update failed: 403 refused/);
  f.route(async () => response({ errors: [{ message: 'not admin' }] }));
  await assert.rejects(f.api.deleteGithubIssue('N1'), /GitHub delete failed: not admin/);
  f.quota(true); f.api.setGithubTriageToken('ignored'); assert.equal(f.api.githubTriageToken(), 'personal');
});
