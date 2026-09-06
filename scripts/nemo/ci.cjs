#!/usr/bin/env node
'use strict';
// PR orchestration over the same command surface used locally. See engineering/ci.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');

const ROOT = path.resolve(__dirname, '../..');
const PROFILE = 'engineering/boundaries/profiles/scripts-nemo.profile.json';
const LANES = ['quick', 'boundaries', 'surfaces'];
const QUICK = ['doctor', 'check', 'test:unit', 'test:rust'];
const SURFACES = ['test:integration', 'test:browser', 'test:rust-tauri', 'build:wasm', 'build:desktop', 'test:desktop'];

function aggregate(needs) {
  const problems = [];
  for (const lane of LANES) {
    if (!needs || needs[lane]?.result !== 'success') problems.push(`${lane}: ${needs?.[lane]?.result || 'missing'}`);
  }
  return { ok: problems.length === 0, problems };
}

function validateReceipt(receipt, required, processStatus) {
  const problems = [];
  if (processStatus !== 0) problems.push(`runner exit: ${processStatus ?? 'unavailable'}`);
  if (receipt?.schema !== 'nemo.receipt/1' || !Array.isArray(receipt?.jobs)) {
    return { ok: false, problems: problems.concat('missing or malformed nemo.receipt/1') };
  }
  if (!required.length) problems.push('required job list is empty');
  if (receipt.summary?.overall !== 'pass' || receipt.summary?.exitCode !== 0) problems.push('receipt summary did not pass');
  for (const name of required) {
    const entries = receipt.jobs.filter((job) => job?.name === name);
    if (entries.length !== 1) problems.push(`${name}: expected one result, got ${entries.length}`);
    else if (entries[0].status !== 'pass') problems.push(`${name}: ${entries[0].status || 'missing status'} (${entries[0].reason || 'no reason'})`);
    else if (entries[0].exitCode != null && entries[0].exitCode !== 0) problems.push(`${name}: nonzero exit despite pass`);
  }
  for (const job of receipt.jobs) {
    if (!required.includes(job?.name)) problems.push(`unexpected receipt job: ${job?.name || 'malformed'}`);
  }
  return { ok: !problems.length, problems };
}

function applicability(files) {
  // Explicit exemption only for documentation and the isolated tooling command surface.
  // Unknown files, executable docs, dependency/build changes and app tests require all surfaces.
  const exempt = (file) => /^(?:docs|engineering)\/.*\.md$/.test(file)
    || /^engineering\/boundaries\/.*\.json$/.test(file)
    || file.startsWith('engineering/boundaries/profiles/scripts-nemo.fixture/')
    || file === 'scripts/nemo/README.md'
    || /^(?:README|CONTRIBUTING|CLAUDE|AGENTS|THIRD_PARTY_NOTICES)\.md$/.test(file)
    || file === 'LICENSE' || file === '.github/workflows/nemo-validation.yml'
    || /^scripts\/nemo\/(?:ci|boundaries|boundaries-ratchet)(?:\.test)?\.cjs$/.test(file)
    || /^scripts\/nemo\/lib\/boundaries(?:-ratchet)?\.cjs$/.test(file)
    || /^tests\/nemo-(?:ci|boundaries|boundaries-ratchet)\.test\.cjs$/.test(file);
  const affected = files.filter((file) => !exempt(file));
  return { required: affected.length ? [...SURFACES] : [], affected,
    reason: affected.length ? 'Application, build, test or unclassified paths require all runtime surfaces.'
      : 'Only explicitly exempt documentation/tooling paths changed; quick and boundaries remain required.' };
}

function command(commandName, args, root = ROOT) {
  return spawnSync(commandName, args, { cwd: root, encoding: 'utf8', shell: false,
    timeout: 60 * 60 * 1000, maxBuffer: 64 * 1024 * 1024, env: process.env });
}

function git(args, root) {
  const result = command('git', args, root);
  if (result.status !== 0) throw new Error(`git ${args[0]} failed: ${result.stderr || result.error || result.signal}`);
  return result.stdout;
}

