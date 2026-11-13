'use strict';
// #1049 / F03: the original opacity workflow, as two prompts for two separate agent client processes (a session that works, then a second one that
// reconnects to the same running application and document and continues) and one judge that never reads what the client said. The workflow is the one of
// the issue: static 25 -> 40, 60, save, undo, REDO, stale write [session 1] -> the second session undoes again, saves, replaces the project, a write against
// the replaced project, keyed 0/20, reopen [session 2], on the fixed fixture (tests/animation/fixtures/curve-workflow.json, layer r08_curve_layer, the
// same shells N21 builds). A third, fresh session of the same client is asked to identify the installed candidate from the repository documentation.
//
// The judge only trusts: files the application wrote, and reads and probes this harness makes itself through the package's
// nemo-mcp after the client has exited. Anything the client printed or any tool trace it produced is recorded elsewhere and
// never decides a verdict.
//
// The fixture opens as an OBJECT project, which answers only the native operations (apiVersion 2) plus project.open/saveAs/new.
// The operations used here, each declared in engineering/application/capabilities-v2 (cited per step in the change note):
//   read   query.document.object (view presentation, the pin) then query.document.object.property.read (view rawSample)  native-property-read.json
//   write  command.document.object.property.edit (setValue)                                                                native-property-edit.json
//   undo   command.document.object.undo, command.document.object.redo ; depth read query.document.object.history                                         native-object-history.json
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
const DOCS_ROOT = ROOT, DOCS = path.join(ROOT, 'engineering/remediation/reference/03_TESTING_AND_DEBUGGING.md');
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

// What the clients are told. Goals and absolute paths only: no tool name, operation name, payload or hint about where anything is offered;
// compact discovery (nemo_discover) is how a client learns the route, and the Claude half has no file tools to work around it.
// Two separate client processes work on the SAME running application and document: the first ends with the project open and its history in a known state
// (the redo is the last change), the second starts from that state without being told what it is beyond "continue". The order differs from the issue's
// list in one place: the application refuses to open a project file that is already the open project (it says so and how to continue), so the saved file
// is opened again after the keyed project has replaced it, which is also the only moment a reopen reads the file rather than the open document.
const INTRO = [
  'You control a running Nemo desktop application through the MCP server named "nemo" and through nothing else. Start by asking the',
  'server what it offers. Never invent a revision or a document identity; use only what the application reported to you.',
];
function promptOne(paths) {
  return [
    ...INTRO, 'The last step expects the application to refuse a request: report that refusal and do not retry it.',
    '',
    'The project that is open has one layer, "R08 rectangle"; its opacity is a property of that layer, on a 0 to 100 scale (40 means',
    '40). Do these steps in order:',
    '1. Discover the running instance and the open project. Note the document revision, and read the layer\'s current opacity at frame 0.',
    '2. Set the layer opacity to 40, then set it to 60 (frame 0), and read it back each time.',
    '3. Save a copy of the project as ' + paths.saved60 + ' (the file must not exist yet).',
    '4. Undo once, so the opacity is 40 again, and read it back.',
    '5. Redo once, so the opacity is 60 again, and read it back.',
    '6. Try to set the opacity to 99 while naming the document revision you noted in step 1, before your edits. The application should',
    '   refuse it as out of date. Do not repeat it with a fresh revision.',
    'Then stop: save nothing more, open nothing and make no further change. Another session continues from the state you leave. Finish with a',
    'short report: each value you read, and the request the application refused, with its reason.',
  ].join('\n');
}
function promptTwo(paths) {
  return [
    ...INTRO, 'Step 4 expects the application to refuse a request: report that refusal and do not retry it.',
    '',
    'Another session has already worked in this same running application and left a project open; you continue from the state it left.',
    'The project has one layer, "R08 rectangle"; its opacity is a property of that layer, on a 0 to 100 scale. Do these steps in order:',
    '1. Discover the running instance and the open project. Note the document revision, read the layer\'s opacity at frame 0, and read how many',
    '   undo and redo steps the project holds.',
    '2. Undo once, and read the opacity back.',
    '3. Save the project as ' + paths.saved + ' (the file must not exist yet).',
    '4. Open ' + paths.keyed + '. It replaces the current project. Try to set the opacity to 99 while naming the document identity',
    '   you used before this replacement. The application should refuse it. Do not repeat it.',
    '5. In the keyed project, read the layer\'s opacity at frame 0 and at frame 20.',
    '6. Open ' + paths.saved + ' again, which reloads the saved file as the open project, and read the opacity at frame 0.',
    'Stop with ' + paths.saved + ' open and make no further change. Finish with a short report: each value you read, and the request the',
    'application refused, with its reason.',
  ].join('\n');
}
// The fresh-agent identification session (plan clause P32, #1034): the repository documentation and the MCP tools, nothing else. Read-only.
function promptIdentify(docs = DOCS) {
  return [
    'You are a fresh session with no earlier context. A Nemo desktop application is running and reachable through the MCP server named "nemo".',
    'The repository documentation of the project is at ' + docs + ' (its section "The installed candidate and the clients that test it"',
    'says what identifies an installed candidate). Using that documentation and the tools you have, identify the candidate that is running now:',
    'its build id, its source head, and the SHA-256 of its app executable (Contents/MacOS/nemo). This is a read-only task: change nothing in the',
    'application. Do not guess; if you cannot establish an item with the tools you have, write "unknown" for it and say why.',
    'Answer with these three lines first, then one short sentence on how you obtained each:',
    'build id: <value>',
    'source head: <value>',
    'executable sha256: <value>',
  ].join('\n');
}

