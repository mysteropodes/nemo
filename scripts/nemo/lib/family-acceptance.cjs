'use strict';
// CS6/#2346 part 2 — per-family headless native acceptance. Pure helpers: the
// process runner is injected, so parsing, verdicts and validation run on fixtures
// without cargo. A family declaration lists the original task IDs and the native
// test checks; a result records what a run at one source SHA actually did.

const FAMILY_SCHEMA = 'nemo.parity-family/1';
const RESULT_SCHEMA = 'nemo.parity-family-result/1';
const KINDS = ['positive', 'oracle', 'undo-redo', 'save-reopen', 'pixels'];
const VERDICTS = ['source-pass', 'incomplete', 'fail'];
const SHA = /^[0-9a-f]{40}$/;
const SAFE = /^[A-Za-z0-9_:.,-]+$/;

// Every `test result:` line of one cargo run (one per test binary), summed.
function parseTestResults(text) {
  const lines = [...String(text).matchAll(/^test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored/gm)];
  const sum = (i) => lines.reduce((a, m) => a + Number(m[i]), 0);
  return { summaries: lines.length, passed: sum(2), failed: sum(3), ignored: sum(4) };
}

function cargoArgs(check) {
  return ['test', '--locked', '--manifest-path', 'native-engine/Cargo.toml', '--no-default-features',
    '--features', check.features, '--test', check.target, check.filter, '--', '--test-threads=1'];
}

// A check passes only when cargo exited 0, saw a summary, ran at least one test and none failed:
// a filter that matches nothing reports "0 passed" and must not count.
function runCheck(check, exec) {
  const r = exec('cargo', cargoArgs(check));
  const counts = parseTestResults(`${r.stdout || ''}\n${r.stderr || ''}`);
  const ok = r.status === 0 && counts.summaries > 0 && counts.passed > 0 && counts.failed === 0;
  return { id: check.id, target: check.target, filter: check.filter, passed: counts.passed, failed: counts.failed,
    ignored: counts.ignored, ok, exitCode: r.status };
}

function kindsOf(family, checkResults) {
  const passing = new Set(checkResults.filter((c) => c.ok).map((c) => c.id));
  const covered = new Set(family.checks.filter((c) => passing.has(c.id)).flatMap((c) => c.covers));
  return { kindsCovered: KINDS.filter((k) => covered.has(k)), kindsMissing: KINDS.filter((k) => !covered.has(k)) };
}

function verdictOf(checkResults, kindsMissing) {
  if (!checkResults.length || checkResults.some((c) => !c.ok)) return 'fail';
  return kindsMissing.length ? 'incomplete' : 'source-pass';
}

function gitState(exec) {
  const out = (args) => { const r = exec('git', args); return r.status === 0 ? String(r.stdout).trim() : ''; };
  return { sourceSha: out(['rev-parse', 'HEAD']), tree: out(['rev-parse', 'HEAD^{tree}']), dirty: out(['status', '--porcelain']) !== '' };
}

// Runs every check of a family and returns the result record (never throws on test failure).
function runFamily(family, { exec, now = () => new Date().toISOString() }) {
  const git = gitState(exec);
  const ran = family.checks.map((check) => runCheck(check, exec));
  const kinds = kindsOf(family, ran);
  return { dirty: git.dirty, result: { schema: RESULT_SCHEMA, family: family.family, sourceSha: git.sourceSha, tree: git.tree,
    generatedAt: now(), checks: ran.map(({ id, target, filter, passed, failed, ignored }) => ({ id, target, filter, passed, failed, ignored })),
    ...kinds, verdict: verdictOf(ran, kinds.kindsMissing), tasks: family.tasks.slice() }, ran };
}

function validateFamily(family, taskIds, label = 'family') {
  const problems = [];
  if (!family || family.schema !== FAMILY_SCHEMA) return [`${label}: schema must be ${FAMILY_SCHEMA}`];
  if (typeof family.family !== 'string' || !/^[a-z][a-z0-9-]*$/.test(family.family)) problems.push(`${label}: invalid family name`);
  if (!Array.isArray(family.tasks) || !family.tasks.length) problems.push(`${label}: tasks must be a non-empty list`);
  for (const t of family.tasks || []) if (!taskIds.has(t)) problems.push(`${label}: unknown task ${t}`);
  const ids = new Set();
  for (const c of Array.isArray(family.checks) ? family.checks : []) {
    if (!c || typeof c.id !== 'string' || ids.has(c.id)) problems.push(`${label}: check id missing or duplicate (${c && c.id})`);
    else ids.add(c.id);
    if (!c || c.kind !== 'native-test') problems.push(`${label}: check ${c && c.id} kind must be native-test`);
    for (const k of ['target', 'features', 'filter']) if (!c || typeof c[k] !== 'string' || !SAFE.test(c[k])) problems.push(`${label}: check ${c && c.id} has an invalid ${k}`);
    if (!c || !Array.isArray(c.covers) || !c.covers.length || c.covers.some((k) => !KINDS.includes(k))) problems.push(`${label}: check ${c && c.id} covers must be from ${KINDS.join('/')}`);
  }
  if (!ids.size) problems.push(`${label}: no checks declared`);
  return problems;
}