function checkedBase(base, root) {
  if (!/^[0-9a-f]{40}$/.test(base || '')) throw new Error('NEMO_CI_BASE_SHA must be the full protected event base SHA');
  git(['cat-file', '-e', `${base}^{commit}`], root);
  return base;
}

function changedFiles(base, root = ROOT) {
  checkedBase(base, root);
  // --no-renames keeps BOTH sides of a rename in the applicability decision.
  return git(['diff', '--name-only', '--no-renames', '-z', base, 'HEAD', '--'], root).split('\0').filter(Boolean);
}

function materializeBaseline(base, directory, root = ROOT) {
  checkedBase(base, root);
  const content = git(['show', `${base}:${PROFILE}`], root);
  JSON.parse(content); // malformed/missing policy fails before calling the checker
  const file = path.join(directory, 'protected-base-profile.json');
  fs.writeFileSync(file, content, { flag: 'wx' });
  return { file, base, sourcePath: PROFILE, sha256: crypto.createHash('sha256').update(content).digest('hex') };
}

function verify(required, root = ROOT) {
  const result = command(process.execPath, ['scripts/nemo/verify.cjs', '--jobs', required.join(','), '--json'], root);
  let receipt;
  try { receipt = JSON.parse(result.stdout); } catch { /* rejected below */ }
  return { ...validateReceipt(receipt, required, result.status), required,
    jobs: receipt?.jobs || [], receiptRunId: receipt?.runId || null,
    runnerError: result.error?.code || result.signal || null,
    stderr: result.stderr || null };
}

function boundaries(base, root = ROOT) {
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-ci-baseline-'));
  try {
    const baseline = materializeBaseline(base, scratch, root);
    const result = command(process.execPath, ['scripts/nemo/boundaries.cjs', PROFILE,
      '--baseline', baseline.file, '--root', root, '--json'], root);
    let report;
    try { report = JSON.parse(result.stdout); } catch { /* rejected below */ }
    const ok = result.status === 0 && report?.ok === true && report?.ratchet?.ok === true;
    return { ok, problems: ok ? [] : ['boundary checker or required baseline ratchet did not pass'],
      baseline: { base: baseline.base, sourcePath: baseline.sourcePath, sha256: baseline.sha256 },
      report: report || null, stderr: result.stderr || null };
  } finally { fs.rmSync(scratch, { recursive: true, force: true }); }
}

function main(argv = process.argv.slice(2)) {
  const lane = argv[0];
  if (argv.length !== 1 || ![...LANES, 'aggregate'].includes(lane)) throw new Error('usage: node scripts/nemo/ci.cjs quick|boundaries|surfaces|aggregate');
  let result;
  if (lane === 'aggregate') result = aggregate(JSON.parse(process.env.NEMO_CI_NEEDS || 'null'));
  else if (lane === 'quick') result = verify(QUICK);
  else if (lane === 'boundaries') result = boundaries(process.env.NEMO_CI_BASE_SHA);
  else {
    const files = changedFiles(process.env.NEMO_CI_BASE_SHA);
    const selection = applicability(files);
    result = { ...(selection.required.length ? verify(selection.required) : { ok: true, problems: [] }), selection, files };
  }
  result = { schema: 'nemo.ci/1', lane, source: { head: git(['rev-parse', 'HEAD'], ROOT).trim(),
    base: process.env.NEMO_CI_BASE_SHA || null }, ...result };
  const dir = process.env.NEMO_CI_REPORT_DIR || path.join(ROOT, 'reports', 'ci');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, `${lane}.json`), JSON.stringify(result, null, 2) + '\n');
  console.log(JSON.stringify(result, null, 2));
  return result.ok ? 0 : 1;
}

if (require.main === module) {
  try { process.exitCode = main(); }
  catch (err) { console.error(`CI blocked: ${err.message}`); process.exitCode = 1; }
}
module.exports = { LANES, QUICK, SURFACES, PROFILE, aggregate, validateReceipt, applicability,
  changedFiles, materializeBaseline, verify, boundaries, main };
