'use strict';
// Explicit installed declarations, one app lifetime, no source/issue-state inference.
const fs = require('node:fs');
const path = require('node:path');
const { isDeepStrictEqual } = require('node:util');
const same = isDeepStrictEqual;
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

// Optional allow-list from the private config. Refuses before any app launch; it never
// widens, reorders or auto-adds rows, so the unselected Ready rows stay visible as omitted.
function selectRows(declarations, requested) {
  check(Array.isArray(requested) && requested.length > 0 && requested.every(id => typeof id === 'string' && ID.test(id)) &&
    new Set(requested).size === requested.length, 'installed row selection must be a non-empty list of unique row ids');
  const known = new Map();
  for (const declaration of declarations) for (const row of declaration?.rows || []) if (row && !known.has(row.id)) known.set(row.id, row);
  const ids = new Set(requested);
  for (const id of requested) {
    const row = known.get(id);
    check(row, 'selected installed row unknown: ' + id);
    check(row.status === 'Ready', 'selected installed row is not Ready: ' + id);
    check(Array.isArray(row.dependsOn), 'installed dependencies required');
    for (const dependency of row.dependsOn) check(ids.has(dependency),
      'selected installed row ' + id + ' depends on a row outside the selection' + (ID.test(dependency) ? ': ' + dependency : ''));
  }
  return { ids, omittedReady: [...known.values()].filter(row => row.status === 'Ready' && !ids.has(row.id)).map(row => row.id) };
}

// task -> set of the matrix surfaces that name it. A task absent from every surface has an empty set.
function taskSurfaceIndex(matrix) {
  const index = new Map(matrix.tasks.map(t => [t.task, new Set()]));
  for (const surface of matrix.surfaces) for (const task of surface.tasks || []) index.get(task)?.add(surface.id);
  return index;
}

// A task listed in `tasks` must have EVERY one of its matrix surfaces in the row. A task the row exercises only in
// part goes in `partialTasks` ({task: [the matrix surfaces of that task the row does not exercise]}, exactly that
// set, never empty). The receipt publishes the two separately, so a passing row never reads as installed acceptance
// of a task whose surfaces it did not all exercise.
function checkTaskCoverage(row, taskSurfaces) {
  const surfaces = new Set(row.surfaces), partial = row.partialTasks === undefined ? {} : row.partialTasks;
  check(partial !== null && typeof partial === 'object' && !Array.isArray(partial), 'installed partialTasks must be an object');
  for (const task of row.tasks) {
    check(taskSurfaces.has(task), 'installed row references unknown tasks');
    const missing = [...taskSurfaces.get(task)].filter(id => !surfaces.has(id));
    check(missing.length === 0, 'installed row lists task ' + task + ' without all its matrix surfaces (' + missing.length + ' missing); list it in partialTasks');
  }
  for (const [task, listed] of Object.entries(partial)) {
    check(taskSurfaces.has(task) && !row.tasks.includes(task), 'installed partialTasks entry unknown or also in tasks');
    check(Array.isArray(listed) && listed.length > 0 && new Set(listed).size === listed.length, 'installed partialTasks entry must list its missing surfaces');
    const expected = [...taskSurfaces.get(task)].filter(id => !surfaces.has(id)).sort();
    check(same(listed.slice().sort(), expected), 'installed partialTasks entry for ' + task + ' differs from the matrix surfaces the row does not exercise');
  }
}

function readyRows(declarations, { taskIds, surfaceIds, taskSurfaces, scenarios, only }) {
  check(taskSurfaces instanceof Map, 'installed task coverage index required');
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
        check(Array.isArray(row[key]) && (key === 'surfaces' || row.status === 'Blocked' || row[key].length > 0 || Object.keys(row.partialTasks || {}).length > 0) &&
          new Set(row[key]).size === row[key].length && row[key].every(id => known.has(id)), 'installed row references unknown or duplicate ' + key);
      }
      checkTaskCoverage(row, taskSurfaces);
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
  const ready = all.filter(row => row.status === 'Ready' && (!only || only.has(row.id)));
  check(!only || ready.length === only.size, 'selected installed row missing or not Ready');
  check(ready.length > 0, 'no explicitly Ready installed rows');
  const ordered = [], active = new Set(), done = new Set();
  function visit(row) {
    if (done.has(row.id)) return;
    check(!active.has(row.id), 'installed dependency cycle'); active.add(row.id);
    for (const id of row.dependsOn) {
      const dependency = byId.get(id);
      check(dependency.status === 'Ready', 'Ready row has a Blocked dependency');
      check(!only || only.has(id), 'selected installed row depends on a row outside the selection'); visit(dependency);
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
    id: row.id, issue: row.issue, status, tasks: row.tasks, partialTasks: structuredClone(row.partialTasks || {}), surfaces: row.surfaces,
    ...(row.acceptanceReference ? { acceptanceReference: row.acceptanceReference } : {}),
    checks: row.checks.map(c => {
      const result = byId.get(c.id);
      check(!result || ['pass', 'fail', 'unrun'].includes(result.status), 'invalid installed assertion outcome');
      return { id: c.id, clause: c.clause, status: result?.status || 'unrun', covers: c.covers };
    }),
  };
}

