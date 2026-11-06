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

module.exports = { PLACEMENT, readTimeline, readDocument, plain, int, text, sha256, malformed, rawOnly, privateEvidence, readInside,
  luma, rectOk, pixel, meanColour, rectStats, textBands, loadPng };
