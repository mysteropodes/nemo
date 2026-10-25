#!/usr/bin/env node
'use strict';
// Parity matrix generator (CS6/#2346).
//
// engineering/inventory/parity/ holds the remediation's parity denominator:
//   config.json            commencement baseline SHA and its approval status
//   tasks.json             the 228 original task contracts (imported once)
//   baseline-surfaces.json surfaces of the frozen baseline, stable CS3 IDs
//   mapping.json           CURATED: surface → tasks / capability / disposition
//   owners.json            snapshot: task → issues declaring that task ID
//   ownership.json         CURATED: task → issues owning it without declaring the ID
//   matrix.json, PARITY.md DERIVED from the above; `--check` fails when stale
//
//   node scripts/nemo/parity-matrix.cjs                    regenerate matrix
//   node scripts/nemo/parity-matrix.cjs --check            gate (no git/network)
//   node scripts/nemo/parity-matrix.cjs --check --deep     also rebuild baseline
//   node scripts/nemo/parity-matrix.cjs --import-tasks <228-task traceability.csv>
//   node scripts/nemo/parity-matrix.cjs --freeze-baseline  rebuild baseline-surfaces.json
//   node scripts/nemo/parity-matrix.cjs --refresh-owners   snapshot owners via `gh`
//   node scripts/nemo/parity-matrix.cjs --strict           fail on unmapped/unowned rows
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const lib = require('./lib/parity-matrix.cjs');

const ROOT = path.resolve(__dirname, '..', '..');
const DIR = path.join(ROOT, 'engineering', 'inventory', 'parity');
const file = (name) => path.join(DIR, name);
const readJson = (name) => JSON.parse(fs.readFileSync(file(name), 'utf8'));
const json = (value) => JSON.stringify(value, null, 2) + '\n';
const readOptional = (name) => (fs.existsSync(file(name)) ? readJson(name) : { tasks: {} });

function freezeBaseline(sha) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-parity-baseline-'));
  try {
    const tar = execFileSync('git', ['archive', sha, 'src'], { cwd: ROOT, maxBuffer: 1 << 30 });
    execFileSync('tar', ['-x', '-C', tmp], { input: tar });
    const rows = require('./inventory.cjs').build({ root: tmp }).rows.map(lib.baselineRow);
    rows.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
    return { schema: 'nemo.parity-baseline/1', baseline: sha, generator: 'scripts/nemo/inventory.cjs (stable IDs)', rows };
  } finally { fs.rmSync(tmp, { recursive: true, force: true }); }
}

function refreshOwners(tasks) {
  const out = execFileSync('gh', ['issue', 'list', '--repo', 'mysteropodes/nemo', '--state', 'all',
    '--limit', '3000', '--json', 'number,title,state,body,labels'], { cwd: ROOT, maxBuffer: 1 << 30, encoding: 'utf8' });
  return lib.ownersSnapshot(JSON.parse(out), tasks.map((t) => t.task), new Date().toISOString().slice(0, 10), readOptional('ownership.json'));
}

function outputs() {
  const input = { config: readJson('config.json'), tasks: readJson('tasks.json'),
    baseline: readJson('baseline-surfaces.json'), mapping: readJson('mapping.json'), owners: readJson('owners.json'),
    ownership: readOptional('ownership.json') };
  const problems = lib.validate(input);
  if (!fs.existsSync(file('ownership.json'))) problems.push('ownership.json is missing');
  if (input.baseline.baseline !== input.config.baseline) problems.push('baseline-surfaces.json was frozen at a different SHA than config.json');
  const matrix = lib.buildMatrix(input);
  return { problems, matrix, files: { 'matrix.json': json(matrix), 'PARITY.md': lib.renderMarkdown(matrix) } };
}

function main() {
  const args = process.argv.slice(2);
  const at = (flag) => { const i = args.indexOf(flag); return i >= 0 ? args[i + 1] : null; };
  if (args.includes('--import-tasks')) {
    fs.writeFileSync(file('tasks.json'), json(lib.tasksFromCsv(fs.readFileSync(at('--import-tasks'), 'utf8'))));
  }
  if (args.includes('--freeze-baseline')) fs.writeFileSync(file('baseline-surfaces.json'), json(freezeBaseline(readJson('config.json').baseline)));
  if (args.includes('--refresh-owners')) fs.writeFileSync(file('owners.json'), json(refreshOwners(readJson('tasks.json'))));
  const { problems, matrix, files } = outputs();
  if (args.includes('--check')) {
    if (args.includes('--deep') && json(freezeBaseline(readJson('config.json').baseline)) !== fs.readFileSync(file('baseline-surfaces.json'), 'utf8')) {
      problems.push('baseline-surfaces.json differs from a fresh rebuild at the baseline SHA');
    }
    for (const [name, content] of Object.entries(files)) {
      if (!fs.existsSync(file(name)) || fs.readFileSync(file(name), 'utf8') !== content) problems.push(`${name} is stale — run node scripts/nemo/parity-matrix.cjs`);
    }
  } else {
    for (const [name, content] of Object.entries(files)) fs.writeFileSync(file(name), content);
  }
  if (args.includes('--strict')) {
    const open = (matrix.counts.surfaceStatus.unmapped || 0) + (matrix.counts.surfaceStatus.unowned || 0);
    if (open) problems.push(`${open} surface(s) are unmapped or unowned (strict)`);
  }
  if (problems.length) { console.error('parity matrix: ' + problems.slice(0, 10).join('; ')); process.exit(1); }
  console.log(`parity matrix: ${matrix.counts.surfaces} surfaces ${JSON.stringify(matrix.counts.surfaceStatus)}, ${matrix.counts.tasks} tasks ${JSON.stringify(matrix.counts.taskStatus)}`);
}

if (require.main === module) main();
module.exports = { freezeBaseline, outputs, DIR };