// `tasks`: matrix tasks whose every surface a run row exercises. `partialTasks`: {task: surfaces no row of the run
// exercises}, for tasks only partly exercised. A task is never in both. When several rows list the same task as
// partial, only the surfaces missing from ALL of them are published.
function publishedTasks(rows) {
  const tasks = [...new Set(rows.flatMap(r => r.tasks))], partial = {};
  for (const row of rows) for (const [task, missing] of Object.entries(row.partialTasks || {})) {
    partial[task] = task in partial ? partial[task].filter(id => missing.includes(id)) : [...missing];
  }
  for (const task of Object.keys(partial)) if (tasks.includes(task) || partial[task].length === 0) delete partial[task];
  return { tasks, partialTasks: partial };
}

// The session supplies the existing native owner handshake and real input driver.
// A scenario asserts product behavior; this runner never equates posted input with PASS.
async function runInstalledRows({ rows, scenarios, session, now = () => new Date().toISOString(), save = async () => {}, selection }) {
  check(rows.length > 0, 'empty installed batch refused');
  // Present only for a config `rows` allow-list. A passing run that left Ready rows out gets the distinct verdict
  // `installed-subset-pass` (never `installed-pass`), and the receipt says which Ready rows were not asked for.
  // Absent otherwise, so the whole-census receipt is unchanged.
  const selected = selection ? { scope: selection.omittedReady.length ? 'subset' : 'all-ready',
    selected: rows.map(r => r.id), omittedReady: [...selection.omittedReady] } : null;
  const results = rows.map(row => rowResult(row, [], 'unrun'));
  const finalizers = [], recorded = new Set();
  let identity, operatorInputFailures = 0, evidenceFailures = 0, cleanup = 'unrun', terminalCode = null, activeRow = null, failure = null;
  function record(i, observed) {
    check(!recorded.has(i), 'installed row already recorded');
    check(observed && Array.isArray(observed.checks) && typeof observed.safeToContinue === 'boolean', 'installed scenario returned no disposition');
    const result = rowResult(rows[i], observed.checks, 'unrun');
    check(observed.checks.length === rows[i].checks.length, 'installed scenario omitted assertions');
    result.status = result.checks.every(c => c.status === 'pass') ? 'pass' :
      result.checks.some(c => c.status === 'fail') ? 'fail' : 'incomplete';
    if (observed.finalize !== undefined) {
      check(typeof observed.finalize === 'function' && result.status !== 'pass', 'deferred evidence cannot claim an early pass');
      finalizers.push({ index: i, run: observed.finalize });
      if (result.status !== 'fail') result.status = 'pending-verification';
    }
    results[i] = result; recorded.add(i);
  }
  try {
    identity = verifiedIdentity(await session.start());
    for (let i = 0; i < rows.length; i++) {
      if (recorded.has(i)) continue;
      const row = rows[i];
      activeRow = row.id;
      if (row.dependsOn.some(id => results.find(r => r.id === id)?.status !== 'pass')) {
        results[i].status = 'dependency-blocked'; continue;
      }
      check(isDeepStrictEqual(verifiedIdentity(await session.identity()), identity), 'installed candidate changed');
      const observed = await scenarios[row.scenario].run({ session, row: structuredClone(row) });
      record(i, observed);
      // A reviewed scenario may already have executed an entire later workflow.
      // Record that evidence before honoring its stop; never call another actor
      // merely to retrieve the failure, or replay the shared work on success.
      const shared = observed.sharedResults || [];
      check(Array.isArray(shared) && shared.length < rows.length, 'invalid shared installed results');
      let safeToContinue = observed.safeToContinue;
      for (const item of shared) {
        const index = rows.findIndex(r => r.id === item.rowId);
        check(index > i && !item.result?.sharedResults, 'shared installed result must name a later declared row');
        record(index, item.result);
        safeToContinue = safeToContinue && item.result.safeToContinue;
      }
      check(isDeepStrictEqual(verifiedIdentity(await session.identity()), identity), 'installed candidate changed');
      await save({ identity, ...(selected ? { selection: selected } : {}), rows: results, cleanup, complete: false });
      if (!safeToContinue) { terminalCode = 'scenario-stopped'; break; }
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
  // `installed-pass` is the issue #2348 outcome (every Ready row on one candidate). A pass of fewer rows is a different
  // verdict, so neither the verdict nor a process exit status can be read as that outcome.
  const verdict = passed ? (selected?.scope === 'subset' ? 'installed-subset-pass' : 'installed-pass') : checks.some(c => c.failed) || operatorInputFailures || cleanup === 'incomplete' ||
    evidenceFailures > 0 ? 'fail' : 'incomplete';
  const result = {
    // Family-result fields are preserved, but this is a distinct installed extension.
    // It must not enter the headless results/ census or acquire a source-pass verdict.
    schema: 'nemo.installed-family-result/1', format: RESULT_SCHEMA, family: 'installed-desktop',
    sourceSha: identity?.sourceSha || null, tree: identity?.tree || null, generatedAt: now(), checks,
    kindsCovered: KINDS.filter(k => covered.has(k)), kindsMissing: KINDS.filter(k => !covered.has(k)), verdict,
    ...(selected ? { selection: selected } : {}),
    ...publishedTasks(rows),
    installed: { identity: identity || null, operatorInputFailures, evidenceFailures, cleanup, terminalCode, failure, rows: results },
  };
  await save(result);
  return result;
}

module.exports = { DECLARATION_SCHEMA, OperatorInputError, selectRows, taskSurfaceIndex, checkTaskCoverage, readyRows, readDeclarations, verifiedIdentity, runInstalledRows };
