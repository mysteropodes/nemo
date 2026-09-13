'use strict';
// P17 — single-frame SVG export adapter.
//
// Two halves. The pure half drives the adapter through fake ports and pins the
// document shape and the visibility contract. The characterization half runs
// the REAL production `exportFrameSVGString` sliced out of export.js (export.js
// cannot be loaded whole headlessly: it reaches for Paper, state and the DOM),
// with the real adapter module loaded next to it, and compares its output
// against an INDEPENDENT expectation — the pre-P17 inline string assembly,
// reproduced here verbatim from export.js at 2b5375d — so a drift in either
// the adapter or the production binding shows up as a byte difference.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const adapterPath = path.join(root, 'src/js/adapters/export-svg-frame.js');
const adapter = require(adapterPath);
const adapterSource = fs.readFileSync(adapterPath, 'utf8');
const exportSource = fs.readFileSync(path.join(root, 'src/js/export.js'), 'utf8');

// A minimal stand-in for the hidden export Paper layer: records the visibility
// it was in when exportSVG() ran and the bounds it was asked for.
function fakeLayer(inner) {
  const layer = { visible: false, calls: [] };
  layer.exportSVG = function (opts) {
    layer.calls.push({ visibleDuringCall: layer.visible, opts: opts });
    return inner;
  };
  return layer;
}

function FakeRectangle(x, y, w, h) { this.x = x; this.y = y; this.width = w; this.height = h; }

function ports(layer, width, height) {
  const built = [];
  return {
    built,
    buildFrame(frameIdx) { built.push(frameIdx); return layer; },
    dimensions() { return { width: width, height: height }; },
    Rectangle: FakeRectangle,
  };
}

// The exact pre-P17 inline assembly (export.js exportFrameSVGString at
// 2b5375d), kept as the independent oracle for the moved code.
function inlineExpectation(inner, w, h) {
  return '<?xml version="1.0" encoding="UTF-8"?>\n' +
    '<svg xmlns="http://www.w3.org/2000/svg" width="' + w + '" height="' + h + '" ' +
    'viewBox="0 0 ' + w + ' ' + h + '">\n' + inner + '\n</svg>';
}

// ---- pure adapter semantics -----------------------------------------------

test('document wrapper: XML preamble, canvas-sized root and viewBox, inner on its own line', () => {
  const inner = '<g id="frame"><rect x="20" y="60" width="20" height="20" fill="#ff0000"/></g>';
  assert.equal(adapter.svgDocument(inner, 320, 180), inlineExpectation(inner, 320, 180));
  assert.equal(adapter.XML_PREAMBLE, '<?xml version="1.0" encoding="UTF-8"?>\n');
  // Dimensions are interpolated as-is: no rounding, no unit suffix.
  assert.equal(adapter.svgDocument('', 1920.5, 1080), inlineExpectation('', 1920.5, 1080));
});

test('exportFrameSVGString builds the requested frame once and returns the wrapped markup', () => {
  const layer = fakeLayer('<g/>');
  const p = ports(layer, 320, 180);
  assert.equal(adapter.exportFrameSVGString(7, p), inlineExpectation('<g/>', 320, 180));
  assert.deepEqual(p.built, [7]);
  assert.equal(layer.calls.length, 1);
});

test('the hidden export layer is visible only for the exportSVG call itself', () => {
  const layer = fakeLayer('<g/>');
  assert.equal(layer.visible, false);
  adapter.exportFrameSVGString(0, ports(layer, 320, 180));
  assert.equal(layer.calls[0].visibleDuringCall, true, 'exportSVG skips invisible items; the layer must be visible during the call');
  assert.equal(layer.visible, false, 'flipped back off synchronously after the call');
});

test('exportSVG receives asString and a Rectangle port instance covering the whole canvas', () => {
  const layer = fakeLayer('<g/>');
  adapter.exportFrameSVGString(0, ports(layer, 640, 360));
  const opts = layer.calls[0].opts;
  assert.equal(opts.asString, true);
  assert.ok(opts.bounds instanceof FakeRectangle, 'bounds built through the injected Rectangle constructor');
  assert.deepEqual([opts.bounds.x, opts.bounds.y, opts.bounds.width, opts.bounds.height], [0, 0, 640, 360]);
});

