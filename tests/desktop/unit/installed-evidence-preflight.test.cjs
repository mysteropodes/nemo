'use strict';
// The pre-flight of the installed runners: a locked macOS login session is refused BEFORE the app starts (exit status refused, with the
// reason), and the first command after launch that times out says the window is probably not presenting instead of a bare transport
// timeout. Stubs only: ioreg is injected, nothing is launched.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { sessionState, requireUnlockedSession, LOCKED_REASON, FIRST_COMMAND_TIMEOUT } = require('../installed-evidence/session-preflight.cjs');
const { isCheckRefusal } = require('../native-harness.cjs');
const { openCandidate } = require('../installed-evidence/candidate.cjs');
const { main } = require('../installed-evidence/run.cjs');
const f03 = require('../installed-evidence/f03-clients.cjs');
const { createWire } = require('../installed-evidence/wire.cjs');
const { runRows } = require('../installed-evidence/runner.cjs');
const K = require('../installed-evidence/kit.cjs');
const { createStubApp, INSTANCE, BUILD } = require('./installed-evidence-stubs.cjs');

// Literal ioreg output of this Mac while the screen was locked (text form, the key sits in the IOConsoleUsers entry), the same read with -a, and an unlocked session (the key is absent).
const LOCKED_TEXT = '  | +-o Root  <class IORegistryEntry>\n  |   "IOConsoleUsers" = ({"kCGSSessionOnConsoleKey"=Yes,"kCGSSessionUserNameKey"="u","CGSSessionScreenLockedTime"=1794533062,"CGSSessionScreenIsLocked"=Yes,"kCGSSessionUserIDKey"=501})\n';
const LOCKED_XML = '\t\t\t<key>CGSSessionScreenIsLocked</key>\n\t\t\t<true/>\n';
const UNLOCKED_TEXT = '  | +-o Root  <class IORegistryEntry>\n  |   "IOConsoleUsers" = ({"kCGSSessionOnConsoleKey"=Yes,"kCGSSessionUserNameKey"="u","kCGSSessionUserIDKey"=501})\n';
const locked = { read: () => LOCKED_TEXT, platform: 'darwin' };

test('the session is read from ioreg: locked in both spellings, unlocked when the key is absent, unprobed when it cannot be read or is not macOS', () => {
  assert.deepEqual(sessionState({ platform: 'darwin', read: () => LOCKED_TEXT }), { locked: true, probed: true });
  assert.deepEqual(sessionState({ platform: 'darwin', read: () => LOCKED_XML }), { locked: true, probed: true });
  assert.deepEqual(sessionState({ platform: 'darwin', read: () => UNLOCKED_TEXT }), { locked: false, probed: true });
  assert.deepEqual(sessionState({ platform: 'darwin', read: () => '"CGSSessionScreenIsLocked"=No' }), { locked: false, probed: true });
  assert.deepEqual(sessionState({ platform: 'darwin', read: () => { throw new Error('ioreg missing'); } }), { locked: false, probed: false });
  assert.deepEqual(sessionState({ platform: 'linux', read: () => LOCKED_TEXT }), { locked: false, probed: false });
});

test('a locked session is a check refusal with a plain reason; an unlocked one passes', () => {
  assert.throws(() => requireUnlockedSession(locked), error => isCheckRefusal(error) && error.message === LOCKED_REASON);
  assert.match(LOCKED_REASON, /locked/);
  assert.doesNotThrow(() => requireUnlockedSession({ platform: 'darwin', read: () => UNLOCKED_TEXT }));
});

test('openCandidate refuses a locked session before it reads the checkout, the package or starts anything', async () => {
  let touched = 0;
  const trap = new Proxy({}, { get() { touched++; throw new Error('must not be reached'); } });
  await assert.rejects(openCandidate({}, { session: locked, identity: trap, runtime: trap, isolation: trap, createController: trap }), error => isCheckRefusal(error) && error.message === LOCKED_REASON);
  assert.equal(touched, 0);
});

function fixture(t) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'ie-preflight-')));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const reportDir = path.join(dir, 'report');
  fs.mkdirSync(reportDir);
  fs.writeFileSync(path.join(dir, 'proof.json'), '{}');
  return { dir, reportDir, config: { app: path.join(dir, 'Nemo.app'), buildProof: path.join(dir, 'proof.json'), reportDir } };
}

