#!/usr/bin/env node
'use strict';
// Explicit manual entry point; never an automatic app-launching test.
const fs = require('node:fs');
const path = require('node:path');
const { requireCheck: check } = require('./native-harness.cjs');
const { readDeclarations, readyRows, runInstalledRows, selectRows, taskSurfaceIndex } = require('./cs7-runner.cjs');
const { createInstalledSession } = require('./cs7-session.cjs');
const ROOT = path.resolve(__dirname, '../..');

// `deps` exists so the refusals can be proven with disposable declarations and a stub session.
async function main(config, deps = {}) {
  const root = deps.root || ROOT, scenarioDirectory = deps.scenarioDirectory || path.join(__dirname, 'cs7-scenarios');
  const openSession = deps.createInstalledSession || createInstalledSession;
  const matrix = JSON.parse(fs.readFileSync(path.join(root, 'engineering/inventory/parity/matrix.json'), 'utf8'));
  const declarations = readDeclarations(path.join(root, 'engineering/inventory/parity/installed'));
  // Optional `rows` allow-list: only these Ready rows are loaded, prepared and run. Without it
  // every Ready row is, exactly as before.
  const selection = config.rows === undefined ? null : selectRows(declarations, config.rows);
  const scenarios = Object.create(null);
  for (const row of declarations.flatMap(d => d.rows || []).filter(row => row.status === 'Ready' && (!selection || selection.ids.has(row.id)))) {
    check(/^[a-z][a-z0-9-]*$/.test(row.scenario || ''), 'invalid installed scenario');
    if (scenarios[row.scenario]) continue;
    const file = path.join(scenarioDirectory, row.scenario + '.cjs');
    check(fs.realpathSync(file) === file && fs.lstatSync(file).isFile(), 'reviewed installed scenario required');
    const module = require(file);
    scenarios[row.scenario] = typeof module.prepare === 'function' ? await module.prepare({ config, root }) : module;
  }
  const rows = readyRows(declarations, { taskIds: new Set(matrix.tasks.map(t => t.task)),
    surfaceIds: new Set(matrix.surfaces.map(s => s.id)), taskSurfaces: taskSurfaceIndex(matrix), scenarios, ...(selection ? { only: selection.ids } : {}) });
  check(typeof config.reportDirectory === 'string' && path.isAbsolute(config.reportDirectory), 'absolute report directory required');
  const resultFile = path.join(config.reportDirectory, 'installed-results.json');
  check(!fs.existsSync(resultFile), 'existing installed receipt must not be overwritten');
  // Validate the entire census and all scripts before creating/starting a runtime.
  const session = openSession(config);
  let ordinal = 0;
  const result = await runInstalledRows({ rows, scenarios, session, ...(selection ? { selection } : {}), save: async record => {
    const file = record.schema ? resultFile : path.join(config.reportDirectory, 'progress-' + ++ordinal + '.json');
    fs.writeFileSync(file, JSON.stringify(record, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  } });
  (deps.stdout || process.stdout).write(JSON.stringify({ verdict: result.verdict, ...(result.selection ? { selection: result.selection } : {}), sourceSha: result.sourceSha,
    rows: result.installed.rows.length, operatorInputFailures: result.installed.operatorInputFailures,
    cleanup: result.installed.cleanup }) + '\n');
  return result;
}
// A refusal must be diagnosable without leaking evidence. Only a plain `check` Error (no code, no
// subclass) with a short fixed-looking message is quoted; anything else (fs errors with paths,
// JSON syntax errors that quote the file, scenario or OS errors) yields its class and code only.
function refusalReason(error) {
  const message = error?.message;
  if (error && Object.getPrototypeOf(error) === Error.prototype && error.code === undefined &&
    typeof message === 'string' && /^[A-Za-z][A-Za-z0-9 .,;:()-]{0,199}$/.test(message)) return message;
  const name = /^[A-Za-z][A-Za-z0-9]{0,40}$/.test(error?.name || '') ? error.name : 'Error';
  return /^[A-Z][A-Z0-9_]{0,40}$/.test(error?.code || '') ? name + ' ' + error.code : name;
}

// Exit status: 0 only for `installed-pass` (every Ready row on one candidate); 3 for `installed-subset-pass`
// (the selected rows passed, other Ready rows were not run); 1 for anything else.
const EXIT_SUBSET_PASS = 3;
function cli(argv, deps = {}) {
  const stderr = deps.stderr || process.stderr;
  return Promise.resolve().then(() => {
    check(argv.length === 3, 'one private config file required');
    return main(JSON.parse(fs.readFileSync(argv[2], 'utf8')), deps);
  }).then(result => {
    if (result.verdict === 'installed-pass') return 0;
    if (result.verdict !== 'installed-subset-pass') return 1;
    stderr.write('CS7 subset run passed (installed-subset-pass): ' + result.selection.selected.length + ' selected row(s) passed, ' +
      result.selection.omittedReady.length + ' Ready row(s) were not run; this is not the every-Ready-row outcome (exit status ' + EXIT_SUBSET_PASS + ').\n');
    return EXIT_SUBSET_PASS;
  })
    .catch(error => {
      stderr.write('CS7 batch refused or incomplete (' + refusalReason(error) + '); inspect its private receipt. No acceptance claimed.\n');
      return 1;
    });
}
if (require.main === module) cli(process.argv).then(code => { process.exitCode = code; });
module.exports = { main, cli, refusalReason, EXIT_SUBSET_PASS };
