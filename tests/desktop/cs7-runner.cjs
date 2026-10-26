'use strict';
// Explicit installed declarations, one app lifetime, no source/issue-state inference.
const fs = require('node:fs');
const path = require('node:path');
const { isDeepStrictEqual } = require('node:util');
const { KINDS, RESULT_SCHEMA } = require('../../scripts/nemo/lib/family-acceptance.cjs');
const { requireCheck: check } = require('./native-harness.cjs');
const DECLARATION_SCHEMA = 'nemo.installed-parity/1';
const ID = /^[a-z][a-z0-9-]*$/;
const SHA = /^[a-f0-9]{40}$/;

class OperatorInputError extends Error {
  constructor(code, detail = {}) {
    super(code); this.code = code;
    this.command = /^[a-z][a-z0-9.-]*$/.test(detail.command || '') ? detail.command : null;
    this.ordinal = Number.isSafeInteger(detail.ordinal) && detail.ordinal >= 0 ? detail.ordinal : null;
  }
}

function readyRows(declarations, { taskIds, surfaceIds, scenarios }) {
  const all = [], ids = new Set();
  for (const declaration of declarations) {
    check(declaration?.schema === DECLARATION_SCHEMA && Array.isArray(declaration.rows), 'installed declaration required');
    for (const row of declaration.rows) {
      check(row && ID.test(row.id) && !ids.has(row.id), 'installed row id missing or duplicate');
      ids.add(row.id);
      check(['Ready', 'Blocked'].includes(row.status), 'installed readiness must be explicit');
      if (row.status === 'Blocked') check(typeof row.reason === 'string' && row.reason.trim(), 'Blocked installed row needs a reason');
      else check(typeof row.evidence === 'string' && /https:\/\/github\.com\/mysteropodes\/nemo\/pull\/[1-9][0-9]*/.test(row.evidence) &&
        /\b[a-f0-9]{40}\b/.test(row.evidence), 'Ready installed row needs cited source PR and SHA');
      check(Number.isSafeInteger(row.issue) && row.issue > 0, 'installed issue required');
      for (const [key, known] of [['tasks', taskIds], ['surfaces', surfaceIds]]) {
        check(Array.isArray(row[key]) && (key === 'surfaces' || row.status === 'Blocked' || row[key].length > 0) && new Set(row[key]).size === row[key].length &&
          row[key].every(id => known.has(id)), 'installed row references unknown or duplicate ' + key);
      }
      check(Array.isArray(row.dependsOn) && new Set(row.dependsOn).size === row.dependsOn.length, 'installed dependencies required');
      check(Array.isArray(row.checks) && row.checks.length > 0, 'installed assertions required');
      const checks = new Set();
      for (const c of row.checks) {
        check(c && ID.test(c.id) && !checks.has(c.id), 'installed assertion missing or duplicate');
        checks.add(c.id);
        check(typeof c.clause === 'string' && c.clause.trim(), 'installed original clause required');
        check(Array.isArray(c.covers) && c.covers.length > 0 && new Set(c.covers).size === c.covers.length &&
          c.covers.every(k => KINDS.includes(k)), 'installed assertion coverage invalid');
      }
      all.push(row);
    }
  }
  const byId = new Map(all.map(row => [row.id, row]));
  for (const row of all) for (const id of row.dependsOn) check(byId.has(id) && id !== row.id, 'installed dependency unknown or self');
  const ready = all.filter(row => row.status === 'Ready');
  check(ready.length > 0, 'no explicitly Ready installed rows');
  const ordered = [], active = new Set(), done = new Set();
  function visit(row) {
    if (done.has(row.id)) return;
    check(!active.has(row.id), 'installed dependency cycle'); active.add(row.id);
    for (const id of row.dependsOn) {
      const dependency = byId.get(id);
      check(dependency.status === 'Ready', 'Ready row has a Blocked dependency'); visit(dependency);
    }
    check(ID.test(row.scenario) && typeof scenarios[row.scenario]?.run === 'function', 'installed scenario unavailable');
    active.delete(row.id); done.add(row.id); ordered.push(structuredClone(row));
  }
  ready.forEach(visit);
  return ordered;
}

function readDeclarations(directory) {
  check(fs.lstatSync(directory).isDirectory() && fs.realpathSync(directory) === directory, 'installed declaration directory unavailable');
  return fs.readdirSync(directory).filter(name => name.endsWith('.json')).sort().map(name => {
    const file = path.join(directory, name), stat = fs.lstatSync(file);
    check(stat.isFile() && !stat.isSymbolicLink() && stat.size < 256 * 1024, 'installed declaration file invalid');
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  });
}

function verifiedIdentity(value) {
  check(value && SHA.test(value.sourceSha) && SHA.test(value.tree) && SHA.test(value.driverSha) &&
    /^[a-f0-9]{64}$/.test(value.executableSha256) && typeof value.sessionId === 'string' && value.sessionId.length > 0 &&
    Number.isSafeInteger(value.pid) && value.pid > 0 && value.dirty === false, 'installed candidate identity invalid');
  return structuredClone(value);
}

function rowResult(row, observations, status) {
  const byId = new Map(observations.map(c => [c.id, c]));
  check(byId.size === observations.length && observations.every(c => row.checks.some(d => d.id === c.id)), 'unexpected installed assertion');
  return {
    id: row.id, issue: row.issue, status, tasks: row.tasks, surfaces: row.surfaces,
    ...(row.acceptanceReference ? { acceptanceReference: row.acceptanceReference } : {}),
    checks: row.checks.map(c => {
      const result = byId.get(c.id);
      check(!result || ['pass', 'fail', 'unrun'].includes(result.status), 'invalid installed assertion outcome');
      return { id: c.id, clause: c.clause, status: result?.status || 'unrun', covers: c.covers };
    }),
  };
}