test('dimensions are read at call time, so a canvas resize between calls is honored', () => {
  const layer = fakeLayer('<g/>');
  let size = { width: 320, height: 180 };
  const p = { buildFrame() { return layer; }, dimensions() { return size; }, Rectangle: FakeRectangle };
  assert.equal(adapter.exportFrameSVGString(0, p), inlineExpectation('<g/>', 320, 180));
  size = { width: 1280, height: 720 };
  assert.equal(adapter.exportFrameSVGString(0, p), inlineExpectation('<g/>', 1280, 720));
});

test('a throwing exportSVG leaves the layer exactly as the inline code did (visible, no try/finally)', () => {
  const layer = { visible: false, exportSVG() { throw new Error('boom'); } };
  assert.throws(() => adapter.exportFrameSVGString(0, ports(layer, 320, 180)), /boom/);
  // Behaviour-preserving: the pre-P17 code had no finally either. Recorded so
  // that a future "improvement" here is a deliberate, tested change.
  assert.equal(layer.visible, true);
});

// ---- characterization of the real production binding ----------------------

function slice(source, startMarker, endMarker, label) {
  const start = source.indexOf(startMarker);
  assert.notEqual(start, -1, `${label}: start marker not found — the source moved`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(end, -1, `${label}: end marker not found — the source moved`);
  return source.slice(start, end);
}

// Evaluate the real adapter module and the real production function together
// in one context whose only legacy globals are the three the binding reaches
// for: exportBuildFrame, state and Rectangle. Anything else would throw.
function productionContext(layer, canvasW, canvasH) {
  const built = [];
  const context = {
    console, JSON, Object, Array, String, Number, Error,
    state: { canvasW: canvasW, canvasH: canvasH },
    Rectangle: FakeRectangle,
    exportBuildFrame(frameIdx) { built.push(frameIdx); return layer; },
  };
  context.window = context;
  vm.createContext(context);
  vm.runInContext(adapterSource, context, { filename: 'export-svg-frame.js' });
  const fn = slice(exportSource, 'function exportFrameSVGString(frameIdx){', '\nfunction exportDataURLToBytes(', 'exportFrameSVGString');
  const exportFrameSVGString = vm.runInContext(`(${fn})`, context, { filename: 'export-exportFrameSVGString.js' });
  return { exportFrameSVGString, built };
}

test('production exportFrameSVGString delegates to the adapter and is byte-identical to the inline oracle', () => {
  const inner = '<g id="layer-0"><path d="M20,60h20v20h-20z" fill="rgb(255,0,0)" fill-opacity="0.2"/></g>';
  for (const [frame, w, h] of [[0, 320, 180], [10, 320, 180], [20, 1920, 1080]]) {
    const layer = fakeLayer(inner);
    const production = productionContext(layer, w, h);
    const out = production.exportFrameSVGString(frame);
    assert.equal(out, inlineExpectation(inner, w, h));
    assert.deepEqual(production.built, [frame], 'exportBuildFrame is the frame-builder port, called once');
    assert.equal(layer.calls.length, 1);
    assert.equal(layer.calls[0].visibleDuringCall, true);
    assert.equal(layer.visible, false);
    assert.deepEqual([layer.calls[0].opts.bounds.width, layer.calls[0].opts.bounds.height], [w, h]);
  }
});

test('routing guard: the inline assembly did not come back into export.js', () => {
  const fn = slice(exportSource, 'function exportFrameSVGString(frameIdx){', '\nfunction exportDataURLToBytes(', 'exportFrameSVGString');
  assert.match(fn, /NemoExportSvgFrame\.exportFrameSVGString\(frameIdx,\{/);
  assert.doesNotMatch(fn, /<\?xml version/, 'the XML preamble is owned by the adapter now');
  assert.doesNotMatch(fn, /exportSVG\(/, 'the Paper exportSVG call is owned by the adapter now');
  // Exactly one XML preamble in the whole of export.js would mean a second
  // inline assembly; there must be none.
  assert.equal(exportSource.split('<?xml version="1.0" encoding="UTF-8"?>').length, 1);
  // The adapter reads no legacy globals: its only entry points are the ports.
  // (Its header comments name the legacy owners on purpose; only code counts.)
  const adapterCode = adapterSource.split('\n').filter((line) => !/^\s*\/\//.test(line)).join('\n');
  assert.doesNotMatch(adapterCode, /\bstate\.|\bexportBuildFrame\b|new Rectangle\(/);
});
