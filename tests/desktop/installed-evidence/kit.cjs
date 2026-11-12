'use strict';
// What a row is built from: a context that records every assertion with observed and expected values, and the shared
// observations (history depth, PNG export, save and reopen, independent object reads) that every lane needs.
//
// Assertion kinds: `reply` (what the command answered), `read` (an independent read of document state through a query),
// `pixel` (a decoded PNG export), `file` (the saved project file). A row can only pass if at least one assertion is not a
// `reply`: "the command returned ok" is never evidence by itself (the runner enforces it).
//
// Bookkeeping: the kit's own plumbing (setup, the revision and history deltas of every command, the export and save mechanics)
// is recorded through `ctx.book`, and a `bookkeeping` assertion never counts as an observation of a behaviour. The row's own
// claims are made through `ctx.eq` / `ctx.ok` / `ctx.near` / `ctx.within`, and `ctx.behavior(label, fn)` fails a step whose
// claim rests on replies and bookkeeping alone.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { canonical, EvidenceError } = require('./wire.cjs');
const P = require('./payloads.cjs');
const probe = require('./png-probe.cjs');

class AssertionFailure extends Error {
  constructor(id) { super('assertion failed: ' + id); this.name = 'AssertionFailure'; this.assertionId = id; }
}
const short = value => { const text = canonical(value) ?? 'undefined'; return text.length > 600 ? { truncated: text.slice(0, 600), length: text.length } : value; };
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const KINDS = new Set(['reply', 'read', 'pixel', 'file']);

function createContext({ wire, work, rowId }) {
  const assertions = [];
  let counter = 0;
  function record(kind, id, observed, expected, pass, note, bookkeeping) {
    if (!KINDS.has(kind)) throw new Error('unknown assertion kind ' + kind);
    assertions.push({ id: rowId + ': ' + id, kind, ...(bookkeeping ? { bookkeeping: true } : {}), observed: short(observed), expected: short(expected), verdict: pass ? 'pass' : 'fail', ...(note ? { note } : {}) });
    if (!pass) throw new AssertionFailure(id);
  }
  const make = bookkeeping => ({
    eq: (kind, id, observed, expected) => record(kind, id, observed, expected, canonical(observed) === canonical(expected), undefined, bookkeeping),
    ok: (kind, id, condition, observed, expected) => record(kind, id, observed, expected === undefined ? true : expected, condition === true, undefined, bookkeeping),
    near(kind, id, observed, expected, tol) {
      const a = Array.isArray(observed) ? observed : [observed], b = Array.isArray(expected) ? expected : [expected];
      record(kind, id, observed, expected, a.length === b.length && a.every((v, i) => typeof v === 'number' && Math.abs(v - b[i]) <= tol), 'tolerance ' + tol, bookkeeping);
    },
    within: (kind, id, observed, lo, hi) => record(kind, id, observed, [lo, hi], typeof observed === 'number' && observed >= lo && observed <= hi, 'inclusive range', bookkeeping),
  });
  return {
    wire, work, rowId, assertions,
    next: () => ++counter,
    ...make(false),
    book: make(true),
    // One behaviour, one claim: the step must leave at least one read, pixel or file assertion of the row's own behind.
    async behavior(label, fn) {
      const start = assertions.length, out = await fn();
      if (!assertions.slice(start).some(a => !a.bookkeeping && a.kind !== 'reply')) {
        record('reply', 'behaviour "' + label + '" is observed independently of its command reply', 'no read, pixel or file assertion of its own', 'at least one', false, undefined, true);
      }
      return out;
    },
  };
}