// The session supplies the existing native owner handshake and real input driver.
// A scenario asserts product behavior; this runner never equates posted input with PASS.
async function runInstalledRows({ rows, scenarios, session, now = () => new Date().toISOString(), save = async () => {} }) {
  check(rows.length > 0, 'empty installed batch refused');
  const results = rows.map(row => rowResult(row, [], 'unrun'));
  const finalizers = [];
  let identity, operatorInputFailures = 0, evidenceFailures = 0, cleanup = 'unrun', terminalCode = null, activeRow = null, failure = null;
  try {
    identity = verifiedIdentity(await session.start());
    for (let i = 0; i < rows.length; i++) {
      const row = rows[i];
      activeRow = row.id;
      if (row.dependsOn.some(id => results.find(r => r.id === id)?.status !== 'pass')) {
        results[i].status = 'dependency-blocked'; continue;
      }
      check(isDeepStrictEqual(verifiedIdentity(await session.identity()), identity), 'installed candidate changed');
      const observed = await scenarios[row.scenario].run({ session, row: structuredClone(row) });
      check(observed && Array.isArray(observed.checks) && typeof observed.safeToContinue === 'boolean', 'installed scenario returned no disposition');
      const result = rowResult(row, observed.checks, 'unrun');
      check(observed.checks.length === row.checks.length, 'installed scenario omitted assertions');
      result.status = result.checks.every(c => c.status === 'pass') ? 'pass' :
        result.checks.some(c => c.status === 'fail') ? 'fail' : 'incomplete';
      if (observed.finalize !== undefined) {
        check(typeof observed.finalize === 'function' && result.status !== 'pass', 'deferred evidence cannot claim an early pass');
        finalizers.push({ index: i, run: observed.finalize });
        if (result.status !== 'fail') result.status = 'pending-verification';
      }
      results[i] = result;
      check(isDeepStrictEqual(verifiedIdentity(await session.identity()), identity), 'installed candidate changed');
      await save({ identity, rows: results, cleanup, complete: false });
      if (!observed.safeToContinue) { terminalCode = 'scenario-stopped'; break; }
    }
  } catch (error) {
    // Error messages can contain private paths/tokens. Publish only the fixed classification.
    operatorInputFailures += 1;
    terminalCode = error instanceof OperatorInputError ? 'operator-input-failed' : 'installed-precondition-failed';
    failure = { row: activeRow, command: error instanceof OperatorInputError ? error.command : null,
      ordinal: error instanceof OperatorInputError ? error.ordinal : null };
  } finally {
    try { await session.cleanup(); cleanup = 'complete'; }
    catch { cleanup = 'incomplete'; terminalCode = 'cleanup-incomplete'; }
  }
  // Play's original oracle requires a sealed, retained copy after normal owner stop.
  // Never run an evidence finalizer while an app/helper may remain alive.
  if (cleanup === 'complete') for (const finalizer of finalizers) {
    const i = finalizer.index;
    try {
      const observed = await finalizer.run();
      check(Array.isArray(observed) && observed.length === rows[i].checks.length, 'final evidence omitted assertions');
      const result = rowResult(rows[i], observed, 'unrun');
      check(results[i].checks.every(c => c.status === 'unrun' || result.checks.find(r => r.id === c.id)?.status === c.status),
        'final evidence changed an observed outcome');
      result.status = result.checks.every(c => c.status === 'pass') ? 'pass' :
        result.checks.some(c => c.status === 'fail') ? 'fail' : 'incomplete';
      results[i] = result;
    } catch {
      evidenceFailures += 1;
      terminalCode ||= 'evidence-finalization-failed';
      failure ||= { row: rows[i].id, command: null, ordinal: null };
    }
  }
  const checks = results.flatMap(row => row.checks.map(c => ({
    id: row.id + '.' + c.id, target: 'installed-desktop', filter: row.id,
    passed: c.status === 'pass' ? 1 : 0, failed: c.status === 'fail' ? 1 : 0, ignored: c.status === 'unrun' ? 1 : 0,
  })));
  const covered = new Set(results.flatMap(r => r.checks.filter(c => c.status === 'pass').flatMap(c => c.covers)));
  const passed = results.every(r => r.status === 'pass') && operatorInputFailures === 0 && cleanup === 'complete' && terminalCode === null;
  const verdict = passed ? 'installed-pass' : checks.some(c => c.failed) || operatorInputFailures || cleanup === 'incomplete' ||
    evidenceFailures > 0 ? 'fail' : 'incomplete';
  const result = {
    // Family-result fields are preserved, but this is a distinct installed extension.
    // It must not enter the headless results/ census or acquire a source-pass verdict.
    schema: 'nemo.installed-family-result/1', format: RESULT_SCHEMA, family: 'installed-desktop',
    sourceSha: identity?.sourceSha || null, tree: identity?.tree || null, generatedAt: now(), checks,
    kindsCovered: KINDS.filter(k => covered.has(k)), kindsMissing: KINDS.filter(k => !covered.has(k)), verdict,
    tasks: [...new Set(rows.flatMap(r => r.tasks))],
    installed: { identity: identity || null, operatorInputFailures, evidenceFailures, cleanup, terminalCode, failure, rows: results },
  };
  await save(result);
  return result;
}

module.exports = { DECLARATION_SCHEMA, OperatorInputError, readyRows, readDeclarations, verifiedIdentity, runInstalledRows };
