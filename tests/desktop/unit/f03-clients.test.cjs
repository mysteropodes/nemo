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
const GOOD_ANSWER = ['build id: ' + BUILD, 'source head: ' + HEAD, 'executable sha256: ' + 'e'.repeat(64), 'Read from the discovery answer and the executable.'].join('\n');
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
    modelEnv: { env: {}, home: '/home/x', readFile: () => 'model = "gpt-test-9"\nmodel_reasoning_effort = "high"\n[agents]\nmodel = "not-this"\n' },
    openCandidate: async () => {
      if (over.refuse) throw Object.assign(new Error('clean committed source required'), { name: 'CheckRefusal' });
      const app = createApp(over.flags || {}); apps.push(app); opened.push(app);
      return { client: app, work, bound: { instanceId: INSTANCE, buildId: BUILD }, recheck: async () => {}, cleanup: async () => { app.state.closed = true; },
        mcpCommand: '/pkg/Nemo.app/Contents/MacOS/nemo-mcp', mcpRegistry: '/iso/mcp', identity: { ...IDENTITY, instanceId: INSTANCE, pid: 4242 } };
    },
    // Which invocation this is comes from the prompt (a session that failed early must not shift the next one): `over.client[name].session1|session2|identify`.
    runClient: async spec => {
      invoked.push(spec);
      const name = spec.args[0] === 'exec' ? 'codex' : 'claude', format = name === 'codex' ? 'codex' : 'claude';
      const kind = spec.input.includes('fresh session') ? 'identify' : spec.input.includes('Another session has already worked') ? 'session2' : 'session1';
      const script = ((over.client || {})[name] || {})[kind] || {};
      if (script.result) return { ms: 5, timedOut: false, stdout: '', stderr: '', ...script.result };
      const app = apps[apps.length - 1];
      if (kind === 'session2' && over.beforeSession2) over.beforeSession2(app);
      const out = await scriptedClient(app, files, { ...script.script, session: kind === 'session1' ? 1 : kind === 'session2' ? 2 : 'identify', format, writeInIdentify: script.write });
      const final = kind === 'identify' ? JSON.stringify({ type: 'result', result: script.answer ?? GOOD_ANSWER }) + '\n' : '';
      return { ms: 5, exitCode: 0, signal: null, timedOut: false, stdout: script.stdout !== undefined ? script.stdout : out.stdout + final, stderr: '' };
    },
  };
  return { config, deps, apps, opened, invoked, reportDir };
}
const reasonOf = (receipt, id) => receipt.rows.find(r => r.id === id).reason;

test('pass: both clients complete the workflow and the harness confirms it from its own reads and the saved files', async () => {
  const h = harness();
  const { exit, receipt } = await main(h.config, h.deps);
  assert.equal(exit, 0, JSON.stringify(receipt.rows.map(r => r.reason)));
  assert.equal(receipt.verdict, 'pass');
  assert.deepEqual(receipt.rows.map(r => [r.id, r.verdict]), [['f03-codex', 'pass'], ['f03-codex-identify', 'pass'], ['f03-claude', 'pass'], ['f03-claude-identify', 'pass']]);
  assert.equal(h.opened.length, 2, 'one fresh isolated session per client');
  assert.ok(h.apps.every(app => app.state.closed), 'every session is cleaned up');
  const kinds = new Set(receipt.rows.flatMap(r => r.assertions.map(a => a.kind)));
  assert.ok(kinds.has('file') && kinds.has('read'), 'verdicts rest on files and independent reads');
  assert.ok(receipt.rows[0].assertions.length >= 20);
  assert.deepEqual(receipt.candidate, IDENTITY);
  assert.equal(receipt.clients.length, 4);
  for (const c of receipt.clients) {
    assert.equal(c.version, '9.9.9'); assert.equal(c.sha256, '0'.repeat(64));
    assert.ok(c.sessions.length === (c.row.endsWith('-identify') ? 1 : 2) && c.sessions.every(x => x.outcome === 'ok' && x.argv.length > 3 && x.prompt.length > 100));
  }
  const work = receipt.clients.filter(c => !c.row.endsWith('-identify'));
  assert.match(work[0].sessions[0].prompt, /Redo once/); assert.match(work[0].sessions[1].prompt, /Open .*f03-keyed\.json/);
  assert.equal(JSON.parse(fs.readFileSync(path.join(h.reportDir, 'f03-clients-receipt.json'), 'utf8')).schema, 'nemo.installed-client-f03/1');
  for (const name of ['codex', 'claude']) {
    assert.ok(fs.existsSync(path.join(h.reportDir, name + '.wire.jsonl')));
    for (const tag of ['session1', 'session2', 'identify']) for (const ext of ['stream.jsonl', 'stderr.txt']) assert.ok(fs.existsSync(path.join(h.reportDir, name + '.' + tag + '.' + ext)), name + tag + ext);
  }
  assert.match(receipt.statement, /wire\.jsonl files are the harness's own reads and probes, not the client's calls/);
});

