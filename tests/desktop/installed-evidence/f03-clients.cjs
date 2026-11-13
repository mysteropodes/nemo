#!/usr/bin/env node
'use strict';
// #1049 / F03. Explicit manual entry point; never an automatic test, builds nothing. Runs the issue's original opacity workflow against an
// already built package, twice: once through the real Codex CLI (`codex exec`), once through the real Claude Code CLI (`claude -p`). In each half the
// work is split over TWO separate client processes against the same running application and document (the second rediscovers the instance and
// continues from what the first left: a reconnect), it includes a redo, and a THIRD fresh process is asked to identify the installed candidate from the
// repository documentation (P32, #1034). Each half gets its own fresh isolated app session and a per-invocation MCP configuration that points at the
// package's own bundled nemo-mcp. No keyboard or mouse input is sent. The candidate is bound exactly as installed-evidence/run.cjs
// binds it (candidate.cjs), and the verdict of each row comes only from this harness's own reads of the application after the
// client has exited (f03-workflow.cjs), never from what the client printed.
//
//   lockf -k /tmp/nemo-heavy-job.lock node tests/desktop/installed-evidence/f03-clients.cjs \
//     --app /abs/Nemo.app --build-proof /abs/build-proof.json --report-dir /abs/empty-dir [--client codex,claude] [--timeout-min 20]
//     [--claude-model sonnet] [--codex-model NAME]   (Claude is always run with an explicit --model, default sonnet; Codex only when --codex-model is given)
//
// Exit status as run.cjs: 0 both halves passed (5 when only one client was selected and it passed); 1 a half failed or cleanup was incomplete; 2 refused before anything ran;
// 4 nothing failed but a half is unrun (client missing, unauthenticated or over quota, or an operation unavailable).
const fs = require('node:fs');
const path = require('node:path');
const { randomUUID } = require('node:crypto');
const { createWire, EvidenceError } = require('./wire.cjs');
const { runRows } = require('./runner.cjs');
const { buildReceipt, exitStatus, EXIT } = require('./receipt.cjs');
const { validateConfig, refusalReason, UsageError } = require('./run.cjs');
const W = require('./f03-workflow.cjs');
const C = require('./f03-client-cli.cjs');

const SCHEMA = 'nemo.installed-client-f03/1';
const STATEMENT = 'Agent evidence for #1049: the original opacity workflow (with redo) driven by the real Codex CLI and the real Claude Code CLI, each over two ' +
  'separate client processes against the same running application and document (a reconnect), then a fresh third session that identifies the installed candidate, ' +
  'each through a per-invocation MCP configuration for the candidate package\'s bundled nemo-mcp, with no keyboard or mouse input. The verdict of each row rests ' +
  'only on this harness\'s own reads and probes of the application and on files the application wrote, plus, for the reconnect, what the application showed the ' +
  'second client as recorded in its stream; what a client says it did and its tool trace are recorded as information and never decide a verdict. The *.wire.jsonl ' +
  'files are the harness\'s own reads and probes, not the client\'s calls: the client\'s calls are in the *.stream.jsonl files, counted per step under ' +
  'clients[].sessions[].trace.stepCalls. A client that is missing, unauthenticated or over quota leaves its rows unrun. It is not the CS7 batched installed ' +
  'session and it does not record the acceptance disposition of #1049, which belongs to the project owners.';
const LIMITATIONS = Object.freeze([
  'The fixture opens as an object project: the opacity-document reads (query.document.opacity, query.document.evaluate) and the apiVersion 1 property operations are not offered on it, so the workflow runs on the native object property read (rawSample, which ignores expressions) and edit, undo and project.open/saveAs; the evaluated read of the issue is the rawSample read.',
  'The application refuses to open the project file that is already the open project (it says so and how to continue), so the saved file is reopened after the keyed project replaced it, not before.',
  'Whether the client itself attempted the stale-revision and wrong-document writes is visible only in its event stream (informational); the harness proves the application refuses them, not that the client sent them.',
  'Redo is evidenced by the harness\'s own reads between the sessions (opacity 60, history undo 2 and redo 0, four revisions after set, set, undo, redo); the undo of session 2 by saved.json holding 40 and the refused out-of-date write; neither by the history depth at the moment the client acted.',
  'The reconnect is two separate client processes against one running application and document: the harness reads the state session 1 left, reads it again before session 2 starts, and requires the application\'s first discovery answer in the session 2 stream (the application\'s words, not the client\'s) to name that document and revision. That session 2 reached 40 by undoing, and not by writing 40, is visible only in its stream (informational), because the project is replaced afterwards.',
  'The fresh-agent identification session is given the repository documentation path and the MCP tools, and to read the documentation and hash a file the tools its client needs for that (Codex its default read-only shell; Claude Code Read of the repository folder and a shell limited to shasum and ps; the workflow sessions of Claude Code keep MCP tools only): an item it cannot establish with them is reported as unknown and the row fails, it is not inferred. The harness compares the answer with the package proof (source head, app executable sha256) and the registered build id.',
  'The model each client used is read from the client\'s own stream (Claude Code reports it in its init event) or, for Codex whose stream does not name it, from the top-level model keys of its configuration file; it sits next to the requested model.',
  'The keyed frame 0 and frame 20 reads by the client are evidenced by the harness reading the keyed fixture itself (20, 50, 80 at frames 0, 10, 20) and by the project the client left (saved.json open, untouched, reading 40); the values the client read are in its stream, informational.',
  'Only the Nemo MCP route is exercised: no window, visual or keyboard behaviour of the installed UI is observed.',
]);
const TITLE = { codex: 'original opacity workflow through the real Codex CLI', claude: 'original opacity workflow through the real Claude Code CLI' };

