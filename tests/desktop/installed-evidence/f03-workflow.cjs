'use strict';
// #1049 / F03: the original opacity workflow, as one prompt for an agent client and one judge that never reads what the client
// said. The workflow is the one of the issue: static 25 -> 40, 60, undo -> replacement and stale write -> keyed 0/20 -> save and
// reopen, on the fixed fixture (tests/animation/fixtures/curve-workflow.json, layer r08_curve_layer, the same shells N21 builds).
//
// The judge only trusts: files the application wrote, and reads and probes this harness makes itself through the package's
// nemo-mcp after the client has exited. Anything the client printed or any tool trace it produced is recorded elsewhere and
// never decides a verdict.
//
// The fixture opens as an OBJECT project, which answers only the native operations (apiVersion 2) plus project.open/saveAs/new.
// The operations used here, each declared in engineering/application/capabilities-v2 (cited per step in the change note):
//   read   query.document.object (view presentation, the pin) then query.document.object.property.read (view rawSample)  native-property-read.json
//   write  command.document.object.property.edit (setValue)                                                                native-property-edit.json
//   undo   command.document.object.undo ; depth read query.document.object.history                                         native-object-history.json
//   files  project.open / project.saveAs (apiVersion 1; a saved project is a nemo.native-project-document)
// The opacity-document operations (query.document.opacity, query.document.evaluate, command.document.apply) are refused on it
// ("Unsupported object read operation.") and the apiVersion 1 property.* operations are `unavailable`; both refusals name the
// operation to use instead, and nemo_discover lists only what the open document answers.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const K = require('./kit.cjs');
const P = require('./payloads.cjs');
const { structured } = require('../cs7-read.cjs');
const ROOT = path.resolve(__dirname, '../../..');
const FIXTURE = path.join(ROOT, 'tests/animation/fixtures/curve-workflow.json');
const FIXTURE_SHA256 = 'dceb05d13576a4dda0eb1a1a9d8c0184e8617e9a3a2662150ee54f4badedf08d';
const KEYS = path.join(ROOT, 'native-engine/tests/fixtures/opacity-v2/project.json');
const LAYER = 'r08_curve_layer';
const TARGET = { layerUid: LAYER };
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

// The two project shells N21 stages: static (base opacity 25, no opacity keys) and keyed (opacity keys 20 at frame 0, 80 at 20).
function loadShells() {
  const bytes = fs.readFileSync(FIXTURE);
  if (sha(bytes) !== FIXTURE_SHA256) throw new K.EvidenceError('malformed', 'the fixed opacity fixture differs from its pinned bytes');
  const base = JSON.parse(bytes.toString('utf8')), keys = JSON.parse(fs.readFileSync(KEYS, 'utf8'));
  const stat = JSON.parse(JSON.stringify(base)), keyed = JSON.parse(JSON.stringify(base));
  stat.layers[0].motionStatic = { opacity: [25] };
  keyed.layers[0].motionStatic = { opacity: [25] };
  keyed.layers[0].motion.opacity = keys.layers[0].motion.opacity;
  return { static: stat, keyed };
}
function stageFixtures(dir, shells = loadShells()) {
  const paths = { static: path.join(dir, 'f03-static.json'), keyed: path.join(dir, 'f03-keyed.json'),
    saved60: path.join(dir, 'f03-saved-60.json'), saved: path.join(dir, 'f03-saved.json') };
  const staged = {};
  for (const name of ['static', 'keyed']) {
    const bytes = Buffer.from(JSON.stringify(shells[name], null, 2) + '\n');
    fs.writeFileSync(paths[name], bytes, { mode: 0o600 });
    staged[name] = sha(bytes);
  }
  for (const out of [paths.saved60, paths.saved]) fs.rmSync(out, { force: true });
  return { paths, staged };
}

