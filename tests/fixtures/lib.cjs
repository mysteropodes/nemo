'use strict';
// Builders for deterministic Nemo project fixtures (R03).
//
// Everything here is pure: the same arguments produce byte-identical output,
// so tests/fixtures/generate.cjs can regenerate the corpus and `--check` can
// prove the committed files are exactly what the generator writes.
//
// Shapes follow the persisted document format: the top-level and layer
// fields exportJSON writes (src/js/timeline.js), the stroke dict serP writes
// and desP reads (src/js/app.js), the raster dict images.js writes at import,
// frame records {strokes, isKeyframe, isInterpolated} (app.js), Motion tracks
// {keys:[{frame, v, hOut, hIn, hold?, curvePoints?}]} (src/js/motion.js) and
// the image-mesh store entry (src/js/image-mesh.js). See README.md.
const crypto = require('node:crypto');
const zlib = require('node:zlib');

const SCHEMA_VERSION = 13;

// mulberry32 — a tiny seeded PRNG that behaves identically on every platform.
function rng(seed) {
  let a = seed >>> 0;
  return function next() {
    a = (a + 0x6D2B79F5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const sha256 = (s) => crypto.createHash('sha256').update(s).digest('hex');
const r3 = (n) => Math.round(n * 1000) / 1000;

function seg(x, y, hin, hout) {
  return { point: [r3(x), r3(y)], handleIn: hin ? [r3(hin[0]), r3(hin[1])] : [0, 0], handleOut: hout ? [r3(hout[0]), r3(hout[1])] : [0, 0] };
}

// A stroke dict as serP writes it. A fill-only shape carries strokeColor:null
// and hasRealStroke:false, which desP treats as authoritative (no phantom
// white outline on reload).
function shape(points, opts) {
  opts = opts || {};
  const d = {
    segments: points.map((p) => seg(p[0], p[1], p[2], p[3])),
    closed: opts.closed !== false,
    strokeColor: opts.stroke || null,
    hasRealStroke: !!opts.stroke,
    strokeWidth: opts.width || 2,
    strokeCap: 'round',
    strokeJoin: 'round',
    opacity: opts.opacity === undefined ? 1 : opts.opacity,
  };
  if (opts.fill) d.fillColor = opts.fill;
  if (opts.extra) Object.assign(d, opts.extra);
  return d;
}
function rectPts(x, y, w, h) { return [[x, y], [x + w, y], [x + w, y + h], [x, y + h]]; }
function rect(x, y, w, h, opts) { return shape(rectPts(x, y, w, h), opts); }
function polygon(cx, cy, radius, n, opts) {
  const pts = [];
  for (let i = 0; i < n; i++) { const a = -Math.PI / 2 + (i * 2 * Math.PI) / n; pts.push([cx + radius * Math.cos(a), cy + radius * Math.sin(a)]); }
  return shape(pts, opts);
}
// Circle from four cubic arcs (kappa construction).
function circle(cx, cy, r, opts) {
  const k = 0.5522847498 * r;
  return shape([[cx, cy - r, [-k, 0], [k, 0]], [cx + r, cy, [0, -k], [0, k]], [cx, cy + r, [k, 0], [-k, 0]], [cx - r, cy, [0, k], [0, -k]]], opts);
}
// Axis-aligned bounds of a stroke dict's anchor points (handles ignored).
function bounds(d) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const s of d.segments) { x0 = Math.min(x0, s.point[0]); y0 = Math.min(y0, s.point[1]); x1 = Math.max(x1, s.point[0]); y1 = Math.max(y1, s.point[1]); }
  return [r3(x0), r3(y0), r3(x1 - x0), r3(y1 - y0)];
}

// Raster dict as images.js writes it at import: center x/y, width/height.
function raster(src, cx, cy, w, h, extra) {
  return Object.assign({ isRaster: true, src, x: cx, y: cy, width: w, height: h, opacity: 1 }, extra || {});
}

// ---- a byte-exact PNG so media fixtures need no binary files ---------------
// Stored (level 0) deflate blocks: output does not depend on the zlib
// compressor version, so the fixture hash is stable across Node releases.
const CRC_TABLE = (() => { const t = new Uint32Array(256); for (let n = 0; n < 256; n++) { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xEDB88320 ^ (c >>> 1) : c >>> 1; t[n] = c >>> 0; } return t; })();
function crc32(buf) { let c = 0xFFFFFFFF; for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xFF] ^ (c >>> 8); return (c ^ 0xFFFFFFFF) >>> 0; }
function pngChunk(type, data) {
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
}
// Solid-colour RGBA PNG, w×h, as a data: URL.
function tinyPng(w, h, rgba) {
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 6; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) { raw[y * (w * 4 + 1)] = 0; for (let x = 0; x < w; x++) Buffer.from(rgba).copy(raw, y * (w * 4 + 1) + 1 + x * 4); }
  const png = Buffer.concat([Buffer.from([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]), pngChunk('IHDR', ihdr), pngChunk('IDAT', zlib.deflateSync(raw, { level: 0 })), pngChunk('IEND', Buffer.alloc(0))]);
  return 'data:image/png;base64,' + png.toString('base64');
}

