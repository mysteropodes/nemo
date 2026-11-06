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
function setup(t, faults = {}, tamper) {
  const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-draw-')));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const world = new DrawWorld(directory, faults);
  if (tamper) world.tamper = (phase, pair) => tamper(phase, pair.request.operation, pair);
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
    [{ noise: true }, 'component-instances-pixels', 'component-bitmap-export'],         // the canvas changed everywhere
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
  // Attached reads of any kind are refused: the harness reads the document, the operator never attaches it.
  for (const key of ['reads', 'sceneLayers', 'kinds']) {
    ({ session, workflow } = setup(t));
    const original = session.requestEvidence;
    session.requestEvidence = async request => { const ev = await original(request); if (request.phase === 'comp-pen') ev[key] = {}; return ev; };
    await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed' && error.command === 'component-edit-draw.comp-pen', key);
  }
  // Harness reads that do not tile, are unpinned, are not ok, or disagree between the scene and the component.
  for (const [phase, damage] of [
    ['comp-pen', (op, pair) => { if (op === 'query.document.object.layer.timing') pair.response.result.cells.runs[0][0] = 1; }],
    ['comp-pen', (op, pair) => { if (op === 'query.document.object.layer.timing') pair.response.result.atRevision = 1; }],
    ['comp-pen', (op, pair) => { if (op === 'query.document.object.history') pair.response.ok = false; }],
    ['comp-pen', (op, pair) => { if (op === 'query.document.object.history') delete pair.response.result.undoDepth; }],
    // the scene layers page must give every layer a kind
    ['bake-break', (op, pair) => { if (op === 'query.document.object.component.read' && pair.request.payload.selector.contextId === 'scene-root') pair.response.result.items[0].layerKind = ''; }],
  ]) {
    ({ session, workflow } = setup(t, {}, (p, op, pair) => { if (p === phase) damage(op, pair); }));
    await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed', phase);
  }
  // an export the step did not write is the operator's mistake, not a verdict
  ({ session, workflow } = setup(t, { noExport: 'bitmap-export' }));
  await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed' && error.command === 'component-edit-draw.bitmap-export');
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

// ---- the findings of the installed-rows review ------------------------------------------------------------------
const outcomeOf = directory => JSON.parse(fs.readFileSync(path.join(directory, 'component-edit-draw/outcome.json'), 'utf8'));
const others = (s, ...ids) => Object.entries(s).filter(([id]) => !ids.includes(id)).every(([, v]) => v === 'pass');

test('P2813-1: the tool that drew, the Fill Brush placement and a bitmap record are read from the app, not assumed', async t => {
  // Before: every gesture used the same judge, so seven strokes drawn with any tool passed.
  for (const wrongTool of ['comp-pen', 'comp-ellipse', 'comp-fillbrush']) {
    const { session, workflow } = setup(t, { wrongTool });
    await assert.rejects(workflow.run({ session, row: row() }), error => error.code === 'operator-evidence-malformed' && error.command === 'component-edit-draw.' + wrongTool, wrongTool);
  }
  let ctx = setup(t, { aboveOff: true });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error.command === 'component-edit-draw.comp-fillbrush');
  // A non-bitmap stroke at the bitmap step has no bitmap-brush record: a product failure, found by a read-only query.
  ctx = setup(t, { notBitmap: true });
  const failed = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(failed)['component-draw-tools'], 'fail');
  assert.match(outcomeOf(ctx.directory).reason, /no-bitmap-brush-record-for-the-stroke/);
  // The page's classes are not exposed: the tool cannot be read, so the clause is unrun, never passed.
  ctx = setup(t, { noClasses: true });
  const unread = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(unread)['component-draw-tools'], 'unrun');
  assert.equal(statuses(unread)['component-motion-draw'], 'unrun');
  assert.match(outcomeOf(ctx.directory).reasons['component-draw-tools'], /no tool button with DOM classes/);
  assert.ok(others(statuses(unread), 'component-draw-tools', 'component-motion-draw'));
  // The bitmap read route refused: unrun with the reason.
  ctx = setup(t, { bitmapRefused: true });
  const refused = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(refused)['component-draw-tools'], 'unrun');
  assert.match(outcomeOf(ctx.directory).reasons['component-draw-tools'], /bitmap brush read was refused/);
  assert.equal(statuses(refused)['component-bitmap-export'], 'unrun');
  // The pure functions behind it.
  const { activeTool, aboveSelected } = require('../cs7-scenarios/component-edit-draw.cjs');
  const btn = (label, ...classes) => ({ role: 'AXButton', labels: [label], classes });
  assert.deepEqual(activeTool([btn('Brush (B)', 'tool-btn', 'active'), btn('Pen', 'tool-btn')]), { tool: 'draw' });
  assert.deepEqual(activeTool([btn('Pinceau de remplissage', 'tool-btn', 'active'), btn('Pinceau', 'tool-btn')]), { tool: 'fillbrush' });
  assert.deepEqual(activeTool([btn('Pinceau', 'tool-btn', 'active')]), { tool: 'draw' });
  assert.ok(activeTool([btn('Brush (B)', 'tool-btn')]).unread);
  assert.deepEqual(aboveSelected([btn('Above — on top of existing fills', 'icon-btn', 'active')]), { on: true });
});

