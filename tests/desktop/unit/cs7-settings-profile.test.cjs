'use strict';
// settings-profile (#2172): a disposable settings-dialog model answers the UI32 helper commands; no app,
// helper or installed run. Operator phases are stubs that edit the model, never a verdict.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createWorkflow, CLAUSES, TABS } = require('../cs7-scenarios/settings-profile.cjs');
const sync = require('../cs7-scenarios/settings-sync-owner.cjs');
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
  'profile-color-alpha': 'pass', 'shortcuts-reset': 'pass', 'sync-empty-folder': 'pass', 'sync-owner-paths': 'pass' };
const SYNC_PHASES = ['sync-document-setup', 'sync-publish-no-folder', 'sync-choose-folder', 'sync-publish', 'sync-check', 'sync-merge',
  'sync-publish-unwritable'];

test('every clause passes on a faithful stub, sync-owner-paths included, and the sync parts are all recorded as passes', async t => {
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
  assert.deepEqual(outcome.sync.parts, { 'no-folder-publish': 'pass', 'choose-folder': 'pass', publish: 'pass', check: 'pass', merge: 'pass', 'unwritable-publish': 'pass' });
  // The runner made the folder read-only for the failing Publish and gave it back, so the report directory is removable.
  assert.ok(fs.statSync(path.join(directory, 'settings-profile/shared', app.profileId)).mode & 0o200);
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
  assert.deepEqual(asked.map(a => a.phase), ['window-placement', 'new-project', 'shortcut-reset-first', 'shortcut-rebind', 'shortcut-reset',
    'profile-color-opaque', 'profile-color-alpha', 'profile-color-restore', ...SYNC_PHASES]);
  // Every operator phase is plain text a person can follow, and the folder phase names the exact folder.
  const choose = asked.find(a => a.phase === 'sync-choose-folder');
  assert.match(choose.instruction, /go to exactly \/.*shared/);
  assert.equal(choose.details.sharedFolder, fs.realpathSync(path.join(choose.details.reportDirectory, 'settings-profile/shared')));
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
    [{ fallbackSuffix: true }, 'profile-persist-restore'],      // the baseline fallback is exactly "Animateur"
    [{ chooseIgnored: true }, 'sync-owner-paths'],
    [{ publishNothing: true }, 'sync-owner-paths'],
    [{ publishWrongProfile: true }, 'sync-owner-paths'],
    [{ selfListed: true }, 'sync-owner-paths'],
    [{ checkEmpty: true }, 'sync-owner-paths'],
    [{ mergeNoop: true }, 'sync-owner-paths'],                  // toast says +1 ajout(s), the document read does not move
    [{ mergeWrongToast: true }, 'sync-owner-paths'],
    [{ failLies: true }, 'sync-owner-paths'],                   // a read-only folder, yet "Published for the team"
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
  assert.deepEqual([s['profile-role-guard'], s['sync-empty-folder'], s['shortcuts-reset'], s['profile-color-alpha'], s['sync-owner-paths']], ['unrun', 'unrun', 'unrun', 'unrun', 'unrun']);
});

test('what the first installed run may expose differently is unrun with a reason, never a guessed pass or fail', async t => {
  for (const [faults, reason] of [
    [{ toastHidden: true }, /no toast text|set up a folder|accessibility read/],   // no toast reaches the accessibility tree
    [{ listHidden: true }, /update list is not in the accessibility read/],
  ]) {
    const { session, workflow, directory } = setup(t, faults);
    const result = await workflow.run({ session, row: row() });
    const s = statuses(result);
    assert.equal(s['sync-owner-paths'], 'unrun', JSON.stringify(faults));
    assert.ok(Object.entries(s).filter(([id]) => id !== 'sync-owner-paths').every(([, v]) => v === 'pass'), JSON.stringify(faults));
    assert.equal(result.safeToContinue, true);
    const outcome = JSON.parse(fs.readFileSync(path.join(directory, 'settings-profile/outcome.json'), 'utf8'));
    assert.match(outcome.reasons['sync-owner-paths'], reason);
  }
});

