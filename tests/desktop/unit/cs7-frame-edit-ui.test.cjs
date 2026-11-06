'use strict';
// frame-edit-ui (#2248): stub session and a disposable timeline model only; no app, helper or installed run.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createWorkflow, summarize, JUDGES, CLAUSES, PLAN } = require('../cs7-scenarios/frame-edit-ui.cjs');
const { OperatorInputError } = require('../cs7-runner.cjs');
const { World, frameSession, stylingPng } = require('./cs7-h-stubs.cjs');

const row = extra => ({ id: 'frame-edit-ui', issue: 2248, checks: CLAUSES.map(id => ({ id })), ...extra });
function setup(t, faults = {}, styling) {
  const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-frame-')));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const world = new World(directory, faults);
  return { directory, world, session: frameSession({ directory, world, styling }),
    workflow: createWorkflow({ reportDirectory: directory }) };
}
const statuses = result => Object.fromEntries(result.checks.map(c => [c.id, c.status]));

test('every declared clause passes from raw reads, and the plan starts with the display placement', async t => {
  const { session, workflow, directory } = setup(t);
  assert.match(PLAN[0].how, /TOP of display 2: position 2880,-270, size 1620x1420/);
  assert.equal(PLAN[0].id, 'window-placement');
  const result = await workflow.run({ session, row: row() });
  assert.deepEqual(statuses(result), Object.fromEntries(CLAUSES.map(id => [id, 'pass'])));
  assert.equal(result.safeToContinue, true);
  assert.equal(result.finalize, undefined);
  const outcome = JSON.parse(fs.readFileSync(path.join(directory, 'frame-edit-ui/outcome.json'), 'utf8'));
  assert.equal(outcome.reason, null);
});

test('a product failure read back from the document fails its clause, stops, and leaves the rest unrun', async t => {
  for (const [faults, failed, tail] of [
    [{ double: 'insertFrame' }, 'scene-f5-f6-f7', 'scene-menu-and-shortcuts'],   // F5 applied twice: +2 frames, +2 revisions
    [{ noop: 'insertKey' }, 'scene-f5-f6-f7', 'scene-undo-redo'],                // F6 did nothing
    [{ badUndo: true }, 'scene-undo-redo', 'scene-save-reopen'],                 // undo restored a different timeline
    [{ lostEditOnReopen: true }, 'scene-save-reopen', 'component-edits'],
    [{ touchFileOnOpen: true }, 'scene-save-reopen', 'component-edits'],
    [{ leak: 'insertFrame' }, 'component-edits', 'component-undo-redo'],         // component F5 also grew the Scene
  ]) {
    const { session, workflow } = setup(t, faults);
    const result = await workflow.run({ session, row: row() });
    const s = statuses(result);
    assert.equal(s[failed], 'fail', JSON.stringify(faults));
    assert.equal(s[tail], 'unrun', JSON.stringify(faults));
    assert.equal(result.safeToContinue, false);
  }
});

test('a failure the operator reports is taken as stated, never softened', async t => {
  const { session, workflow } = setup(t, { failureAt: 'component-f6' });
  const s = statuses(await workflow.run({ session, row: row() }));
  assert.equal(s['scene-undo-redo'], 'pass');
  assert.equal(s['component-edits'], 'fail');
  assert.equal(s['component-save-reopen'], 'unrun');
  const early = setup(t, { failureAt: 'scene-setup' });
  const result = await early.workflow.run({ session: early.session, row: row() });
  assert.ok(result.checks.every(c => c.status === 'unrun') && result.safeToContinue === false);
});

test('the layer-row styling clause is judged from the owned-window capture pixels and accessible names', async t => {
  for (const [styling, buttons, why] of [
    [stylingPng({ light: true }), true, 'light button fill'],
    [stylingPng({ wrapped: true }), true, 'name wraps to two lines'],
    [stylingPng(), false, 'controls lost their accessible names'],
  ]) {
    const { session, workflow } = setup(t, {}, styling);
    const result = await workflow.run({ session: { ...session, input: (c, o) => (c === 'observe' && !buttons ? Promise.resolve({ status: 'ok', nodes: [] }) : session.input(c, o)) }, row: row() });
    const s = statuses(result);
    assert.equal(s['layer-row-styling'], 'fail', why);
    assert.equal(s['scene-f5-f6-f7'], 'unrun', why);
  }
});