// ---- frames / layers / documents -----------------------------------------
// `keyed` maps frame index → strokes for keyframes; other frames are empty
// non-key frames (getEffectiveStrokes inherits the previous keyframe's array).
function frames(total, keyed) {
  const out = [];
  for (let i = 0; i < total; i++) {
    const ks = keyed && keyed[i];
    if (ks) out.push({ strokes: ks, isKeyframe: true, isInterpolated: false });
    else out.push({ strokes: [], isKeyframe: !keyed && i === 0, isInterpolated: false });
  }
  return out;
}
function slug(s) { return String(s).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, ''); }
function layer(name, fr, extra) {
  return Object.assign({ name, visible: true, locked: false, frames: fr, layerUid: 'L-' + slug(name) }, extra || {});
}
function doc(opts) {
  const total = opts.totalFrames;
  const d = {
    version: SCHEMA_VERSION, totalFrames: total, fps: opts.fps || 12, canvasW: opts.w || 320, canvasH: opts.h || 240, canvasBg: opts.bg || '#ffffff',
    waIn: 0, waOut: total - 1, mediaMode: 'embedded', layers: opts.layers, symbols: opts.symbols || {},
  };
  if (opts.extra) Object.assign(d, opts.extra);
  return d;
}

// ---- Motion ----------------------------------------------------------------
// Waypoints of motion.js's DEFAULT_CURVE (ease in/out) and an exactly linear
// curve in the same five-point form. A key without curvePoints uses the default.
const DEFAULT_CURVE_WAYPOINTS = { 0: 0, 0.25: 0.156, 0.5: 0.5, 0.75: 0.844, 1: 1 };
const LINEAR_CURVE = [{ x: 0, y: 0 }, { x: 0.25, y: 0.25 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.75 }, { x: 1, y: 1 }];
function track(keys) {
  return { keys: keys.map((k) => { const o = { frame: k.frame, v: k.v.slice(), hOut: [0, 0], hIn: [0, 0] }; if (k.hold) o.hold = true; if (k.curve) o.curvePoints = k.curve.map((p) => ({ x: p.x, y: p.y })); return o; }) };
}

// ---- image mesh (src/js/image-mesh.js createMesh/rebuild) ----------------
// Outline of a cols×rows grid, then the interior grid points, all in the
// raster's normalized 0..1 space; triangles by Delaunay over the vertices.
function gridMesh(cols, rows) {
  const outline = [];
  for (let x = 0; x < cols; x++) outline.push([x / cols, 0]);
  for (let y = 0; y < rows; y++) outline.push([1, y / rows]);
  for (let x = cols; x > 0; x--) outline.push([x / cols, 1]);
  for (let y = rows; y > 0; y--) outline.push([0, y / rows]);
  const verts = outline.map((p) => [p[0], p[1]]);
  for (let gy = 1; gy < rows; gy++) for (let gx = 1; gx < cols; gx++) verts.push([gx / cols, gy / rows]);
  const Delaunator = require('../../src/js/delaunator.vendor.js');
  const del = Delaunator.from(verts);
  const tris = Array.from(del.triangles);
  return { outline, cols, rows, verts, tris, offsets: verts.map(() => [0, 0]) };
}

// ---- scale documents (bench; generated on demand, never committed) --------
const PALETTE = ['#1d3557', '#457b9d', '#a8dadc', '#e63946', '#f1faee', '#2a9d8f', '#e9c46a', '#f4a261'];
function scaleDocument(strokeCount, seed) {
  const next = rng(seed);
  const strokes = [];
  for (let i = 0; i < strokeCount; i++) {
    let x = 100 + next() * 1720, y = 100 + next() * 880;
    const pts = [];
    for (let s = 0; s < 8; s++) { pts.push([x, y]); x += (next() - 0.5) * 120; y += (next() - 0.5) * 120; }
    const filled = next() < 0.3;
    strokes.push(shape(pts, { closed: filled, stroke: PALETTE[i % PALETTE.length], width: 1 + Math.floor(next() * 6), fill: filled ? PALETTE[(i + 3) % PALETTE.length] : null, opacity: 1 }));
  }
  return doc({ w: 1920, h: 1080, fps: 24, totalFrames: 24, layers: [layer('Scale ' + strokeCount, frames(24, { 0: strokes }))] });
}

module.exports = { SCHEMA_VERSION, rng, sha256, r3, seg, shape, rect, rectPts, polygon, circle, bounds, raster, tinyPng, frames, layer, doc, track, LINEAR_CURVE, DEFAULT_CURVE_WAYPOINTS, gridMesh, scaleDocument, slug };