// ---- observations -------------------------------------------------------------------------------------------------
// The pin of the current revision. An object project answers `query.document.snapshot.acquire` with "Unsupported object read
// operation" (that op, query.document.serialize and query.document.evaluate belong to the opacity document, authority
// native-engine::history::NativeOpacityHistory in engineering/application/capabilities-v2/native-opacity.json); the public pin of an
// object project is the {atRevision, documentSnapshotId} the object query's presentation view returns
// (tests/desktop/native-image-mesh.test.cjs presentation bootstrap; native-engine/src/object_snapshot.rs presentation read).
async function pinNow(ctx) {
  const at = await ctx.wire.discover();
  const b = P.presentationRead(at.contentRevision), pin = await ctx.wire.query(b.operation, b.payload);
  ctx.book.ok('reply', 'snapshot pin acquired', pin.ok && typeof pin.result.documentSnapshotId === 'string', pin.ok ? { atRevision: pin.result.atRevision } : pin.error);
  return { revision: pin.result.atRevision, pin: pin.result.documentSnapshotId };
}
async function history(ctx) {
  const { revision, pin } = await pinNow(ctx);
  const b = P.historyRead(revision, pin);
  const read = await ctx.wire.query(b.operation, b.payload);
  ctx.book.ok('reply', 'history read', read.ok && Number.isSafeInteger(read.result.undoDepth) && Number.isSafeInteger(read.result.redoDepth), read.ok ? read.result : read.error);
  return { undo: read.result.undoDepth, redo: read.result.redoDepth, revision };
}

// A negative control proves the NATIVE OWNER's rule, so a refusal counts only when it is a typed owner refusal: not the
// transport's own request-shape check (`frontDoor`), and not "unavailable" / "internal" (an operation that is not there or
// broke would refuse everything). A control names the codes its rule produces; the default is the owner's rule codes.
const OWNER_REFUSALS = Object.freeze(['invalid_request', 'not_found', 'busy_conflict']);
function refusal(ctx, label, res, before, after, h0, h1, opts) {
  ctx.book.ok('reply', label + ': refused', res.ok === false, res.ok ? res.result : res.error);
  ctx.book.ok('reply', label + ': refused by the native owner, not by the transport front door', res.frontDoor !== true, res.frontDoor ? res.error : 'owner reply');
  const codes = opts.codes || OWNER_REFUSALS;
  ctx.ok('reply', label + ': typed owner refusal', codes.includes(res.error.code), res.error.code, codes);
  if (opts.details) ctx.ok('reply', label + ': refusal details', opts.details(res.error) === true, res.error.details === undefined ? null : res.error.details);
  ctx.book.eq('read', label + ': no revision', after.contentRevision, before.contentRevision);
  ctx.book.eq('read', label + ': history and redo untouched', [h1.undo, h1.redo], [h0.undo, h0.redo]);
}

// One command against the contract: exactly one history entry and one revision when applied; nothing when refused or a
// no-op. `opts.refused` (optionally with `codes` and `details`) and `opts.noop` select the other two outcomes.
// `built` may be a function evaluated after the pre-read, for requests that carry a snapshot pin or lifecycle generation.
async function apply(ctx, label, built, opts = {}) {
  const before = await ctx.wire.discover(), h0 = await history(ctx);
  if (typeof built === 'function') built = await built();
  const res = await ctx.wire.command(built.operation, built.payload, opts.send || {});
  const after = await ctx.wire.discover(), h1 = await history(ctx);
  if (opts.refused) { refusal(ctx, label, res, before, after, h0, h1, opts); return { res, before, after }; }
  ctx.book.ok('reply', label + ': accepted', res.ok === true, res.ok ? 'ok' : res.error);
  if (opts.noop) {
    ctx.book.eq('reply', label + ': no-op receipt', [res.result.applied, res.result.historyEntriesAdded], [false, 0]);
    ctx.book.eq('read', label + ': no-op keeps revision', after.contentRevision, before.contentRevision);
    ctx.book.eq('read', label + ': no-op keeps history', [h1.undo, h1.redo], [h0.undo, h0.redo]);
    return { res, before, after, result: res.result };
  }
  ctx.book.eq('reply', label + ': receipt', [res.result.applied, res.result.historyEntriesAdded], [true, 1]);
  ctx.book.eq('read', label + ': exactly one revision', after.contentRevision, before.contentRevision + 1);
  ctx.book.eq('reply', label + ': reply names that revision', res.revision, after.contentRevision);
  ctx.book.eq('read', label + ': exactly one history entry, redo cleared', [h1.undo, h1.redo], [h0.undo + 1, 0]);
  return { res, before, after, result: res.result };
}