test('malformed or verdict-bearing evidence is an operator input error, never a pass or a product failure', async t => {
  // No reads: the operator's say-so alone.
  let { session, workflow } = setup(t, { malformedAt: 'scene-f5' });
  await assert.rejects(workflow.run({ session, row: row() }), error => error instanceof OperatorInputError && error.code === 'operator-evidence-malformed' &&
    error.command === 'frame-edit-ui.scene-f5');
  // A verdict field in the reply.
  ({ session, workflow } = setup(t, { verdictAt: 'scene-setup' }));
  await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed');
  // Reads that do not tile the timeline, are unpinned, or come from a command.* call.
  for (const damage of [
    ev => { ev.reads.layers[0].response.result.cells.runs[0][0] = 1; },
    ev => { ev.reads.layers[0].request.payload.atRevision = 1; },
    ev => { ev.reads.history.request.operation = 'command.document.object.frames.insert'; },
    ev => { delete ev.reads.history; },
    ev => { ev.reads.layers = []; },
  ]) {
    ({ session, workflow } = setup(t));
    const original = session.requestEvidence;
    session.requestEvidence = async request => { const ev = await original(request); if (request.phase === 'scene-f6') damage(ev); return ev; };
    await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed');
  }
  // Gesture applied to the wrong cell: the precondition is the operator's mistake, not a product verdict.
  ({ session, workflow } = setup(t));
  const original = session.requestEvidence;
  session.requestEvidence = async request => { const ev = await original(request); if (request.phase === 'scene-f6') ev.selection.frame = 0; return ev; };
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

test('the gesture oracle encodes the baseline effect of each edit', () => {
  const rows = (k, c) => ({ k, c });
  const base = rows(['key', 'held', 'held'], [true, true, true]);
  assert.equal(JUDGES.insertFrame(base, rows(['key', 'held', 'held', 'held'], [true, true, true, true]), 0), null);
  assert.equal(JUDGES.insertFrame(base, rows(['key', 'held', 'held'], [true, true, true]), 0).fail, 'total-frames-not-plus-one');
  assert.equal(JUDGES.insertKey(base, rows(['key', 'key', 'held'], [true, true, true]), 1), null);
  assert.ok(JUDGES.insertKey(base, base, 0).pre);
  assert.equal(JUDGES.insertBlankKey(base, rows(['key', 'key', 'held'], [true, false, false]), 1), null);
  assert.equal(JUDGES.clearKey(rows(['key', 'key'], [true, false]), rows(['key', 'held'], [true, true]), 1), null);
  assert.equal(JUDGES.clearKey(rows(['key', 'key'], [true, false]), rows(['key', 'held'], [true, false]), 1).fail, 'key-not-demoted-to-the-previous-hold');
  assert.equal(JUDGES.removeFrames(base, rows(['key', 'held'], [true, true]), 1, [1, 1]), null);
  assert.equal(JUDGES.duplicateKey(base, rows(['key', 'key', 'held', 'held'], [true, true, true, true]), 0), null);
});

test('summarize refuses reads that disagree on revision, document or total', () => {
  const world = new World(os.tmpdir());
  const good = world.reads('scene');
  assert.ok(summarize(good, 'scene-root'));
  assert.equal(summarize(good, 'native-component:1:0'), null);
  const other = structuredClone(good); other.layers[1].response.documentId = 'doc-9';
  assert.equal(summarize(other, 'scene-root'), null);
  const short = structuredClone(good); short.layers[1].response.result.cells.runs.pop();
  assert.equal(summarize(short, 'scene-root'), null);
});
