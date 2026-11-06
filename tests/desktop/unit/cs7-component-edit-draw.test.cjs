'use strict';
// component-edit-draw (#2256): stub session and a disposable document model only; no app, helper or installed run.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createWorkflow, JUDGES, CLAUSES, PLAN } = require('../cs7-scenarios/component-edit-draw.cjs');
const { OperatorInputError } = require('../cs7-runner.cjs');
const { DrawWorld, drawSession } = require('./cs7-i-stubs.cjs');

const row = extra => ({ id: 'component-edit-draw', issue: 2256, checks: CLAUSES.map(id => ({ id })), ...extra });
function setup(t, faults = {}) {
  const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-draw-')));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const world = new DrawWorld(directory, faults);
  return { directory, world, session: drawSession({ world }), workflow: createWorkflow({ reportDirectory: directory }) };
}
const statuses = result => Object.fromEntries(result.checks.map(c => [c.id, c.status]));

test('every declared clause passes from raw reads and pixels, and the plan starts with the display placement', async t => {
  const { session, workflow, directory } = setup(t);
  assert.match(PLAN[0].how, /TOP of display 2: position 2880,-270, size 1620x1420/);
  assert.equal(PLAN[0].id, 'window-placement');
  const result = await workflow.run({ session, row: row() });
  assert.deepEqual(statuses(result), Object.fromEntries(CLAUSES.map(id => [id, 'pass'])));
  assert.equal(result.safeToContinue, true);
  const outcome = JSON.parse(fs.readFileSync(path.join(directory, 'component-edit-draw/outcome.json'), 'utf8'));
  assert.equal(outcome.reason, null);
});

test('a product failure read back from the document fails its clause, stops, and leaves the rest unrun', async t => {
  for (const [faults, failed, tail] of [
    [{ noop: 'motion-pen' }, 'component-motion-draw', 'component-draw-tools'],        // Pen drew nothing in Motion mode
    [{ noop: 'comp-pen' }, 'component-draw-tools', 'component-draw-undo-redo'],         // Pen refused inside the component
    [{ double: 'comp-line' }, 'component-draw-tools', 'component-draw-undo-redo'],      // one gesture, two revisions
    [{ noOwnKey: 'comp-rect' }, 'component-draw-tools', 'component-draw-undo-redo'],    // held frame did not get its own key
    [{ leak: 'comp-ellipse' }, 'component-draw-tools', 'component-draw-undo-redo'],     // the Scene changed
    [{ badUndo: true }, 'component-draw-undo-redo', 'component-instances-pixels'],
    [{ noInstance: true }, 'component-instances-pixels', 'component-bitmap-export'],    // the instance does not show the drawing
    [{ noise: true }, 'component-instances-pixels', 'component-bitmap-export'],         // the artboard changed elsewhere
    [{ noBitmapInk: true }, 'component-bitmap-export', 'component-break-apart'],
    [{ bakeNoKind: true }, 'component-break-apart', 'component-draw-save-reopen'],
    [{ bakeWrongFrames: true }, 'component-break-apart', 'component-draw-save-reopen'],
    [{ lostDrawOnReopen: true }, 'component-draw-save-reopen', null],
    [{ touchFileOnOpen: true }, 'component-draw-save-reopen', null],
    [{ staleHistory: true }, 'component-draw-save-reopen', null],
    [{ modeEdits: 'motion-enter' }, 'component-motion-draw', 'component-draw-tools'],   // switching mode edited the document
  ]) {
    const { session, workflow } = setup(t, faults);
    const result = await workflow.run({ session, row: row() });
    const s = statuses(result);
    assert.equal(s[failed], 'fail', JSON.stringify(faults));
    if (tail) assert.equal(s[tail], 'unrun', JSON.stringify(faults));
    assert.equal(result.safeToContinue, false);
  }
});

test('a uniform capture leaves the instance pixels unrun with the reason and the other clauses still pass', async t => {
  const { session, workflow, directory } = setup(t, { blankCapture: true });
  const result = await workflow.run({ session, row: row() });
  const s = statuses(result);
  assert.equal(s['component-instances-pixels'], 'unrun');
  assert.ok(Object.entries(s).filter(([id]) => id !== 'component-instances-pixels').every(([, v]) => v === 'pass'));
  assert.equal(result.safeToContinue, true);
  const outcome = JSON.parse(fs.readFileSync(path.join(directory, 'component-edit-draw/outcome.json'), 'utf8'));
  assert.match(outcome.reasons['component-instances-pixels'], /does not show the artboard/);
});

