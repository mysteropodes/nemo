#!/usr/bin/env node
'use strict';
// Per-family headless native acceptance (CS6 part 2, #2346).
//
//   node scripts/nemo/family-acceptance.cjs <family> [--write] [--json]
//
// Runs every native-test check of engineering/inventory/parity/families/<family>.json
// (one cargo job at a time), prints the verdict, and with --write records
// results/<family>.json for the parity matrix. --write refuses a dirty tree: the
// record names the source SHA it proves. Run it only while holding the compiler slot.
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const lib = require('./lib/family-acceptance.cjs');

const ROOT = path.resolve(__dirname, '..', '..');
const DIR = path.join(ROOT, 'engineering', 'inventory', 'parity');

function exec(cmd, args) {
  const r = spawnSync(cmd, args, { cwd: ROOT, encoding: 'utf8', maxBuffer: 1 << 28 });
  return { status: r.status === null ? 1 : r.status, stdout: r.stdout || '', stderr: r.stderr || '' };
}

function main(argv = process.argv.slice(2), run = exec) {
  const family = argv.find((a) => !a.startsWith('--'));
  if (!family || !/^[a-z][a-z0-9-]*$/.test(family)) { console.error('usage: family-acceptance.cjs <family> [--write] [--json]'); return 2; }
  const declFile = path.join(DIR, 'families', `${family}.json`);
  if (!fs.existsSync(declFile)) { console.error(`family-acceptance: ${path.relative(ROOT, declFile)} does not exist`); return 2; }
  const decl = JSON.parse(fs.readFileSync(declFile, 'utf8'));
  const taskIds = new Set(JSON.parse(fs.readFileSync(path.join(DIR, 'tasks.json'), 'utf8')).map((t) => t.task));
  const problems = lib.validateFamily(decl, taskIds, family);
  if (problems.length) { console.error('family-acceptance: ' + problems.join('; ')); return 2; }
  if (argv.includes('--write') && lib.gitState(run).dirty) {
    console.error('family-acceptance: refusing --write on a dirty tree; commit first so the record names the SHA it proves');
    return 2;
  }
  const { result, ran } = (() => { const o = lib.runFamily(decl, { exec: run }); return { result: o.result, ran: o.ran }; })();
  if (argv.includes('--json')) console.log(JSON.stringify(result, null, 2));
  else {
    for (const c of ran) console.log(`${c.ok ? 'ok  ' : 'FAIL'} ${c.id}: ${c.passed} passed, ${c.failed} failed, ${c.ignored} ignored (${c.target} ${c.filter})`);
    console.log(`${family}: ${result.verdict} at ${result.sourceSha.slice(0, 8)}${result.kindsMissing.length ? '; missing ' + result.kindsMissing.join(', ') : ''}`);
  }
  if (argv.includes('--write')) {
    fs.mkdirSync(path.join(DIR, 'results'), { recursive: true });
    fs.writeFileSync(path.join(DIR, 'results', `${family}.json`), JSON.stringify(result, null, 2) + '\n');
  }
  return result.verdict === 'fail' ? 1 : 0;
}

if (require.main === module) process.exit(main());
module.exports = { main, exec };
