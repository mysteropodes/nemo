'use strict';
// CS1b (#2363) — the Tauri raw object client allowlist is GENERATED from the
// capability declarations, not hand-listed beside them. A descriptor flags an
// operation with `rawClient: { "<op>": { revisionDelta } }`; `generate` turns the
// flagged set into engineering/application/native-raw-client.json, which
// src-tauri/src/native_object_client.rs embeds with include_str!. The artifact is
// written by native-transport-schema.cjs --write and held fresh by its --check
// (the Tauri beforeBuildCommand), so a new capability needs no central Rust edit.
// Split from capability-drift.cjs to keep both inside the size limits; same
// module (nemo.lib.capabilityDrift).
const fs = require('node:fs');
const path = require('node:path');
const { validate } = require('./capability-schema.cjs');

const CLIENT_RS = ['src-tauri', 'src', 'native_object_client.rs'];
const ARTIFACT = 'engineering/application/native-raw-client.json';
const EMBED = '../../engineering/application/native-raw-client.json';

// The generated artifact as committed: { operations: [{ operation, revisionDelta }] }.
function artifactAllowlist(root) {
  const file = path.join(root, ARTIFACT);
  if (!fs.existsSync(file)) return { error: `missing ${ARTIFACT}; run node scripts/nemo/lib/native-transport-schema.cjs --write` };
  try {
    const rows = JSON.parse(fs.readFileSync(file, 'utf8')).operations;
    if (!Array.isArray(rows)) return { error: `${ARTIFACT}: operations is not an array` };
    return { ops: rows.map((r) => r.operation), deltas: new Map(rows.map((r) => [r.operation, r.revisionDelta])) };
  } catch (e) { return { error: `${ARTIFACT}: ${e.message}` }; }
}

// The Rust client must read the artifact and keep no operation list of its own.
function clientConsumesArtifact(root) {
  const file = path.join(root, ...CLIENT_RS);
  if (!fs.existsSync(file)) return [`missing ${CLIENT_RS.join('/')}`];
  const src = fs.readFileSync(file, 'utf8');
  const problems = [];
  if (!src.includes(`include_str!("${EMBED}")`)) problems.push(`${CLIENT_RS.join('/')} must embed the generated ${ARTIFACT}`);
  if (/OPERATIONS\s*:\s*\[/.test(src)) problems.push(`${CLIENT_RS.join('/')} keeps a hand-maintained OPERATIONS list`);
  return problems;
}

// descriptors: [{ name, d }] already parsed v2 declarations.
// Returns { problems, flagged: Map(op -> { name, revisionDelta }) }.
function flaggedOperations(descriptors, schema, transportOps) {
  const problems = [], flagged = new Map(), declared = new Map(), host = new Map();
  for (const { d } of descriptors) for (const op of d.operations || []) declared.set(op, d.id);
  for (const { name, d } of descriptors) {
    if (schema) problems.push(...validate(schema, d).map((e) => `${name}: ${e}`));
    if (!d.rawClient && !d.hostOperations) continue;
    for (const op of d.hostOperations || []) {
      if (declared.has(op)) problems.push(`${name}: host operation ${op} is also declared as a transport operation by ${declared.get(op)}`);
      else if (transportOps && transportOps.has(op)) problems.push(`${name}: host operation ${op} is in the v2 transport union`);
      if (host.has(op)) problems.push(`duplicate host operation ${op} (${host.get(op)}, ${name})`); else host.set(op, name);
    }
    for (const [op, meta] of Object.entries(d.rawClient || {})) {
      if (!(d.operations || []).includes(op) && !(d.hostOperations || []).includes(op)) {
        problems.push(`${name}: rawClient operation ${op} is in neither operations nor hostOperations`);
      }
      const delta = meta && meta.revisionDelta;
      if (op.startsWith('query.') && delta !== 0) problems.push(`${name}: read ${op} must declare revisionDelta 0`);
      if (op.startsWith('command.') && delta !== 1) problems.push(`${name}: command ${op} must declare revisionDelta 1`);
      if (flagged.has(op)) problems.push(`duplicate rawClient operation ${op} (${flagged.get(op).name}, ${name})`);
      else flagged.set(op, { name, revisionDelta: delta });
    }
  }
  return { problems, flagged };
}

// Diagnostic comparison of the committed artifact with the flagged set.
function compareToAllowlist(flagged, allow) {
  const problems = [];
  if (allow.error) return [allow.error];
  const listed = new Set(allow.ops);
  if (listed.size !== allow.ops.length) problems.push('native-raw-client.json lists an operation twice');
  const unflagged = allow.ops.filter((op) => !flagged.has(op));
  const unlisted = [...flagged.keys()].filter((op) => !listed.has(op)).sort();
  if (unflagged.length) problems.push(`native-raw-client.json lists operation(s) with no rawClient flag in any descriptor: ${unflagged.join(', ')}`);
  if (unlisted.length) problems.push(`rawClient-flagged operation(s) missing from native-raw-client.json (regenerate it): ${unlisted.join(', ')}`);
  for (const [op, meta] of flagged) {
    if (listed.has(op) && allow.deltas.get(op) !== meta.revisionDelta) problems.push(`native-raw-client.json revisionDelta for ${op} is stale (regenerate it)`);
  }
  return problems;
}

// Generator: descriptors -> the committed artifact text. Sorted by operation so
// the output does not depend on file names. Invariants are enforced here so a
// bad flag stops generation; the drift gate additionally schema-validates.
function generate(root) {
  const dir = path.join(root, 'engineering', 'application', 'capabilities-v2');
  const descriptors = fs.readdirSync(dir).filter((f) => f.endsWith('.json')).sort()
    .map((f) => ({ name: f, d: JSON.parse(fs.readFileSync(path.join(dir, f), 'utf8')) }));
  const { problems, flagged } = flaggedOperations(descriptors, null, null);
  if (problems.length) throw new Error(`native raw client: ${problems.join('; ')}`);
  const operations = [...flagged.keys()].sort().map((operation) => ({ operation, revisionDelta: flagged.get(operation).revisionDelta }));
  return JSON.stringify({ operations }, null, 2) + '\n';
}

// Whole check over already-parsed inputs; the caller owns file discovery and
// turns the result into a pass/fail record.
function check(root, descriptors, schema, transport) {
  const union = transport ? new Set((transport.$defs.Request.oneOf || []).flatMap((b) => {
    const o = b.allOf && b.allOf[1] && b.allOf[1].properties && b.allOf[1].properties.operation;
    return !o ? [] : 'const' in o ? [o.const] : (o.enum || []);
  })) : null;
  const { problems, flagged } = flaggedOperations(descriptors, schema, union);
  problems.push(...compareToAllowlist(flagged, artifactAllowlist(root)), ...clientConsumesArtifact(root));
  return { problems, summary: `${flagged.size} rawClient operation(s) with revisionDelta, equal to the generated ${ARTIFACT} embedded by ${CLIENT_RS.join('/')}` };
}

module.exports = { check, generate, artifactAllowlist, clientConsumesArtifact, flaggedOperations, compareToAllowlist, CLIENT_RS, ARTIFACT };