// A resource publication (a brush, an uploaded bitmap): it is NOT an edit. It adds no revision and no history entry, and the
// reply names the revision it was answered at. `opts.applied` is the receipt's `applied` member (false for an identical
// republication); `opts.refused` selects a typed refusal, which changes nothing either.
async function resource(ctx, label, built, opts = {}) {
  const before = await ctx.wire.discover(), h0 = await history(ctx);
  if (typeof built === 'function') built = await built();
  const res = await ctx.wire.command(built.operation, built.payload, opts.send || {});
  const after = await ctx.wire.discover(), h1 = await history(ctx);
  if (opts.refused) { refusal(ctx, label, res, before, after, h0, h1, opts); return { res, before, after }; }
  ctx.book.ok('reply', label + ': accepted', res.ok === true, res.ok ? 'ok' : res.error);
  if (opts.applied !== undefined) ctx.eq('reply', label + ': receipt applied', res.result.applied, opts.applied);
  ctx.eq('read', label + ': publishing is not an edit: no revision', after.contentRevision, before.contentRevision);
  ctx.eq('reply', label + ': reply names the unchanged revision', res.revision, before.contentRevision);
  ctx.eq('read', label + ': publishing adds no history entry and keeps redo', [h1.undo, h1.redo], [h0.undo, h0.redo]);
  return { res, before, after, result: res.result };
}

// Undo or redo: one new revision, history moves by one entry in the right direction.
async function step(ctx, label, direction) {
  const built = direction === 'undo' ? P.undo() : P.redo();
  const before = await ctx.wire.discover(), h0 = await history(ctx);
  const res = await ctx.wire.command(built.operation, built.payload);
  const after = await ctx.wire.discover(), h1 = await history(ctx);
  ctx.book.ok('reply', label + ': accepted', res.ok === true, res.ok ? 'ok' : res.error);
  ctx.book.eq('reply', label + ': receipt', [res.result.applied, res.result.historyEntriesAdded], [true, 0]);
  ctx.book.eq('read', label + ': one revision', after.contentRevision, before.contentRevision + 1);
  ctx.book.eq('read', label + ': history moved by one', [h1.undo, h1.redo], direction === 'undo' ? [h0.undo - 1, h0.redo + 1] : [h0.undo + 1, h0.redo - 1]);
  return { res, before, after };
}

function walkFiles(dir, out = []) {
  for (const name of fs.readdirSync(dir)) { const file = path.join(dir, name), stat = fs.lstatSync(file); if (stat.isDirectory()) walkFiles(file, out); else if (stat.isFile()) out.push(file); }
  return out;
}
// PNG export of frame 0 through the app; the export must not change the document.
async function exportPng(ctx, tag) {
  const dir = path.join(ctx.work, 'png-' + tag + '-' + ctx.next());
  fs.mkdirSync(dir, { recursive: true });
  const before = await ctx.wire.discover();
  const res = await ctx.wire.v1('project.export', { format: 'png', destination: dir, start: 0, end: 0 });
  ctx.book.ok('reply', 'export ' + tag + ': accepted', res.ok === true, res.ok ? 'ok' : res.error);
  const after = await ctx.wire.discover();
  ctx.book.eq('read', 'export ' + tag + ': document untouched', after.contentRevision, before.contentRevision);
  const files = walkFiles(dir).filter(f => f.toLowerCase().endsWith('.png'));
  ctx.book.eq('file', 'export ' + tag + ': exactly one PNG written', files.length, 1);
  const bytes = fs.readFileSync(files[0]);
  let image;
  try { image = probe.decodeExport(bytes); } catch (error) { ctx.book.ok('pixel', 'export ' + tag + ': decodes', false, String(error.message).slice(0, 120)); }
  return { image, digest: probe.pixels(image), file: files[0], fileSha: sha(bytes), width: image.width, height: image.height };
}
// An export already written is evidence on disk: it must still be there, byte for byte, after later edits and exports.
function exportIntact(ctx, id, exported) {
  const bytes = fs.existsSync(exported.file) ? fs.readFileSync(exported.file) : null;
  ctx.eq('file', id, bytes === null ? null : sha(bytes), exported.fileSha);
}

