'use strict';
// The two declared rows against the real matrix and the real runner, with stub sessions only.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { readDeclarations, readyRows, selectRows, runInstalledRows } = require('../cs7-runner.cjs');
const frame = require('../cs7-scenarios/frame-edit-ui.cjs');
const settings = require('../cs7-scenarios/settings-profile.cjs');
const { World, frameSession, SettingsApp, settingsSession } = require('./cs7-h-stubs.cjs');

const ROOT = path.resolve(__dirname, '../../..');
const matrix = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/inventory/parity/matrix.json'), 'utf8'));
const declarations = readDeclarations(path.join(ROOT, 'engineering/inventory/parity/installed'));
const known = { taskIds: new Set(matrix.tasks.map(t => t.task)), surfaceIds: new Set(matrix.surfaces.map(s => s.id)) };
const IDS = ['frame-edit-ui', 'settings-profile'];
const dirs = [];
test.after(() => { for (const d of dirs) fs.rmSync(d, { recursive: true, force: true }); });
function temp() { const d = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-h-'))); dirs.push(d); return d; }

test('both rows are Ready with merged-PR evidence, existing tasks and surfaces, and load through the real runner', () => {
  const rows = declarations.flatMap(d => d.rows).filter(r => IDS.includes(r.id));
  assert.deepEqual(rows.map(r => r.id).sort(), IDS);
  for (const r of rows) {
    assert.equal(r.status, 'Ready');
    assert.match(r.evidence, /https:\/\/github\.com\/mysteropodes\/nemo\/pull\/[1-9][0-9]*/);
    assert.match(r.evidence, /\b[a-f0-9]{40}\b/);
    assert.deepEqual(r.dependsOn, []);
    assert.ok(r.tasks.length && r.surfaces.length);
  }
  assert.deepEqual(rows.find(r => r.id === 'frame-edit-ui').checks.map(c => c.id), frame.CLAUSES);
  assert.deepEqual(rows.find(r => r.id === 'settings-profile').checks.map(c => c.id), settings.CLAUSES);
  const scenarios = { 'frame-edit-ui': frame.prepare({ config: { reportDirectory: temp() } }), 'settings-profile': settings.prepare({ config: { reportDirectory: temp() } }) };
  const ordered = readyRows(declarations, { ...known, scenarios, only: new Set(IDS) });
  assert.deepEqual(ordered.map(r => r.id), IDS);
});

test('the rows load alone through the allow-list, leaving the three older Ready rows (and their private packets) out', () => {
  const selection = selectRows(declarations, IDS);
  assert.deepEqual(selection.omittedReady.sort(), ['play-resume', 'text-targets', 'text20']);
});

test('frame-edit-ui reaches installed-pass through the runner on a passing stub', async () => {
  const directory = temp(), world = new World(directory), stub = frameSession({ directory, world });
  const rows = readyRows(declarations, { ...known, scenarios: { 'frame-edit-ui': frame.prepare({ config: { reportDirectory: directory } }) }, only: new Set(['frame-edit-ui']) });
  const result = await runInstalledRows({ rows, scenarios: { 'frame-edit-ui': frame.prepare({ config: { reportDirectory: directory } }) }, session: stub });
  assert.equal(result.verdict, 'installed-pass');
  assert.deepEqual(result.installed.rows[0].checks.map(c => c.status), Array(frame.CLAUSES.length).fill('pass'));
  assert.deepEqual(result.kindsMissing, []);
});

test('settings-profile reaches installed-pass through the runner on a passing stub, sync-owner-paths included', async () => {
  const directory = temp(), app = new SettingsApp(directory), stub = settingsSession({ app });
  const scenarios = { 'settings-profile': settings.createWorkflow({ reportDirectory: directory, sleep: async () => {} }) };
  const rows = readyRows(declarations, { ...known, scenarios, only: new Set(['settings-profile']) });
  const result = await runInstalledRows({ rows, scenarios, session: stub });
  assert.equal(result.verdict, 'installed-pass');
  assert.deepEqual(result.installed.rows[0].checks.map(c => c.status), Array(settings.CLAUSES.length).fill('pass'));
});

test('settings-profile cannot pass when the sync-owner clause is not provable: the receipt stays incomplete', async () => {
  const directory = temp(), app = new SettingsApp(directory, { toastHidden: true }), stub = settingsSession({ app });
  const scenarios = { 'settings-profile': settings.createWorkflow({ reportDirectory: directory, sleep: async () => {} }) };
  const rows = readyRows(declarations, { ...known, scenarios, only: new Set(['settings-profile']) });
  const result = await runInstalledRows({ rows, scenarios, session: stub });
  assert.equal(result.verdict, 'incomplete');
  const checks = Object.fromEntries(result.installed.rows[0].checks.map(c => [c.id, c.status]));
  assert.equal(checks['sync-owner-paths'], 'unrun');
  assert.ok(Object.entries(checks).filter(([id]) => id !== 'sync-owner-paths').every(([, s]) => s === 'pass'));
});

test('a product failure in a row keeps the receipt honest and the batch from continuing', async () => {
  const directory = temp(), world = new World(directory, { double: 'insertFrame' }), stub = frameSession({ directory, world });
  const scenarios = { 'frame-edit-ui': frame.prepare({ config: { reportDirectory: directory } }) };
  const rows = readyRows(declarations, { ...known, scenarios, only: new Set(['frame-edit-ui']) });
  const result = await runInstalledRows({ rows, scenarios, session: stub });
  assert.equal(result.verdict, 'fail');
  assert.equal(result.installed.terminalCode, 'scenario-stopped');
});
