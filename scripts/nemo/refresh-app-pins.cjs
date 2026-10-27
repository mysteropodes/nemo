#!/usr/bin/env node
'use strict';
// CS3/#2342 — regenerate the derived provenance pins of the application
// coverage policy (engineering/boundaries/profiles/app-js.coverage.json) so no
// delivery hand-edits them. The values are exactly what
// lib/boundaries-application.cjs checkApplicationPolicy verifies:
//   sourceRootTree / applicationTree.gitTree  `git rev-parse <commit>:<path>`
//   inventoryDigest                           sha256 over discovered sources + blob ids
//   bootstrap / profiles / exclusionSupport   sha256 + git blob of each pinned file
//   loadSites / bootstrapObservations         each tag's line/ordinal and the tag counts, read from the page
//                                             (CS3c/#2383: the body region is generated from the script manifest)
// Policy, membership, exclusions and thresholds stay human-owned and untouched.
//
//   node scripts/nemo/refresh-app-pins.cjs              rewrite pins for HEAD (commit the source first)
//   node scripts/nemo/refresh-app-pins.cjs --check      list drifted pins, exit 1 if any
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { discoverSourcePaths } = require('./lib/boundaries-discovery.cjs');
const { observeScripts } = require('./lib/script-manifest.cjs');

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
  derivePositions(root, policy);
  return { raw, text: JSON.stringify(policy, null, 2) + (raw.endsWith('\n') ? '\n' : '') };
}

// Positions are facts of the page, not policy: every retained source's index.html load site and the bootstrap
// tag counts are recomputed exactly as boundaries-application.test.cjs observes them.
function derivePositions(root, policy) {
  const observed = policy.bootstrapObservations;
  if (!observed) return;
  const html = fs.readFileSync(path.join(root, observed.path), 'utf8');
  const scripts = observeScripts(html, observed.path);
  const bySrc = new Map(scripts.filter((s) => s.src).map((s) => ['src/' + s.src, s]));
  const retained = new Map(policy.retainedSources.map((record) => [record.path, record]));
  for (const record of policy.retainedSources) {
    for (const site of record.loadSites || []) {
      const actual = site.path === observed.path && bySrc.get(record.path);
      if (actual) { site.line = actual.line; site.scriptOrdinal = actual.scriptOrdinal; }
    }
  }
  observed.totalScriptTags = scripts.length;
  observed.externalClassicTags = scripts.filter((s) => s.src && !s.module).length;
  observed.externalModuleTags = scripts.filter((s) => s.src && s.module).length;
  observed.inlineClassicTags = scripts.filter((s) => !s.src && !s.module).length;
  observed.retainedClassicTags = scripts.filter((s) => s.src && !s.module && retained.get('src/' + s.src)?.executionClass === 'document-classic').length;
  observed.retainedModuleTags = scripts.filter((s) => s.src && s.module && retained.get('src/' + s.src)?.executionClass === 'document-module').length;
  observed.classicTagsUnderSrcJsIncludingVendors = scripts.filter((s) => s.src?.startsWith('js/') && !s.module).length;
  observed.inlineScripts = scripts.filter((s) => !s.src).map(({ path: at, line, scriptOrdinal }) => ({ path: at, line, scriptOrdinal }));
}

// Pins that differ, ignoring the commit bookkeeping fields that only record
// where a refresh happened.
function driftedPins(before, after) {
  const strip = (s) => JSON.parse(s, (k, v) => (['candidateCommit', 'lastChangedCommit', 'basedOnCommit'].includes(k) ? undefined : v));
  const pick = (text) => { const p = strip(text); return { provenance: p.provenance, bootstrapObservations: p.bootstrapObservations,
    loadSites: (p.retainedSources || []).map((r) => [r.path, r.loadSites]) }; };
  const a = pick(before), b = pick(after), out = [];
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