test('a failure the operator reports is taken as stated, never softened', async t => {
  const { session, workflow } = setup(t, { failureAt: 'comp-line' });
  const s = statuses(await workflow.run({ session, row: row() }));
  assert.equal(s['component-motion-draw'], 'pass');
  assert.equal(s['component-draw-tools'], 'fail');
  assert.equal(s['component-draw-save-reopen'], 'unrun');
  const early = setup(t, { failureAt: 'scene-setup' });
  const result = await early.workflow.run({ session: early.session, row: row() });
  assert.ok(result.checks.every(c => c.status === 'unrun') && result.safeToContinue === false);
});

test('malformed or verdict-bearing evidence is an operator input error, never a pass or a product failure', async t => {
  let { session, workflow } = setup(t, { malformedAt: 'comp-draw' });
  await assert.rejects(workflow.run({ session, row: row() }), error => error instanceof OperatorInputError && error.code === 'operator-evidence-malformed' &&
    error.command === 'component-edit-draw.comp-draw');
  ({ session, workflow } = setup(t, { verdictAt: 'scene-setup' }));
  await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed');
  ({ session, workflow } = setup(t, { wrongFrame: 'motion-draw' }));
  await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed');
  for (const damage of [
    ev => { ev.reads.layers[0].response.result.cells.runs[0][0] = 1; },
    ev => { ev.reads.layers[0].request.payload.atRevision = 1; },
    ev => { ev.reads.history.request.operation = 'command.document.object.frames.insert'; },
    ev => { delete ev.reads.history; },
    ev => { delete ev.sceneLayers; },
  ]) {
    ({ session, workflow } = setup(t));
    const original = session.requestEvidence;
    session.requestEvidence = async request => { const ev = await original(request); if (request.phase === 'comp-pen') damage(ev); return ev; };
    await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed');
  }
  // kinds reads must be the pinned layers page of the scene
  for (const damage of [ev => { delete ev.kinds; }, ev => { ev.kinds.request.payload.selector.contextId = 'native-component:1:0'; }, ev => { ev.kinds.response.result.items.pop(); }]) {
    ({ session, workflow } = setup(t));
    const original = session.requestEvidence;
    session.requestEvidence = async request => { const ev = await original(request); if (request.phase === 'bake-break') damage(ev); return ev; };
    await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed');
  }
  // an exported image whose "background" is not empty is the operator's mistake, not a verdict
  ({ session, workflow } = setup(t));
  const original = session.requestEvidence;
  session.requestEvidence = async request => { const ev = await original(request); if (request.phase === 'bitmap-export') ev.rects.background = { x: 100, y: 100, w: 100, h: 20 }; return ev; };
  await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed');
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

test('the gesture oracle encodes the baseline effect of a drawing', () => {
  const rows = (k, c) => ({ k, c });
  const blank = rows(['key', 'held', 'held'], [true, false, false]);
  assert.equal(JUDGES.draw(blank, rows(['key', 'key', 'held'], [true, true, true]), 1), null);
  assert.ok(JUDGES.draw(rows(['key', 'held'], [true, true]), blank, 1).pre);
  assert.equal(JUDGES.draw(blank, blank, 1).fail, 'stroke-did-not-land-in-the-cell');
  assert.equal(JUDGES.draw(blank, rows(['key', 'held', 'held'], [true, true, true]), 1).fail, 'held-frame-did-not-get-its-own-key');
  assert.equal(JUDGES.draw(blank, rows(['key', 'key'], [true, true]), 1).fail, 'total-frames-changed');
  assert.equal(JUDGES.draw(blank, rows(['key', 'held', 'key'], [true, false, true]), 1).fail, 'stroke-did-not-land-in-the-cell');
  assert.equal(JUDGES.drawAtPlayhead(blank, rows(['key', 'held', 'held'], [true, true, true]), 1, true), null);
  assert.ok(JUDGES.drawAtPlayhead(rows(['key', 'held'], [true, true]), blank, 1, true).pre);
  assert.equal(JUDGES.drawAtPlayhead(rows(['key', 'held'], [true, true]), rows(['key', 'held'], [true, true]), 1, false), null);
});