// What the client is told. Goals and absolute paths only: no tool name, operation name, payload or hint about where anything is
// offered; compact discovery (nemo_discover) is how it learns the route, and the Claude half has no file tools to work around it.
// The order differs from the issue's list in one place: the application refuses to open a project file that is already the open
// project (it says so and how to continue), so the saved file is opened again after the keyed project has replaced it, which is
// also the only moment a reopen reads the file rather than the open document.
function promptFor(paths) {
  return [
    'You control a running Nemo desktop application through the MCP server named "nemo" and through nothing else. Start by asking the',
    'server what it offers. Never invent a revision or a document identity; use only what the application reported to you. Steps 5 and',
    '7 expect the application to refuse a request: report that refusal and do not retry it.',
    '',
    'The project that is open has one layer, "R08 rectangle"; its opacity is a property of that layer, on a 0 to 100 scale (40 means',
    '40). Do these steps in order:',
    '1. Discover the running instance and the open project. Note the document revision, and read the layer\'s current opacity at frame 0.',
    '2. Set the layer opacity to 40, then set it to 60 (frame 0), and read it back each time.',
    '3. Save a copy of the project as ' + paths.saved60 + ' (the file must not exist yet).',
    '4. Undo once, so the opacity is 40 again, and read it back.',
    '5. Try to set the opacity to 99 while naming the document revision you noted in step 1, before your edits. The application should',
    '   refuse it as out of date. Do not repeat it with a fresh revision.',
    '6. Save the project as ' + paths.saved + ' (the file must not exist yet).',
    '7. Open ' + paths.keyed + '. It replaces the current project. Try to set the opacity to 99 while naming the document identity',
    '   you used before this replacement. The application should refuse it. Do not repeat it.',
    '8. In the keyed project, read the layer\'s opacity at frame 0 and at frame 20.',
    '9. Open ' + paths.saved + ' again, which reloads the saved file as the open project, and read the opacity at frame 0.',
    'Stop with ' + paths.saved + ' open and make no further change. Finish with a short report: each value you read, and each',
    'request the application refused, with its reason.',
  ].join('\n');
}

const savedOpacity = document => {
  const holders = document && document.drawing && Array.isArray(document.drawing.propertyHolders) ? document.drawing.propertyHolders : [];
  const holder = holders.find(h => h && h.target && h.target.layerUid === LAYER && h.target.strokeId === undefined);
  const prop = holder && Array.isArray(holder.properties) ? holder.properties.find(p => p && p.property === 'opacity') : undefined;
  return prop ? prop.static : undefined;
};
const readJson = (ctx, id, file) => {
  ctx.ok('file', id + ': written by the application', fs.existsSync(file) && fs.statSync(file).size > 0, fs.existsSync(file));
  try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { ctx.ok('file', id + ': parses as JSON', false, 'not JSON'); return null; }
};
// The layer's own sample of opacity at a frame (rawSample view), pinned to the current revision; null when the read is refused.
async function opacityAt(ctx, frame = 0) {
  const { revision, pin } = await K.pinNow(ctx);
  const b = P.propertyRead(revision, pin, LAYER, 'opacity', frame);
  const res = await ctx.wire.query(b.operation, b.payload);
  ctx.ok('reply', 'opacity read at frame ' + frame + ' accepted', res.ok === true, res.ok ? 'ok' : res.error);
  return res.ok && Array.isArray(res.result.value) ? res.result.value[0] : null;
}

// Before the client runs: the harness opens the static fixture itself and reads the starting point.
async function arrange(ctx, { paths }) {
  const res = await ctx.wire.v1('project.open', { path: paths.static });
  ctx.ok('reply', 'setup: the static fixture opens', res.ok === true, res.ok ? 'ok' : res.error);
  const initial = await ctx.wire.discover();
  ctx.ok('read', 'setup: a native document is open', typeof initial.documentId === 'string' && initial.documentId.length > 0, initial.documentId);
  ctx.eq('read', 'setup: the document is at revision 0', initial.contentRevision, 0);
  ctx.eq('read', 'setup: the static opacity is 25', await opacityAt(ctx, 0), 25);
  return initial;
}