test('P2813-2: Motion mode is read from the app, and a stroke after the first must change the layer rows or the clause stays unrun', async t => {
  // Before: motion-enter/exit asserted only "no edit", and Pen/Rectangle passed on cells that were already true.
  let ctx = setup(t, { modeNotSwitched: true });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error.code === 'operator-evidence-malformed' && error.command === 'component-edit-draw.motion-enter');
  ctx = setup(t, { wrongTool: 'motion-pen' });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error.command === 'component-edit-draw.motion-pen');
  // Strokes routed onto the origin key: the first lands on the playhead cell, the others change no row and cannot be told from any other edit.
  ctx = setup(t, { originKey: true });
  const result = await ctx.workflow.run({ session: ctx.session, row: row() });
  const s = statuses(result);
  assert.equal(s['component-motion-draw'], 'unrun');
  assert.match(outcomeOf(ctx.directory).reasons['component-motion-draw'], /motion-pen: the layer rows did not change/);
  assert.ok(others(s, 'component-motion-draw'));
  // The same stroke on a layer that already shows content: a layer edit that moves nothing is not a stroke.
  ctx = setup(t, { noop: 'motion-pen' });
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['component-motion-draw'], 'fail');
});

test('P2813-3: the instance pixels are judged on the canvas\'s own accessibility frame at one playhead frame, not on operator rectangles', async t => {
  // Before: canvas, drawZone and outside were operator rects; the playhead frame and the UI around the canvas were not considered.
  let ctx = setup(t, { chrome: true });
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['component-instances-pixels'], 'pass');   // UI outside the canvas is ignored
  ctx = setup(t, { exitFrame: 5 });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error.code === 'operator-evidence-malformed' && error.command === 'component-edit-draw.component-exit');
  ctx = setup(t, { noPlayhead: true });
  const unread = await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.equal(statuses(unread)['component-instances-pixels'], 'unrun');
  assert.match(outcomeOf(ctx.directory).reasons['component-instances-pixels'], /playhead frame was not readable/);
  ctx = setup(t, { noise: true });
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['component-instances-pixels'], 'fail');
  assert.match(outcomeOf(ctx.directory).reason, /canvas tiles/);
  // No operator phase asks for rectangles, and both captures are hashed by the runner.
  ctx = setup(t);
  const asked = []; const original = ctx.session.requestEvidence;
  ctx.session.requestEvidence = request => { asked.push(request.phase); return original(request); };
  await ctx.workflow.run({ session: ctx.session, row: row() });
  assert.ok(!asked.includes('instance-before'));
  const shot = JSON.parse(fs.readFileSync(path.join(ctx.directory, 'component-edit-draw/instance-before.json'), 'utf8'));
  assert.equal(shot.sha256, require('node:crypto').createHash('sha256').update(fs.readFileSync(path.join(ctx.directory, 'capture-21.png'))).digest('hex'));
  assert.deepEqual(shot.canvas, { x: 100, y: 80, w: 500, h: 300 });
});

test('P2813-4: the bitmap export is measured by the runner, pinned to frame 3, compared with a baseline export and a bitmap record', async t => {
  // Before: the PNG was whatever the operator placed, with operator rectangles and no baseline.
  for (const [faults, status] of [[{ noBitmapInk: true }, 'fail'], [{ inkEverywhere: true }, 'fail']]) {
    const ctx = setup(t, faults);
    assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['component-bitmap-export'], status, JSON.stringify(faults));
  }
  // The app records no bitmap-brush stroke at frame 3 (the export would be a plain stroke): found at the gesture, by a read-only query.
  let ctx = setup(t, { noBitmapRecord: true });
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['component-draw-tools'], 'fail');
  ctx = setup(t, { wrongExportFrame: 'bitmap-export' });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error.command === 'component-edit-draw.bitmap-export');
  ctx = setup(t, { noExport: 'bitmap-baseline' });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error.command === 'component-edit-draw.bitmap-baseline');
  ctx = setup(t);
  assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['component-bitmap-export'], 'pass');
  const file = JSON.parse(fs.readFileSync(path.join(ctx.directory, 'component-edit-draw/file-bitmap-export.json'), 'utf8'));
  assert.equal(file.playhead, 3);
  assert.equal(file.sha256, require('node:crypto').createHash('sha256').update(fs.readFileSync(path.join(ctx.directory, 'exports/bitmap-brush.png'))).digest('hex'));
});

test('P2813-6: the saved file is compared with the verified layers and keys, and reopening must give a new document', async t => {
  for (const [faults, reason] of [[{ savedLayersWrong: true }, /saved-layers-differ/], [{ sameDocumentOnReopen: true }, /reopen-did-not-create-a-new-document/], [{ touchFileOnOpen: true }, /changed-by-opening/]]) {
    const ctx = setup(t, faults);
    assert.equal(statuses(await ctx.workflow.run({ session: ctx.session, row: row() }))['component-draw-save-reopen'], 'fail', JSON.stringify(faults));
    assert.match(outcomeOf(ctx.directory).reason, reason);
  }
  const ctx = setup(t, { noWrite: true });
  await assert.rejects(ctx.workflow.run({ session: ctx.session, row: row() }), error => error.command === 'component-edit-draw.draw-save');
});

test('P2813-0: a reply that answers for another Nemo instance is refused (document state is read by the harness, bound to the candidate)', async t => {
  const { FOREIGN } = require('./cs7-h-stubs.cjs');
  const { session, workflow } = setup(t, {}, (phase, op, pair) => { if (phase === 'comp-pen') pair.response.instanceId = FOREIGN; });
  await assert.rejects(workflow.run({ session, row: row() }), /reply is not from the candidate document/);
});
