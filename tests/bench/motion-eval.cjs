'use strict';
// The REAL evaluators, lifted out of the browser modules with the same
// brace-matching extraction tests/easing-reference.test.cjs uses, so the
// fixture expectations and the bench characterize the code that ships rather
// than a re-implementation:
//   motion.js  rawValueAtFrame (+ trackFor, staticValue, segmentIndexAtFrame,
//              evalCurvePoints and its curve helpers, DEFAULT_CURVE, PROP_DEFAULT)
//   app.js     resolveSymbolFrameIdx (component instance frame mapping)
// Anything that needs the DOM, Paper.js or the expression sandbox is out of
// reach here and stays with the browser/desktop harnesses (R12/R13/R21).
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..', '..');

function sliceFunction(source, marker, file) {
  const start = source.indexOf(marker);
  if (start === -1) throw new Error(marker + ' not found in ' + file);
  // The closing brace sits at the same indentation as the `function` keyword
  // (module IIFE members are indented by two spaces; app.js helpers by none).
  const indent = source.slice(source.lastIndexOf('\n', start) + 1, start);
  const end = source.indexOf('\n' + indent + '}', start) + indent.length + 2;
  return source.slice(start, end);
}
// `var NAME = {…};` / `var NAME = […];` — brace/bracket matched, comments skipped,
// so a literal that spans several lines (PROP_DEFAULT) is taken whole.
function sliceStatement(source, marker, file) {
  const start = source.indexOf(marker);
  if (start === -1) throw new Error(marker + ' not found in ' + file);
  const open = start + marker.length - 1;
  const openCh = source[open], closeCh = openCh === '{' ? '}' : ']';
  let depth = 0;
  for (let i = open; i < source.length; i++) {
    const c = source[i];
    if (c === '/' && source[i + 1] === '/') { i = source.indexOf('\n', i); continue; }
    if (c === '/' && source[i + 1] === '*') { i = source.indexOf('*/', i + 2) + 1; continue; }
    if (c === '\'' || c === '"') { const q = c; for (i++; i < source.length && source[i] !== q; i++) if (source[i] === '\\') i++; continue; }
    if (c === '{' || c === '[') depth++;
    else if (c === '}' || c === ']') { depth--; if (depth === 0) { const semi = source.indexOf(';', i); return source.slice(start, semi + 1); } }
  }
  throw new Error('unbalanced literal for ' + marker + ' in ' + file);
}

let cached = null;
function load() {
  if (cached) return cached;
  const motionFile = 'src/js/motion.js', appFile = 'src/js/app.js';
  const motion = fs.readFileSync(path.join(ROOT, motionFile), 'utf8');
  const app = fs.readFileSync(path.join(ROOT, appFile), 'utf8');
  const pieces = [
    sliceStatement(motion, 'var DEFAULT_CURVE = [', motionFile),
    sliceStatement(motion, 'var PROP_DEFAULT = {', motionFile),
  ];
  for (const m of ['function curveCubicAt(', 'function curveCubicDerivAt(', 'function curveTangentAt(', 'function curveSegCtrl(', 'function curveSegFor(', 'function evalCurvePoints(', 'function segmentIndexAtFrame(', 'function staticValue(', 'function trackFor(', 'function rawValueAtFrame(']) pieces.push(sliceFunction(motion, m, motionFile));
  pieces.push(sliceFunction(app, 'function resolveSymbolFrameIdx(', appFile));
  const sandbox = { window: {}, Math };
  vm.runInNewContext(pieces.join('\n') + '\nthis.api = { rawValueAtFrame, evalCurvePoints, staticValue, trackFor, resolveSymbolFrameIdx, DEFAULT_CURVE, PROP_DEFAULT };', sandbox, { filename: 'nemo-motion-eval.vm.js' });
  cached = sandbox.api;
  return cached;
}

module.exports = { load };
