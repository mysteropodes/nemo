'use strict';
// #1049 harness against stubs only: a real pass, a product failure, a client that claims success while the state is wrong, a client
// that is unavailable. No client is invoked and no app is started.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { main, cli, parse } = require('../installed-evidence/f03-clients.cjs');
const C = require('../installed-evidence/f03-client-cli.cjs');
const W = require('../installed-evidence/f03-workflow.cjs');
const { createApp, scriptedClient, INSTANCE, BUILD } = require('./f03-clients-stub.cjs');

const HEAD = 'a'.repeat(40);
const IDENTITY = { sourceHead: HEAD, tree: 't'.repeat(40), appVersion: '1.2.3', buildId: BUILD, executableSha256: 'e'.repeat(64), mcpSha256: 'c'.repeat(64), proofSha256: 'p'.repeat(64), app: 'Nemo.app' };

function harness(over = {}) {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'f03-test-')));
  const reportDir = path.join(root, 'report'), work = path.join(root, 'work');
  fs.mkdirSync(reportDir); fs.mkdirSync(work);
  fs.writeFileSync(path.join(root, 'build-proof.json'), '{}');
  const config = { app: path.join(root, 'Nemo.app'), buildProof: path.join(root, 'build-proof.json'), reportDir };
  const apps = [], opened = [], invoked = [];
  const files = { saved60: path.join(work, 'f03-saved-60.json'), saved: path.join(work, 'f03-saved.json'), keyed: path.join(work, 'f03-keyed.json') };
  const deps = {
    stdout: { write() {} },
    uuid: (() => { let n = 0; return () => 'raw-' + (++n); })(),
    probeClient: name => (over.probe && over.probe[name]) || { name, available: true, version: '9.9.9', versionText: name + ' 9.9.9', path: '/fake/bin/' + name, realPath: '/fake/bin/' + name, sha256: '0'.repeat(64) },
    openCandidate: async () => {
      if (over.refuse) throw Object.assign(new Error('clean committed source required'), { name: 'CheckRefusal' });
      const app = createApp(over.flags || {}); apps.push(app); opened.push(app);
      return { client: app, work, bound: { instanceId: INSTANCE, buildId: BUILD }, recheck: async () => {}, cleanup: async () => { app.state.closed = true; },
        mcpCommand: '/pkg/Nemo.app/Contents/MacOS/nemo-mcp', mcpRegistry: '/iso/mcp', identity: { ...IDENTITY, instanceId: INSTANCE, pid: 4242 } };
    },
    runClient: async spec => {
      invoked.push(spec);
      const script = (over.client && over.client[spec.args[0] === 'exec' ? 'codex' : 'claude']) || {};
      if (script.result) return { ms: 5, timedOut: false, stdout: '', stderr: '', ...script.result };
      await scriptedClient(apps[apps.length - 1], files, script.script);
      return { ms: 5, exitCode: 0, signal: null, timedOut: false, stdout: script.stdout || '', stderr: '' };
    },
  };
  return { config, deps, apps, opened, invoked, reportDir };
}
const reasonOf = (receipt, id) => receipt.rows.find(r => r.id === id).reason;

test('pass: both clients complete the workflow and the harness confirms it from its own reads and the saved files', async () => {
  const h = harness();
  const { exit, receipt } = await main(h.config, h.deps);
  assert.equal(exit, 0);
  assert.equal(receipt.verdict, 'pass');
  assert.deepEqual(receipt.rows.map(r => [r.id, r.verdict]), [['f03-codex', 'pass'], ['f03-claude', 'pass']]);
  assert.equal(h.opened.length, 2, 'one fresh isolated session per client');
  assert.ok(h.apps.every(app => app.state.closed), 'every session is cleaned up');
  const kinds = new Set(receipt.rows.flatMap(r => r.assertions.map(a => a.kind)));
  assert.ok(kinds.has('file') && kinds.has('read'), 'verdicts rest on files and independent reads');
  assert.ok(receipt.rows[0].assertions.length >= 20);
  assert.deepEqual(receipt.candidate, IDENTITY);
  for (const c of receipt.clients) {
    assert.equal(c.version, '9.9.9'); assert.equal(c.sha256, '0'.repeat(64)); assert.equal(c.outcome, 'ok');
    assert.match(c.prompt, /Open .*f03-keyed\.json/); assert.ok(c.argv.length > 3);
  }
  assert.equal(JSON.parse(fs.readFileSync(path.join(h.reportDir, 'f03-clients-receipt.json'), 'utf8')).schema, 'nemo.installed-client-f03/1');
  for (const name of ['codex', 'claude']) for (const ext of ['stream.jsonl', 'stderr.txt', 'wire.jsonl']) assert.ok(fs.existsSync(path.join(h.reportDir, name + '.' + ext)));
});