// After the client exited. `raw(request)` sends one request without the wire's own document binding (for the wrong-document probe).
async function judge(ctx, { paths, staged, initial, raw }) {
  const doc60 = readJson(ctx, 'saved-60.json', paths.saved60);
  ctx.eq('file', 'saved-60.json holds opacity 60', savedOpacity(doc60), [60]);
  const doc40 = readJson(ctx, 'saved.json', paths.saved);
  ctx.eq('file', 'saved.json holds opacity 40 (60, then undone, and the out-of-date 99 refused)', savedOpacity(doc40), [40]);
  ctx.eq('file', 'the static fixture was not overwritten', sha(fs.readFileSync(paths.static)), staged.static);
  ctx.eq('file', 'the keyed fixture was not overwritten', sha(fs.readFileSync(paths.keyed)), staged.keyed);

  // The state the client left: saved.json open as a project of its own, never edited after it was opened.
  const final = await ctx.wire.discover();
  ctx.ok('read', 'the project the client started in was replaced', final.documentId !== initial.documentId, { was: initial.documentId, now: final.documentId });
  ctx.eq('read', 'the open project was opened and not edited afterwards (revision 0, so the wrong-document write changed nothing)', final.contentRevision, 0);
  const left = await K.history(ctx);
  ctx.eq('read', 'the open project has no history (nothing was written after it was opened)', [left.undo, left.redo], [0, 0]);
  ctx.eq('read', 'the project the client left reads opacity 40 at frame 0', await opacityAt(ctx, 0), 40);

  // Independent keyed read: the harness opens the keyed fixture itself (this replaces saved.json) and reads frames 0, 10 and 20.
  await K.reopen(ctx, paths.keyed);
  ctx.eq('read', 'the keyed project evaluates 20, 50, 80 at frames 0, 10, 20',
    [await opacityAt(ctx, 0), await opacityAt(ctx, 10), await opacityAt(ctx, 20)], [20, 50, 80]);

  // Save and reopen, independently: the harness opens the file the client saved and reads the value from the application.
  const keyedDoc = await ctx.wire.discover();
  await K.reopen(ctx, paths.saved);
  const reopened = await ctx.wire.discover();
  ctx.ok('read', 'reopening saved.json gives another project', reopened.documentId !== keyedDoc.documentId, reopened.documentId);
  ctx.eq('read', 'the reopened project reads opacity 40', await opacityAt(ctx, 0), 40);

  // The refusals, produced here: a fresh valid write, then the same project with the revision it had before, then the identity of
  // the project the client started in.
  const set = value => P.propertyOpacity(LAYER, [value]);
  const rev = reopened.contentRevision;
  const w = set(77), wrote = await ctx.wire.command(w.operation, w.payload, { expectedRevision: rev });
  ctx.ok('reply', 'a write that names the current revision is accepted', wrote.ok === true, wrote.ok ? 'ok' : wrote.error);
  const s11 = set(11), stale = await ctx.wire.command(s11.operation, s11.payload, { expectedRevision: rev });
  ctx.ok('reply', 'a write that names the previous revision is refused as stale_revision', stale.ok === false && stale.error.code === 'stale_revision', stale.ok ? 'accepted' : stale.error.code, 'stale_revision');
  const now = await ctx.wire.discover();
  ctx.eq('read', 'the stale write changed neither value nor revision', [await opacityAt(ctx, 0), now.contentRevision], [77, rev + 1]);
  const wrong = await raw({ apiVersion: 2, instanceId: now.instanceId, documentId: initial.documentId, operation: s11.operation,
    expectedRevision: now.contentRevision, payload: s11.payload });
  ctx.ok('reply', 'a write that names the replaced project is refused as wrong_document', wrong.ok === false && wrong.error && wrong.error.code === 'wrong_document', wrong.ok ? 'accepted' : (wrong.error || {}).code, 'wrong_document');
  const after = await ctx.wire.discover();
  ctx.eq('read', 'the wrong-document write changed neither value nor revision', [await opacityAt(ctx, 0), after.contentRevision], [77, rev + 1]);
  await K.step(ctx, 'the harness undoes its own write', 'undo');
  ctx.eq('read', 'undo returns the reopened project to 40', await opacityAt(ctx, 0), 40);
}

function makeRaw(client, uuid) {
  return async request => structured(await client.callTool('nemo_command', { requestId: uuid(), ...request }));
}

module.exports = { FIXTURE_SHA256, LAYER, loadShells, stageFixtures, promptFor, arrange, judge, makeRaw };