// Where each client's calls fall in the steps of its prompt (informational; see f03-client-cli.stepCounts). A step starts at its marker call.
const OP = name => call => typeof call.operation === 'string' && call.operation.endsWith(name);
const OPEN = file => call => call.operation === 'project.open' && typeof call.path === 'string' && call.path.endsWith(file);
const STEPS = Object.freeze({
  session1: [{ n: 1, label: 'discover, note the revision, read frame 0' }, { n: 2, label: 'set 40 then 60, read back', match: OP('property.edit') },
    { n: 3, label: 'save the copy', match: OP('project.saveAs') }, { n: 4, label: 'undo, read back', match: OP('object.undo') },
    { n: 5, label: 'redo, read back', match: OP('object.redo') }, { n: 6, label: 'out-of-date write', match: OP('property.edit') }],
  session2: [{ n: 1, label: 'discover, read frame 0 and the history' }, { n: 2, label: 'undo, read back', match: OP('object.undo') },
    { n: 3, label: 'save as saved.json', match: OP('project.saveAs') }, { n: 4, label: 'open the keyed project', match: OPEN('f03-keyed.json') },
    { n: 5, label: 'write against the replaced project, keyed reads', match: OP('property.edit') }, { n: 6, label: 'open saved.json, read frame 0', match: OPEN('f03-saved.json') }],
  identify: [{ n: 1, label: 'identify the candidate' }],
});

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

// One reading of the open project as the application reports it: identity, revision, the layer's opacity at frame 0 and the history depth.
async function stateOf(ctx) {
  const at = await ctx.wire.discover(), h = await K.history(ctx);
  return { instanceId: at.instanceId, documentId: at.documentId, revision: at.contentRevision, opacity: await opacityAt(ctx, 0), undo: h.undo, redo: h.redo };
}

// After the first client exited, before the second starts: the harness reads what session 1 left. The redo is judged here, from the application's own
// value, history and revision: set 40, set 60, undo, redo leaves 60 with two edits in the undo history, nothing to redo, and four revisions made. A client
// that only undid leaves 40 with one undo and one redo; a client that did not undo and redo leaves two revisions.
async function judgeSessionOne(ctx, { paths, initial }) {
  const doc60 = readJson(ctx, 'saved-60.json', paths.saved60);
  ctx.eq('file', 'saved-60.json holds opacity 60 (the copy taken before the undo)', savedOpacity(doc60), [60]);
  ctx.ok('file', 'saved.json is not written yet (session 1 saved only the copy and stopped)', !fs.existsSync(paths.saved), fs.existsSync(paths.saved), false);
  const left = await stateOf(ctx);
  ctx.eq('read', 'session 1 left the project it started in open', left.documentId, initial.documentId);
  ctx.eq('read', 'after undo and redo the opacity is 60 again (the redo was applied)', left.opacity, 60);
  ctx.eq('read', 'after undo and redo the history holds both edits and nothing to redo (undo 2, redo 0)', [left.undo, left.redo], [2, 0]);
  ctx.eq('read', 'four revisions were made: set, set, undo, redo', left.revision, initial.contentRevision + 4);
  return left;
}
// Between the sessions: the application still holds exactly that state, so the second client starts from it.
async function judgeHandover(ctx, left) {
  ctx.eq('read', 'nothing moved between the two sessions (same instance, document, revision, value and history)', await stateOf(ctx), left);
}
// What the application showed the second client when it first asked what is running, taken from the client's own stream. The reply is the application's,
// not the client's words: a client cannot make it name another revision.
function judgeStartedFrom(ctx, { stream, left }) {
  const first = stream.length ? stream[0] : null;
  ctx.eq('stream', 'session 2: the first discovery answer the application gave the client names the document and revision session 1 left',
    first ? { instanceId: first.instanceId, documentId: first.documentId, revision: first.revision } : null,
    { instanceId: left.instanceId, documentId: left.documentId, revision: left.revision });
}

