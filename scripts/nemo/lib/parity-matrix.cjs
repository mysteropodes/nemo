'use strict';
// CS6/#2346 — the parity denominator. Joins the frozen commencement-baseline
// surfaces (stable CS3 IDs) with the 228 original remediation tasks, a curated
// surface → task/capability/disposition mapping and a task → issue owner
// snapshot, and reports what is unmapped, owned, closed or dispositioned.
// Pure: every input is passed in, so the gate and its negative controls run on
// fixtures without git or GitHub.

const SCHEMA = 'nemo.parity-matrix/1';
const TASK_ID = /^[A-Z][0-9]{2}[a-z]?$/;
const CONFIDENCE = ['high', 'medium', 'low'];

// Minimal RFC 4180 reader: quoted fields may hold commas, newlines and "".
function parseCsv(text) {
  text = String(text).replace(/^\uFEFF/, '');
  const rows = [];
  let row = [], field = '', quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') { field += '"'; i++; }
      else if (c === '"') quoted = false;
      else field += c;
    } else if (c === '"') quoted = true;
    else if (c === ',') { row.push(field); field = ''; }
    else if (c === '\n' || c === '\r') {
      if (c === '\r' && text[i + 1] === '\n') i++;
      row.push(field); rows.push(row); row = []; field = '';
    } else field += c;
  }
  if (field || row.length) { row.push(field); rows.push(row); }
  const [head, ...body] = rows.filter((r) => r.some((v) => v !== ''));
  return body.map((r) => Object.fromEntries(head.map((h, i) => [h, r[i] || ''])));
}

function tasksFromCsv(text) {
  return parseCsv(text).map((r) => ({
    task: r.task, title: r.title, outcome: r.original_outcome,
    findings: (r.findings || '').split(',').map((s) => s.trim()).filter(Boolean),
    acceptance: r.original_acceptance,
  })).sort((a, b) => (a.task < b.task ? -1 : a.task > b.task ? 1 : 0));
}

// A baseline surface keeps only what identifies and describes it.
function baselineRow(r) {
  return { id: r.id, kind: r.kind, area: r.area, capability: r.capability,
    handler: (r.handler || []).slice(0, 2), source: r.source };
}

// Task IDs an issue declares: title parenthesis lists "(F09, F10)" and body
// "**IDs: …**" lines. Free text is not parsed, to avoid false owners.
function issueTaskIds(issue) {
  const out = new Set();
  const collect = (s) => {
    // Expand same-prefix ranges such as "I27–I34" or "D20-D24" before splitting.
    const text = String(s).replace(/\b([A-Z])([0-9]{2})\s*[–-]\s*\1([0-9]{2})\b/g, (m, p, a, b) => {
      const ids = []; for (let n = Number(a); n <= Number(b) && ids.length < 50; n++) ids.push(p + String(n).padStart(2, '0'));
      return ids.join(',');
    });
    for (const t of text.split(/[^A-Za-z0-9]+/)) if (TASK_ID.test(t)) out.add(t);
  };
  for (const m of String(issue.title || '').matchAll(/\(([^)]*)\)/g)) collect(m[1]);
  // "[NEMO:U03]", "[NEMO:D01-G41A]": prefix tokens, a part letter folding to its task.
  const prefix = /^\[NEMO:([^\]]+)\]/.exec(String(issue.title || ''));
  if (prefix) for (const tok of prefix[1].split(/[-:]/)) {
    const m = /^([A-Z][0-9]{2})([A-Za-z])?$/.exec(tok);
    if (m) { out.add(m[1]); if (m[2]) out.add(m[1] + m[2].toLowerCase()); }
  }
  for (const m of String(issue.body || '').matchAll(/\*\*IDs?:\s*([^*\n]+)\*\*/g)) collect(m[1]);
  return [...out];
}

function ownersFromIssues(issues, taskIds) {
  const known = new Set(taskIds), owners = {};
  for (const issue of issues) {
    // Tracking parents group work; only executable issues own a task.
    if ((issue.labels || []).some((l) => (l.name || l) === 'remediation:parent')) continue;
    for (const t of issueTaskIds(issue)) {
      if (!known.has(t)) continue;
      (owners[t] = owners[t] || []).push({ issue: issue.number, state: String(issue.state).toLowerCase() });
    }
  }
  for (const t of Object.keys(owners)) owners[t].sort((a, b) => a.issue - b.issue);
  return Object.fromEntries(Object.keys(owners).sort().map((t) => [t, owners[t]]));
}

