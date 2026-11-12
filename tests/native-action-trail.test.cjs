'use strict';
// #2373: the native transport reports every APPLIED command (and only those) to the Record action trail.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Transport = require('../src/js/adapters/application-mcp.js');

const status = { apiVersion: 2, available: true, instanceId: 'inst', documentId: 'doc', contentRevision: 1 };
async function connected(responder) {
  const seen = [];
  global.SMFeedback = { logNativeCommand: (request) => seen.push(request) };
  const t = Transport.createNativeTauriTransport(async (command, args) => {
    if (command === 'nemo_native_status') return status;
    return responder(command, args);
  });
  await t.connect();
  return { t, seen };
}
const req = (operation, extra) => ({ apiVersion: 2, requestId: 'r' + Math.random().toString(36).slice(2), instanceId: 'inst', documentId: 'doc', expectedRevision: 1, operation, ...extra });
const ok = (request) => ({ apiVersion: 2, requestId: request.requestId, instanceId: 'inst', documentId: 'doc', contentRevision: 2, ok: true });

test('applied commands are reported once each, queries and refusals are not', async () => {
  try {
    const { t, seen } = await connected((command, args) => {
      const request = args.request || JSON.parse(args.requestJson);
      return request.operation === 'command.document.object.refused' ? { ...ok(request), ok: false, contentRevision: 1 } : ok(request);
    });
    const edit = req('command.document.object.property.edit', { payload: { property: 'position' } });
    await t.dispatch(edit);
    await t.dispatch(req('query.document.revision', { payload: {} }));
    await t.dispatch(req('command.document.object.refused', { payload: {} }));
    const obj = req('command.document.object.layer.create', { payload: { name: 'x' } });
    await t.dispatchObject('nemo_native_object_dispatch', obj);
    assert.deepEqual(seen.map((r) => r.operation), ['command.document.object.property.edit', 'command.document.object.layer.create']);
    assert.equal(seen[0], edit);
  } finally { delete global.SMFeedback; }
});

test('a throwing or absent trail hook never fails the command', async () => {
  try {
    const { t } = await connected((command, args) => ok(JSON.parse(args.requestJson)));
    global.SMFeedback = { logNativeCommand() { throw new Error('boom'); } };
    const r = await t.dispatchObject('nemo_native_object_dispatch', req('command.document.object.layer.create', { payload: {} }));
    assert.equal(r.ok, true);
    delete global.SMFeedback;
    assert.equal((await t.dispatchObject('nemo_native_object_dispatch', req('command.document.object.layer.create', { payload: {} }))).ok, true);
  } finally { delete global.SMFeedback; }
});

test('the action trail is never read by any project serializer', () => {
  const root = path.join(__dirname, '../src/js');
  const hits = [];
  (function walk(dir) {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith('.js') && /\bactionLog\b/.test(fs.readFileSync(p, 'utf8'))) hits.push(path.relative(root, p));
    }
  }(root));
  // state initialiser, the ring buffer itself and a comment (a serializer would have to appear here).
  assert.deepEqual(hits.sort(), ['app.js', 'feedback-bridge.js', 'history-panel.js']);
  const app = fs.readFileSync(path.join(root, 'app.js'), 'utf8');
  assert.equal(app.split(/\bactionLog\b/).length - 1, 1);
});
