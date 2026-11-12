'use strict';
// The two declared rows against the real matrix and the real runner, with stub sessions only.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { readDeclarations, readyRows, selectRows, runInstalledRows, taskSurfaceIndex, checkTaskCoverage } = require('../cs7-runner.cjs');
const frame = require('../cs7-scenarios/frame-edit-ui.cjs');
const settings = require('../cs7-scenarios/settings-profile.cjs');
const draw = require('../cs7-scenarios/component-edit-draw.cjs');
const legacy = require('../cs7-scenarios/legacy-import.cjs');
const { World, frameSession, SettingsApp, settingsSession } = require('./cs7-h-stubs.cjs');
const { DrawWorld, drawSession, LegacyWorld, legacySession } = require('./cs7-i-stubs.cjs');

const ROOT = path.resolve(__dirname, '../../..');
const matrix = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/inventory/parity/matrix.json'), 'utf8'));
const declarations = readDeclarations(path.join(ROOT, 'engineering/inventory/parity/installed'));
const known = { taskIds: new Set(matrix.tasks.map(t => t.task)), surfaceIds: new Set(matrix.surfaces.map(s => s.id)), taskSurfaces: taskSurfaceIndex(matrix) };
const IDS = ['frame-edit-ui', 'settings-profile'];
const IDS3 = ['component-edit-draw', 'legacy-import'];
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
    assert.ok((r.tasks.length || Object.keys(r.partialTasks).length) && (r.surfaces.length + (r.operatorAttestedSurfaces || []).length));
  }
  assert.deepEqual(rows.find(r => r.id === 'frame-edit-ui').checks.map(c => c.id), frame.CLAUSES);
  assert.deepEqual(rows.find(r => r.id === 'settings-profile').checks.map(c => c.id), settings.CLAUSES);
  const scenarios = { 'frame-edit-ui': frame.prepare({ config: { reportDirectory: temp() } }), 'settings-profile': settings.prepare({ config: { reportDirectory: temp() } }) };
  const ordered = readyRows(declarations, { ...known, scenarios, only: new Set(IDS) });
  assert.deepEqual(ordered.map(r => r.id), IDS);
});

