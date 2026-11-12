#!/usr/bin/env node
'use strict';
// Explicit manual entry point; never an automatic app-launching test, and it builds nothing. It takes an already built package,
// starts it in a fresh isolated session, drives it only through the package's bundled nemo-mcp, and writes a receipt.
//
//   lockf -k /tmp/nemo-heavy-job.lock node tests/desktop/installed-evidence/run.cjs \
//     --app /abs/Nemo.app --build-proof /abs/build-proof.json --report-dir /abs/empty-dir [--lane 2107,2108] [--row id,id] [--reserve gpu-reference|none]
//
// Exit status: 0 every catalog row ran and passed; 1 a row failed (or cleanup was incomplete); 2 refused before any row (usage, not
// the candidate); 4 no row failed but at least one is unrun (an operation is unavailable); 5 the selected rows all passed but the
// selection left catalog rows out (`subset-pass`: not a lane receipt).
const fs = require('node:fs');
const path = require('node:path');
const { isCheckRefusal } = require('../native-harness.cjs');
const { allRows, selectRows, runRows, LANES } = require('./runner.cjs');
const { createWire } = require('./wire.cjs');
const { buildReceipt, exitStatus, EXIT } = require('./receipt.cjs');

function refusalReason(error) {
  const message = error && error.message;
  if ((isCheckRefusal(error) || error?.name === 'UsageError') && typeof message === 'string' && /^[A-Za-z][A-Za-z0-9 .,;:()'/-]{0,199}$/.test(message)) return message;
  const name = /^[A-Za-z][A-Za-z0-9]{0,40}$/.test(error?.name || '') ? error.name : 'Error';
  return /^[A-Z][A-Z0-9_]{0,40}$/.test(error?.code || '') ? name + ' ' + error.code : name;
}
class UsageError extends Error { constructor(message) { super(message); this.name = 'UsageError'; } }
const need = (ok, message) => { if (!ok) throw new UsageError(message); };

function validateConfig(config) {
  need(config && typeof config === 'object', 'configuration required');
  for (const key of ['app', 'buildProof', 'reportDir']) need(typeof config[key] === 'string' && path.isAbsolute(config[key]), key + ' must be an absolute path');
  need(config.app.endsWith('.app'), 'app must be a .app bundle');
  need(fs.existsSync(config.buildProof) && fs.statSync(config.buildProof).isFile(), 'build proof file not found');
  const dir = config.reportDir;
  need(fs.existsSync(dir) && fs.lstatSync(dir).isDirectory() && !fs.lstatSync(dir).isSymbolicLink() && fs.realpathSync(dir) === dir, 'report directory must exist, be a real directory and be given by its real path');
  need(fs.readdirSync(dir).length === 0, 'report directory must be empty');
  if (config.reserve !== undefined) need(Array.isArray(config.reserve) && config.reserve.every(slot => /^[a-z][a-z0-9-]{0,40}$/.test(slot)), 'invalid slot reservation');
  if (config.taskId !== undefined) need(/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(config.taskId), 'invalid task id');
  if (config.sourceSha !== undefined) need(/^[0-9a-f]{40}$/.test(config.sourceSha), 'source sha must be 40 hex characters');
}

// `deps` exists so the refusals and verdicts can be proven with a stub candidate and stub MCP client.
async function main(config, deps = {}) {
  const catalog = deps.catalog || allRows();
  let selection;
  try {
    validateConfig(config);
    selection = selectRows({ lanes: config.lanes, rows: config.rows }, catalog);
  } catch (error) { return { exit: EXIT.refused, refused: refusalReason(error), receipt: null }; }
  const write = (name, value, extra = {}) => fs.writeFileSync(path.join(config.reportDir, name), value, { flag: 'wx', mode: 0o600, ...extra });
  const transcript = [];
  const sel = { lanes: selection.lanes, rows: selection.rows.map(r => r.id), omitted: selection.omitted, catalogRows: selection.catalogRows };
  let candidate;
  try { candidate = await (deps.openCandidate || require('./candidate.cjs').openCandidate)(config, deps); }
  catch (error) {
    const reason = refusalReason(error);
    const receipt = buildReceipt({ candidate: null, selection: sel, rows: [], cleanup: { ok: true, reason: 'nothing started or cleanup completed' }, refusal: reason });
    write('installed-client-evidence.json', JSON.stringify(receipt, null, 2) + '\n');
    return { exit: EXIT.refused, refused: reason, receipt };
  }
  let rows = [], cleanup, failure = null;
  try {
    const wire = createWire({ client: candidate.client, expect: { instanceId: candidate.bound.instanceId, buildId: candidate.bound.buildId }, sink: entry => transcript.push(entry) });
    rows = await runRows({ rows: selection.rows, session: { wire, work: candidate.work, recheck: candidate.recheck } });
  } catch (error) { failure = refusalReason(error); }
  finally {
    try { await candidate.cleanup(); cleanup = { ok: true }; } catch (error) { cleanup = { ok: false, reason: refusalReason(error) }; }
  }
  if (failure) rows.push({ id: 'run', lane: 'run', issue: 0, title: 'runner', tasks: [], limitations: [], verdict: 'fail', reason: 'runner failure: ' + failure, operations: [], assertions: [], ms: 0 });
  const receipt = buildReceipt({ candidate: candidate.identity, selection: sel, rows, cleanup });
  write('installed-client-evidence.json', JSON.stringify(receipt, null, 2) + '\n');
  write('transcript.jsonl', transcript.map(entry => JSON.stringify(entry)).join('\n') + (transcript.length ? '\n' : ''));
  const exit = exitStatus(receipt.verdict);
  (deps.stdout || process.stdout).write(JSON.stringify({ verdict: receipt.verdict, selection: { lanes: sel.lanes, rows: sel.rows.length, omitted: sel.omitted.length, catalogRows: sel.catalogRows }, totals: receipt.totals, uncoveredIds: Object.fromEntries(Object.entries(receipt.coverage).map(([lane, c]) => [lane, c.uncoveredIds])), cleanup, candidate: { sourceHead: receipt.candidate.sourceHead, buildId: receipt.candidate.buildId } }) + '\n');
  return { exit, receipt };
}

function parse(argv) {
  const out = {}, list = value => String(value).split(',').map(s => s.trim()).filter(Boolean);
  const keys = { '--app': 'app', '--build-proof': 'buildProof', '--report-dir': 'reportDir', '--task': 'taskId', '--source-sha': 'sourceSha' };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i], value = argv[i + 1];
    if (keys[arg]) { need(value !== undefined && !value.startsWith('--'), arg + ' needs a value'); out[keys[arg]] = value; i++; }
    else if (arg === '--lane') { need(value !== undefined, '--lane needs a value'); out.lanes = value === 'all' ? [...LANES] : list(value); i++; }
    else if (arg === '--reserve') { need(value !== undefined, '--reserve needs a value'); out.reserve = value === 'none' ? [] : list(value); i++; }
    else if (arg === '--row') { need(value !== undefined, '--row needs a value'); out.rows = list(value); i++; }
    else throw new UsageError('unknown argument');
  }
  return out;
}

async function cli(argv, deps = {}) {
  let config;
  try { config = parse(argv); } catch (error) { (deps.stderr || process.stderr).write(refusalReason(error) + '\n'); return EXIT.refused; }
  const { exit, refused } = await main(config, deps);
  if (refused) (deps.stderr || process.stderr).write('installed evidence refused: ' + refused + '\n');
  return exit;
}

if (require.main === module) cli(process.argv.slice(2)).then(code => { process.exitCode = code; }, error => { process.stderr.write(refusalReason(error) + '\n'); process.exitCode = EXIT.refused; });

module.exports = { main, cli, parse, validateConfig, refusalReason, UsageError };