async function saveAs(ctx, tag) {
  const file = path.join(ctx.work, 'save-' + tag + '-' + ctx.next() + '.json');
  const before = await ctx.wire.discover();
  const res = await saveRetrying(ctx, 'save ' + tag, file);
  ctx.book.ok('reply', 'save ' + tag + ': accepted', res.ok === true, res.ok ? 'ok' : res.error);
  const after = await ctx.wire.discover();
  ctx.book.eq('read', 'save ' + tag + ': saving is not an edit', after.contentRevision, before.contentRevision);
  ctx.book.ok('file', 'save ' + tag + ': file written', fs.existsSync(file) && fs.statSync(file).size > 0, fs.existsSync(file));
  const bytes = fs.readFileSync(file);
  let document;
  try { document = JSON.parse(bytes.toString('utf8')); } catch { ctx.book.ok('file', 'save ' + tag + ': parses as JSON', false, 'not JSON'); }
  return { file, bytes, sha: sha(bytes), document };
}
// Open a saved file. The app must load a NEW document instance (a different documentId, and a lifecycle generation that
// advanced when the host reports one): a host that kept or re-focused the in-memory document would otherwise pass every
// check that follows without the file ever being read.
async function reopen(ctx, file) {
  const before = await ctx.wire.discover();
  const res = await ctx.wire.v1('project.open', { path: file });
  ctx.book.ok('reply', 'reopen: accepted', res.ok === true, res.ok ? 'ok' : res.error);
  const after = await ctx.wire.discover();
  ctx.ok('read', 'reopen: the file was loaded as a different document instance', typeof after.documentId === 'string' && after.documentId.length > 0 && after.documentId !== before.documentId, { before: before.documentId, after: after.documentId });
  if (Number.isSafeInteger(before.lifecycleGeneration) && Number.isSafeInteger(after.lifecycleGeneration)) {
    ctx.ok('read', 'reopen: the lifecycle generation advanced', after.lifecycleGeneration > before.lifecycleGeneration, { before: before.lifecycleGeneration, after: after.lifecycleGeneration });
  }
  return res.result;
}

