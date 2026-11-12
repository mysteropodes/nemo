'use strict';
// What a row is built from: a context that records every assertion with observed and expected values, and the shared
// observations (history depth, PNG export, save and reopen, independent object reads) that every lane needs.
//
// Assertion kinds: `reply` (what the command answered), `read` (an independent read of document state through a query),
// `pixel` (a decoded PNG export), `file` (the saved project file). A row can only pass if at least one assertion is not a
// `reply`: "the command returned ok" is never evidence by itself (the runner enforces it).
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { canonical, EvidenceError } = require('./wire.cjs');
const P = require('./payloads.cjs');
const probe = require('./png-probe.cjs');

class AssertionFailure extends Error {
  constructor(id) { super('assertion failed: ' + id); this.name = 'AssertionFailure'; this.assertionId = id; }
}
const short = value => { const text = canonical(value); return text.length > 600 ? { truncated: text.slice(0, 600), length: text.length } : value; };
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const KINDS = new Set(['reply', 'read', 'pixel', 'file']);

function createContext({ wire, work, rowId }) {
  const assertions = [];
  let counter = 0;
  function record(kind, id, observed, expected, pass, note) {
    if (!KINDS.has(kind)) throw new Error('unknown assertion kind ' + kind);
    assertions.push({ id: rowId + ': ' + id, kind, observed: short(observed), expected: short(expected), verdict: pass ? 'pass' : 'fail', ...(note ? { note } : {}) });
    if (!pass) throw new AssertionFailure(id);
  }
  const ctx = {
    wire, work, rowId, assertions,
    next: () => ++counter,
    eq: (kind, id, observed, expected) => record(kind, id, observed, expected, canonical(observed) === canonical(expected)),
    ok: (kind, id, condition, observed, expected) => record(kind, id, observed, expected === undefined ? true : expected, condition === true),
    near(kind, id, observed, expected, tol) {
      const a = Array.isArray(observed) ? observed : [observed], b = Array.isArray(expected) ? expected : [expected];
      record(kind, id, observed, expected, a.length === b.length && a.every((v, i) => typeof v === 'number' && Math.abs(v - b[i]) <= tol), 'tolerance ' + tol);
    },
    within: (kind, id, observed, lo, hi) => record(kind, id, observed, [lo, hi], typeof observed === 'number' && observed >= lo && observed <= hi, 'inclusive range'),
  };
  return ctx;
}

// ---- observations -------------------------------------------------------------------------------------------------
async function pinNow(ctx) {
  const pin = await ctx.wire.query(P.snapshotAcquire().operation, P.snapshotAcquire().payload);
  ctx.ok('reply', 'snapshot pin acquired', pin.ok && typeof pin.result.documentSnapshotId === 'string', pin.ok ? { atRevision: pin.result.atRevision } : pin.error);
  return { revision: pin.result.atRevision, pin: pin.result.documentSnapshotId };
}
async function history(ctx) {
  const { revision, pin } = await pinNow(ctx);
  const b = P.historyRead(revision, pin);
  const read = await ctx.wire.query(b.operation, b.payload);
  ctx.ok('reply', 'history read', read.ok && Number.isSafeInteger(read.result.undoDepth) && Number.isSafeInteger(read.result.redoDepth), read.ok ? read.result : read.error);
  return { undo: read.result.undoDepth, redo: read.result.redoDepth, revision };
}