test('one client selected and passing is a subset pass, never the full result', async () => {
  const h = harness();
  const { exit, receipt } = await main({ ...h.config, clients: ['claude'] }, h.deps);
  assert.equal(exit, 5); assert.equal(receipt.verdict, 'subset-pass'); assert.deepEqual(receipt.selection.omitted, ['f03-codex', 'f03-codex-identify']);
});

test('product failure: an application that accepts an out-of-date write, saves the wrong value or accepts a replaced project fails the half', async () => {
  for (const [flags, pattern] of [[{ acceptStale: true }, /the opacity is 60 again/], [{ saveWrong: true }, /saved-60\.json holds opacity 60/], [{ acceptWrongDocument: true }, /refused as wrong_document/]]) {
    const h = harness({ flags });
    const { exit, receipt } = await main(h.config, h.deps);
    assert.equal(exit, 1, JSON.stringify(flags));
    assert.equal(receipt.verdict, 'fail');
    assert.match(reasonOf(receipt, 'f03-codex'), /^assertion failed/);
    assert.match(receipt.rows[0].assertions.find(a => a.verdict === 'fail').id, pattern);
  }
});

test('the harness probes judge the application itself: a stub that never refuses a stale write fails even when the client did nothing wrong', async () => {
  const h = harness({ flags: { acceptStale: true }, client: { codex: { session1: { script: { stop: 0 } } } } });
  const { receipt } = await main({ ...h.config, clients: ['codex'] }, h.deps);
  assert.equal(receipt.verdict, 'fail');
});

test('a client that claims success while the application state is wrong never passes', async () => {
  const claim = text => ({ script: { skip: true }, stdout: JSON.stringify({ type: 'result', result: text }) + '\n' });
  const h = harness({ client: { codex: { session1: claim('All steps completed; every check passed.'), identify: claim('build id: ' + BUILD) }, claude: { session1: claim('Done. Opacity is 40 and the keyed project is open.') } } });
  const { exit, receipt } = await main(h.config, h.deps);
  assert.equal(exit, 1);
  assert.deepEqual(receipt.rows.map(r => r.verdict), ['fail', 'fail', 'fail', 'pass'], 'a client that only claims it fails; the identification row is judged on its own answer');
  assert.match(receipt.rows[0].assertions.slice(-1)[0].id, /saved-60\.json: written by the application/);
  assert.match(receipt.clients[0].sessions[0].trace.lastMessage, /All steps completed/, 'what it said is recorded');
  assert.equal(receipt.clients[0].sessions[0].trace.informational, true);
  // Half way through: saved-60 exists but the client stopped before the undo.
  const half = harness({ client: { codex: { session1: { script: { stop: 3 } } } } });
  assert.equal((await main({ ...half.config, clients: ['codex'] }, half.deps)).receipt.rows[0].verdict, 'fail');
});