test('the rows load alone through the allow-list, leaving the three older Ready rows (and their private packets) out', () => {
  const selection = selectRows(declarations, IDS);
  assert.deepEqual(selection.omittedReady.sort(), ['component-edit-draw', 'legacy-import', 'play-resume', 'text-targets', 'text20']);
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

test('component-edit-draw and legacy-import are Ready with merged-PR evidence, existing tasks and surfaces, and load alone through the runner', () => {
  const rows = declarations.flatMap(d => d.rows).filter(r => IDS3.includes(r.id));
  assert.deepEqual(rows.map(r => r.id).sort(), IDS3);
  for (const r of rows) {
    assert.equal(r.status, 'Ready');
    assert.match(r.evidence, /https:\/\/github\.com\/mysteropodes\/nemo\/pull\/[1-9][0-9]*/);
    assert.match(r.evidence, /\b[a-f0-9]{40}\b/);
    assert.deepEqual(r.dependsOn, []);
    assert.ok((r.tasks.length || Object.keys(r.partialTasks).length) && (r.surfaces.length + (r.operatorAttestedSurfaces || []).length));
  }
  assert.equal(rows.find(r => r.id === 'component-edit-draw').issue, 2256);
  assert.equal(rows.find(r => r.id === 'legacy-import').issue, 2355);
  assert.deepEqual(rows.find(r => r.id === 'component-edit-draw').checks.map(c => c.id), draw.CLAUSES);
  assert.deepEqual(rows.find(r => r.id === 'legacy-import').checks.map(c => c.id), legacy.CLAUSES);
  const scenarios = { 'component-edit-draw': draw.prepare({ config: { reportDirectory: temp() } }), 'legacy-import': legacy.prepare({ config: { reportDirectory: temp() } }) };
  assert.deepEqual(readyRows(declarations, { ...known, scenarios, only: new Set(IDS3) }).map(r => r.id), IDS3);
  assert.deepEqual(selectRows(declarations, IDS3).omittedReady.sort(), ['frame-edit-ui', 'play-resume', 'settings-profile', 'text-targets', 'text20']);
});

test('component-edit-draw reaches installed-pass through the runner on a passing stub', async () => {
  const directory = temp(), world = new DrawWorld(directory), stub = drawSession({ world });
  const scenarios = { 'component-edit-draw': draw.prepare({ config: { reportDirectory: directory } }) };
  const rows = readyRows(declarations, { ...known, scenarios, only: new Set(['component-edit-draw']) });
  const result = await runInstalledRows({ rows, scenarios, session: stub });
  assert.equal(result.verdict, 'installed-pass');
  assert.deepEqual(result.installed.rows[0].checks.map(c => c.status), Array(draw.CLAUSES.length).fill('pass'));
  assert.deepEqual(result.kindsMissing.filter(k => ['pixels', 'undo-redo', 'save-reopen'].includes(k)), []);
});

test('legacy-import reaches installed-pass through the runner on a passing stub', async () => {
  const directory = temp(), world = new LegacyWorld(directory), stub = legacySession({ world });
  const scenarios = { 'legacy-import': legacy.prepare({ config: { reportDirectory: directory } }) };
  const rows = readyRows(declarations, { ...known, scenarios, only: new Set(['legacy-import']) });
  const result = await runInstalledRows({ rows, scenarios, session: stub });
  assert.equal(result.verdict, 'installed-pass');
  assert.deepEqual(result.installed.rows[0].checks.map(c => c.status), Array(legacy.CLAUSES.length).fill('pass'));
});

test('an unreadable toast keeps the legacy-import receipt incomplete, and a drawing that never reaches the instance fails the draw receipt', async () => {
  let directory = temp(), stub = legacySession({ world: new LegacyWorld(directory, { toastHidden: true }) });
  let scenarios = { 'legacy-import': legacy.prepare({ config: { reportDirectory: directory } }) };
  let rows = readyRows(declarations, { ...known, scenarios, only: new Set(['legacy-import']) });
  let result = await runInstalledRows({ rows, scenarios, session: stub });
  assert.equal(result.verdict, 'incomplete');
  directory = temp(); stub = drawSession({ world: new DrawWorld(directory, { noInstance: true }) });
  scenarios = { 'component-edit-draw': draw.prepare({ config: { reportDirectory: directory } }) };
  rows = readyRows(declarations, { ...known, scenarios, only: new Set(['component-edit-draw']) });
  result = await runInstalledRows({ rows, scenarios, session: stub });
  assert.equal(result.verdict, 'fail');
  assert.equal(result.installed.terminalCode, 'scenario-stopped');
});

test('every shipped row lists as tasks only tasks whose matrix surfaces it fully exercises; the rest are partialTasks with their missing surfaces', () => {
  const rows = declarations.flatMap(d => d.rows);
  assert.ok(rows.length >= 9);
  for (const r of rows) {
    assert.doesNotThrow(() => checkTaskCoverage(r, known.taskSurfaces), r.id);
    for (const task of r.tasks) assert.deepEqual([...known.taskSurfaces.get(task)].filter(id => !r.surfaces.includes(id)), [], r.id + ' ' + task);
    for (const [task, missing] of Object.entries(r.partialTasks || {})) {
      assert.ok(missing.length > 0 && missing.every(id => known.taskSurfaces.get(task).has(id) && !r.surfaces.includes(id)), r.id + ' ' + task);
    }
  }
  const by = Object.fromEntries(rows.map(r => [r.id, r]));
  // The reviewed findings: these tasks were published as covered while their surfaces were not all exercised.
  // The input route (F-key, menu item) of frame-edit-ui is not observable, so none of its surfaces counts as exercised.
  assert.deepEqual(by['frame-edit-ui'].tasks, []);
  assert.deepEqual(by['frame-edit-ui'].surfaces, []);
  assert.deepEqual(Object.keys(by['frame-edit-ui'].partialTasks), ['F09', 'F10', 'F11', 'F12', 'F13']);
  assert.equal(by['frame-edit-ui'].partialTasks.F12.length, 8);
  assert.deepEqual(by['settings-profile'].tasks, []);
  assert.deepEqual(Object.keys(by['settings-profile'].partialTasks), ['U02', 'D06']);
  assert.deepEqual(by['legacy-import'].tasks, []);
  assert.deepEqual(by['legacy-import'].partialTasks, { F03: ['dom:#file-input'] });
  assert.deepEqual(by['component-edit-draw'].tasks, []);
  assert.deepEqual(Object.keys(by['component-edit-draw'].partialTasks).sort(), ['D08', 'D29', 'G15', 'I18']);
  // A declaration that lists a partly exercised task as covered, or hides a missing surface, is refused by the real runner.
  const lying = structuredClone(by['frame-edit-ui']); lying.tasks = ['F12']; delete lying.partialTasks.F12;
  assert.throws(() => checkTaskCoverage(lying, known.taskSurfaces), /task F12 without all its matrix surfaces/);
  const hiding = structuredClone(by['frame-edit-ui']); hiding.partialTasks.F12.pop();
  assert.throws(() => checkTaskCoverage(hiding, known.taskSurfaces), /differs from the matrix surfaces/);
});

test('finding 2: routes the harness cannot observe are operatorAttestedSurfaces, reported apart from the observed surfaces and counted toward no task', async () => {
  const rows = declarations.flatMap(d => d.rows), by = Object.fromEntries(rows.map(r => [r.id, r]));
  assert.equal(by['frame-edit-ui'].operatorAttestedSurfaces.length, 10);
  assert.ok(by['frame-edit-ui'].operatorAttestedSurfaces.every(id => /^(menu|shortcut):/.test(id)));
  assert.deepEqual(by['legacy-import'].operatorAttestedSurfaces, ['dom:#btn-open', 'dom:#btn-saveas']);
  assert.deepEqual(by['legacy-import'].operatorAttestedRoutes, ['keyboard:Cmd+S']);
  assert.ok(by['component-edit-draw'].operatorAttestedSurfaces.every(id => /^(shortcut|menu):|^dom:(tools-panel|shape-tool-stack)>/.test(id)));
  assert.deepEqual(by['component-edit-draw'].surfaces, ['dom:p-fillbrushmode-grp>button[data-value=above]', 'dom:app-mode-switch>button[data-mode=motion]', 'dom:app-mode-switch>button[data-mode=anim2d]']);
  for (const r of rows) assert.ok((r.operatorAttestedSurfaces || []).every(id => !r.surfaces.includes(id)), r.id);
  // The receipt carries them per row and in total, never inside `surfaces`.
  const directory = temp(), stub = frameSession({ directory, world: new World(directory) });
  const scenarios = { 'frame-edit-ui': frame.prepare({ config: { reportDirectory: directory } }) };
  const result = await runInstalledRows({ rows: readyRows(declarations, { ...known, scenarios, only: new Set(['frame-edit-ui']) }), scenarios, session: stub });
  assert.deepEqual(result.installed.rows[0].surfaces, []);
  assert.deepEqual(result.installed.rows[0].operatorAttestedSurfaces, by['frame-edit-ui'].operatorAttestedSurfaces);
  assert.deepEqual(result.operatorAttestedSurfaces, by['frame-edit-ui'].operatorAttestedSurfaces);
  assert.deepEqual(result.expected.operatorAttestedSurfaces, by['frame-edit-ui'].operatorAttestedSurfaces);
  assert.deepEqual(result.tasks, []);
  assert.deepEqual(Object.keys(result.partialTasks).sort(), ['F09', 'F10', 'F11', 'F12', 'F13']);
  // A declaration may not list one surface as both observed and attested, nor attest an unknown surface.
  const both = structuredClone(by['settings-profile']); both.operatorAttestedSurfaces = [both.surfaces[0]];
  const unknown = structuredClone(by['settings-profile']); unknown.operatorAttestedSurfaces = ['dom:#no-such-surface'];
  for (const bad of [both, unknown]) {
    assert.throws(() => readyRows([{ schema: 'nemo.installed-parity/1', rows: [bad] }], { ...known, scenarios: { 'settings-profile': { run() {} } } }), /operatorAttestedSurfaces/);
  }
});