// One command against the contract: exactly one history entry and one revision when applied; nothing when refused or a
// no-op. `opts.refused` (optionally with `codes`) and `opts.noop` select the other two outcomes.
// `built` may be a function evaluated after the pre-read, for requests that carry a snapshot pin or lifecycle generation.
async function apply(ctx, label, built, opts = {}) {
  const before = await ctx.wire.discover(), h0 = await history(ctx);
  if (typeof built === 'function') built = await built();
  const res = await ctx.wire.command(built.operation, built.payload, opts.send || {});
  const after = await ctx.wire.discover(), h1 = await history(ctx);
  if (opts.refused) {
    ctx.ok('reply', label + ': refused', res.ok === false, res.ok ? res.result : res.error);
    if (opts.codes) ctx.ok('reply', label + ': typed refusal', opts.codes.includes(res.error.code), res.error.code, opts.codes);
    ctx.eq('read', label + ': no revision', after.contentRevision, before.contentRevision);
    ctx.eq('read', label + ': history and redo untouched', [h1.undo, h1.redo], [h0.undo, h0.redo]);
    return { res, before, after };
  }
  ctx.ok('reply', label + ': accepted', res.ok === true, res.ok ? 'ok' : res.error);
  if (opts.noop) {
    ctx.eq('reply', label + ': no-op receipt', [res.result.applied, res.result.historyEntriesAdded], [false, 0]);
    ctx.eq('read', label + ': no-op keeps revision', after.contentRevision, before.contentRevision);
    ctx.eq('read', label + ': no-op keeps history', [h1.undo, h1.redo], [h0.undo, h0.redo]);
    return { res, before, after };
  }
  ctx.eq('reply', label + ': receipt', [res.result.applied, res.result.historyEntriesAdded], [true, 1]);
  ctx.eq('read', label + ': exactly one revision', after.contentRevision, before.contentRevision + 1);
  ctx.eq('reply', label + ': reply names that revision', res.revision, after.contentRevision);
  ctx.eq('read', label + ': exactly one history entry, redo cleared', [h1.undo, h1.redo], [h0.undo + 1, 0]);
  return { res, before, after, result: res.result };
}

// Undo or redo: one new revision, history moves by one entry in the right direction.
async function step(ctx, label, direction) {
  const built = direction === 'undo' ? P.undo() : P.redo();
  const before = await ctx.wire.discover(), h0 = await history(ctx);
  const res = await ctx.wire.command(built.operation, built.payload);
  const after = await ctx.wire.discover(), h1 = await history(ctx);
  ctx.ok('reply', label + ': accepted', res.ok === true, res.ok ? 'ok' : res.error);
  ctx.eq('reply', label + ': receipt', [res.result.applied, res.result.historyEntriesAdded], [true, 0]);
  ctx.eq('read', label + ': one revision', after.contentRevision, before.contentRevision + 1);
  ctx.eq('read', label + ': history moved by one', [h1.undo, h1.redo], direction === 'undo' ? [h0.undo - 1, h0.redo + 1] : [h0.undo + 1, h0.redo - 1]);
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
  ctx.ok('reply', 'export ' + tag + ': accepted', res.ok === true, res.ok ? 'ok' : res.error);
  const after = await ctx.wire.discover();
  ctx.eq('read', 'export ' + tag + ': document untouched', after.contentRevision, before.contentRevision);
  const files = walkFiles(dir).filter(f => f.toLowerCase().endsWith('.png'));
  ctx.eq('file', 'export ' + tag + ': exactly one PNG written', files.length, 1);
  const bytes = fs.readFileSync(files[0]);
  let image;
  try { image = probe.decodeExport(bytes); } catch (error) { ctx.ok('pixel', 'export ' + tag + ': decodes', false, String(error.message).slice(0, 120)); }
  return { image, digest: probe.pixels(image), fileSha: sha(bytes), width: image.width, height: image.height };
}

async function saveAs(ctx, tag) {
  const file = path.join(ctx.work, 'save-' + tag + '-' + ctx.next() + '.json');
  const before = await ctx.wire.discover();
  const res = await ctx.wire.v1('project.saveAs', { path: file });
  ctx.ok('reply', 'save ' + tag + ': accepted', res.ok === true, res.ok ? 'ok' : res.error);
  const after = await ctx.wire.discover();
  ctx.eq('read', 'save ' + tag + ': saving is not an edit', after.contentRevision, before.contentRevision);
  ctx.ok('file', 'save ' + tag + ': file written', fs.existsSync(file) && fs.statSync(file).size > 0, fs.existsSync(file));
  const bytes = fs.readFileSync(file);
  let document;
  try { document = JSON.parse(bytes.toString('utf8')); } catch { ctx.ok('file', 'save ' + tag + ': parses as JSON', false, 'not JSON'); }
  return { file, bytes, sha: sha(bytes), document };
}
async function reopen(ctx, file) {
  const res = await ctx.wire.v1('project.open', { path: file });
  ctx.ok('reply', 'reopen: accepted', res.ok === true, res.ok ? 'ok' : res.error);
  return res.result;
}