test('the exact baseline wording is what the oracle looks for, and it matches the source', () => {
  const read = file => fs.readFileSync(path.join(__dirname, '../../../src/js', file), 'utf8');
  const i18n = read('i18n.js');
  for (const text of Object.values(sync.STR).flat()) assert.ok(i18n.includes("'" + text.replace(/'/g, "\\'") + "'") || i18n.includes('"' + text + '"'), text);
  assert.ok(read('adapters/settings-preferences-ui.js').includes(sync.NONE.replace(/'/g, "\\'")));
  assert.ok(read('adapters/settings-preferences-ui.js').includes("this.value||'Animateur'"));
  assert.ok(read('timeline.js').includes('Profil "Producteur" : lecture seule + commentaires'));
  assert.ok(read('project.js').includes("showToast('Fusion de '+entry.profileName+' : +'+report.added+' ajout(s)'"));
  assert.equal(sync.MERGE_TOAST, 'Fusion de CS7 Second Profile : +1 ajout(s)');
});

test('a Producer role with tool state exposed but no toast is judged from the tool buttons', async t => {
  const { session, workflow } = setup(t, { toastHidden: true });
  const result = await workflow.run({ session, row: row() });
  assert.equal(statuses(result)['profile-role-guard'], 'pass');
});

test('a Producer role with an unexposed tool state is judged from the literal baseline toast when it is fresh', async t => {
  const { session, workflow } = setup(t, { noToolState: true });
  assert.equal(statuses(await workflow.run({ session, row: row() }))['profile-role-guard'], 'pass');
});

test('the helper refusing the backdrop press leaves that clause unrun with the reason, keeping every other result', async t => {
  const { session, workflow, directory } = setup(t);
  const input = session.input;
  session.input = async (command, ...rest) => {
    if (command === 'settings.backdrop') throw new OperatorInputError('input-refused', { command, ordinal: 99 });
    return input(command, ...rest);
  };
  const result = await workflow.run({ session, row: row() });
  assert.deepEqual(statuses(result), { ...PASSING, 'settings-entries-tabs': 'unrun' });
  assert.equal(result.safeToContinue, false);
  const outcome = JSON.parse(fs.readFileSync(path.join(directory, 'settings-profile/outcome.json'), 'utf8'));
  assert.match(outcome.reasons['settings-entries-tabs'], /helper refused the backdrop press/);
});

test('a Producer role whose tool state is not exposed is unrun rather than a guessed pass', async t => {
  const { session, workflow, directory } = setup(t, { noToolState: true, noGuard: true });
  const result = await workflow.run({ session, row: row({}) });
  // With no toast and no selection state the guard cannot be judged; the whole run still completes.
  assert.equal(statuses(result)['profile-role-guard'], 'unrun');
  assert.equal(statuses(result)['profile-color-alpha'], 'pass');
  const outcome = JSON.parse(fs.readFileSync(path.join(directory, 'settings-profile/outcome.json'), 'utf8'));
  assert.match(outcome.reasons['profile-role-guard'], /do not expose their selected state and the baseline read-only toast was not read/);
});

test('reported failures stop the row; malformed evidence is an operator input error', async t => {
  let ctx = setup(t, { failureAt: 'shortcut-reset' });
  let result = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(result)['shortcuts-reset'], 'fail');
  ctx = setup(t, { failureAt: 'new-project' });
  result = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.ok(result.checks.every(c => c.status === 'unrun') && result.safeToContinue === false);
  ctx = setup(t, { verdictAt: 'profile-color-opaque' });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error instanceof OperatorInputError &&
    error.code === 'operator-evidence-malformed' && error.command === 'settings-profile.profile-color-opaque');
  ctx = setup(t, { malformedAt: 'sync-document-setup' });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error.code === 'operator-evidence-malformed' &&
    error.command === 'settings-profile.sync-document-setup');
  // The operator chose some other non-empty folder in the picker.
  ctx = setup(t);
  const orig = ctx.app.operator.bind(ctx.app);
  ctx.app.operator = async request => { const r = await orig(request); if (request.phase === 'sync-choose-folder') ctx.app.s.sync = os.tmpdir(); return r; };
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error.command === 'settings-profile.sync-choose-folder');
  // A reported failure in a sync phase is taken as stated.
  ctx = setup(t, { failureAt: 'sync-publish' });
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['sync-owner-paths'], 'fail');
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
    if (command === 'tool.hand') throw new OperatorInputError('input-refused', { command, ordinal: 99 });
    return input(command, ...rest);
  };
  await assert.rejects(workflow.run({ session, row: row() }), error => error instanceof OperatorInputError && error.command === 'tool.hand');
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

test('each sync fault fails for its own stated reason, and the folder is given back after the failing Publish', async t => {
  const reasonOf = async faults => {
    const { session, workflow, directory } = setup(t, faults);
    await workflow.run({ session, row: row() });
    return JSON.parse(fs.readFileSync(path.join(directory, 'settings-profile/outcome.json'), 'utf8')).reasons['sync-owner-paths'];
  };
  assert.match(await reasonOf({ chooseIgnored: true }), /folder field stayed empty/);
  assert.match(await reasonOf({ publishNothing: true }), /publish: _profile\.json is missing/);
  assert.match(await reasonOf({ publishWrongProfile: true }), /_profile\.json is missing or is not the publisher/);
  assert.match(await reasonOf({ selfListed: true }), /own published state/);
  assert.match(await reasonOf({ checkEmpty: true }), /reports no update although a second profile published/);
  assert.match(await reasonOf({ mergeNoop: true }), /still has no content on frame 4/);
  assert.match(await reasonOf({ mergeWrongToast: true }), /merge toast is not/);
  assert.match(await reasonOf({ failLies: true }), /reported success although nothing could be written/);
  assert.match(await reasonOf({ mergeNoRevision: true }), /without a new document revision/);
});

// ---- the findings of the installed-rows review ----------------------------------------------------------------
const readOutcome = directory => JSON.parse(fs.readFileSync(path.join(directory, 'settings-profile/outcome.json'), 'utf8'));

test('finding 8: a tab passes only when its own pane element is shown; another tab\'s pane fails, an unreadable pane stays unrun', async t => {
  // Before: only "the labels differ from the previous tab's" was asserted, so a tab showing the wrong pane passed.
  let ctx = setup(t, { wrongPane: 'Feedback' });
  let result = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(result)['settings-entries-tabs'], 'fail');
  assert.match(readOutcome(ctx.directory).reasons['settings-entries-tabs'], /tab Feedback showed the pane of General/);
  ctx = setup(t, { noAnchors: true });
  result = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(result)['settings-entries-tabs'], 'unrun');
  assert.match(readOutcome(ctx.directory).reasons['settings-entries-tabs'], /pane element of Updates, Feedback, Figma, Labs, About was not in the accessibility read/);
  assert.deepEqual(Object.entries(statuses(result)).filter(([id, v]) => id !== 'settings-entries-tabs' && v !== 'pass'), []);
});

