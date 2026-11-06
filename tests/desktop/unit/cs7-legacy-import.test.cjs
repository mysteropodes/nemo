'use strict';
// legacy-import (#2355): stub session, disposable copies of the repository's baseline fixtures; no app, helper or installed run.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createWorkflow, summarizeDoc, matches, expectedOf, CLAUSES, PLAN, SAVED } = require('../cs7-scenarios/legacy-import.cjs');
const { OperatorInputError } = require('../cs7-runner.cjs');
const { LegacyWorld, legacySession } = require('./cs7-i-stubs.cjs');

const row = extra => ({ id: 'legacy-import', issue: 2355, checks: CLAUSES.map(id => ({ id })), ...extra });
function setup(t, faults = {}) {
  const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-legacy-')));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const world = new LegacyWorld(directory, faults);
  return { directory, world, session: legacySession({ world }), workflow: createWorkflow({ reportDirectory: directory }) };
}
const statuses = result => Object.fromEntries(result.checks.map(c => [c.id, c.status]));
const FIXTURES = path.resolve(__dirname, '../../fixtures/legacy-import');

test('every declared clause passes from the file, raw reads and toast text, and the plan starts with the display placement', async t => {
  const { session, workflow, directory } = setup(t);
  assert.match(PLAN[0].how, /TOP of display 2: position 2880,-270, size 1620x1420/);
  const result = await workflow.run({ session, row: row() });
  assert.deepEqual(statuses(result), Object.fromEntries(CLAUSES.map(id => [id, 'pass'])));
  assert.equal(result.safeToContinue, true);
  const outcome = JSON.parse(fs.readFileSync(path.join(directory, 'legacy-import/outcome.json'), 'utf8'));
  assert.equal(outcome.reason, null);
});

test('the copied sources are the repository fixtures, and the two refusals add one unowned field each', async t => {
  const { session, workflow, directory } = setup(t);
  await workflow.run({ session, row: row() });
  const read = name => JSON.parse(fs.readFileSync(path.join(directory, 'legacy-source', name), 'utf8'));
  const animation = JSON.parse(fs.readFileSync(path.join(FIXTURES, 'animation.json'), 'utf8'));
  assert.deepEqual(read('animation.json'), animation);
  assert.deepEqual(read('media.json'), JSON.parse(fs.readFileSync(path.join(FIXTURES, 'media.json'), 'utf8')));
  assert.equal(read('refused-expr.json').exprGlobals, 'var a = 1;');
  assert.deepEqual(read('refused-roles.json').trackRoles, { r: { name: 'a' } });
  assert.deepEqual(expectedOf(animation), { frames: 12, names: animation.layers.map(l => l.name).sort(), comps: [6] });
});

test('a product failure fails its clause, stops, and leaves the rest unrun', async t => {
  for (const [faults, failed, tail] of [
    [{ wrongLayers: true }, 'open-as-new-document', 'first-save-save-as'],
    [{ wrongNotice: true }, 'open-as-new-document', 'first-save-save-as'],
    [{ silentSave: true }, 'first-save-save-as', 'edit-undo-redo'],             // Cmd+S saved without asking for a name
    [{ saveLegacyFormat: true }, 'first-save-save-as', 'edit-undo-redo'],       // the saved file is not a native project
    [{ savedOverOriginal: true }, 'first-save-save-as', 'edit-undo-redo'],      // Save As onto the original reported success
    [{ editInvisible: true }, 'edit-undo-redo', 'save-reopen'],
    [{ badUndo: true }, 'edit-undo-redo', 'save-reopen'],
    [{ saveNoEdit: true }, 'save-reopen', 'media-text-open'],
    [{ staleHistory: true }, 'save-reopen', 'media-text-open'],
    [{ touchSaved: true }, 'save-reopen', 'media-text-open'],
    [{ lostEditOnReopen: true }, 'save-reopen', 'media-text-open'],
    [{ refusalNoPath: true }, 'refused-field-path', null],
    [{ refusalReplacesDoc: true }, 'refused-field-path', null],
  ]) {
    const { session, workflow } = setup(t, faults);
    const result = await workflow.run({ session, row: row() });
    const s = statuses(result);
    assert.equal(s[failed], 'fail', JSON.stringify(faults));
    if (tail) assert.equal(s[tail], 'unrun', JSON.stringify(faults));
    assert.equal(s['original-untouched'], 'unrun', JSON.stringify(faults));
    assert.equal(result.safeToContinue, false);
  }
});

test('a changed baseline file fails original-untouched and the clause being judged at once', async t => {
  const { session, workflow } = setup(t, { overwriteOriginal: true });
  const result = await workflow.run({ session, row: row() });
  const s = statuses(result);
  assert.equal(s['original-untouched'], 'fail');
  assert.equal(s['first-save-save-as'], 'fail');
  assert.equal(s['edit-undo-redo'], 'unrun');
  assert.equal(result.safeToContinue, false);
});

