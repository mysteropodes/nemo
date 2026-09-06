'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { checkSourceSizes } = require('./boundaries-size.cjs');
const { checkSourceCoverage } = require('./boundaries-coverage.cjs');
const { discoverSourcePaths } = require('./boundaries-discovery.cjs');

function invalid(message) { throw new Error(`invalid application coverage policy: ${message}`); }

function git(root, args) {
  return execFileSync('git', ['-C', root, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
}

function sha256(bytes) { return crypto.createHash('sha256').update(bytes).digest('hex'); }

function profilePaths(profile) {
  if (!profile || !Array.isArray(profile.modules)) invalid('profile.modules must be an array');
  return profile.modules.flatMap((module) => {
    if (!module || typeof module.id !== 'string' || typeof module.dir !== 'string' || !Array.isArray(module.files)) {
      invalid('every profile module must declare id, dir and files');
    }
    return module.files.map((file) => ({ path: path.posix.join(module.dir, file), moduleId: module.id }));
  }).sort((a, b) => a.path.localeCompare(b.path));
}

function assertUnique(records, label) {
  const seen = new Set();
  for (const record of records) {
    if (!record || typeof record.path !== 'string' || !record.path) invalid(`${label} contains an invalid path`);
    if (seen.has(record.path)) invalid(`${label} repeats ${record.path}`);
    seen.add(record.path);
  }
}

function verifyPinnedFile(root, record, label) {
  if (!record || typeof record.path !== 'string' || !record.path) invalid(`${label} path is missing`);
  if (typeof record.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(record.sha256)) invalid(`${label} ${record.path} SHA-256 pin is missing`);
  if (typeof record.gitBlob !== 'string' || !/^[a-f0-9]{40}$/.test(record.gitBlob)) invalid(`${label} ${record.path} Git blob pin is missing`);
  const absolute = path.join(root, record.path);
  let bytes;
  try { bytes = fs.readFileSync(absolute); } catch (error) { invalid(`${label} ${record.path} is unavailable (${error.code || 'read error'})`); }
  if (sha256(bytes) !== record.sha256) invalid(`${label} ${record.path} SHA-256 changed`);
  if (git(root, ['hash-object', '--', record.path]) !== record.gitBlob) invalid(`${label} ${record.path} Git blob changed`);
}

function checkApplicationPolicy(profile, policy, opts = {}) {
  const root = path.resolve(opts.root || '.');
  if (!policy || policy.schemaVersion !== 1 || policy.policyId !== 'nemo.app-js.coverage') invalid('unsupported schema or policyId');
  if (policy.status !== 'adopted') invalid('status must be adopted before standard enforcement');
  if (!policy.scope || !Array.isArray(policy.scope.sourceKinds) || typeof policy.scope.sourceRoot !== 'string') invalid('scope is incomplete');
  if (!Array.isArray(policy.retainedSources) || !Array.isArray(policy.exclusions)) invalid('retainedSources and exclusions must be arrays');
  assertUnique(policy.retainedSources, 'retainedSources');
  assertUnique(policy.exclusions, 'exclusions');

  const declared = profilePaths(profile);
  const retained = [...policy.retainedSources].sort((a, b) => a.path.localeCompare(b.path));
  if (declared.length !== retained.length || declared.some((entry, index) => entry.path !== retained[index].path || entry.moduleId !== retained[index].moduleId)) {
    invalid('retained paths and module IDs must exactly match the application profile');
  }

  const sourcePaths = discoverSourcePaths({ root, sourceRoots: [policy.scope.sourceRoot], extensions: policy.scope.sourceKinds });
  const exclusions = policy.exclusions.map((record) => record.path);
  const coverage = checkSourceCoverage(profile, { root, sourcePaths, exclusions });
  const counts = policy.snapshotCounts || {};
  if (counts.selectedSources !== sourcePaths.length || counts.retainedSources !== retained.length || counts.exclusions !== exclusions.length) {
    invalid('snapshot counts do not match fresh discovery');
  }

  const provenance = policy.provenance || {};
  if (git(root, ['rev-parse', `HEAD:${policy.scope.sourceRoot}`]) !== provenance.sourceRootTree) invalid('source root tree pin changed');
  if (!provenance.applicationTree || git(root, ['rev-parse', `HEAD:${provenance.applicationTree.path}`]) !== provenance.applicationTree.gitTree) {
    invalid('application tree pin changed');
  }
  verifyPinnedFile(root, provenance.bootstrap || {}, 'bootstrap');
  for (const record of provenance.profiles || []) verifyPinnedFile(root, record, 'profile');
  for (const record of provenance.exclusionSupport || []) verifyPinnedFile(root, record, 'exclusion support');
  for (const record of policy.exclusions) {
    if (!['vendor', 'generated'].includes(record.category)) invalid(`exclusion ${record.path} has unsupported category`);
    if (typeof record.component !== 'string' || !record.component.trim()) invalid(`exclusion ${record.path} component is missing`);
    if (typeof record.reason !== 'string' || !record.reason.trim()) invalid(`exclusion ${record.path} reason is missing`);
    if (!Array.isArray(record.evidence) || !record.evidence.length) invalid(`exclusion ${record.path} evidence is missing`);
    verifyPinnedFile(root, { ...(record.provenance || {}), path: record.path }, 'exclusion');
  }

  const inventory = sourcePaths.map((file) => `${file}\0${git(root, ['hash-object', '--', file])}\n`).join('');
  if (provenance.inventoryDigest?.algorithm !== 'sha256' || sha256(inventory) !== provenance.inventoryDigest.value) {
    invalid('fresh source inventory digest changed');
  }

  return { ok: coverage.ok, policyId: policy.policyId, sourcePathCount: sourcePaths.length,
    retainedPathCount: retained.length, excludedPathCount: exclusions.length, coverage };
}

function checkApplicationSize(profile, opts = {}) {
  return checkSourceSizes(profile, opts);
}

module.exports = { checkApplicationPolicy, checkApplicationSize };