test('product failure: an application that accepts an out-of-date write, saves the wrong value or accepts a replaced project fails the half', async () => {
  for (const [flags, pattern] of [[{ acceptStale: true }, /saved\.json holds opacity 40/], [{ saveWrong: true }, /saved-60\.json holds opacity 60/], [{ acceptWrongDocument: true }, /revision 0/]]) {
    const h = harness({ flags });
    const { exit, receipt } = await main(h.config, h.deps);
    assert.equal(exit, 1, JSON.stringify(flags));
    assert.equal(receipt.verdict, 'fail');
    assert.match(reasonOf(receipt, 'f03-codex'), /^assertion failed/);
    assert.match(receipt.rows[0].assertions.find(a => a.verdict === 'fail').id, pattern);
  }
});

test('the harness probes judge the application itself: a stub that never refuses a stale write fails even when the client did nothing wrong', async () => {
  const h = harness({ flags: { acceptStale: true }, client: { codex: { script: { stop: 0 } } } });
  const { receipt } = await main({ ...h.config, clients: ['codex'] }, h.deps);
  assert.equal(receipt.verdict, 'fail');
});

test('a client that claims success while the application state is wrong never passes', async () => {
  const h = harness({ client: { codex: { script: { skip: true }, stdout: '{"type":"result","result":"All nine steps completed; every check passed."}\n' },
    claude: { script: { skip: true }, stdout: '{"type":"result","result":"Done. Opacity is 40 and the keyed project is open."}\n' } } });
  const { exit, receipt } = await main(h.config, h.deps);
  assert.equal(exit, 1);
  assert.deepEqual(receipt.rows.map(r => r.verdict), ['fail', 'fail']);
  assert.match(receipt.rows[0].assertions.slice(-1)[0].id, /saved-60\.json: written by the application/);
  assert.match(receipt.clients[0].trace.lastMessage, /All nine steps completed/, 'what it said is recorded');
  assert.equal(receipt.clients[0].trace.informational, true);
  // Half way through: saved-60 exists but the client stopped before replacing the project.
  const half = harness({ client: { codex: { script: { stop: 4 } } } });
  assert.equal((await main({ ...half.config, clients: ['codex'] }, half.deps)).receipt.verdict, 'fail');
});

test('client unavailable: missing, unauthenticated or over quota is unrun with the reason, never a pass', async () => {
  const missing = harness({ probe: { codex: { name: 'codex', available: false, reason: 'codex is not installed (not on PATH)' } } });
  let out = await main(missing.config, missing.deps);
  assert.equal(out.exit, 4);
  assert.equal(out.receipt.verdict, 'unrun');
  assert.deepEqual(out.receipt.rows.map(r => r.verdict), ['unrun', 'pass']);
  assert.match(reasonOf(out.receipt, 'f03-codex'), /not installed/);
  assert.equal(missing.opened.length, 1, 'no application is started for a client that is not there');
  for (const [stderr, reason] of [['Error: Not logged in. Please run /login', /not authenticated/], ['429 Too Many Requests: usage limit reached', /quota/], ['error: unknown option --tools', /rejected the harness command line/]]) {
    const h = harness({ client: { claude: { result: { exitCode: 1, stderr } } } });
    out = await main({ ...h.config, clients: ['claude'] }, h.deps);
    assert.equal(out.exit, 4, stderr);
    assert.equal(out.receipt.rows[0].verdict, 'unrun');
    assert.match(out.receipt.rows[0].reason, reason);
  }
  const ghost = harness({ client: { claude: { result: { spawnError: 'ENOENT' } } } });
  assert.equal((await main({ ...ghost.config, clients: ['claude'] }, ghost.deps)).receipt.rows[0].verdict, 'unrun');
  const crash = harness({ client: { claude: { result: { exitCode: 2, stderr: 'segfault' } } } });
  assert.equal((await main({ ...crash.config, clients: ['claude'] }, crash.deps)).receipt.rows[0].verdict, 'fail', 'a client that ran and left the application untouched fails');
});

test('refusals: a candidate that does not bind refuses the whole run; usage errors refuse before anything starts', async () => {
  const h = harness({ refuse: true });
  const out = await main(h.config, h.deps);
  assert.equal(out.exit, 2); assert.equal(out.receipt.verdict, 'refused'); assert.equal(h.invoked.length, 0);
  assert.equal((await main({ ...h.config, reportDir: '/not/absolute/../x' }, h.deps)).exit, 2);
  assert.equal((await main({ ...h.config, clients: ['gemini'] }, h.deps)).exit, 2);
  assert.equal((await main({ ...h.config, timeoutMin: 0 }, h.deps)).exit, 2);
  const errors = []; assert.equal(await cli(['--bogus'], { stderr: { write: s => errors.push(s) } }), 2);
  assert.deepEqual(parse(['--app', '/a.app', '--build-proof', '/p', '--report-dir', '/r', '--client', 'codex', '--timeout-min', '5']), { app: '/a.app', buildProof: '/p', reportDir: '/r', clients: ['codex'], timeoutMin: 5 });
});

