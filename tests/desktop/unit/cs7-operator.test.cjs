'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const { createOperator } = require('../cs7-scenarios/operator.cjs');
function fixture(t, change = x => x) {
  const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-operator-')));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const identity = { sourceSha: 'a'.repeat(40), pid: 77, sessionId: 'one' };
  const requests = [];
  const operator = createOperator({ directory, timeout: 100, interval: 1, announce: ref => {
    requests.push(ref); const request = JSON.parse(fs.readFileSync(ref.requestFile));
    const reply = change({ schema: 'nemo.cs7-operator-response/1', requestId: request.requestId,
      identity: request.identity, evidence: { nativeResponse: { actual: 'raw-data' } } }, ref);
    if (reply) fs.writeFileSync(ref.responseFile, JSON.stringify(reply), { flag: 'wx', mode: 0o600 });
  } });
  const phase = { row: 'text20', phase: 'open', instruction: 'Use the real fixture picker.', identity };
  return { operator, phase, requests, directory };
}
test('real operator handoff returns raw evidence once, with owner and nonce binding', async t => {
  const { operator, phase, requests } = fixture(t);
  assert.deepEqual(await operator.request(phase), { nativeResponse: { actual: 'raw-data' } });
  await assert.rejects(operator.request(phase), /replayed/);
  assert.equal(requests.length, 1); operator.close();
});
test('stale nonce, foreign owner, pass flags, missing evidence and timeout stop without replay', async t => {
  for (const change of [r => ({ ...r, requestId: 'old' }), r => ({ ...r, identity: { ...r.identity, pid: 88 } }),
    r => ({ ...r, pass: true }), r => ({ ...r, evidence: null }), () => null]) {
    const { operator, phase, requests } = fixture(t, change);
    await assert.rejects(operator.request(phase), /operator-evidence-refused/);
    await assert.rejects(operator.request({ ...phase, phase: 'next' }), /unavailable/);
    assert.equal(requests.length, 1); operator.close();
  }
});
test('a symlink reply is refused without reading its target', async t => {
  const { operator, phase } = fixture(t, (_r, ref) => {
    fs.symlinkSync(ref.requestFile, ref.responseFile); return null;
  });
  await assert.rejects(operator.request(phase), /operator-evidence-refused/); operator.close();
});
test('cleanup cancels a pending handoff and no second phase can run concurrently', async t => {
  const { operator, phase } = fixture(t, () => null);
  const request = operator.request(phase);
  await assert.rejects(operator.request({ ...phase, phase: 'next' }), /unavailable/);
  operator.close(); await assert.rejects(request, /operator-evidence-refused/);
});