// Curated ownership (ownership.json: task -> [{issue, confidence, evidence}]) covers
// issues that own a task without declaring its ID; the snapshot records their state.
function ownersSnapshot(issues, taskIds, snapshotAt, ownership) {
  const wanted = new Set(Object.values((ownership && ownership.tasks) || {}).flat().map((o) => o.issue));
  const states = {};
  for (const issue of issues) if (wanted.has(issue.number)) states[issue.number] = String(issue.state).toLowerCase();
  return { schema: 'nemo.parity-owners/1', snapshotAt, owners: ownersFromIssues(issues, taskIds),
    curatedStates: Object.fromEntries(Object.keys(states).sort((x, y) => x - y).map((k) => [k, states[k]])) };
}

function validate({ tasks, baseline, mapping, ownership, owners }) {
  const problems = [];
  const taskIds = new Set(tasks.map((t) => t.task));
  const surfaceIds = new Set();
  for (const t of tasks) if (!TASK_ID.test(t.task)) problems.push(`invalid task id ${t.task}`);
  if (taskIds.size !== tasks.length) problems.push('duplicate task id in tasks');
  for (const r of baseline.rows) {
    if (surfaceIds.has(r.id)) problems.push(`duplicate baseline surface ${r.id}`);
    surfaceIds.add(r.id);
  }
  for (const [id, m] of Object.entries(mapping.surfaces || {})) {
    if (!surfaceIds.has(id)) problems.push(`mapping names unknown baseline surface ${id}`);
    const ts = Array.isArray(m.tasks) ? m.tasks : [];
    for (const t of ts) if (!taskIds.has(t)) problems.push(`mapping for ${id} names unknown task ${t}`);
    if (!ts.length && !m.disposition && m.noTask !== true) problems.push(`mapping for ${id} needs tasks, a disposition or noTask`);
    if (m.disposition !== undefined && (typeof m.disposition !== 'string' || !m.disposition.trim())) {
      problems.push(`mapping for ${id} has an empty disposition`);
    }
    if (m.confidence !== undefined && !CONFIDENCE.includes(m.confidence)) problems.push(`mapping for ${id} has invalid confidence ${m.confidence}`);
    if (m.noTask === true && ts.length) problems.push(`mapping for ${id} sets noTask but lists tasks`);
  }
  for (const [task, list] of Object.entries((ownership && ownership.tasks) || {})) {
    if (!taskIds.has(task)) problems.push(`ownership names unknown task ${task}`);
    const seen = new Set();
    for (const o of Array.isArray(list) ? list : []) {
      if (!Number.isSafeInteger(o.issue) || o.issue <= 0) problems.push(`ownership for ${task} has an invalid issue`);
      if (seen.has(o.issue)) problems.push(`ownership for ${task} lists #${o.issue} twice`);
      seen.add(o.issue);
      if (!CONFIDENCE.includes(o.confidence)) problems.push(`ownership for ${task} #${o.issue} has invalid confidence`);
      if (typeof o.evidence !== 'string' || !o.evidence.trim()) problems.push(`ownership for ${task} #${o.issue} has no evidence`);
      if (owners && !(owners.curatedStates || {})[o.issue]) problems.push(`ownership for ${task} #${o.issue} has no state in the owners snapshot (refresh owners)`);
    }
  }
  return problems;
}

function surfaceStatus(m, owners) {
  if (!m) return 'unmapped';
  if (m.disposition) return 'disposition';
  // A baseline surface that no original task covers is a gap in the task contract.
  if (m.noTask === true && !(m.tasks || []).length) return 'no-task';
  const states = m.tasks.flatMap((t) => (owners[t] || []).map((o) => o.state));
  if (m.tasks.some((t) => !(owners[t] || []).length)) return 'unowned';
  // A task whose only owners are curated guesses is reported apart from declared ownership.
  const curated = m.tasks.some((t) => (owners[t] || []).every((o) => o.curated));
  const closed = states.every((s) => s === 'closed');
  return (curated ? 'curated-' : 'issues-') + (closed ? 'closed' : 'open');
}

function mergedOwners(snapshot, ownership) {
  const owners = JSON.parse(JSON.stringify(snapshot.owners || {}));
  for (const [task, list] of Object.entries((ownership && ownership.tasks) || {})) {
    for (const o of list) {
      if ((owners[task] || []).some((x) => x.issue === o.issue)) continue;
      (owners[task] = owners[task] || []).push({ issue: o.issue, state: (snapshot.curatedStates || {})[o.issue] || 'unknown', curated: o.confidence || 'low' });
    }
    if (owners[task]) owners[task].sort((x, y) => x.issue - y.issue);
  }
  return owners;
}

