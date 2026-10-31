'use strict';
// CS1b-a (#2363) — the Tauri raw object client allowlist is DERIVED from the
// capability declarations, not hand-listed beside them. A descriptor flags an
// operation with `rawClient: { "<op>": { revisionDelta } }`; the set of flagged
// operations must equal `OPERATIONS` in src-tauri/src/native_object_client.rs,
// in both directions, and the value shape is held by capability-v2.schema.json.
// Split from capability-drift.cjs to keep both inside the 400-line hard max;
// same module (nemo.lib.capabilityDrift). MCP pre-validation as a trust boundary
// is an open owner decision and is NOT touched here.
const fs = require('node:fs');
const path = require('node:path');
const { validate } = require('./capability-schema.cjs');

const CLIENT_RS = ['src-tauri', 'src', 'native_object_client.rs'];

// Operations declared by the Rust allowlist, in source order. Also verifies the
// declared array length so a stale `[&str; N]` is its own finding.
function rustAllowlist(root) {
  const file = path.join(root, ...CLIENT_RS);
  if (!fs.existsSync(file)) return { error: `missing ${CLIENT_RS.join('/')}` };
  const m = /const\s+OPERATIONS\s*:\s*\[\s*&str\s*;\s*(\d+)\s*\]\s*=\s*\[([^\]]*)\]/.exec(fs.readFileSync(file, 'utf8'));
  if (!m) return { error: `${CLIENT_RS.join('/')}: cannot find the OPERATIONS const` };
  const ops = [...m[2].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
  return { ops, declaredLength: Number(m[1]) };
}

// descriptors: [{ name, d }] already parsed v2 declarations.
// Returns { problems, flagged: Map(op -> { name, revisionDelta }) }.
function flaggedOperations(descriptors, schema, transportOps) {
  const problems = [], flagged = new Map(), declared = new Map(), host = new Map();
  for (const { d } of descriptors) for (const op of d.operations || []) declared.set(op, d.id);
  for (const { name, d } of descriptors) {
    problems.push(...validate(schema, d).map((e) => `${name}: ${e}`));
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

function compareToAllowlist(flagged, allow) {
  const problems = [];
  if (allow.error) return [allow.error];
  if (allow.declaredLength !== allow.ops.length) problems.push(`OPERATIONS declares length ${allow.declaredLength} but lists ${allow.ops.length}`);
  const listed = new Set(allow.ops);
  if (listed.size !== allow.ops.length) problems.push('OPERATIONS lists an operation twice');
  const unflagged = allow.ops.filter((op) => !flagged.has(op));
  const unlisted = [...flagged.keys()].filter((op) => !listed.has(op)).sort();
  if (unflagged.length) problems.push(`OPERATIONS lists operation(s) with no rawClient flag in any descriptor: ${unflagged.join(', ')}`);
  if (unlisted.length) problems.push(`rawClient-flagged operation(s) missing from OPERATIONS: ${unlisted.join(', ')}`);
  return problems;
}

// Whole check over already-parsed inputs; the caller owns file discovery and
// turns the result into a pass/fail record.
function check(root, descriptors, schema, transport) {
  const union = transport ? new Set((transport.$defs.Request.oneOf || []).flatMap((b) => {
    const o = b.allOf && b.allOf[1] && b.allOf[1].properties && b.allOf[1].properties.operation;
    return !o ? [] : 'const' in o ? [o.const] : (o.enum || []);
  })) : null;
  const { problems, flagged } = flaggedOperations(descriptors, schema, union);
  problems.push(...compareToAllowlist(flagged, rustAllowlist(root)));
  return { problems, summary: `${flagged.size} rawClient operation(s) with revisionDelta, equal to OPERATIONS in ${CLIENT_RS.join('/')}` };
}

module.exports = { check, rustAllowlist, flaggedOperations, compareToAllowlist, CLIENT_RS };