test('the prompt is goals and paths only, and the invocations carry a per-invocation MCP configuration and no user configuration', () => {
  const paths = { static: '/w/s.json', keyed: '/w/k.json', saved60: '/w/60.json', saved: '/w/40.json' };
  const prompt = W.promptFor(paths);
  for (const word of ['nemo_command', 'nemo_discover', 'command.document', 'layer.opacity', 'stableTarget', 'apiVersion', 'expectedRevision', 'history.undo']) assert.ok(!prompt.includes(word), word);
  for (const p of Object.values(paths).filter(p => p !== paths.static)) assert.ok(prompt.includes(p));
  const base = { command: '/bin/x', mcpCommand: '/pkg/MacOS/nemo-mcp', mcpRegistry: '/iso/mcp', prompt, cwd: '/w/cwd', reportDir: '/r' };
  const codex = C.invocation('codex', base);
  assert.equal(codex.input, prompt); assert.equal(codex.args.at(-1), '-');
  assert.ok(codex.args.includes('mcp_servers.nemo.command="/pkg/MacOS/nemo-mcp"'));
  assert.ok(codex.args.includes('mcp_servers.nemo.env.NEMO_MCP_REGISTRY="/iso/mcp"'));
  assert.deepEqual(codex.files, {});
  const claude = C.invocation('claude', base), [file, text] = Object.entries(claude.files)[0];
  assert.deepEqual(JSON.parse(text), { mcpServers: { nemo: { command: '/pkg/MacOS/nemo-mcp', args: [], env: { NEMO_MCP_REGISTRY: '/iso/mcp' } } } });
  assert.ok(claude.args.includes('--strict-mcp-config') && claude.args[claude.args.indexOf('--mcp-config') + 1] === file);
  assert.equal(claude.args.indexOf('-p'), 0); assert.equal(claude.input, prompt);
  assert.deepEqual(Object.keys(C.scrubbedEnv({ NEMO_MCP_REGISTRY: 'x', NEMO_TASK_ID: 'y', HOME: '/h' })), ['HOME']);
});

test('the fixtures are the pinned opacity fixture: static 25 without keys, keyed 20 at frame 0 and 80 at frame 20', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'f03-fixtures-'));
  const { paths, staged } = W.stageFixtures(dir);
  const stat = JSON.parse(fs.readFileSync(paths.static, 'utf8')).layers[0], keyed = JSON.parse(fs.readFileSync(paths.keyed, 'utf8')).layers[0];
  assert.equal(stat.layerUid, W.LAYER); assert.deepEqual(stat.motionStatic.opacity, [25]); assert.equal(stat.motion.opacity, undefined);
  assert.deepEqual(keyed.motion.opacity.keys.map(k => [k.frame, k.v[0]]), [[0, 20], [20, 80]]);
  assert.match(staged.static, /^[0-9a-f]{64}$/);
});

test('running a client: bounded output, a time limit, and ENOENT becomes unavailable', async () => {
  const run = (script, options) => C.runClient({ command: process.execPath, args: ['-e', script], cwd: os.tmpdir(), input: 'prompt text', files: {} }, options);
  const echoed = await run('process.stdin.on("data", d => process.stdout.write(d))');
  assert.equal(echoed.stdout, 'prompt text'); assert.equal(echoed.exitCode, 0); assert.equal(C.classify(echoed).kind, 'ok');
  const slow = await run('setTimeout(() => {}, 60000)', { timeoutMs: 200 });
  assert.equal(slow.timedOut, true); assert.equal(C.classify(slow).kind, 'timeout');
  const gone = await C.runClient({ command: '/no/such/client', args: [], cwd: os.tmpdir(), input: '', files: {} });
  assert.equal(C.classify(gone).kind, 'unavailable');
  assert.equal(C.probeClient('codex', { env: { PATH: '/no/such/dir' } }).available, false);
  const t = C.trace({ stdout: ['{"type":"item.completed","item":{"type":"mcp_tool_call","tool":"nemo_command","result":"{\\"error\\":{\\"code\\":\\"stale_revision\\"}}"}}',
    '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__nemo__nemo_discover"}]}}', 'not json'].join('\n') }, null);
  assert.deepEqual(t.toolCalls, { nemo_command: 1, nemo_discover: 1 }); assert.deepEqual(t.errorCodesSeen, { stale_revision: 1 });
});
