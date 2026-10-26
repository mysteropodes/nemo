#!/usr/bin/env node
'use strict';
// Explicit manual entry point; never an automatic app-launching test.
const fs = require('node:fs');
const path = require('node:path');
const { requireCheck: check } = require('./native-harness.cjs');
const { readDeclarations, readyRows, runInstalledRows } = require('./cs7-runner.cjs');
const { createInstalledSession } = require('./cs7-session.cjs');
const ROOT = path.resolve(__dirname, '../..');

async function main(config) {
  const matrix = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/inventory/parity/matrix.json'), 'utf8'));
  const declarations = readDeclarations(path.join(ROOT, 'engineering/inventory/parity/installed'));
  const scenarios = Object.create(null);
  for (const row of declarations.flatMap(d => d.rows || []).filter(row => row.status === 'Ready')) {
    check(/^[a-z][a-z0-9-]*$/.test(row.scenario || ''), 'invalid installed scenario');
    if (scenarios[row.scenario]) continue;
    const file = path.join(__dirname, 'cs7-scenarios', row.scenario + '.cjs');
    check(fs.realpathSync(file) === file && fs.lstatSync(file).isFile(), 'reviewed installed scenario required');
    const module = require(file);
    scenarios[row.scenario] = typeof module.prepare === 'function' ? await module.prepare({ config, root: ROOT }) : module;
  }
  const rows = readyRows(declarations, { taskIds: new Set(matrix.tasks.map(t => t.task)),
    surfaceIds: new Set(matrix.surfaces.map(s => s.id)), scenarios });
  check(typeof config.reportDirectory === 'string' && path.isAbsolute(config.reportDirectory), 'absolute report directory required');
  const resultFile = path.join(config.reportDirectory, 'installed-results.json');
  check(!fs.existsSync(resultFile), 'existing installed receipt must not be overwritten');
  // Validate the entire census and all scripts before creating/starting a runtime.
  const session = createInstalledSession(config);
  let ordinal = 0;
  const result = await runInstalledRows({ rows, scenarios, session, save: async record => {
    const file = record.schema ? resultFile : path.join(config.reportDirectory, 'progress-' + ++ordinal + '.json');
    fs.writeFileSync(file, JSON.stringify(record, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  } });
  process.stdout.write(JSON.stringify({ verdict: result.verdict, sourceSha: result.sourceSha,
    rows: result.installed.rows.length, operatorInputFailures: result.installed.operatorInputFailures,
    cleanup: result.installed.cleanup }) + '\n');
  return result;
}
if (require.main === module) {
  Promise.resolve().then(() => {
    check(process.argv.length === 3, 'one private config file required');
    return main(JSON.parse(fs.readFileSync(process.argv[2], 'utf8')));
  }).then(result => { process.exitCode = result.verdict === 'installed-pass' ? 0 : 1; })
    .catch(() => { process.stderr.write('CS7 batch refused or incomplete; inspect its private receipt. No acceptance claimed.\n'); process.exitCode = 1; });
}
module.exports = { main };