// New 640x360 project; a dirty open project is first saved to a scratch file, because project.new refuses unsaved changes.
async function fresh(ctx, { w = 640, h = 360, fps = 24 } = {}) {
  const was = await ctx.wire.discover();
  if (was.documentId) {
    const scratch = await ctx.wire.v1('project.saveAs', { path: path.join(ctx.work, 'scratch-' + ctx.next() + '.json') });
    ctx.ok('reply', 'setup: previous project saved aside', scratch.ok === true, scratch.ok ? 'ok' : scratch.error);
  }
  const res = await ctx.wire.v1('project.new', { canvasW: w, canvasH: h, fps });
  ctx.ok('reply', 'setup: project.new accepted', res.ok === true, res.ok ? 'ok' : res.error);
  const layers = res.result.layers;
  ctx.ok('read', 'setup: new project has a layer', Array.isArray(layers) && layers.length >= 1 && typeof layers[0].layerUid === 'string', layers);
  const now = await ctx.wire.discover();
  ctx.ok('read', 'setup: a different document is open', typeof now.documentId === 'string' && now.documentId !== was.documentId, { documentId: now.documentId });
  return { layerUid: layers[0].layerUid, documentId: now.documentId };
}

async function readObject(ctx, stableTarget) {
  const { revision } = await pinNow(ctx);
  const b = P.objectRead(revision, stableTarget);
  const res = await ctx.wire.query(b.operation, b.payload);
  return res.ok ? { ok: true, object: res.result.object } : { ok: false, error: res.error };
}
async function layerOrder(ctx) {
  const { revision } = await pinNow(ctx);
  const b = P.serialize(revision);
  const res = await ctx.wire.query(b.operation, b.payload);
  ctx.ok('reply', 'serialize read', res.ok && Array.isArray(res.result.document && res.result.document.layers), res.ok ? 'ok' : res.error);
  return res.result.document.layers.map(layer => layer.layerUid);
}

// Save, reopen, re-export and save again in the same session: same pixels, and the second save is byte-identical.
async function roundTrip(ctx, label, before) {
  const first = await saveAs(ctx, label + '-1');
  await reopen(ctx, first.file);
  const exported = await exportPng(ctx, label + '-reopened');
  ctx.eq('pixel', label + ': reopened export equals the export before saving', exported.digest, before.digest);
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
// A structurally valid PNG signature and header that declares an image far beyond the admitted size, with no pixel data.
function oversizePng() {
  const header = Buffer.alloc(13); header.writeUInt32BE(9000, 0); header.writeUInt32BE(9000, 4); header[8] = 8; header[9] = 6;
  const chunk = (type, data) => { const body = Buffer.concat([Buffer.from(type, 'ascii'), data]), out = Buffer.alloc(12 + data.length);
    out.writeUInt32BE(data.length, 0); body.copy(out, 4); out.writeUInt32BE(require('../../fixtures/lib/png.cjs').crc32(body) >>> 0, 8 + data.length); return out; };
  return Buffer.concat([SIGNATURE, chunk('IHDR', header), chunk('IEND', Buffer.alloc(0))]);
}
const corruptPng = () => Buffer.concat([SIGNATURE, Buffer.from('this is not png data at all')]);
function stage(ctx, name, bytes) {
  const file = path.join(ctx.work, ctx.next() + '-' + name);
  fs.writeFileSync(file, bytes, { mode: 0o600 });
  return file;
}

const savedObjects = document => (document && document.drawing && Array.isArray(document.drawing.objects) ? document.drawing.objects : []);
const objectStrokeIds = document => savedObjects(document).map(object => object && object.target && object.target.strokeId).filter(Boolean);

module.exports = { solidPng, oversizePng, corruptPng, stage, AssertionFailure, createContext, pinNow, pinned, history, apply, step, exportPng, saveAs, reopen, fresh, readObject, layerOrder, roundTrip, savedObjects, objectStrokeIds, sha, EvidenceError };
