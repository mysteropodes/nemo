'use strict';
// Freeze ratchet for rust.coverage.json (CS3/#2342). The policy file once gained
// one prose key per delivery; those keys stay (tests pin some), new ones are
// refused. A delivery's provenance belongs in its PR description and git history.
// The allow-list is a reviewed JSON beside the profile so that any change to it
// shows up in the diff. Removing a key is always allowed; policy-field edits to
// existing keys are untouched because only key names are compared.

const fs = require('node:fs');
const path = require('node:path');

const POLICY = 'engineering/boundaries/profiles/rust.coverage.json';
const FROZEN = 'engineering/boundaries/profiles/rust.coverage.frozen-keys.json';
const HINT = 'rust.coverage.json is frozen: record delivery provenance in the PR description (see CONTRIBUTING.md), not as a new key';

function readJson(root, file) {
  return JSON.parse(fs.readFileSync(path.join(root, file), 'utf8'));
}

function stringList(value, label) {
  if (!Array.isArray(value) || value.some((key) => typeof key !== 'string')) {
    throw new Error(`invalid ${FROZEN}: ${label} must be an array of strings`);
  }
  return new Set(value);
}

/** Compare the policy's key names with the frozen allow-list. Absent policy: null. */
function checkCoverageFreeze(root, opts = {}) {
  const policyPath = path.join(root, POLICY);
  if (opts.policy === undefined && !fs.existsSync(policyPath)) return null;
  const policy = opts.policy === undefined ? readJson(root, POLICY) : opts.policy;
  const frozen = opts.frozen === undefined ? readJson(root, FROZEN) : opts.frozen;
  const allowed = { '': stringList(frozen.topLevel, 'topLevel'), provenance: stringList(frozen.provenance, 'provenance') };
  const violations = [];
  const scan = (scope, object) => {
    for (const key of Object.keys(object || {}).sort()) {
      if (allowed[scope].has(key)) continue;
      const where = scope ? `${scope}.${key}` : key;
      violations.push({ rule: 'rust-coverage-frozen', file: POLICY, key: where, message: `${HINT} (new key: ${where})` });
    }
  };
  scan('', policy);
  if (policy.provenance && typeof policy.provenance === 'object') scan('provenance', policy.provenance);
  return { ok: violations.length === 0, violations };
}

module.exports = { checkCoverageFreeze, POLICY, FROZEN, HINT };