test('run.cjs on a locked session exits refused with the reason in the receipt and starts no row', async t => {
  const f = fixture(t), out = [];
  const { exit, refused, receipt } = await main(f.config, { session: locked, stdout: { write: l => out.push(l) } });
  assert.deepEqual([exit, refused, receipt.verdict], [2, LOCKED_REASON, 'refused']);
  assert.equal(receipt.refusal, LOCKED_REASON);
  assert.deepEqual(receipt.rows, []);
  assert.equal(JSON.parse(fs.readFileSync(path.join(f.reportDir, 'installed-client-evidence.json'), 'utf8')).refusal, LOCKED_REASON);
});

test('f03-clients.cjs on a locked session exits refused before a client is invoked, with the reason', async t => {
  const f = fixture(t), invoked = [];
  const deps = { session: locked, stdout: { write() {} }, probeClient: name => ({ name, available: true, version: '9.9.9', versionText: name, path: '/fake/' + name, realPath: '/fake/' + name, sha256: '0'.repeat(64) }),
    runClient: async spec => { invoked.push(spec); return { ms: 1, exitCode: 0, timedOut: false, stdout: '', stderr: '' }; } };
  const { exit, receipt } = await f03.main(f.config, deps);
  assert.deepEqual([exit, receipt.verdict, receipt.refusal], [2, 'refused', LOCKED_REASON]);
  assert.equal(invoked.length, 0, 'no client was invoked');
  assert.deepEqual(receipt.rows.map(r => [r.id, r.verdict]), [['f03-codex', 'unrun'], ['f03-codex-identify', 'unrun']], 'the first half (its workflow and identification rows) is recorded as not run, the second is not started');
  assert.match(receipt.rows[0].reason, /session is locked/);
});

// A client whose commands never answer, as a window that is not presenting leaves them. Discovery is answered by nemo-mcp itself.
function silentCommands(app, { answerFirst = 0 } = {}) {
  let commands = 0;
  return { async callTool(name, args) {
    if (name === 'nemo_command' && ++commands > answerFirst) throw new Error('timeout waiting for tools/call');
    return app.callTool(name, args);
  }, async close() {} };
}
const wireOf = client => createWire({ client, expect: { instanceId: INSTANCE, buildId: BUILD } });
const run = async (t, client, run) => {
  const work = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'ie-timeout-')));
  t.after(() => fs.rmSync(work, { recursive: true, force: true }));
  const [result] = await runRows({ rows: [{ id: '9001-x', lane: '9001', issue: 1, title: 'x title', tasks: [], run }], session: { wire: wireOf(client), work, recheck: async () => {} } });
  return result;
};

test('the first command after launch timing out says the window is probably not presenting, not a bare transport timeout', async t => {
  const result = await run(t, silentCommands(createStubApp()), async ctx => { await K.fresh(ctx); });
  assert.equal(result.verdict, 'fail');
  assert.match(result.reason, /^transport: /);
  assert.ok(result.reason.includes(FIRST_COMMAND_TIMEOUT), result.reason);
  assert.match(result.reason, /not presenting/);
  assert.match(result.reason, /locked or covered|Mac locked or window covered/);
  assert.match(result.reason, /timeout waiting for tools\/call/, 'the underlying timeout is kept next to the explanation');
});

test('a later command timing out stays an ordinary transport failure: the window had answered before', async t => {
  // project.new is answered; the next command (the transparent-canvas edit) never is.
  const result = await run(t, silentCommands(createStubApp(), { answerFirst: 1 }), async ctx => { await K.fresh(ctx, { transparent: true }); });
  assert.equal(result.verdict, 'fail');
  assert.match(result.reason, /^transport: MCP client failed: timeout waiting for tools\/call/);
  assert.doesNotMatch(result.reason, /not presenting/);
});

test('a first command that fails for another reason keeps its own message', async t => {
  const client = { async callTool(name, args) { if (name === 'nemo_command') throw new Error('stdin closed'); return createStubApp().callTool(name, args); }, async close() {} };
  const result = await run(t, client, async ctx => { await K.fresh(ctx); });
  assert.match(result.reason, /^transport: MCP client failed: stdin closed/);
});
