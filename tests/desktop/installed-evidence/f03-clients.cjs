#!/usr/bin/env node
'use strict';
// #1049 / F03. Explicit manual entry point; never an automatic test, builds nothing. Runs the issue's original opacity workflow
// twice against an already built package: once through the real Codex CLI (`codex exec`), once through the real Claude Code CLI
// (`claude -p`). Each half gets its own fresh isolated app session and a per-invocation MCP configuration that points at the
// package's own bundled nemo-mcp. No keyboard or mouse input is sent. The candidate is bound exactly as installed-evidence/run.cjs
// binds it (candidate.cjs), and the verdict of each half comes only from this harness's own reads of the application after the
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
const STATEMENT = 'Agent evidence for #1049: the original opacity workflow driven by the real Codex CLI and the real Claude Code CLI, each through a ' +
  'per-invocation MCP configuration for the candidate package\'s bundled nemo-mcp, with no keyboard or mouse input. The verdict of each half rests only on ' +
  'this harness\'s own reads and probes of the application after the client exited, and on files the application wrote; client output and tool traces ' +
  'are recorded as information and never decide a verdict. A client that is missing, unauthenticated or over quota leaves its half unrun. It is not the ' +
  'CS7 batched installed session and it does not record the acceptance disposition of #1049, which belongs to the project owners.';
const LIMITATIONS = Object.freeze([
  'Whether the client itself attempted the stale-revision and wrong-document writes is visible only in its event stream (informational); the harness proves the application refuses them, not that the client sent them.',
  'Undo is evidenced by saved-60.json (60) followed by saved.json (40) and the refused out-of-date write, not by reading the history depth at that moment.',
  'The keyed 0/20 read by the client is evidenced by the final open project evaluating 20, 50, 80 at frames 0, 10, 20 and being untouched (revision 0).',
  'Only the Nemo MCP route is exercised: no window, visual or keyboard behaviour of the installed UI is observed.',
]);
const TITLE = { codex: 'original opacity workflow through the real Codex CLI', claude: 'original opacity workflow through the real Claude Code CLI' };

// One half. `deps` makes every external effect replaceable: probeClient, runClient, openCandidate, uuid.
async function runHalf(name, config, deps, sessions) {
  const base = { id: 'f03-' + name, lane: 'f03', issue: 1049, title: TITLE[name], tasks: ['F03'], limitations: [] };
  const probe = (deps.probeClient || C.probeClient)(name), model = C.requestedModel(name, config);
  if (!probe.available) return { result: { ...base, verdict: 'unrun', reason: probe.reason, operations: [], assertions: [], ms: 0, client: { name, available: false, requestedModel: model } }, cleanup: { ok: true } };
  const taskId = (config.taskId || 'f03-' + randomUUID().slice(0, 8)) + '-' + name;
  let candidate;
  try { candidate = await (deps.openCandidate || require('./candidate.cjs').openCandidate)({ ...config, taskId }, deps); }
  catch (error) {
    return { refusal: refusalReason(error), result: { ...base, verdict: 'unrun', reason: 'candidate refused: ' + refusalReason(error), operations: [], assertions: [], ms: 0, client: { name, ...probe, requestedModel: model } }, cleanup: { ok: true } };
  }
  sessions.push(candidate.identity);
  // `requestedModel` sits next to the client version: the model the client was ASKED to use (Claude: always explicit; Codex: 'client default' unless the runner set one).
  const transcript = [], meta = { name, ...probe, requestedModel: model };
  const wire = createWire({ client: candidate.client, expect: { instanceId: candidate.bound.instanceId, buildId: candidate.bound.buildId }, sink: entry => transcript.push(entry) });
  const row = { ...base, async run(ctx) {
    const { paths, staged } = W.stageFixtures(candidate.work);
    const initial = await W.arrange(ctx, { paths });
    const cwd = path.join(candidate.work, 'f03-client-cwd-' + name);
    fs.mkdirSync(cwd, { recursive: true });
    const prompt = W.promptFor(paths);
    const spec = C.invocation(name, { command: probe.path, mcpCommand: candidate.mcpCommand, mcpRegistry: candidate.mcpRegistry, prompt, cwd, reportDir: config.reportDir, model });
    meta.prompt = prompt; meta.argv = [path.basename(probe.path), ...spec.args]; meta.fixtures = staged;
    const done = await (deps.runClient || C.runClient)(spec, { timeoutMs: (config.timeoutMin || 20) * 60 * 1000 });
    const how = C.classify(done);
    Object.assign(meta, { exitCode: done.exitCode ?? null, signal: done.signal || null, timedOut: !!done.timedOut, ms: done.ms, outcome: how.kind,
      stdoutBytes: Buffer.byteLength(done.stdout), stdoutSha256: C.digest(done.stdout), stderrSha256: C.digest(done.stderr),
      trace: C.trace(done, spec.lastMessage) });
    fs.writeFileSync(path.join(config.reportDir, name + '.stream.jsonl'), done.stdout, { flag: 'wx', mode: 0o600 });
    fs.writeFileSync(path.join(config.reportDir, name + '.stderr.txt'), done.stderr.slice(-65536), { flag: 'wx', mode: 0o600 });
    if (how.kind === 'unavailable') throw new EvidenceError('unavailable', how.reason);
    await W.judge(ctx, { paths, staged, initial, raw: W.makeRaw(candidate.client, deps.uuid || randomUUID) });
  } };
  let results = [], cleanup, failure = null;
  try { results = await runRows({ rows: [row], session: { wire, work: candidate.work, recheck: candidate.recheck } }); }
  catch (error) { failure = refusalReason(error); }
  finally { try { await candidate.cleanup(); cleanup = { ok: true }; } catch (error) { cleanup = { ok: false, reason: refusalReason(error) }; } }
  const result = results[0] || { ...base, verdict: 'fail', reason: 'runner failure: ' + failure, operations: [], assertions: [], ms: 0 };
  fs.writeFileSync(path.join(config.reportDir, name + '.wire.jsonl'), transcript.map(e => JSON.stringify(e)).join('\n') + (transcript.length ? '\n' : ''), { flag: 'wx', mode: 0o600 });
  return { result: { ...result, client: meta, session: { taskId, instanceId: candidate.identity.instanceId, pid: candidate.identity.pid } }, cleanup };
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
    rows.push(half.result); cleanups.push(half.cleanup);
    if (half.refusal) { refusal = half.refusal; break; } // the candidate, not the client, is wrong: a second session would be refused the same way
  }
  const first = sessions[0] || null;
  if (first && sessions.some(s => COMMON.some(k => s[k] !== first[k]))) rows.push({ id: 'run', lane: 'f03', issue: 1049, title: 'candidate identity', tasks: [], limitations: [], verdict: 'fail', reason: 'the two sessions did not bind the same candidate', operations: [], assertions: [], ms: 0 });
  const failed = cleanups.filter(c => !c.ok);
  const cleanup = failed.length ? { ok: false, reason: failed.map(c => c.reason).join('; ') } : { ok: true };
  const candidate = first ? Object.fromEntries(COMMON.map(k => [k, first[k]])) : null;
  const receipt = { ...buildReceipt({ candidate, selection: { lanes: ['f03'], rows: rows.map(r => r.id), omitted: C.NAMES.filter(n => !names.includes(n)).map(n => 'f03-' + n), catalogRows: { f03: C.NAMES.length } }, rows, cleanup, refusal }),
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
