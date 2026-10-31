'use strict';
// #2356: the Paper node writer (vertex/tangent drag, nudge/scale/rotate, insert, point type, Delete) is retired; Direct Select is native-only.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ROOT = path.join(__dirname, '..'), src = (f) => fs.readFileSync(path.join(ROOT, f), 'utf8');
const tools = src('src/js/tools.js');
function fn(name) { const i = tools.indexOf(`function ${name}(`); assert.ok(i >= 0, name); let d = 0, j = tools.indexOf('{', i); for (let k = j; ; k++) { if (tools[k] === '{') d++; if (tools[k] === '}' && --d === 0) return tools.slice(i, k + 1); } }
function load(name, ctx) { vm.runInNewContext(fn(name), ctx); return ctx[name]; }
const point = (x, y) => ({ x, y });
const segs = () => [{ point: point(0, 0), handleIn: point(0, 0), handleOut: point(1, 1) }, { point: point(10, 0), handleIn: point(-1, 0), handleOut: point(1, 0) }];
test('with no native project open (legacy guard admits everything) Direct Select is still never admitted to Paper', () => {
  const ctx = { window: null, console }; ctx.window = ctx; ctx.SMEngineBridge = { nativeEditGuard: { allow: () => true } };
  const allow = load('allowLegacySelectionEdit', ctx);
  let stopped = 0; const e = { stopImmediatePropagation() { stopped++; }, preventDefault() {} };
  assert.equal(allow(null, 'select'), true, 'other tools keep their own guard');
  assert.equal(allow(e, 'subselect'), false); assert.equal(stopped, 1, 'the gesture is consumed');
  assert.equal(allow(null, 'subselect'), false);
});
test('former node-editing entries leave Paper segments byte-identical and never push undo', () => {
  const ctx = { window: null, console, SMEngineBridge: { nativeEditGuard: { allow: () => true } }, Point: function (x, y) { return point(x, y); } }; ctx.window = ctx;
  const p = { segments: segs(), closed: false, data: {}, getNearestLocation() { throw new Error('Paper must not run'); }, divideAt() { throw new Error('Paper must not run'); }, removeSegment() { throw new Error('Paper must not run'); } };
  let undo = 0; Object.assign(ctx, { pushUndo() { undo++; }, nodeEditTargetPath: () => p, _nodeSel: [0, 1], state: {}, showToast() {} });
  load('allowLegacySelectionEdit', ctx); for (const n of ['nodeSelApplyMove', 'nodeSelApplyScale', 'nodeSelApplyRotate', 'insertVertexAt', 'setPointType']) load(n, ctx);
  const before = JSON.stringify(p.segments);
  ctx.nodeSelApplyMove(5, 5); ctx.nodeSelApplyScale(2, 2, point(0, 0)); ctx.nodeSelApplyRotate(30, point(0, 0)); assert.equal(ctx.insertVertexAt(p, point(5, 0)), -1);
  assert.equal(JSON.stringify(p.segments), before); assert.equal(undo, 0);
});
test('setPointType only ever reaches the native controller, or reports unavailable', () => {
  const sent = [], toasts = []; const ctx = { window: null, console, showToast: (m) => toasts.push(m), pushUndo() { throw new Error('legacy undo'); }, _nodeSel: [0] }; ctx.window = ctx;
  const set = load('setPointType', ctx); set('smooth'); assert.equal(toasts.length, 1);
  ctx.NemoNativeSubselectController = { setPointType: (t) => sent.push(t) }; set('corner'); assert.deepEqual(sent, ['corner']);
});
test('no Paper vertex writer remains reachable: Delete branch and drag handlers are refusal-gated', () => {
  const tl = src('src/js/timeline.js'); assert.ok(!/ntp\.removeSegment|centerSegments\.splice/.test(tl), 'timeline Delete vertex writer removed');
  assert.match(tools, /if\(\(state\.tool==='select'\|\|state\.tool==='subselect'\|\|state\.tool==='fsselect'\)&&!allowLegacySelectionEdit\(event\.event,state\.tool\)\)return;/);
  assert.match(tools, /if\(selectionGestureActive\(\)&&!allowLegacySelectionEdit\(event\.event,state\.tool\)\)return;/);
});