test('finding 4: the empty-folder check passes only on the baseline completion text shown after the press', async t => {
  // Before: a Check that did nothing left the folder empty and no search text, which passed.
  let ctx = setup(t, { checkInert: true });
  let result = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(result)['sync-empty-folder'], 'unrun');
  assert.match(readOutcome(ctx.directory).reasons['sync-empty-folder'], /completion text of the empty-folder check was not read/);
  assert.equal(statuses(result)['shortcuts-reset'], 'pass');           // the run goes on
  ctx = setup(t, { stuckSearch: true });
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['sync-empty-folder'], 'fail');
  ctx = setup(t);
  const seen = []; const original = ctx.session.input;
  ctx.session.input = async (command, ...rest) => { const r = await original(command, ...rest); if (command === 'observe') seen.push(r.nodes); return r; };
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['sync-empty-folder'], 'pass');
});

test('finding 9: the Shortcuts baseline is taken after a Reset all, so a binding left by an earlier row is not blamed on the product', async t => {
  // Before: `defaults` was read from the live tab, so a leaked Duplicate keyframe binding made Reset all look broken.
  const { session, workflow, app } = setup(t, { leakedBinding: true });
  const result = await workflow.run({ session, row: row() });
  assert.equal(statuses(result)['shortcuts-reset'], 'pass');
  assert.equal(app.s.keys.duplicateKey, undefined);
  // A Reset all that does nothing is still a failure.
  const broken = setup(t, { leakedBinding: true, resetBroken: true });
  assert.equal(statuses(await broken.workflow.run({ session: broken.session, row: row() }))['shortcuts-reset'], 'fail');
});

test('finding 1: the swatch rectangle comes from its accessibility frame; without it the clause is unrun and no colour step is asked', async t => {
  const { session, workflow, directory } = setup(t, { noWellFrame: true });
  const asked = []; const original = session.requestEvidence;
  session.requestEvidence = request => { asked.push(request.phase); return original(request); };
  const result = await workflow.run({ session, row: row() });
  assert.equal(statuses(result)['profile-color-alpha'], 'unrun');
  assert.match(readOutcome(directory).reasons['profile-color-alpha'], /swatch has no accessibility frame/);
  assert.ok(!asked.some(p => p.startsWith('profile-color')));
  // The reply of a person cannot supply rectangles any more: a verdict-bearing colour reply is refused as before.
  assert.ok(!asked.includes('profile-color-measure'));
});

