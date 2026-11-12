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
function setup(t, faults = {}, tamper) {
  const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-legacy-')));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const world = new LegacyWorld(directory, faults);
  if (tamper) world.tamper = (phase, pair) => tamper(phase, pair.request.operation, pair);
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
  assert.deepEqual(expectedOf(animation), { frames: 12, names: animation.layers.map(l => l.name).sort(), comps: [6],
    kinds: { Shapes: 'drawing', Child: 'drawing', 'Matte 2': 'drawing', Null: 'null', Guide: 'drawing', Folder: 'folder', Rig: 'widget', Dup: 'drawing', Comp: 'instance' } });
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

test('Save-again holds and stops when the accessibility read is unavailable', async t => {
  const { session, workflow, directory } = setup(t, { observeUnavailableAt: 'save-again' });
  const result = await workflow.run({ session, row: row() });
  const s = statuses(result);
  assert.equal(s['first-save-save-as'], 'pass');
  assert.equal(s['save-reopen'], 'unrun');
  assert.equal(s['media-text-open'], 'unrun');
  assert.equal(result.safeToContinue, false);
  const outcome = JSON.parse(fs.readFileSync(path.join(directory, 'legacy-import/outcome.json'), 'utf8'));
  assert.match(outcome.reason, /accessibility read was unavailable, so a repeat name dialog was not observed/);
  assert.match(outcome.reasons['save-reopen'], /accessibility read was unavailable/);
});

test('toast text that cannot be read leaves the clauses that need it unrun, with the reason, and never passes them', async t => {
  const { session, workflow, directory } = setup(t, { toastHidden: true });
  const result = await workflow.run({ session, row: row() });
  const s = statuses(result);
  for (const id of ['open-as-new-document', 'original-untouched', 'first-save-save-as', 'media-text-open', 'refused-field-path']) assert.equal(s[id], 'unrun', id);
  for (const id of ['edit-undo-redo', 'save-reopen']) assert.equal(s[id], 'pass', id);
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
  // The reply claims the file opened, but the harness' own read finds no open document: the batch stops, nothing passes.
  let { session, workflow, directory } = setup(t, { malformedAt: 'open-animation' });
  await assert.rejects(workflow.run({ session, row: row() }), /candidate has no open native document/);
  assert.match(JSON.parse(fs.readFileSync(path.join(directory, 'legacy-import/read-refused-open-animation.json'), 'utf8')).reason, /no open native document/);
  ({ session, workflow } = setup(t, { verdictAt: 'open-animation' }));
  await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed');
  // Attached reads are refused: the harness reads the document, the operator never attaches it.
  for (const key of ['reads', 'sceneLayers', 'kinds']) {
    ({ session, workflow } = setup(t));
    const original = session.requestEvidence;
    session.requestEvidence = async request => { const ev = await original(request); if (request.phase === 'legacy-edit') ev[key] = {}; return ev; };
    await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed' && error.command === 'legacy-import.legacy-edit', key);
  }
  for (const damage of [
    (op, pair) => { if (op === 'query.document.object.component.read' && pair.request.payload.selector.kind === 'contexts') pair.response.ok = false; },
    (op, pair) => { if (op === 'query.document.object.component.read' && pair.request.payload.selector.kind === 'layers') pair.response.result.cursor = 5; },
    (op, pair) => { if (op === 'query.document.object.history') pair.response.result.atRevision = 'x'; },
    (op, pair) => { if (op === 'query.document.object.component.read' && pair.request.payload.selector.kind === 'contexts') pair.response.result.contextCount = 9; },
    (op, pair) => { if (op === 'query.document.object.component.read' && pair.request.payload.selector.kind === 'layers') pair.response.result.atRevision = 7; },
  ]) {
    ({ session, workflow } = setup(t, {}, (phase, op, pair) => { if (phase === 'legacy-edit') damage(op, pair); }));
    await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed' || /not one complete page/.test(error.message));
  }
  // a save reply that does not name the agreed file, or a first save without the dialog fact
  for (const [phase, damage] of [['save-first-name', ev => { ev.savedFile = 'elsewhere.json'; }], ['save-first-press', ev => { delete ev.nameDialogOpened; }]]) {
    ({ session, workflow } = setup(t));
    const original = session.requestEvidence;
    session.requestEvidence = async request => { const ev = await original(request); if (request.phase === phase) damage(ev); return ev; };
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
    world.newDocument(json, expectedOf(json).kinds);
    const good = world.reads(), summary = summarizeDoc(good);
    assert.ok(summary);
    assert.equal(matches(summary, expectedOf(json)), null);
    assert.equal(matches(summary, { ...expectedOf(json), frames: 13 }), 'scene-total-frames-differ-from-the-file');
    assert.equal(matches(summary, { ...expectedOf(json), comps: [] }), 'component-contexts-differ-from-the-file');
    assert.equal(matches(summary, { ...expectedOf(json), names: ['x'] }), 'scene-layer-count-differs-from-the-file');
    assert.equal(matches(summary, { ...expectedOf(json), kinds: { ...expectedOf(json).kinds, Null: 'drawing' } }), 'scene-layer-kinds-differ-from-the-file');
    const why = {};
    const partial = structuredClone(good); partial.layers.response.result.nextCursor = 10;
    assert.equal(summarizeDoc(partial, why), null);
    assert.match(why.reason, /layers is not one complete pinned/);
    const other = structuredClone(good); other.contexts.response.documentId = 'doc-9';
    assert.equal(summarizeDoc(other), null);
    assert.equal(SAVED, 'saved/legacy-animation.json');
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

// ---- the findings of the installed-rows review ------------------------------------------------------------------
const outcomeOf = directory => JSON.parse(fs.readFileSync(path.join(directory, 'legacy-import/outcome.json'), 'utf8'));

test('P2813-7: original-untouched passes only when the open, the refused Save As and the refused opens were each seen', async t => {
  // Before: `!changed().length` at the end passed an operator who replied "fine" to every step without doing anything.
  const { session, workflow, directory } = setup(t, { silentRefusal: true });
  const result = await workflow.run({ session, row: row() });
  const s = statuses(result);
  assert.equal(s['original-untouched'], 'unrun');
  assert.equal(s['first-save-save-as'], 'unrun');
  assert.match(outcomeOf(directory).reasons['original-untouched'], /attempt was not observed for: save-original-refused/);
  // Nothing observed at all (no toast reaches the accessibility read): the files are unchanged and that proves nothing.
  const blind = setup(t, { toastHidden: true });
  await blind.workflow.run({ session: blind.session, row: row() });
  assert.match(outcomeOf(blind.directory).reasons['original-untouched'], /open-animation, open-media, save-original-refused, refuse-expr, refuse-roles/);
});

test('P2813-8: the open needs the native notice, rejects a lossy one, and compares layer kinds', async t => {
  // Before: any fresh text containing the base name passed, the extended "not carried over" notice passed, and kinds were not compared.
  let ctx = setup(t, { lossyNotice: true });
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['open-as-new-document'], 'fail');
  assert.match(outcomeOf(ctx.directory).reason, /open-notice-reports-items-not-carried-over/);
  ctx = setup(t, { wrongKinds: true });
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['open-as-new-document'], 'fail');
  assert.match(outcomeOf(ctx.directory).reason, /scene-layer-kinds-differ-from-the-file/);
  // A title that merely contains the base name is not the notice.
  ctx = setup(t, { toastHidden: true });
  const original = ctx.session.input;
  ctx.session.input = async command => { const r = await original(command); return { ...r, nodes: [{ role: 'AXStaticText', labels: ['animation.json'], value: 'animation.json' }] }; };
  const result = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(result)['open-as-new-document'], 'unrun');
  assert.match(outcomeOf(ctx.directory).reasons['open-as-new-document'], /open notice for animation was not read/);
});

test('P2813-9: the name dialog is looked for in the accessibility read, Save asks no name twice, and F5 is a held cell after frame 1', async t => {
  // Before: an operator boolean decided the dialog, save-again asserted nothing about a dialog, and only totalFrames +1 judged the edit.
  let ctx = setup(t, { dialogHidden: true });
  let result = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(result)['first-save-save-as'], 'unrun');
  assert.match(outcomeOf(ctx.directory).reasons['first-save-save-as'], /name dialog was reported but is not visible/);
  assert.equal(statuses(result)['save-reopen'], 'pass');
  ctx = setup(t, { askAgain: true });
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['save-reopen'], 'fail');
  assert.match(outcomeOf(ctx.directory).reason, /save-asked-for-a-name-again/);
  ctx = setup(t, { askAgainHidden: true });                // the operator denies a dialog but the accessibility read shows one
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['save-reopen'], 'fail');
  ctx = setup(t, { editTotalOnly: true });                 // Total frames +1 without a held cell after frame 1
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['edit-undo-redo'], 'fail');
  assert.match(outcomeOf(ctx.directory).reason, /later-frames-not-shifted|inserted-cell-not-held/);
});