// One half: two rows on one fresh isolated session. `deps` makes every external effect replaceable: probeClient, runClient, openCandidate, uuid.
//   f03-<name>           the workflow in two separate client processes against the same running application and document (reconnect)
//   f03-<name>-identify  a third, fresh process asked to identify the installed candidate from the repository documentation (P32, #1034)
async function runHalf(name, config, deps, sessions) {
  const base = { id: 'f03-' + name, lane: 'f03', issue: 1049, title: TITLE[name], tasks: ['F03'], limitations: [] };
  const idBase = { id: 'f03-' + name + '-identify', lane: 'f03', issue: 1049, title: 'a fresh ' + name + ' session identifies the installed candidate from the repository documentation', tasks: ['F03', 'P32'], limitations: [] };
  const probe = (deps.probeClient || C.probeClient)(name), model = C.requestedModel(name, config);
  if (!probe.available) {
    const unrun = { verdict: 'unrun', reason: probe.reason, operations: [], assertions: [], ms: 0, client: { name, available: false, requestedModel: model } };
    return { results: [{ ...base, ...unrun }, { ...idBase, ...unrun }], cleanup: { ok: true } };
  }
  const taskId = (config.taskId || 'f03-' + randomUUID().slice(0, 8)) + '-' + name;
  let candidate;
  try { candidate = await (deps.openCandidate || require('./candidate.cjs').openCandidate)({ ...config, taskId }, deps); }
  catch (error) {
    const refused = { verdict: 'unrun', reason: 'candidate refused: ' + refusalReason(error), operations: [], assertions: [], ms: 0, client: { name, ...probe, requestedModel: model } };
    return { refusal: refusalReason(error), results: [{ ...base, ...refused }, { ...idBase, ...refused }], cleanup: { ok: true } };
  }
  sessions.push(candidate.identity);
  // `requestedModel` sits next to the client version: the model the client was ASKED to use (Claude: always explicit; Codex: 'client default' unless the runner set one);
  // each session records the model the client REPORTED using (`modelUsed`).
  const transcript = [], head = { name, ...probe, requestedModel: model };
  const metaWork = { ...head, sessions: [] }, metaId = { ...head, sessions: [] };
  const wire = createWire({ client: candidate.client, expect: { instanceId: candidate.bound.instanceId, buildId: candidate.bound.buildId }, sink: entry => transcript.push(entry) });
  let unavailable = null;
  // One client process: its own working directory, private files and event stream, recorded as information (never as a verdict).
  async function invoke(tag, prompt, steps, meta) {
    const cwd = path.join(candidate.work, 'f03-client-cwd-' + name + '-' + tag);
    fs.mkdirSync(cwd, { recursive: true });
    const spec = C.invocation(name, { command: probe.path, mcpCommand: candidate.mcpCommand, mcpRegistry: candidate.mcpRegistry, prompt, cwd, reportDir: config.reportDir, model, tag, identify: tag === 'identify', docsRoot: W.DOCS_ROOT });
    const done = await (deps.runClient || C.runClient)(spec, { timeoutMs: (config.timeoutMin || 20) * 60 * 1000 });
    const how = C.classify(done);
    const record = { session: tag, prompt, argv: [path.basename(probe.path), ...spec.args], exitCode: done.exitCode ?? null, signal: done.signal || null, timedOut: !!done.timedOut, ms: done.ms,
      outcome: how.kind, stdoutBytes: Buffer.byteLength(done.stdout), stdoutSha256: C.digest(done.stdout), stderrSha256: C.digest(done.stderr),
      modelUsed: C.modelUsed(name, done.stdout, { requested: model, ...(deps.modelEnv || {}) }), trace: C.trace(done, spec.lastMessage, steps) };
    meta.sessions.push(record);
    fs.writeFileSync(path.join(config.reportDir, name + '.' + tag + '.stream.jsonl'), done.stdout, { flag: 'wx', mode: 0o600 });
    fs.writeFileSync(path.join(config.reportDir, name + '.' + tag + '.stderr.txt'), done.stderr.slice(-65536), { flag: 'wx', mode: 0o600 });
    if (how.kind === 'unavailable') { unavailable = how.reason; throw new EvidenceError('unavailable', how.reason); }
    return { done, record };
  }
  const row = { ...base, async run(ctx) {
    const { paths, staged } = W.stageFixtures(candidate.work);
    const initial = await W.arrange(ctx, { paths });
    metaWork.fixtures = staged;
    await invoke('session1', W.promptOne(paths), W.STEPS.session1, metaWork);
    const left = await W.judgeSessionOne(ctx, { paths, initial });
    await W.judgeHandover(ctx, left);
    const two = await invoke('session2', W.promptTwo(paths), W.STEPS.session2, metaWork);
    const stream = C.discoverReplies(two.done.stdout), first = stream[0] || null;
    // The reconnect, as the receipt records it: two separate client processes, the same instance, the second one's first discovery showing what the first left.
    metaWork.reconnect = { separateProcesses: true, sameInstanceId: initial.instanceId, session1: { ms: metaWork.sessions[0].ms, exitCode: metaWork.sessions[0].exitCode },
      left: { documentId: left.documentId, revision: left.revision, opacity: left.opacity, undo: left.undo, redo: left.redo }, session2StartedFrom: first ? { documentId: first.documentId, revision: first.revision } : null,
      startedFromTheRevisionSession1Left: !!first && first.documentId === left.documentId && first.revision === left.revision && first.instanceId === left.instanceId };
    await W.judge(ctx, { paths, staged, initial, raw: W.makeRaw(candidate.client, deps.uuid || randomUUID), left, stream });
  } };
  const idRow = { ...idBase, async run(ctx) {
    if (unavailable) throw new EvidenceError('unavailable', unavailable);
    const before = await W.stateOf(ctx), prompt = W.promptIdentify();
    const truth = { buildId: candidate.identity.buildId, sourceHead: candidate.identity.sourceHead, executableSha256: candidate.identity.executableSha256 };
    const got = await invoke('identify', prompt, W.STEPS.identify, metaId);
    const answer = got.record.trace.lastMessage, said = W.parseIdentity(answer);
    metaId.identification = { prompt, answer, parsed: said, expected: truth, source: 'build-proof.json (package proof) and the registered build id',
      match: Object.fromEntries(Object.keys(truth).map(k => [k, said[k] !== null && said[k] === truth[k]])) };
    await W.judgeIdentify(ctx, { before, answer, truth });
  } };
  let results = [], cleanup, failure = null;
  try { results = await runRows({ rows: [row, idRow], session: { wire, work: candidate.work, recheck: candidate.recheck } }); }
  catch (error) { failure = refusalReason(error); }
  finally { try { await candidate.cleanup(); cleanup = { ok: true }; } catch (error) { cleanup = { ok: false, reason: refusalReason(error) }; } }
  const done = [base, idBase].map((b, n) => results[n] || { ...b, verdict: 'fail', reason: 'runner failure: ' + failure, operations: [], assertions: [], ms: 0 });
  fs.writeFileSync(path.join(config.reportDir, name + '.wire.jsonl'), transcript.map(e => JSON.stringify(e)).join('\n') + (transcript.length ? '\n' : ''), { flag: 'wx', mode: 0o600 });
  const session = { taskId, instanceId: candidate.identity.instanceId, pid: candidate.identity.pid };
  return { results: [{ ...done[0], client: metaWork, session }, { ...done[1], client: metaId, session }], cleanup };
}