test('finding 1: each capture is hashed by the runner, and a swatch that moved between captures is not judged', async t => {
  const { session, workflow, directory, app } = setup(t);
  const sha = require('node:crypto');
  const result = await workflow.run({ session, row: row() });
  assert.equal(statuses(result)['profile-color-alpha'], 'pass');
  const shot = JSON.parse(fs.readFileSync(path.join(directory, 'settings-profile/shot-capture-12.json'), 'utf8'));
  assert.equal(shot.sha256, sha.createHash('sha256').update(fs.readFileSync(path.join(directory, 'capture-12.png'))).digest('hex'));
  assert.deepEqual(shot.well, { x: 50, y: 40, w: 24, h: 24 });
  // The swatch moves after the colour is set: the capture is not judged against the first capture's rectangle.
  const moving = setup(t); const inner = moving.app.nodes.bind(moving.app);
  moving.app.nodes = () => inner().map(n => (n.identifiers?.resolved === 'profile-color-well' && moving.app.s.colour[0] === 0x33 ? { ...n, frame: { ...n.frame, x: 120 } } : n));
  const r2 = await moving.workflow.run({ session: moving.session, row: row() });
  assert.equal(statuses(r2)['profile-color-alpha'], 'unrun');
  assert.match(readOutcome(moving.directory).reasons['profile-color-alpha'], /swatch moved/);
  void app;
});

test('finding 7: the merge passes only with a fresh toast, a later revision and an undisturbed history', async t => {
  // Before: content on frame 4 plus a Merged row passed with "toast not read", and no revision or history was asserted.
  let ctx = setup(t, { mergeNoToast: true });
  let result = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(result)['sync-owner-paths'], 'unrun');
  assert.match(readOutcome(ctx.directory).reasons['sync-owner-paths'], /merge toast was not read, so the content on frame 4 cannot be attributed/);
  assert.equal(readOutcome(ctx.directory).sync.parts.merge, 'unrun');
  ctx = setup(t, { mergeNoRevision: true });
  result = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(result)['sync-owner-paths'], 'fail');
  assert.match(readOutcome(ctx.directory).reasons['sync-owner-paths'], /without a new document revision/);
});

test('the second profile is derived from the snapshot the app published, in the shape the baseline merge reads', () => {
  const frames = n => Array.from({ length: n }, (_, i) => ({ isKeyframe: i === 0 || i === 3, strokes: i === 0 ? [{ strokeId: 's0', ownerId: 'me', segments: [] }] : [] }));
  const published = { layers: [{ name: 'Layer 1', frames: frames(6) }] };
  const out = sync.deriveSecondSnapshot(published, 'marker-1');
  assert.equal(out.snapshot.layers[0].frames[3].strokes[0].strokeId, 'marker-1');
  assert.equal(Object.hasOwn(out.snapshot.layers[0].frames[3].strokes[0], 'ownerId'), false);
  assert.equal(published.layers[0].frames[3].strokes.length, 0);           // the original is untouched
  assert.match(sync.deriveSecondSnapshot({ format: 'native' }, 'm').unrun, /no layers array/);
  assert.match(sync.deriveSecondSnapshot({ layers: [{ name: 'L', frames: [{ isKeyframe: true }] }] }, 'm').unrun, /no strokes field/);
  assert.match(sync.deriveSecondSnapshot({ layers: [{ name: 'L', frames: [{ strokes: [{ strokeId: 'a' }] }, {}, {}, { strokes: [] }] }] }, 'm').unrun, /no isKeyframe field/);
  assert.match(sync.deriveSecondSnapshot({ layers: [{ name: 'L', frames: frames(6).map(f => ({ ...f, strokes: [] })) }] }, 'm').fail, /no layer with the stroke drawn/);
  const notBlank = frames(6); notBlank[3].isKeyframe = false;
  assert.match(sync.deriveSecondSnapshot({ layers: [{ name: 'L', frames: notBlank }] }, 'm').fail, /frame 4 as a blank keyframe/);
  assert.match(sync.deriveSecondSnapshot({ layers: [{ name: 'L', frames: frames(6).map((f, i) => i ? f : { ...f, strokes: [{ segments: [] }] }) }] }, 'm').unrun, /no strokeId/);
});