// After the second client exited. `raw(request)` sends one request without the wire's own document binding (for the wrong-document probe).
async function judge(ctx, { paths, staged, initial, raw, left, stream }) {
  judgeStartedFrom(ctx, { stream, left });
  const doc60 = readJson(ctx, 'saved-60.json', paths.saved60);
  ctx.eq('file', 'saved-60.json holds opacity 60', savedOpacity(doc60), [60]);
  const doc40 = readJson(ctx, 'saved.json', paths.saved);
  ctx.eq('file', 'saved.json holds opacity 40 (60, redone, undone again by session 2, and the out-of-date 99 refused)', savedOpacity(doc40), [40]);
  ctx.eq('file', 'the static fixture was not overwritten', sha(fs.readFileSync(paths.static)), staged.static);
  ctx.eq('file', 'the keyed fixture was not overwritten', sha(fs.readFileSync(paths.keyed)), staged.keyed);

  // The state the client left: saved.json open as a project of its own, never edited after it was opened.
  const final = await ctx.wire.discover();
  ctx.ok('read', 'the project the client started in was replaced', final.documentId !== initial.documentId, { was: initial.documentId, now: final.documentId });
  ctx.eq('read', 'the open project was opened and not edited afterwards (revision 0, so the wrong-document write changed nothing)', final.contentRevision, 0);
  const opened = await K.history(ctx);
  ctx.eq('read', 'the open project has no history (nothing was written after it was opened)', [opened.undo, opened.redo], [0, 0]);
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

// ---- the fresh-agent identification session ---------------------------------------------------------------------------
// The harness compares what the fresh client answered with what the package proof and the registered build say (never the other way round).
function parseIdentity(text) {
  const t = String(text || '').replace(/[`*_]/g, ' ');
  const one = re => { const hit = t.match(re); return hit ? hit[1] : null; };
  return {
    buildId: one(/build[ -]?id\s*[:=]\s*([0-9][0-9A-Za-z.+-]*:[0-9a-f]{40}(?:-dirty)?)/i),
    sourceHead: one(/source[ -]?head\s*[:=]\s*([0-9a-f]{40})\b/i),
    executableSha256: one(/(?:executable[ -]?)?sha-?256\s*[:=]\s*([0-9a-f]{64})\b/i),
  };
}
async function judgeIdentify(ctx, { before, answer, truth }) {
  const after = await stateOf(ctx);
  ctx.eq('read', 'the identification session changed nothing in the application (same document, revision, value and history)', after, before);
  const said = parseIdentity(answer), detail = {};
  for (const key of ['buildId', 'sourceHead', 'executableSha256']) detail[key] = { answered: said[key], expected: truth[key], match: said[key] !== null && said[key] === truth[key] };
  ctx.ok('read', 'the fresh session reported the installed candidate\'s build id, source head and executable sha256, equal to the package proof and the registered build',
    Object.values(detail).every(d => d.match), detail);
  return { said, detail };
}

function makeRaw(client, uuid) {
  return async request => structured(await client.callTool('nemo_command', { requestId: uuid(), ...request }));
}

module.exports = { FIXTURE_SHA256, LAYER, DOCS, DOCS_ROOT, STEPS, loadShells, stageFixtures, promptOne, promptTwo, promptIdentify, arrange, stateOf, judgeSessionOne, judgeHandover, judge, parseIdentity, judgeIdentify, makeRaw };