test('toast text that cannot be read leaves the clauses that need it unrun, with the reason, and never passes them', async t => {
  const { session, workflow, directory } = setup(t, { toastHidden: true });
  const result = await workflow.run({ session, row: row() });
  const s = statuses(result);
  for (const id of ['open-as-new-document', 'first-save-save-as', 'media-text-open', 'refused-field-path']) assert.equal(s[id], 'unrun', id);
  for (const id of ['original-untouched', 'edit-undo-redo', 'save-reopen']) assert.equal(s[id], 'pass', id);
  assert.equal(result.safeToContinue, true);
  const outcome = JSON.parse(fs.readFileSync(path.join(directory, 'legacy-import/outcome.json'), 'utf8'));
  assert.match(outcome.reasons['first-save-save-as'], /refusal text for saving over the original was not read/);
  assert.match(outcome.reasons['refused-field-path'], /exprGlobals/);
});

test('a failure the operator reports is taken as stated, never softened', async t => {
  const { session, workflow } = setup(t, { failureAt: 'legacy-undo' });
  const s = statuses(await workflow.run({ session, row: row() }));
  assert.equal(s['first-save-save-as'], 'pass');
  assert.equal(s['edit-undo-redo'], 'fail');
  assert.equal(s['save-reopen'], 'unrun');
  const early = setup(t, { failureAt: 'open-animation' });
  const result = await early.workflow.run({ session: early.session, row: row() });
  assert.equal(statuses(result)['open-as-new-document'], 'fail');
  assert.equal(result.safeToContinue, false);
});

test('malformed or verdict-bearing evidence is an operator input error, never a pass or a product failure', async t => {
  let { session, workflow } = setup(t, { malformedAt: 'open-animation' });
  await assert.rejects(workflow.run({ session, row: row() }), error => error instanceof OperatorInputError && error.code === 'operator-evidence-malformed' &&
    error.command === 'legacy-import.open-animation');
  ({ session, workflow } = setup(t, { verdictAt: 'open-animation' }));
  await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed');
  for (const damage of [
    ev => { delete ev.reads.contexts; },
    ev => { ev.reads.layers.request.payload.selector.cursor = 5; },
    ev => { ev.reads.history.request.operation = 'command.document.object.undo'; },
    ev => { ev.reads.contexts.response.result.contextCount = 9; },
    ev => { ev.reads.layers.request.payload.atRevision = 7; },
  ]) {
    ({ session, workflow } = setup(t));
    const original = session.requestEvidence;
    session.requestEvidence = async request => { const ev = await original(request); if (request.phase === 'legacy-edit') damage(ev); return ev; };
    await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed');
  }
  // a save reply that does not name the agreed file, or a first save without the dialog fact
  for (const damage of [ev => { ev.savedFile = 'elsewhere.json'; }, ev => { delete ev.nameDialogOpened; }]) {
    ({ session, workflow } = setup(t));
    const original = session.requestEvidence;
    session.requestEvidence = async request => { const ev = await original(request); if (request.phase === 'save-first') damage(ev); return ev; };
    await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed');
  }
});

test('a missing or reordered declared clause is refused before any phase runs', async t => {
  const { session, workflow } = setup(t);
  let asked = 0; const ask = session.requestEvidence; session.requestEvidence = async r => { asked++; return ask(r); };
  await assert.rejects(workflow.run({ session, row: row({ checks: CLAUSES.slice(1).map(id => ({ id })) }) }), /clauses changed/);
  assert.equal(asked, 0);
  const again = setup(t);
  await assert.rejects(again.workflow.run({ session: again.session, row: row({ issue: 1 }) }), /clauses changed/);
  const used = setup(t);
  await used.workflow.run({ session: used.session, row: row() });
  await assert.rejects(used.workflow.run({ session: used.session, row: row() }), /cannot restart/);
});

test('summarizeDoc and matches read only complete, pinned component pages and compare with the file', () => {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-legacy-sum-')));
  try {
    const world = new LegacyWorld(dir);
    const json = JSON.parse(fs.readFileSync(path.join(FIXTURES, 'animation.json'), 'utf8'));
    world.newDocument(json);
    const good = world.reads(), summary = summarizeDoc(good);
    assert.ok(summary);
    assert.equal(matches(summary, expectedOf(json)), null);
    assert.equal(matches(summary, { ...expectedOf(json), frames: 13 }), 'scene-total-frames-differ-from-the-file');
    assert.equal(matches(summary, { ...expectedOf(json), comps: [] }), 'component-contexts-differ-from-the-file');
    assert.equal(matches(summary, { ...expectedOf(json), names: ['x'] }), 'scene-layer-count-differs-from-the-file');
    const why = {};
    const partial = structuredClone(good); partial.layers.response.result.nextCursor = 10;
    assert.equal(summarizeDoc(partial, why), null);
    assert.match(why.reason, /layers is not one complete pinned/);
    const other = structuredClone(good); other.contexts.response.documentId = 'doc-9';
    assert.equal(summarizeDoc(other), null);
    assert.equal(SAVED, 'saved/legacy-animation.json');
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