function buildMatrix({ config, tasks, baseline, mapping, owners: snapshot, ownership }) {
  const owners = mergedOwners(snapshot, ownership);
  const surfaces = baseline.rows.map((r) => {
    const m = (mapping.surfaces || {})[r.id];
    return { id: r.id, kind: r.kind, area: r.area, capability: r.capability,
      tasks: m ? (m.tasks || []) : [], capabilityId: m && m.capability || null,
      disposition: m && m.disposition || null, proposedDisposition: m && m.proposedDisposition || null,
      confidence: m && m.confidence || null, status: surfaceStatus(m, owners) };
  });
  const bySurfaceTask = new Map();
  for (const s of surfaces) for (const t of s.tasks) bySurfaceTask.set(t, (bySurfaceTask.get(t) || 0) + 1);
  const taskRows = tasks.map((t) => {
    const o = owners[t.task] || [];
    return { task: t.task, outcome: t.outcome, title: t.title, surfaces: bySurfaceTask.get(t.task) || 0,
      owners: o, status: !o.length ? 'unowned' : (o.every((x) => x.curated) ? 'curated-' : 'issues-') + (o.every((x) => x.state === 'closed') ? 'closed' : 'open') };
  });
  const count = (rows) => rows.reduce((acc, r) => { acc[r.status] = (acc[r.status] || 0) + 1; return acc; }, {});
  return { schema: SCHEMA, baseline: config.baseline, baselineStatus: config.baselineStatus, ownersSnapshotAt: snapshot.snapshotAt || null,
    counts: { surfaces: surfaces.length, tasks: taskRows.length, surfaceStatus: count(surfaces), taskStatus: count(taskRows),
      mappingConfidence: surfaces.reduce((acc, s) => { if (s.confidence) acc[s.confidence] = (acc[s.confidence] || 0) + 1; return acc; }, {}) },
    surfaces, tasks: taskRows };
}

function renderMarkdown(matrix) {
  const line = (o) => Object.entries(o).sort().map(([k, v]) => `${k} ${v}`).join(' · ');
  const byOutcome = {};
  for (const t of matrix.tasks) {
    const o = (byOutcome[t.outcome] = byOutcome[t.outcome] || { tasks: 0, unowned: [] });
    o.tasks++; if (t.status === 'unowned') o.unowned.push(t.task);
  }
  const out = [
    '# Parity matrix', '',
    `Generated by \`node scripts/nemo/parity-matrix.cjs\` (CS6/#2346). Baseline \`${matrix.baseline}\` (${matrix.baselineStatus}).`,
    'Do not edit by hand: curate `mapping.json`, refresh `owners.json`, then regenerate.',
    `Owners snapshot: ${matrix.ownersSnapshotAt || 'unknown'}. A *declared* owner is a non-parent issue declaring the task ID (title list or range, \`[NEMO:…]\` prefix or \`**IDs:**\` line); a *curated* owner comes from \`ownership.json\` (first-pass research). \`issues-*\` rows rest on declared owners, \`curated-*\` rows on at least one curated-only task. \`*-closed\` means every owning issue is closed, NOT that the task is accepted; acceptance is matrix rows green at a candidate. \`no-task\` marks a baseline surface no original task covers. The mapping itself is a first-pass curation: medium/low rows need human review.`, '',
    `- Surfaces: ${matrix.counts.surfaces} (${line(matrix.counts.surfaceStatus)})`,
    `- Tasks: ${matrix.counts.tasks} (${line(matrix.counts.taskStatus)})`,
    `- Mapping confidence: ${line(matrix.counts.mappingConfidence) || 'none recorded'} (medium/low rows need human review)`, '',
    '| Outcome | Tasks | Tasks with no owning issue |', '|---|---:|---|',
    ...Object.keys(byOutcome).sort().map((k) => `| ${k} | ${byOutcome[k].tasks} | ${byOutcome[k].unowned.join(', ') || '—'} |`),
    '',
  ];
  return out.join('\n');
}

module.exports = { SCHEMA, parseCsv, tasksFromCsv, baselineRow, issueTaskIds, ownersFromIssues, ownersSnapshot, mergedOwners, validate, buildMatrix, renderMarkdown };
