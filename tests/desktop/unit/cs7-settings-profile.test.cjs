'use strict';
// settings-profile (#2172): a disposable settings-dialog model answers the UI32 helper commands; no app,
// helper or installed run. Operator phases are stubs that edit the model, never a verdict.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createWorkflow, CLAUSES, TABS } = require('../cs7-scenarios/settings-profile.cjs');
const { OperatorInputError } = require('../cs7-runner.cjs');
const { SettingsApp, settingsSession } = require('./cs7-h-stubs.cjs');

const row = extra => ({ id: 'settings-profile', issue: 2172, checks: CLAUSES.map(id => ({ id })), ...extra });
function setup(t, faults = {}) {
  const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-settings-')));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const app = new SettingsApp(directory, faults);
  return { directory, app, session: settingsSession({ app }), workflow: createWorkflow({ reportDirectory: directory, sleep: async () => {} }) };
}
const statuses = result => Object.fromEntries(result.checks.map(c => [c.id, c.status]));
const PASSING = { 'settings-entries-tabs': 'pass', 'profile-persist-restore': 'pass', 'profile-role-guard': 'pass',
  'profile-color-alpha': 'pass', 'shortcuts-reset': 'pass', 'sync-empty-folder': 'pass', 'sync-owner-paths': 'unrun' };

test('every clause the helper and the stub operator can establish passes; sync-owner-paths stays unrun with its reason', async t => {
  const { session, workflow, app, directory } = setup(t);
  const result = await workflow.run({ session, row: row() });
  assert.deepEqual(statuses(result), PASSING);
  assert.equal(result.safeToContinue, true);
  // Everything the helper reaches went through session.input; the first helper command is the strict observation.
  assert.equal(app.log[0], 'observe');
  assert.deepEqual(TABS.filter(tab => !app.log.includes('settings.tab')), []);
  assert.ok(['settings.menu.open', 'settings.menu.entry', 'settings.open', 'settings.close', 'settings.backdrop', 'profile.name.restore',
    'profile.role.restore', 'sync.open-cancel', 'sync.check-empty', 'capture'].every(c => app.log.includes(c)));
  assert.equal(app.log.indexOf('settings.backdrop'), app.log.lastIndexOf('settings.backdrop'));
  const outcome = JSON.parse(fs.readFileSync(path.join(directory, 'settings-profile/outcome.json'), 'utf8'));
  assert.match(outcome.unrunReason['sync-owner-paths'], /shared folder/);
  // The profile is restored and the dialog is closed at the end.
  assert.deepEqual([app.s.name, app.s.email, app.s.role, app.s.open], ['Animateur 3I41', '', 'Animator', false]);
});

test('the first operator phase asks for the window at the top of display 2', async t => {
  const { session, workflow } = setup(t);
  const asked = [], original = session.requestEvidence;
  session.requestEvidence = request => { asked.push(request); return original(request); };
  await workflow.run({ session, row: row() });
  assert.equal(asked[0].phase, 'window-placement');
  assert.match(asked[0].instruction, /position 2880,-270, size 1620x1420/);
  assert.deepEqual(asked.map(a => a.phase), ['window-placement', 'new-project', 'shortcut-rebind', 'shortcut-reset', 'profile-color-measure',
    'profile-color-opaque', 'profile-color-alpha', 'profile-color-restore']);
});

test('product failures are judged from the raw accessibility facts and the helper capture', async t => {
  for (const [faults, failed] of [
    [{ closeInert: true }, 'settings-entries-tabs'],
    [{ staticTabs: true }, 'settings-entries-tabs'],
    [{ dropProfile: true }, 'profile-persist-restore'],        // the earlier installed U02-2 failure: reopen reads the original
    [{ noGuard: true }, 'profile-role-guard'],
    [{ stuckSearch: true }, 'sync-empty-folder'],
    [{ resetBroken: true }, 'shortcuts-reset'],
    [{ alphaIgnored: true }, 'profile-color-alpha'],
    [{ noRestore: true }, 'profile-color-alpha'],
    [{ backdropInert: true }, 'settings-entries-tabs'],
  ]) {
    const { session, workflow } = setup(t, faults);
    const result = await workflow.run({ session, row: row() });
    assert.equal(statuses(result)[failed], 'fail', JSON.stringify(faults));
    assert.equal(result.safeToContinue, false, JSON.stringify(faults));
  }
});

