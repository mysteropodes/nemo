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

// P10/#1012 — partition a checkProfile report by the edges policy.
//
// Enforced layers are the migrated slice: any finding there fails the gate.
// Legacy layers are the classic-script files whose relationships the lexical
// checker cannot resolve (script-tag globals carry no import edge), so their
// findings are counted, compared with a no-growth ceiling, and reported under
// `legacy` — explicitly labelled unresolved, never presented as a graph pass.
// Size warnings/violations are the size gate's business and are excluded here.
function validateEdgesPolicy(policy) {
  if (!policy || policy.schemaVersion !== 1 || policy.policyId !== 'nemo.app-js.edges') invalid('unsupported edges policy schema or policyId');
  if (policy.status !== 'adopted') invalid('edges policy status must be adopted before standard enforcement');
  for (const key of ['enforcedLayers', 'legacyLayers', 'enforcedRules']) {
    if (!Array.isArray(policy[key]) || !policy[key].length || policy[key].some((v) => typeof v !== 'string')) invalid(`edges policy ${key} must be a nonempty string array`);
  }
  if (policy.enforcedLayers.some((layer) => policy.legacyLayers.includes(layer))) invalid('a layer cannot be both enforced and legacy');
  if (!Number.isInteger(policy.legacyUnresolvedCeiling) || policy.legacyUnresolvedCeiling < 0) invalid('legacyUnresolvedCeiling must be a non-negative integer');
}

function checkApplicationEdges(profile, report, policy) {
  validateEdgesPolicy(policy);
  const layerOf = new Map(profile.modules.map((m) => [m.id, m.layer]));
  const declared = new Set([...policy.enforcedLayers, ...policy.legacyLayers]);
  const unclassified = [...new Set(profile.modules.map((m) => m.layer))].filter((layer) => !declared.has(layer)).sort();
  const isEdgeRule = (rule) => policy.enforcedRules.includes(rule);
  const enforced = { modules: 0, violations: [] };
  const legacy = { modules: 0, byRule: {}, total: 0 };
  for (const m of profile.modules) {
    if (policy.enforcedLayers.includes(m.layer)) enforced.modules++;
    else if (policy.legacyLayers.includes(m.layer)) legacy.modules++;
  }
  const other = [];
  for (const violation of report.violations) {
    if (!isEdgeRule(violation.rule)) continue;
    const layer = violation.module ? layerOf.get(violation.module) : null;
    if (violation.rule === 'expired-exception' || (layer && policy.enforcedLayers.includes(layer))) {
      enforced.violations.push(violation);
    } else if (layer && policy.legacyLayers.includes(layer)) {
      legacy.byRule[violation.rule] = (legacy.byRule[violation.rule] || 0) + 1;
      legacy.total++;
    } else other.push(violation);
  }
  const legacyOk = legacy.total <= policy.legacyUnresolvedCeiling;
  const problems = [];
  const expired = enforced.violations.filter((v) => v.rule === 'expired-exception').length;
  const inLayers = enforced.violations.length - expired;
  if (inLayers) problems.push(`${inLayers} boundary finding(s) in enforced layers ${policy.enforcedLayers.join('/')}`);
  // An expired exception is enforced whatever file it shielded (today: legacy size
  // waivers) — it must be renewed or retired, and is named as such, not as an edge.
  if (expired) problems.push(`${expired} expired exception(s) must be renewed or retired`);
  if (!legacyOk) problems.push(`legacy-unresolved findings grew to ${legacy.total} above the ceiling ${policy.legacyUnresolvedCeiling}`);
  if (unclassified.length) problems.push(`profile layers not classified by the edges policy: ${unclassified.join(', ')}`);
  if (other.length) problems.push(`${other.length} finding(s) could not be attributed to a classified layer`);
  return {
    ok: problems.length === 0,
    policyId: policy.policyId,
    problems,
    enforced: { layers: policy.enforcedLayers, modules: enforced.modules, violations: enforced.violations,
      analyzed: 'literal require/import edges, window.SM* globals, import cycles, expired exceptions' },
    legacy: { layers: policy.legacyLayers, modules: legacy.modules, findings: legacy.total, byRule: legacy.byRule,
      ceiling: policy.legacyUnresolvedCeiling, ok: legacyOk,
      unresolved: 'classic-script relationships through document-scope globals are not analyzed; this is a no-growth count, not dependency coverage' },
    unclassifiedLayers: unclassified,
    unattributed: other,
  };
}

module.exports = { checkApplicationPolicy, checkApplicationSize, checkApplicationEdges, validateEdgesPolicy };
