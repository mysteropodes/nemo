'use strict';
// CS6/#2346 — the parity matrix gate and its negative controls.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const lib = require('./lib/parity-matrix.cjs');
const cli = require('./parity-matrix.cjs');

const tasks = [{ task: 'F09', outcome: 'C13', title: 'frames' }, { task: 'G43', outcome: 'D05', title: 'svg' }];
const baseline = { rows: [{ id: 'dom:#a', kind: 'dom' }, { id: 'dom:#b', kind: 'dom' }, { id: 'dom:#c', kind: 'dom' }] };
const config = { baseline: 'base', baselineStatus: 'proposed' };

test('the committed matrix is valid and up to date (`parity-matrix.cjs --check`)', () => {
  const { problems, files } = cli.outputs();
  assert.deepEqual(problems, []);
  for (const [name, content] of Object.entries(files)) {
    assert.equal(fs.readFileSync(path.join(cli.DIR, name), 'utf8'), content, name + ' is stale');
  }
});

test('validate rejects unknown surfaces, unknown tasks, empty mappings and duplicates', () => {
  const mapping = { surfaces: { 'dom:#zz': { tasks: ['F09'] }, 'dom:#a': { tasks: ['Q99'] }, 'dom:#b': {}, 'dom:#c': { disposition: ' ' } } };
  const problems = lib.validate({ tasks, baseline: { rows: [...baseline.rows, { id: 'dom:#a' }] }, mapping });
  for (const pattern of [/unknown baseline surface dom:#zz/, /unknown task Q99/, /dom:#b needs tasks or a disposition/,
    /dom:#c has an empty disposition/, /duplicate baseline surface dom:#a/]) {
    assert.ok(problems.some((p) => pattern.test(p)), pattern + ' in ' + problems.join(' | '));
  }
  assert.deepEqual(lib.validate({ tasks, baseline, mapping: { surfaces: { 'dom:#a': { tasks: ['F09'] } } } }), []);
});

test('issue task IDs come from title lists, ranges, NEMO prefixes and IDs lines; parents own nothing', () => {
  // Lane codes such as L05 are candidates here; ownersFromIssues keeps only known task IDs.
  assert.deepEqual(lib.issueTaskIds({ title: '[NEMO:L05-K] JS adapter (F09, F10–F12, F13)' }).sort(), ['F09', 'F10', 'F11', 'F12', 'F13', 'L05']);
  assert.deepEqual(lib.issueTaskIds({ title: '[NEMO:D01-G41A] fonts' }).sort(), ['D01', 'G41', 'G41a']);
  assert.deepEqual(lib.issueTaskIds({ title: 'x', body: '**IDs: G14, G22, G13.**' }).sort(), ['G13', 'G14', 'G22']);
  assert.deepEqual(lib.issueTaskIds({ title: 'mentions F09 in prose only' }), []);
  // Agent names collide with task IDs ("Author **D03**"); only an **IDs:** line declares tasks.
  assert.deepEqual(lib.issueTaskIds({ title: 'x', body: 'Author **D03**, reviewer **D02**' }), []);
  const owners = lib.ownersFromIssues([
    { number: 1, title: 'leaf (F09)', state: 'CLOSED', labels: [] },
    { number: 2, title: 'parent (F09, G43)', state: 'OPEN', labels: [{ name: 'remediation:parent' }] },
  ], ['F09', 'G43']);
  assert.deepEqual(owners, { F09: [{ issue: 1, state: 'closed' }] });
});

test('surface and task status follow mapping, ownership and closure', () => {
  const owners = { owners: { F09: [{ issue: 1, state: 'closed' }], G43: [{ issue: 2, state: 'open' }] } };
  const mapping = { surfaces: { 'dom:#a': { tasks: ['F09'] }, 'dom:#b': { tasks: ['F09', 'G43'] }, 'dom:#c': { disposition: 'baseline defect: x' } } };
  const m = lib.buildMatrix({ config, tasks, baseline, mapping, owners });
  assert.deepEqual(m.surfaces.map((s) => s.status), ['issues-closed', 'issues-open', 'disposition']);
  assert.deepEqual(m.tasks.map((t) => [t.task, t.status, t.surfaces]), [['F09', 'issues-closed', 2], ['G43', 'issues-open', 1]]);
  const none = lib.buildMatrix({ config, tasks, baseline, mapping: { surfaces: { 'dom:#a': { tasks: ['G43'] } } }, owners: { owners: {} } });
  assert.deepEqual(none.surfaces.map((s) => s.status), ['unowned', 'unmapped', 'unmapped']);
});

test('the CSV reader keeps quoted commas, quotes and newlines inside one field', () => {
  const rows = lib.parseCsv('\uFEFFtask,title\nF09,"a, ""b""\nc"\nG43,d\n');
  assert.deepEqual(rows, [{ task: 'F09', title: 'a, "b"\nc' }, { task: 'G43', title: 'd' }]);
});