test('P2813-6: the saved file is compared with the document the app reports and measured by the runner', async t => {
  // Before: any JSON whose format began with nemo.native- and whose hash differed passed.
  for (const [faults, reason] of [[{ savedLayersWrong: true }, /saved-layers-differ-from-the-document/], [{ saveNoEdit: true }, /save-did-not-write-the-edit/]]) {
    const ctx = setup(t, faults);
    const s = statuses(await ctx.workflow.run({ session: ctx.session, row: row() }));
    assert.ok(s['first-save-save-as'] === 'fail' || s['save-reopen'] === 'fail', JSON.stringify(faults));
    assert.match(outcomeOf(ctx.directory).reason, reason);
  }
  const { savedAgrees } = require('../cs7-scenarios/legacy-import.cjs');
  const summary = { contexts: [{ contextId: 'scene-root', contextKind: 'scene', totalFrames: 3, layerCount: 1 }], layers: [{ layerUid: 'l0', name: 'A', layerKind: 'drawing' }] };
  const doc = (extra = {}) => ({ format: 'nemo.native-object-document', totalFrames: 3, layers: [{ layerUid: 'l0', name: 'A' }], ...extra });
  assert.equal(savedAgrees(doc(), summary), null);
  assert.match(savedAgrees(doc({ totalFrames: 4 }), summary), /total-frames/);
  assert.match(savedAgrees(doc({ layers: [{ layerUid: 'l0', name: 'B' }] }), summary), /names/);
  assert.match(savedAgrees({ format: 'nemo.native-x' }, summary), /no-native-object-document/);
});

test('P2813-0: a reply that answers for another Nemo instance is refused', async t => {
  const { FOREIGN } = require('./cs7-h-stubs.cjs');
  const { session, workflow } = setup(t, {}, (phase, op, pair) => { if (phase === 'legacy-edit') pair.response.instanceId = FOREIGN; });
  await assert.rejects(workflow.run({ session, row: row() }), /reply is not from the candidate document|not one complete page/);
});