test('later clauses stay unrun after a failure', async t => {
  const { session, workflow } = setup(t, { dropProfile: true });
  const s = statuses(await workflow.run({ session, row: row() }));
  assert.equal(s['settings-entries-tabs'], 'unrun');   // its backdrop step comes last
  assert.deepEqual([s['profile-role-guard'], s['sync-empty-folder'], s['shortcuts-reset'], s['profile-color-alpha']], ['unrun', 'unrun', 'unrun', 'unrun']);
});

test('a Producer role whose tool state is not exposed is unrun rather than a guessed pass', async t => {
  const { session, workflow, directory } = setup(t, { noToolState: true, noGuard: true });
  const result = await workflow.run({ session, row: row({}) });
  // With no toast and no selection state the guard cannot be judged; the whole run still completes.
  assert.equal(statuses(result)['profile-role-guard'], 'unrun');
  assert.equal(statuses(result)['profile-color-alpha'], 'pass');
  const outcome = JSON.parse(fs.readFileSync(path.join(directory, 'settings-profile/outcome.json'), 'utf8'));
  assert.match(outcome.reasons['profile-role-guard'], /neither the read-only toast nor tool selection state/);
});

test('reported failures stop the row; malformed evidence is an operator input error', async t => {
  let ctx = setup(t, { failureAt: 'shortcut-reset' });
  let result = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(result)['shortcuts-reset'], 'fail');
  ctx = setup(t, { failureAt: 'new-project' });
  result = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.ok(result.checks.every(c => c.status === 'unrun') && result.safeToContinue === false);
  ctx = setup(t, { malformedAt: 'profile-color-measure' });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error instanceof OperatorInputError &&
    error.code === 'operator-evidence-malformed' && error.command === 'settings-profile.profile-color-measure');
  // A reply carrying a verdict, or no observation, is refused.
  ctx = setup(t);
  const original = ctx.session.requestEvidence;
  ctx.session.requestEvidence = async request => ({ ...(await original(request)), ...(request.phase === 'shortcut-rebind' ? { pass: true } : {}) });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error.code === 'operator-evidence-malformed');
  // The operator did not create a project: the gear is missing.
  ctx = setup(t);
  ctx.session.requestEvidence = async request => ({ observation: 'ok', productFailure: false, ...(request.phase === 'shortcut-rebind' ? {} : {}) });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error.command === 'settings-profile.new-project');
});

test('a helper refusal keeps what was established in a private partial file and still stops the batch', async t => {
  const { session, workflow, directory } = setup(t);
  const input = session.input;
  session.input = async (command, ...rest) => {
    if (command === 'settings.backdrop') throw new OperatorInputError('input-refused', { command, ordinal: 99 });
    return input(command, ...rest);
  };
  await assert.rejects(workflow.run({ session, row: row() }), error => error instanceof OperatorInputError && error.command === 'settings.backdrop');
  const partial = JSON.parse(fs.readFileSync(path.join(directory, 'settings-profile/partial.json'), 'utf8'));
  assert.equal(partial.outcome['profile-persist-restore'], 'pass');
  assert.equal(partial.outcome['settings-entries-tabs'], 'unrun');
  assert.equal(partial.stoppedBy, 'input-refused');
});

test('missing or changed clauses are refused before the helper is used', async t => {
  const { session, workflow, app } = setup(t);
  await assert.rejects(workflow.run({ session, row: row({ checks: CLAUSES.slice(0, -1).map(id => ({ id })) }) }), /clauses changed/);
  assert.deepEqual(app.log, []);
});