// A results file is a source-level record at its SHA; it is NOT required to match the current HEAD.
function validateResult(result, family, taskIds, label = 'result') {
  const problems = [];
  if (!result || result.schema !== RESULT_SCHEMA) return [`${label}: schema must be ${RESULT_SCHEMA}`];
  if (!family) return [`${label}: no families/${result.family}.json declares this family`];
  if (result.family !== family.family) problems.push(`${label}: family ${result.family} does not match its declaration`);
  for (const k of ['sourceSha', 'tree']) if (!SHA.test(result[k] || '')) problems.push(`${label}: ${k} must be a 40-hex SHA`);
  if (typeof result.generatedAt !== 'string' || Number.isNaN(Date.parse(result.generatedAt))) problems.push(`${label}: generatedAt is not a timestamp`);
  if (!VERDICTS.includes(result.verdict)) problems.push(`${label}: invalid verdict ${result.verdict}`);
  const declared = new Map((family.checks || []).map((c) => [c.id, c]));
  if (!Array.isArray(result.checks) || result.checks.length !== declared.size) problems.push(`${label}: checks do not match the declaration`);
  for (const c of Array.isArray(result.checks) ? result.checks : []) {
    const d = declared.get(c && c.id);
    if (!d) { problems.push(`${label}: unknown check ${c && c.id}`); continue; }
    if (c.target !== d.target || c.filter !== d.filter) problems.push(`${label}: check ${c.id} target/filter differ from the declaration`);
    for (const k of ['passed', 'failed', 'ignored']) if (!Number.isSafeInteger(c[k]) || c[k] < 0) problems.push(`${label}: check ${c.id} has an invalid ${k}`);
  }
  const ok = Array.isArray(result.checks) && result.checks.length > 0 && result.checks.every((c) => c.passed > 0 && c.failed === 0);
  const covered = new Set((family.checks || []).filter((d) => (result.checks || []).some((c) => c.id === d.id && c.passed > 0 && c.failed === 0)).flatMap((d) => d.covers));
  const missing = KINDS.filter((k) => !covered.has(k));
  if (JSON.stringify(result.kindsMissing) !== JSON.stringify(missing)) problems.push(`${label}: kindsMissing disagrees with the checks`);
  if (JSON.stringify(result.kindsCovered) !== JSON.stringify(KINDS.filter((k) => covered.has(k)))) problems.push(`${label}: kindsCovered disagrees with the checks`);
  if (result.verdict !== (ok ? (missing.length ? 'incomplete' : 'source-pass') : 'fail')) problems.push(`${label}: verdict disagrees with the checks`);
  if (JSON.stringify(result.tasks) !== JSON.stringify(family.tasks)) problems.push(`${label}: tasks differ from families/${family.family}.json`);
  for (const t of result.tasks || []) if (!taskIds.has(t)) problems.push(`${label}: unknown task ${t}`);
  return problems;
}

// Additive evidence per surface: results whose tasks intersect the surface's tasks.
function evidenceFor(surfaceTasks, results) {
  const hits = (results || []).filter((r) => r.tasks.some((t) => surfaceTasks.includes(t)))
    .map((r) => ({ family: r.family, verdict: r.verdict, sourceSha: r.sourceSha }));
  return hits.length ? hits.sort((a, b) => (a.family < b.family ? -1 : 1)) : null;
}

function evidenceCounts(surfaces) {
  const acc = {};
  for (const s of surfaces) for (const e of s.evidence || []) { acc[e.family] = acc[e.family] || {}; acc[e.family][e.verdict] = (acc[e.family][e.verdict] || 0) + 1; }
  return acc;
}

module.exports = { FAMILY_SCHEMA, RESULT_SCHEMA, KINDS, parseTestResults, cargoArgs, runCheck, kindsOf, verdictOf, gitState,
  runFamily, validateFamily, validateResult, evidenceFor, evidenceCounts };