test('client unavailable: missing, unauthenticated or over quota is unrun with the reason, never a pass', async () => {
  const missing = harness({ probe: { codex: { name: 'codex', available: false, reason: 'codex is not installed (not on PATH)' } } });
  let out = await main(missing.config, missing.deps);
  assert.equal(out.exit, 4);
  assert.equal(out.receipt.verdict, 'unrun');
  assert.deepEqual(out.receipt.rows.map(r => r.verdict), ['unrun', 'unrun', 'pass', 'pass']);
  assert.match(reasonOf(out.receipt, 'f03-codex'), /not installed/);
  assert.equal(missing.opened.length, 1, 'no application is started for a client that is not there');
  for (const [stderr, reason] of [['Error: Not logged in. Please run /login', /not authenticated/], ['429 Too Many Requests: usage limit reached', /quota/], ['error: unknown option --tools', /rejected the harness command line/]]) {
    const h = harness({ client: { claude: { session1: { result: { exitCode: 1, stderr } } } } });
    out = await main({ ...h.config, clients: ['claude'] }, h.deps);
    assert.equal(out.exit, 4, stderr);
    assert.deepEqual(out.receipt.rows.map(r => r.verdict), ['unrun', 'unrun']);
    assert.match(out.receipt.rows[0].reason, reason);
    assert.equal(h.invoked.length, 1, 'the identification session is not started for a client that is not usable');
  }
  const ghost = harness({ client: { claude: { session1: { result: { spawnError: 'ENOENT' } } } } });
  assert.equal((await main({ ...ghost.config, clients: ['claude'] }, ghost.deps)).receipt.rows[0].verdict, 'unrun');
  const crash = harness({ client: { claude: { session1: { result: { exitCode: 2, stderr: 'segfault' } } } } });
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
  const prompts = [W.promptOne(paths), W.promptTwo(paths), W.promptIdentify('/r/docs.md')], prompt = prompts.join('\n');
  for (const word of ['nemo_command', 'nemo_discover', 'nemo_query', 'command.document', 'query.document', 'property.edit', 'property.read', 'rawSample', 'presentation', 'documentSnapshotId', 'layer.opacity', 'stableTarget', 'expectedRevision', 'history.undo', 'object.undo', 'object.redo', 'redo(', 'saveAs', 'project.open']) assert.ok(!prompt.includes(word), word);
  // The prompt no longer says where anything is hidden: no project kind, no snapshot or view talk, no route advice.
  for (const hint of ['object project', 'native', 'apiVersion', 'snapshot', 'views', 'declares', 'route', 'schema', 'offers on it']) assert.ok(!prompt.includes(hint), hint);
  for (const p of Object.values(paths).filter(p => p !== paths.static)) assert.ok(prompt.includes(p));
  assert.ok(prompts[2].includes('/r/docs.md') && !Object.values(paths).some(p => prompts[2].includes(p)), 'the identification session gets the documentation path and nothing of the workflow');
  assert.match(prompts[0], /Redo once/); assert.match(prompts[1], /Another session has already worked/);
  const base = { command: '/bin/x', mcpCommand: '/pkg/MacOS/nemo-mcp', mcpRegistry: '/iso/mcp', prompt: prompts[0], cwd: '/w/cwd', reportDir: '/r' };
  const codex = C.invocation('codex', base);
  assert.equal(codex.input, prompts[0]); assert.equal(codex.args.at(-1), '-');
  assert.ok(codex.args.includes('mcp_servers.nemo.command="/pkg/MacOS/nemo-mcp"'));
  assert.ok(codex.args.includes('mcp_servers.nemo.env.NEMO_MCP_REGISTRY="/iso/mcp"'));
  assert.ok(codex.args.includes('mcp_servers.nemo.default_tools_approval_mode="approve"'), 'approval policy never refuses every MCP tool call otherwise');
  assert.deepEqual(codex.files, {});
  const claude = C.invocation('claude', base), [file, text] = Object.entries(claude.files)[0];
  assert.deepEqual(JSON.parse(text), { mcpServers: { nemo: { command: '/pkg/MacOS/nemo-mcp', args: [], env: { NEMO_MCP_REGISTRY: '/iso/mcp' } } } });
  assert.ok(claude.args.includes('--strict-mcp-config') && claude.args[claude.args.indexOf('--mcp-config') + 1] === file);
  assert.equal(claude.args.indexOf('-p'), 0); assert.equal(claude.input, prompts[0]);
  assert.ok(C.invocation('claude', { ...base, tag: 'session2' }).args.includes(path.join('/r', 'claude.session2.mcp-config.json')) && C.invocation('codex', { ...base, tag: 'identify' }).lastMessage === path.join('/r', 'codex.identify.last-message.txt'), 'one private file per invocation');
  const tools = claude.args[claude.args.indexOf('--tools') + 1], allowed = claude.args.slice(claude.args.indexOf('--allowedTools') + 1, claude.args.indexOf('--output-format'));
  assert.equal(tools, '', 'no built-in tools: the compact discovery answer needs no file reader');
  assert.deepEqual(allowed, ['mcp__nemo__nemo_discover', 'mcp__nemo__nemo_query', 'mcp__nemo__nemo_command']);
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

// ---- the model each client is asked to use ------------------------------------------------------------------------------------------
test('Claude Code is always given an explicit --model (default sonnet, overridable); Codex only when the runner sets one', () => {
  const base = { command: '/bin/x', mcpCommand: '/pkg/MacOS/nemo-mcp', mcpRegistry: '/iso/mcp', prompt: 'p', cwd: '/w/cwd', reportDir: '/r' };
  const after = (args, flag) => args[args.indexOf(flag) + 1];
  const claude = C.invocation('claude', base);
  assert.equal(claude.args.filter(a => a === '--model').length, 1);
  assert.equal(after(claude.args, '--model'), 'sonnet'); assert.equal(claude.model, 'sonnet');
  assert.equal(claude.args.indexOf('-p'), 0, '-p stays first');
  assert.equal(after(C.invocation('claude', { ...base, model: 'claude-sonnet-5-5' }).args, '--model'), 'claude-sonnet-5-5');
  const codex = C.invocation('codex', base);
  assert.ok(!codex.args.includes('--model') && !codex.args.includes('-m'), 'no model flag unless the runner sets one'); assert.equal(codex.model, 'client default');
  const set = C.invocation('codex', { ...base, model: 'gpt-5-codex' });
  assert.equal(after(set.args, '--model'), 'gpt-5-codex'); assert.equal(set.model, 'gpt-5-codex'); assert.equal(set.args.at(-1), '-');
  assert.deepEqual([C.requestedModel('claude', {}), C.requestedModel('claude', { claudeModel: 'opus' }), C.requestedModel('codex', {}), C.requestedModel('codex', { codexModel: 'm1' })], ['sonnet', 'opus', 'client default', 'm1']);
  for (const bad of ['', 'two words', '--flag', 'a;b', 7]) assert.equal(C.validModel(bad), false, String(bad));
  for (const good of ['sonnet', 'claude-sonnet-5-5', 'opus[1m]', 'gpt-5.1-codex']) assert.equal(C.validModel(good), true, good);
});

test('the receipt records the requested model next to the client version, and the command line the clients were run with carries it', async () => {
  const h = harness();
  let out = await main(h.config, h.deps);
  assert.equal(out.exit, 0);
  const workOf = receipt => Object.fromEntries(receipt.clients.filter(c => !c.row.endsWith('-identify')).map(c => [c.name, { ...c, argv: c.sessions[0].argv }]));
  const byName = workOf(out.receipt);
  assert.deepEqual([byName.claude.version, byName.claude.requestedModel, byName.codex.version, byName.codex.requestedModel], ['9.9.9', 'sonnet', '9.9.9', 'client default']);
  assert.deepEqual(byName.claude.argv.slice(0, 3), ['claude', '-p', '--model']); assert.equal(byName.claude.argv[3], 'sonnet');
  assert.ok(!byName.codex.argv.includes('--model'));
  const set = harness();
  out = await main({ ...set.config, claudeModel: 'claude-sonnet-5-5', codexModel: 'gpt-5-codex' }, set.deps);
  const named = workOf(out.receipt);
  assert.deepEqual([named.claude.requestedModel, named.codex.requestedModel], ['claude-sonnet-5-5', 'gpt-5-codex']);
  assert.equal(named.claude.argv[3], 'claude-sonnet-5-5'); assert.equal(named.codex.argv[named.codex.argv.indexOf('--model') + 1], 'gpt-5-codex');
  assert.deepEqual(set.invoked.map(s => s.model), [...Array(3).fill('gpt-5-codex'), ...Array(3).fill('claude-sonnet-5-5')], 'every session of a client carries the model');
  // a client that is not there still records what it would have been asked to use
  const missing = harness({ probe: { claude: { name: 'claude', available: false, reason: 'claude is not installed (not on PATH)' } } });
  out = await main({ ...missing.config, claudeModel: 'opus' }, missing.deps);
  assert.equal(out.receipt.clients.find(c => c.name === 'claude').requestedModel, 'opus');
  // an invalid model refuses before anything starts
  const fresh = harness();
  for (const over of [{ claudeModel: 'not a model' }, { codexModel: '--oops' }]) {
    const refused = await main({ ...fresh.config, ...over }, fresh.deps);
    assert.deepEqual([refused.exit, refused.refused], [2, Object.keys(over)[0] + ' must be a model name']);
  }
  assert.equal(fresh.opened.length, 0, 'nothing was started');
});

test('the command line accepts --claude-model and --codex-model', () => {
  assert.deepEqual(parse(['--app', '/a.app', '--build-proof', '/p', '--report-dir', '/r', '--claude-model', 'opus', '--codex-model', 'm1']), { app: '/a.app', buildProof: '/p', reportDir: '/r', claudeModel: 'opus', codexModel: 'm1' });
  assert.throws(() => parse(['--claude-model']), /needs a value/);
  assert.throws(() => parse(['--codex-model', '--client']), /needs a value/);
});

// ---- redo, reconnect, fresh-agent identification, the model used and the per-step call counts (#1049 round 2) ------------------------
const failedAssertion = (receipt, id) => receipt.rows.find(r => r.id === id).assertions.find(a => a.verdict === 'fail');

test('redo: the harness judges it from its own reads between the sessions (value, history, revisions), in both halves', async () => {
  const h = harness();
  const { receipt } = await main(h.config, h.deps);
  for (const id of ['f03-codex', 'f03-claude']) {
    const row = receipt.rows.find(r => r.id === id), by = label => row.assertions.find(a => a.id.includes(label));
    assert.equal(by('after undo and redo the opacity is 60 again').kind, 'read'); assert.deepEqual(by('after undo and redo the opacity is 60 again').observed, 60);
    assert.deepEqual(by('history holds both edits and nothing to redo').observed, [2, 0]);
    assert.equal(by('four revisions were made').observed, 4);
  }
  // Faults: a client that never redoes leaves 40 with one undo and one redo; a product that answers redo ok and does nothing leaves the same; neither passes.
  for (const [over, why] of [[{ client: { codex: { session1: { script: { skipRedo: true } } } } }, 'client left out the redo'], [{ flags: { redoIgnored: true } }, 'redo reported applied but not applied']]) {
    const f = harness(over), out = await main({ ...f.config, clients: ['codex'] }, f.deps);
    assert.equal(out.exit, 1, why);
    assert.equal(out.receipt.rows[0].verdict, 'fail');
    const bad = failedAssertion(out.receipt, 'f03-codex');
    assert.match(bad.id, /the opacity is 60 again/, why); assert.equal(bad.observed, 40);
    assert.equal(f.invoked.filter(s => s.input.includes('Another session has already worked')).length, 0, 'session 2 never starts from a session 1 that failed');
  }
});

test('reconnect: two separate client processes on the same running application, the second starting from the first\'s revision, recorded in the receipt', async () => {
  const h = harness();
  const { exit, receipt } = await main({ ...h.config, clients: ['claude'] }, h.deps);
  assert.equal(exit, 5);
  const sess = h.invoked.map(s => [s.cwd.split('-').pop(), s.input.includes('Another session has already worked')]);
  assert.deepEqual(sess, [['session1', false], ['session2', true], ['identify', false]], 'three separate processes, each with its own working directory');
  assert.equal(h.opened.length, 1, 'one application session for all of them');
  const [work] = receipt.clients, r = work.reconnect;
  assert.equal(r.separateProcesses, true); assert.equal(r.startedFromTheRevisionSession1Left, true);
  assert.deepEqual(r.left, { documentId: r.session2StartedFrom.documentId, revision: 4, opacity: 60, undo: 2, redo: 0 });
  assert.deepEqual(r.session2StartedFrom.revision, 4);
  assert.deepEqual(work.sessions.map(x => x.session), ['session1', 'session2']);
  const row = receipt.rows[0];
  assert.ok(row.assertions.some(a => a.kind === 'stream' && /first discovery answer/.test(a.id) && a.verdict === 'pass'));
  assert.ok(row.assertions.some(a => /nothing moved between the two sessions/.test(a.id) && a.verdict === 'pass'));
  assert.deepEqual(h.apps[0].state.calls.filter(c => c === 'project.saveAs').length, 2);
});

test('reconnect fault: a second session that starts from another revision than the first left fails the row', async () => {
  // Something wrote to the document after the harness's own read and before the second client looked: the client is shown revision 5, not 4.
  const h = harness({ beforeSession2: app => { app.state.doc.rev += 1; } });
  const out = await main({ ...h.config, clients: ['claude'] }, h.deps);
  assert.equal(out.exit, 1);
  const bad = failedAssertion(out.receipt, 'f03-claude');
  assert.match(bad.id, /first discovery answer the application gave the client names the document and revision session 1 left/);
  assert.equal(bad.kind, 'stream'); assert.equal(bad.observed.revision, 5); assert.equal(bad.expected.revision, 4);
  assert.equal(out.receipt.clients[0].reconnect.startedFromTheRevisionSession1Left, false);
  // A second session that never asked what is running shows nothing at all: also a failure, never a pass on what it says.
  const silent = harness({ client: { claude: { session2: { stdout: '{"type":"result","result":"all done, continued from revision 4"}\n' } } } });
  const quiet = await main({ ...silent.config, clients: ['claude'] }, silent.deps);
  assert.equal(quiet.exit, 1);
  assert.equal(failedAssertion(quiet.receipt, 'f03-claude').observed, null);
});

test('fresh-agent identification: the answer is compared with the package proof and the registered build; a wrong or unknown item, or a change to the application, fails', async () => {
  const good = harness();
  const out = await main({ ...good.config, clients: ['codex'] }, good.deps);
  const id = out.receipt.rows.find(r => r.id === 'f03-codex-identify'), meta = out.receipt.clients.find(c => c.row === 'f03-codex-identify');
  assert.equal(id.verdict, 'pass');
  assert.deepEqual(meta.identification.match, { buildId: true, sourceHead: true, executableSha256: true });
  assert.deepEqual(meta.identification.expected, { buildId: BUILD, sourceHead: HEAD, executableSha256: 'e'.repeat(64) });
  assert.match(meta.identification.prompt, /fresh session/); assert.match(meta.identification.answer, /^build id: /);
  assert.ok(meta.identification.prompt.includes(W.DOCS) && !/f03-saved|f03-keyed/.test(meta.identification.prompt));
  assert.equal(meta.sessions[0].session, 'identify');
  const faults = {
    'a wrong build id': GOOD_ANSWER.replace(BUILD, '1.2.3:' + 'b'.repeat(40)),
    'the build id with -dirty': GOOD_ANSWER.replace(BUILD, BUILD + '-dirty'),
    'a wrong executable sha256': GOOD_ANSWER.replace('e'.repeat(64), 'f'.repeat(64)),
    'unknown for the executable sha256': GOOD_ANSWER.replace('e'.repeat(64), 'unknown (no shell to hash the file)'),
    'no answer': 'I could not do this.',
  };
  for (const [why, answer] of Object.entries(faults)) {
    const f = harness({ client: { claude: { identify: { answer } } } });
    const o = await main({ ...f.config, clients: ['claude'] }, f.deps);
    assert.equal(o.exit, 1, why);
    const row = o.receipt.rows.find(r => r.id === 'f03-claude-identify');
    assert.equal(row.verdict, 'fail', why);
    assert.equal(o.receipt.rows.find(r => r.id === 'f03-claude').verdict, 'pass', 'the workflow row is judged on its own: ' + why);
    const bad = row.assertions.find(a => a.verdict === 'fail');
    assert.match(bad.id, /reported the installed candidate's build id, source head and executable sha256/);
    assert.ok(Object.values(bad.observed).some(d => d.match === false), why);
    assert.equal(o.receipt.clients.find(c => c.row === 'f03-claude-identify').identification.answer, answer);
  }
  // A read-only task that writes to the application fails even with the right answer.
  const writes = harness({ client: { claude: { identify: { write: true } } } });
  const w = await main({ ...writes.config, clients: ['claude'] }, writes.deps);
  assert.equal(w.exit, 1);
  assert.match(failedAssertion(w.receipt, 'f03-claude-identify').id, /changed nothing in the application/);
  assert.deepEqual(W.parseIdentity('**Build id:** `' + BUILD + '`\n- source head = ' + HEAD + '\nExecutable SHA-256: ' + 'e'.repeat(64)), { buildId: BUILD, sourceHead: HEAD, executableSha256: 'e'.repeat(64) });
});

test('the model each client used is read from its own stream or configuration and recorded next to the requested model', async () => {
  const h = harness();
  const { receipt } = await main(h.config, h.deps);
  for (const c of receipt.clients) {
    const used = c.name === 'claude' ? { value: 'claude-test-1' } : { value: 'gpt-test-9', reasoningEffort: 'high' };
    for (const s of c.sessions) assert.deepEqual({ value: s.modelUsed.value, ...(s.modelUsed.reasoningEffort ? { reasoningEffort: s.modelUsed.reasoningEffort } : {}) }, used);
    assert.match(c.sessions[0].modelUsed.source, c.name === 'claude' ? /own stream \(init event\)/ : /Codex configuration file .*top-level model key/);
  }
  assert.ok(!JSON.stringify(receipt).includes('not-this'), 'only the top-level keys of the configuration are read');
  assert.deepEqual(C.modelUsed('codex', '{"type":"turn.started","turn":{"model":"gpt-stream"}}\n', { requested: 'client default' }), { value: 'gpt-stream', source: 'the client\'s own stream' });
  assert.deepEqual(C.modelUsed('codex', '', { requested: 'gpt-5-codex' }).value, 'gpt-5-codex');
  assert.deepEqual(C.modelUsed('codex', '', { requested: 'client default', env: {}, home: '/h', readFile: () => { throw new Error('none'); } }), { value: null, source: null });
  assert.deepEqual(C.modelUsed('claude', '{"type":"system","subtype":"init","model":"claude-sonnet-5-5"}\n', { requested: 'sonnet' }).value, 'claude-sonnet-5-5');
});

test('the client\'s own MCP calls are counted per step from its stream, in both stream formats, and are not the wire log', async () => {
  const h = harness();
  const { receipt } = await main(h.config, h.deps);
  const sessionsOf = name => receipt.clients.find(c => c.name === name && !c.row.endsWith('-identify')).sessions;
  for (const name of ['claude', 'codex']) {
    const [one, two] = sessionsOf(name).map(s => s.trace);
    assert.deepEqual(one.stepCalls.map(x => [x.step, x.calls]), [[1, 1], [2, 2], [3, 1], [4, 1], [5, 1], [6, 1]], name);
    assert.deepEqual(two.stepCalls.map(x => [x.step, x.calls]), [[1, 1], [2, 1], [3, 1], [4, 1], [5, 1], [6, 1]], name);
    assert.equal(one.mcpCallCount, 7); assert.equal(two.mcpCallCount, 6);
    assert.deepEqual(one.toolCalls, { nemo_discover: 1, nemo_command: 6 });
    assert.match(one.stepCalls[4].label, /redo/);
    assert.equal(one.stepCalls.reduce((n, x) => n + x.calls, 0), one.mcpCallCount, 'every call lands on a step');
  }
  const wire = fs.readFileSync(path.join(h.reportDir, 'claude.wire.jsonl'), 'utf8').trim().split('\n').length;
  assert.ok(wire > 7 + 6, 'the wire log holds the harness\'s own reads and probes, far more than the client\'s calls');
  // A client that skips a step leaves its calls on the step before; the total stays right.
  const calls = [{ tool: 'nemo_discover', operation: null, path: null }, { tool: 'nemo_command', operation: 'command.document.object.undo', path: null }];
  assert.deepEqual(C.stepCounts(calls, W.STEPS.session1).map(x => x.calls), [2, 0, 0, 0, 0, 0], 'the undo is not the next step, so it stays on step 1');
  assert.deepEqual(C.mcpCalls('{"type":"item.started","item":{"type":"mcp_tool_call","server":"nemo","tool":"nemo_query","arguments":{}}}\n{"type":"item.completed","item":{"type":"mcp_tool_call","server":"other","tool":"nemo_query","arguments":{}}}\n{"type":"item.completed","item":{"type":"mcp_tool_call","server":"nemo","tool":"nemo_query","arguments":{"operation":"snapshot"}}}\n'),
    [{ tool: 'nemo_query', operation: 'snapshot', path: null }], 'only completed calls to the nemo server count');
});
