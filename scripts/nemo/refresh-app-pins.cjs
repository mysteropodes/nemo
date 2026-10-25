#!/usr/bin/env node
'use strict';
// CS3/#2342 — regenerate the derived provenance pins of the application
// coverage policy (engineering/boundaries/profiles/app-js.coverage.json) so no
// delivery hand-edits them. The values are exactly what
// lib/boundaries-application.cjs checkApplicationPolicy verifies:
//   sourceRootTree / applicationTree.gitTree  `git rev-parse <commit>:<path>`
//   inventoryDigest                           sha256 over discovered sources + blob ids
//   bootstrap / profiles / exclusionSupport   sha256 + git blob of each pinned file
// Policy, membership, exclusions and thresholds stay human-owned and untouched.
//
//   node scripts/nemo/refresh-app-pins.cjs              rewrite pins for HEAD (commit the source first)
//   node scripts/nemo/refresh-app-pins.cjs --check      list drifted pins, exit 1 if any
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { discoverSourcePaths } = require('./lib/boundaries-discovery.cjs');

const ROOT = path.resolve(__dirname, '..', '..');
const POLICY = 'engineering/boundaries/profiles/app-js.coverage.json';

// The gate reads trees at HEAD and blobs/digest from the working tree, so pins are
// only consistent for a committed, clean HEAD: anything else is refused.
function refreshedPolicy(root, commit, git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim()) {
  const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');
  const raw = fs.readFileSync(path.join(root, POLICY), 'utf8');
  const policy = JSON.parse(raw), pv = policy.provenance;
  if (git('rev-parse', commit) !== git('rev-parse', 'HEAD')) throw new Error('refresh-app-pins: commit the source and refresh at HEAD');
  const pinned = [pv.bootstrap, ...(pv.profiles || []), ...(pv.exclusionSupport || [])].filter(Boolean).map((r) => r.path);
  if (git('status', '--porcelain', '--', policy.scope.sourceRoot, ...pinned)) throw new Error('refresh-app-pins: source or pinned files are uncommitted');
  const sourcePaths = discoverSourcePaths({ root, sourceRoots: [policy.scope.sourceRoot], extensions: policy.scope.sourceKinds });
  const pinFile = (record) => {
    const bytes = fs.readFileSync(path.join(root, record.path));
    record.sha256 = sha256(bytes); record.gitBlob = git('hash-object', '--', record.path);
  };
  pv.candidateCommit = commit;
  pv.sourceRootTree = git('rev-parse', `${commit}:${policy.scope.sourceRoot}`);
  pv.applicationTree.gitTree = git('rev-parse', `${commit}:${pv.applicationTree.path}`);
  pv.applicationTree.lastChangedCommit = commit;
  pv.inventoryDigest.value = sha256(sourcePaths.map((f) => `${f}\0${git('hash-object', '--', f)}\n`).join(''));
  if (pv.bootstrap) pinFile(pv.bootstrap);
  for (const record of pv.profiles || []) { pinFile(record); if ('basedOnCommit' in record) record.basedOnCommit = commit; }
  for (const record of pv.exclusionSupport || []) pinFile(record);
  return { raw, text: JSON.stringify(policy, null, 2) + (raw.endsWith('\n') ? '\n' : '') };
}

// Pins that differ, ignoring the commit bookkeeping fields that only record
// where a refresh happened.
function driftedPins(before, after) {
  const strip = (s) => JSON.parse(s, (k, v) => (['candidateCommit', 'lastChangedCommit', 'basedOnCommit'].includes(k) ? undefined : v));
  const a = strip(before).provenance, b = strip(after).provenance, out = [];
  (function walk(x, y, at) {
    if (x && typeof x === 'object') { for (const k of new Set([...Object.keys(x), ...Object.keys(y || {})])) walk(x[k], (y || {})[k], at ? at + '.' + k : k); }
    else if (x !== y) out.push(at);
  })(a, b, '');
  return out;
}

function main() {
  const args = process.argv.slice(2);
  const commit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: ROOT, encoding: 'utf8' }).trim();
  let result;
  try { result = refreshedPolicy(ROOT, commit); } catch (error) { console.error(error.message); process.exit(1); }
  const { raw, text } = result;
  if (args.includes('--check')) {
    const drift = driftedPins(raw, text);
    if (drift.length) { console.error('app-js coverage pins drifted: ' + drift.join(', ') + ' — run node scripts/nemo/refresh-app-pins.cjs'); process.exit(1); }
    console.log('app-js coverage pins current');
    return;
  }
  fs.writeFileSync(path.join(ROOT, POLICY), text);
  console.log(`app-js coverage pins refreshed for ${commit.slice(0, 8)}`);
}

if (require.main === module) main();
module.exports = { refreshedPolicy, driftedPins, POLICY };