const COMMON = ['sourceHead', 'tree', 'appVersion', 'buildId', 'executableSha256', 'mcpSha256', 'proofSha256', 'app'];
async function main(config, deps = {}) {
  let names;
  try {
    validateConfig(config);
    names = config.clients || [...C.NAMES];
    if (!names.length || !names.every(n => C.NAMES.includes(n)) || new Set(names).size !== names.length) throw new UsageError('clients must be codex, claude or both');
    for (const key of ['claudeModel', 'codexModel']) if (config[key] !== undefined && !C.validModel(config[key])) throw new UsageError(key + ' must be a model name');
    if (config.timeoutMin !== undefined && !(Number.isFinite(config.timeoutMin) && config.timeoutMin >= 1 && config.timeoutMin <= 120)) throw new UsageError('timeout must be 1 to 120 minutes');
  } catch (error) { return { exit: EXIT.refused, refused: refusalReason(error), receipt: null }; }
  const rows = [], cleanups = [], sessions = [];
  let refusal = null;
  for (const name of names) {
    const half = await runHalf(name, config, deps, sessions);
    rows.push(...half.results); cleanups.push(half.cleanup);
    if (half.refusal) { refusal = half.refusal; break; } // the candidate, not the client, is wrong: a second session would be refused the same way
  }
  const first = sessions[0] || null;
  if (first && sessions.some(s => COMMON.some(k => s[k] !== first[k]))) rows.push({ id: 'run', lane: 'f03', issue: 1049, title: 'candidate identity', tasks: [], limitations: [], verdict: 'fail', reason: 'the two sessions did not bind the same candidate', operations: [], assertions: [], ms: 0 });
  const failed = cleanups.filter(c => !c.ok);
  const cleanup = failed.length ? { ok: false, reason: failed.map(c => c.reason).join('; ') } : { ok: true };
  const candidate = first ? Object.fromEntries(COMMON.map(k => [k, first[k]])) : null;
  const receipt = { ...buildReceipt({ candidate, selection: { lanes: ['f03'], rows: rows.map(r => r.id), omitted: C.NAMES.filter(n => !names.includes(n)).flatMap(n => ['f03-' + n, 'f03-' + n + '-identify']), catalogRows: { f03: C.NAMES.length * 2 } }, rows, cleanup, refusal }),
    schema: SCHEMA, statement: STATEMENT, clients: rows.filter(r => r.client).map(r => ({ row: r.id, ...r.client })), limitations: LIMITATIONS };
  delete receipt.residualHandsOn;
  fs.writeFileSync(path.join(config.reportDir, 'f03-clients-receipt.json'), JSON.stringify(receipt, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  (deps.stdout || process.stdout).write(JSON.stringify({ verdict: receipt.verdict, totals: receipt.totals, cleanup, rows: rows.map(r => ({ id: r.id, verdict: r.verdict, reason: r.reason || null })) }) + '\n');
  return { exit: exitStatus(receipt.verdict), receipt };
}

function parse(argv) {
  const out = {}, need = (ok, m) => { if (!ok) throw new UsageError(m); };
  const keys = { '--app': 'app', '--build-proof': 'buildProof', '--report-dir': 'reportDir', '--task': 'taskId', '--source-sha': 'sourceSha' };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i], value = argv[i + 1];
    if (keys[arg]) { need(value !== undefined && !value.startsWith('--'), arg + ' needs a value'); out[keys[arg]] = value; i++; }
    else if (arg === '--client') { need(value !== undefined, '--client needs a value'); out.clients = value === 'all' ? [...C.NAMES] : value.split(',').map(s => s.trim()).filter(Boolean); i++; }
    else if (arg === '--reserve') { need(value !== undefined, '--reserve needs a value'); out.reserve = value === 'none' ? [] : value.split(',').map(s => s.trim()).filter(Boolean); i++; }
    else if (arg === '--claude-model' || arg === '--codex-model') { need(value !== undefined && !value.startsWith('--'), arg + ' needs a value'); out[arg === '--claude-model' ? 'claudeModel' : 'codexModel'] = value; i++; }
    else if (arg === '--timeout-min') { need(value !== undefined, '--timeout-min needs a value'); out.timeoutMin = Number(value); i++; }
    else throw new UsageError('unknown argument');
  }
  return out;
}

async function cli(argv, deps = {}) {
  let config;
  try { config = parse(argv); } catch (error) { (deps.stderr || process.stderr).write(refusalReason(error) + '\n'); return EXIT.refused; }
  const { exit, refused } = await main(config, deps);
  if (refused) (deps.stderr || process.stderr).write('f03 clients refused: ' + refused + '\n');
  return exit;
}

if (require.main === module) cli(process.argv.slice(2)).then(code => { process.exitCode = code; }, error => { process.stderr.write(refusalReason(error) + '\n'); process.exitCode = EXIT.refused; });

module.exports = { SCHEMA, STATEMENT, LIMITATIONS, main, cli, parse, runHalf };
