'use strict';
// Shared pieces of the foreground installed scenarios: the placement phase, bounded private
// evidence files, and a few raw-pixel measurements. Nothing here decides a verdict; each
// scenario's oracle judges the raw observations itself.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { OperatorInputError } = require('../cs7-runner.cjs');
const { decode } = require('../../fixtures/lib/png.cjs');

const PLACEMENT = 'Before anything else, place the Nemo window under test at the TOP of display 2: position 2880,-270, ' +
  'size 1620x1420 (x, y, width, height in points), then raise it so no other window covers it. Leave it there for the ' +
  'whole run and do not use another app while the run is active.';
const MAX_BYTES = 8 * 1024 * 1024;

const plain = value => !!value && typeof value === 'object' && !Array.isArray(value);
const int = value => Number.isSafeInteger(value) && value >= 0;
const text = value => typeof value === 'string' && value.trim().length > 0;
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

// Unusable testimony is an operator-input problem (stops the batch, no replay), never a verdict.
function malformed(row, phase) {
  return new OperatorInputError('operator-evidence-malformed', { command: row + '.' + phase });
}
// The operator supplies raw observations only. A reply carrying a verdict is refused outright, and so is any
// attached document read: document state is observed by the harness itself (readTimeline / readDocument below),
// never accepted from what a person or agent pastes in.
function rawOnly(evidence) {
  return plain(evidence) && !['pass', 'passed', 'status', 'verdict', 'result', 'ok', 'reads', 'sceneLayers', 'kinds'].some(key => Object.hasOwn(evidence, key));
}
function privateEvidence(directory, name, value) {
  fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
  fs.writeFileSync(path.join(directory, name + '.json'), JSON.stringify(value, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
}
// A file the operator names must be a plain file inside the private report directory.
function readInside(root, relative, max = MAX_BYTES) {
  if (!text(relative) || path.isAbsolute(relative) || relative.split(/[\\/]/).includes('..')) return null;
  try {
    const base = fs.realpathSync(root), file = path.join(base, relative), stat = fs.lstatSync(file);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > max || fs.realpathSync(file) !== file) return null;
    return fs.readFileSync(file);
  } catch { return null; }
}

// ---- document reads the harness makes itself -----------------------------------------------------------
// Every function here goes through session.read, which can only issue read-only query.* operations and binds
// each batch to the candidate (cs7-read.cjs). The pairs returned have the {request, response} shape the
// scenario oracles judge. A refused read is kept privately with its fixed reason, then thrown (batch stops).
const OBJECT = 'query.document.object', HISTORY = 'query.document.object.history',
  TIMING = 'query.document.object.layer.timing', COMPONENT = 'query.document.object.component.read';
async function pinRevision(query, status) {
  const { response } = await query(OBJECT, { atRevision: status.contentRevision, view: { kind: 'presentation' } });
  const r = response.result;
  if (response.ok !== true || !plain(r) || r.atRevision !== status.contentRevision || !text(r.documentSnapshotId)) throw new Error('presentation read refused');
  return { atRevision: r.atRevision, documentSnapshotId: r.documentSnapshotId };
}
async function completePage(query, pin, selector) {
  const pair = await query(COMPONENT, { ...pin, selector: { ...selector, cursor: 0, limit: 16 } });
  const r = pair.response.result;
  if (pair.response.ok !== true || !plain(r) || !Array.isArray(r.items) || r.nextCursor !== null) throw new Error('component read is not one complete page');
  return pair;
}
async function guarded(session, record, fn) {
  try {
    const { value, observed } = await session.read(fn);
    if (record) privateEvidence(record.dir, 'reads-' + record.name, { schema: 'nemo.cs7-harness-reads/1', observed });
    return { ...value, observed };
  } catch (error) {
    if (record) try { privateEvidence(record.dir, 'read-refused-' + record.name, { schema: 'nemo.cs7-harness-reads/1', reason: String(error?.message || 'error').slice(0, 200) }); } catch { /* receipt directory unavailable */ }
    throw error;
  }
}
// History plus the cell runs of EVERY layer of each named context, all pinned to one revision/snapshot.
// Returns { history, layers: {contextId: [pair...]}, pages: {contextId: the layers page pair}, observed }.
function readTimeline(session, { contexts, record }) {
  return guarded(session, record, async ({ query, status }) => {
    const pin = await pinRevision(query, status), history = await query(HISTORY, pin), layers = {}, pages = {};
    for (const contextId of contexts) {
      pages[contextId] = await completePage(query, pin, { kind: 'layers', contextId });
      layers[contextId] = [];
      for (const item of pages[contextId].response.result.items) {
        if (!plain(item) || !text(item.layerUid)) throw new Error('a layer item is malformed');
        layers[contextId].push(await query(TIMING, { ...pin, contextId, layerUid: item.layerUid, frame: 0, view: 'cells' }));
      }
    }
    return { history, layers, pages };
  });
}
// History, the contexts page, and the layers page of the scene, pinned to one revision/snapshot.
function readDocument(session, { record } = {}) {
  return guarded(session, record, async ({ query, status }) => {
    const pin = await pinRevision(query, status), history = await query(HISTORY, pin);
    return { history, contexts: await completePage(query, pin, { kind: 'contexts' }), layers: await completePage(query, pin, { kind: 'layers', contextId: 'scene-root' }) };
  });
}

// ---- what the helper's accessibility read and owned-window capture give the harness ------------------------
// The helper reports, per node, its window-relative `frame` {x,y,w,h} in points and its DOM `classes`, and the window's
// own size (`windowSize`). Rectangles and control state therefore come from the application, never from a person.
const finite = value => typeof value === 'number' && Number.isFinite(value);
function windowOf(observed) {
  const w = observed?.windowSize;
  return plain(w) && finite(w.w) && finite(w.h) && w.w > 0 && w.h > 0 ? { w: w.w, h: w.h } : null;
}
function frameOf(node) {
  const f = node?.frame;
  return plain(f) && ['x', 'y', 'w', 'h'].every(key => finite(f[key])) && f.w > 0 && f.h > 0 ? { x: f.x, y: f.y, w: f.w, h: f.h } : null;
}
const labelsOfNode = node => (Array.isArray(node?.labels) ? node.labels.map(String) : []);
const idOfNode = node => node?.identifiers?.resolved || node?.identifier || '';
const hasClass = (node, name) => Array.isArray(node?.classes) && node.classes.includes(name);
const nodeKey = node => [node.role, idOfNode(node), labelsOfNode(node).join('|')].join('\u0000');
function stableFrames(before, after) {
  const index = nodes => { const m = new Map(); for (const n of nodes) { const k = nodeKey(n), f = frameOf(n); if (f) m.set(k, [...(m.get(k) || []), f]); } return m; };
  const a = index(before), b = index(after);
  if (a.size !== b.size) return false;
  for (const [k, list] of a) {
    const other = b.get(k);
    if (!other || other.length !== list.length) return false;
    for (let i = 0; i < list.length; i++) for (const key of ['x', 'y', 'w', 'h']) if (Math.abs(list[i][key] - other[i][key]) > 1) return false;
  }
  return true;
}
// Window points -> pixels of a capture of that window; null when the capture and the window disagree in proportion.
function toPixels(image, win, frame) {
  const sx = image.width / win.w, sy = image.height / win.h;
  if (!(sx > 0) || Math.abs(sx / sy - 1) > 0.02) return null;
  const x = Math.max(0, Math.round(frame.x * sx)), y = Math.max(0, Math.round(frame.y * sy));
  const w = Math.min(image.width - x, Math.round(frame.w * sx)), h = Math.min(image.height - y, Math.round(frame.h * sy));
  return w > 0 && h > 0 ? { x, y, w, h } : null;
}
const unionOf = rects => { const x = Math.min(...rects.map(r => r.x)), y = Math.min(...rects.map(r => r.y)),
  x2 = Math.max(...rects.map(r => r.x + r.w)), y2 = Math.max(...rects.map(r => r.y + r.h)); return { x, y, w: x2 - x, h: y2 - y }; };
// One capture the harness takes and judges itself: observe, capture, read the PNG ONCE (its sha256 is recorded), observe again.
// A capture whose node frames moved between the two observations is reported unstable, never judged.
async function shot(session, reportDirectory, name) {
  const before = await session.input('observe');
  const captured = await session.input('capture', name);
  if (!plain(captured) || captured.capture !== name) return null;
  const bytes = readInside(reportDirectory, name), image = loadPng(bytes);
  const after = await session.input('observe');
  if (!image || !Array.isArray(before?.nodes) || !Array.isArray(after?.nodes) || !before.nodes.every(plain) || !after.nodes.every(plain)) return null;
  const win = windowOf(after);
  return { name, image, sha256: sha256(bytes), bytes: bytes.length, nodes: after.nodes, window: win,
    stable: !!win && same(windowOf(before), win) && stableFrames(before.nodes, after.nodes) };
}
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

// ---- files the runner measures around a step ------------------------------------------------------------
// A saved file is measured by the runner (hash, size, mtime), never by what the operator says; `since` is the time the
// runner made the request, so a file that predates it was not written by the step.
function measure(root, relative) {
  try {
    const bytes = readInside(root, relative);
    if (!bytes) return null;
    const stat = fs.statSync(path.join(fs.realpathSync(root), relative));
    return { sha256: sha256(bytes), size: bytes.length, mtimeMs: stat.mtimeMs, bytes };
  } catch { return null; }
}
function parseJson(bytes) { try { return bytes ? JSON.parse(bytes.toString('utf8')) : null; } catch { return null; } }
// The native document inside a saved project file: the file itself, or the member that is the object document.
function documentOf(json) {
  const seen = new Set();
  const visit = (value, depth) => {
    if (!plain(value) || depth > 4 || seen.has(value)) return null;
    seen.add(value);
    if (typeof value.format === 'string' && value.format.startsWith('nemo.native-object-document') && Array.isArray(value.layers) && int(value.totalFrames)) return value;
    for (const member of Object.values(value)) { const found = visit(member, depth + 1); if (found) return found; }
    return null;
  };
  return visit(json, 0);
}
// Every object anywhere in the file that is a layer record of `uid` (the scene's layers and a component's layers alike).
function layerRecords(json, uid) {
  const out = []; let budget = 200000;
  const visit = value => {
    if (budget-- <= 0 || !value || typeof value !== 'object') return;
    if (Array.isArray(value)) { for (const item of value) visit(item); return; }
    if (value.layerUid === uid && Array.isArray(value.layers) === false && (Object.hasOwn(value, 'timing') || Object.hasOwn(value, 'name') || Object.hasOwn(value, 'visible'))) out.push(value);
    for (const member of Object.values(value)) visit(member);
  };
  visit(json);
  return out;
}
const keyFramesOf = row => row.k.flatMap((kind, i) => (kind === 'key' ? [i] : []));
// The saved file against the verified rows: same layers, same total, and the stored key frames equal the keys read back.
// `rows`: {layerUid: {k, c}} of the context(s) judged; `total`: the scene total. Returns {fail} | {unrun} | {ok, compared}.
function compareSaved(json, { total, sceneRows, rows }) {
  const doc = documentOf(json);
  if (!doc) return { fail: 'the saved file holds no native object document' };
  if (doc.totalFrames !== total) return { fail: 'saved-total-frames-differs' };
  const sceneIds = Object.keys(sceneRows).sort(), fileIds = doc.layers.map(l => l?.layerUid).filter(text).sort();
  if (!same(sceneIds, fileIds)) return { fail: 'saved-layers-differ-from-the-verified-scene-layers' };
  let compared = 0;
  for (const [uid, row] of Object.entries(rows)) {
    const records = layerRecords(json, uid);
    if (!records.length) return { fail: 'saved-file-lacks-layer-' + uid };
    const keys = records.map(r => r.timing?.keys).filter(Array.isArray);
    if (!keys.length) continue;
    const frames = keys[0].map(k => k?.frame);
    if (!frames.every(int)) return { fail: 'saved-timing-keys-malformed-for-' + uid };
    if (!same(frames.slice().sort((a, b) => a - b), keyFramesOf(row))) return { fail: 'saved-keys-differ-from-the-verified-keys-of-' + uid };
    compared++;
  }
  return compared ? { ok: true, compared } : { unrun: 'the saved file stores no timing keys to compare with the verified rows' };
}

// History plus the cell runs of ONE layer (a layer whose timing is unavailable gives a pair the oracle refuses, not a thrown read).
function readLayerCells(session, { contextId, layerUid, record }) {
  return guarded(session, record, async ({ query, status }) => {
    const pin = await pinRevision(query, status), history = await query(HISTORY, pin);
    return { history, cells: await query(TIMING, { ...pin, contextId, layerUid, frame: 0, view: 'cells' }) };
  });
}
// The brush-bearing records (bitmap brush strokes) of one context at one frame, one entry per read, pinned to one revision.
// A refusal (the route is unavailable in this build) is returned as `refused`, never guessed at.
const BITMAP = 'query.document.object.brush.bitmap';
function readBitmapScene(session, { contextId, frame, record }) {
  return guarded(session, record, async ({ query, status }) => {
    const pin = await pinRevision(query, status), entries = []; let total = null, refused = null;
    for (let offset = 0; offset < 16; offset++) {
      const { response } = await query(BITMAP, { action: 'scene', ...pin, contextId, frame, offset, limit: 1 });
      const r = response.result;
      if (response.ok !== true || !plain(r) || !int(r.totalEntries) || !Array.isArray(r.entries)) { refused = String(response.error?.code || response.error?.reason || 'refused').slice(0, 80); break; }
      total = r.totalEntries; entries.push(...r.entries);
      if (r.nextOffset === null || offset + 1 >= total) break;
    }
    return { pin, total, entries, refused };
  });
}

// ---- raw pixels -------------------------------------------------------------------------
const luma = (r, g, b) => 0.2126 * r + 0.7152 * g + 0.0722 * b;
function rectOk(image, rect, minWidth, minHeight) {
  return plain(rect) && ['x', 'y', 'w', 'h'].every(key => int(rect[key])) && rect.w >= minWidth && rect.h >= minHeight &&
    rect.x + rect.w <= image.width && rect.y + rect.h <= image.height;
}
function pixel(image, x, y) {
  const o = (y * image.width + x) * 4;
  return [image.rgba[o], image.rgba[o + 1], image.rgba[o + 2]];
}
// Mean colour of a small box centred on (x, y): robust against one anti-aliased edge pixel.
function meanColour(image, x, y, radius = 2) {
  const sum = [0, 0, 0]; let n = 0;
  for (let j = Math.max(0, y - radius); j <= Math.min(image.height - 1, y + radius); j++) {
    for (let i = Math.max(0, x - radius); i <= Math.min(image.width - 1, x + radius); i++) {
      const p = pixel(image, i, j); sum[0] += p[0]; sum[1] += p[1]; sum[2] += p[2]; n++;
    }
  }
  return sum.map(v => v / n);
}
function rectStats(image, rect) {
  let total = 0, light = 0, n = 0;
  for (let j = rect.y; j < rect.y + rect.h; j++) for (let i = rect.x; i < rect.x + rect.w; i++) {
    const p = pixel(image, i, j), l = luma(p[0], p[1], p[2]); total += l; n++; if (l >= 200) light++;
  }
  return { meanLuma: total / n, lightFraction: light / n };
}
// Count the separate horizontal text bands inside a rect: rows whose pixels differ clearly from the
// rect's median brightness are "ink"; bands are separated by at least `gap` ink-free rows.
function textBands(image, rect, gap) {
  const values = [];
  for (let j = rect.y; j < rect.y + rect.h; j++) for (let i = rect.x; i < rect.x + rect.w; i++) {
    const p = pixel(image, i, j); values.push(luma(p[0], p[1], p[2]));
  }
  const median = values.slice().sort((a, b) => a - b)[values.length >> 1];
  const inkRow = [];
  for (let j = 0; j < rect.h; j++) {
    let ink = 0;
    for (let i = 0; i < rect.w; i++) if (Math.abs(values[j * rect.w + i] - median) > 60) ink++;
    inkRow.push(ink >= 2);
  }
  const bands = []; let start = -1, blank = 0;
  inkRow.forEach((ink, j) => {
    if (ink) { if (start < 0) start = j; blank = 0; }
    else if (start >= 0 && ++blank >= gap) { bands.push([start, j - blank]); start = -1; blank = 0; }
  });
  if (start >= 0) bands.push([start, inkRow.length - 1 - blank]);
  return bands;
}
function loadPng(bytes) {
  try { return bytes && bytes.length > 33 ? decode(bytes) : null; } catch { return null; }
}

module.exports = { PLACEMENT, readTimeline, readDocument, readLayerCells, readBitmapScene, windowOf, frameOf, labelsOfNode, idOfNode, hasClass, stableFrames, toPixels, unionOf, shot,
  measure, parseJson, documentOf, layerRecords, keyFramesOf, compareSaved, plain, int, text, sha256, malformed, rawOnly, privateEvidence, readInside,
  luma, rectOk, pixel, meanColour, rectStats, textBands, loadPng };