// project.saveAs on the installed app is refused with a typed `unavailable` ("pin-changed-before-picker" or
// "publication-response-unavailable") when it follows a change closely; an immediate second request is accepted. The refused attempts
// are recorded, never hidden, and the save must still be accepted within a few attempts. Any other refusal is returned as it is.
const SAVE_RACE = /pin-changed-before-picker|publication-response-unavailable/;
async function saveRetrying(ctx, label, file) {
  let res;
  for (let attempt = 1; attempt <= 4; attempt++) {
    res = await ctx.wire.v1('project.saveAs', { path: file });
    const race = !res.ok && res.error && res.error.code === 'unavailable' && SAVE_RACE.test(String(res.error.message));
    if (!race) return res;
    ctx.book.ok('reply', label + ': save attempt ' + attempt + ' refused as unavailable (' + (String(res.error.message).match(SAVE_RACE) || [''])[0] + ')', true, 'refused, retried');
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  return res;
}

// New 640x360 project; a dirty open project is first saved to a scratch file, because project.new refuses unsaved changes.
async function fresh(ctx, { w = 640, h = 360, fps = 24 } = {}) {
  const was = await ctx.wire.discover();
  if (was.documentId) {
    const scratch = await saveRetrying(ctx, 'setup', path.join(ctx.work, 'scratch-' + ctx.next() + '.json'));
    ctx.book.ok('reply', 'setup: previous project saved aside', scratch.ok === true, scratch.ok ? 'ok' : scratch.error);
  }
  const res = await ctx.wire.v1('project.new', { canvasW: w, canvasH: h, fps });
  ctx.book.ok('reply', 'setup: project.new accepted', res.ok === true, res.ok ? 'ok' : res.error);
  const layers = res.result.layers;
  ctx.book.ok('read', 'setup: new project has a layer', Array.isArray(layers) && layers.length >= 1 && typeof layers[0].layerUid === 'string', layers);
  const now = await ctx.wire.discover();
  ctx.book.ok('read', 'setup: a different document is open', typeof now.documentId === 'string' && now.documentId !== was.documentId, { documentId: now.documentId });
  return { layerUid: layers[0].layerUid, documentId: now.documentId };
}

async function readObject(ctx, stableTarget) {
  const { revision } = await pinNow(ctx);
  const b = P.objectRead(revision, stableTarget);
  const res = await ctx.wire.query(b.operation, b.payload);
  return res.ok ? { ok: true, object: res.result.object } : { ok: false, error: res.error };
}
// The document's layer order, read from the saved project (drawing.layers is the document order). An object project offers no
// public serialize query: query.document.serialize is an opacity-document operation and the object owner refuses it.
async function layerOrder(ctx) {
  const saved = await saveAs(ctx, 'order');
  const layers = saved.document && saved.document.drawing && saved.document.drawing.layers;
  ctx.book.ok('file', 'layer order read from the saved project', Array.isArray(layers), Array.isArray(layers) ? layers.length : 'no drawing.layers');
  return layers.map(layer => layer.layerUid);
}

// The image mesh of the raster on `layerUid`, read through the object query's imageMesh view (the mesh id is never part of a
// command reply: image.mesh.edit answers only {applied, historyEntriesAdded}). The presentation view supplies the work token
// and view generation the mesh view requires. Returns the mesh counts (null when no mesh is bound) and, when the mesh carries
// offsets, every offset pair.
async function meshState(ctx, layerUid) {
  const { revision, pin } = await pinNow(ctx);
  const view = P.presentationRead(revision), shown = await ctx.wire.query(view.operation, view.payload);
  ctx.book.ok('reply', 'mesh read: presentation answered', shown.ok === true, shown.ok ? 'ok' : shown.error);
  const base = { contextId: shown.result.contextId, layerUid, frame: shown.result.frame, workId: shown.result.workId, viewGeneration: shown.result.viewGeneration };
  const ask = async page => { const b = P.meshQuery(revision, pin, { ...base, page }); return ctx.wire.query(b.operation, b.payload); };
  const selection = await ask({ kind: 'selection' });
  ctx.book.ok('reply', 'mesh read: selection answered', selection.ok === true, selection.ok ? 'ok' : selection.error);
  const { mesh, target, rect } = selection.result;
  if (mesh === null) return { mesh: null, offsets: null, target, rect };
  let offsets = null;
  if (mesh.offsetsPresent) {
    offsets = [];
    for (let cursor = 0, turns = 0; cursor !== null && turns < 64; turns++) {
      const page = await ask({ kind: 'offsets', meshId: mesh.meshId, cursor, limit: 16 });
      ctx.book.ok('reply', 'mesh read: offsets page answered', page.ok === true && Array.isArray(page.result.page.items), page.ok ? 'ok' : page.error);
      offsets.push(...page.result.page.items);
      cursor = page.result.page.nextCursor;
    }
  }
  return { mesh, offsets, target, rect };
}

// Save, reopen, re-export and save again in the same session: same pixels, and the second save is byte-identical. The file is
// opened from a COPY at another path, so the app can only have read the file (see reopen). `reads` runs after the reopen, for
// the row's own persisted-field observations.
async function roundTrip(ctx, label, before, reads) {
  const first = await saveAs(ctx, label + '-1');
  const copy = path.join(ctx.work, 'reopen-' + label + '-' + ctx.next() + '.json');
  fs.copyFileSync(first.file, copy);
  await reopen(ctx, copy);
  const exported = await exportPng(ctx, label + '-reopened');
  ctx.eq('pixel', label + ': reopened export equals the export before saving', exported.digest, before.digest);
  if (reads) await reads({ first, exported });
  const second = await saveAs(ctx, label + '-2');
  ctx.eq('file', label + ': second save is byte-stable', second.sha, first.sha);
  return { first, second, exported };
}

// A request that needs the current snapshot pin and the host's lifecycle generation. The generation is read from the host
// status; a host that does not report it cannot be driven for these operations (unrun, never guessed).
async function pinned(ctx, build) {
  const { pin } = await pinNow(ctx), state = await ctx.wire.discover();
  if (!Number.isSafeInteger(state.lifecycleGeneration) || state.lifecycleGeneration < 1) {
    throw new EvidenceError('unavailable', 'the host status does not report a lifecycle generation');
  }
  return build({ pin, lifecycleGeneration: state.lifecycleGeneration, revision: state.contentRevision });
}

// ---- staged asset files (inside the candidate's own temp root, which the app admits) -------------------------------------
const SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
function solidPng(width, height, [r, g, b, a = 255]) {
  const rgba = Buffer.alloc(width * height * 4);
  for (let i = 0; i < width * height; i++) { rgba[i * 4] = r; rgba[i * 4 + 1] = g; rgba[i * 4 + 2] = b; rgba[i * 4 + 3] = a; }
  return probe.encode(width, height, rgba);
}
// Four distinct flat quadrants [top-left, top-right, bottom-left, bottom-right]: an image whose content a deformation can move.
function quadrantPng(width, height, colors) {
  const rgba = Buffer.alloc(width * height * 4);
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const [r, g, b, a = 255] = colors[(y >= height / 2 ? 2 : 0) + (x >= width / 2 ? 1 : 0)], o = (y * width + x) * 4;
    rgba[o] = r; rgba[o + 1] = g; rgba[o + 2] = b; rgba[o + 3] = a;
  }
  return probe.encode(width, height, rgba);
}
// A VALID PNG one pixel wider than the admitted dimension (8192): it decodes, so only a size cap can refuse it. (A header with
// no pixel data would be refused as corrupt whether or not a cap exists.)
const OVERSIZE_WIDTH = 8193;
const oversizePng = () => solidPng(OVERSIZE_WIDTH, 1, [10, 10, 10, 255]);
const corruptPng = () => Buffer.concat([SIGNATURE, Buffer.from('this is not png data at all')]);
function stage(ctx, name, bytes) {
  const file = path.join(ctx.work, ctx.next() + '-' + name);
  fs.writeFileSync(file, bytes, { mode: 0o600 });
  return file;
}

const savedObjects = document => (document && document.drawing && Array.isArray(document.drawing.objects) ? document.drawing.objects : []);
const objectStrokeIds = document => savedObjects(document).map(object => object && object.target && object.target.strokeId).filter(Boolean);

module.exports = { OWNER_REFUSALS, OVERSIZE_WIDTH, solidPng, quadrantPng, oversizePng, corruptPng, stage, AssertionFailure, createContext, pinNow, pinned, history, apply, resource, step, exportPng, exportIntact, saveAs, reopen,
  fresh, readObject, layerOrder, meshState, roundTrip, savedObjects, objectStrokeIds, sha, EvidenceError };
