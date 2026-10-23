'use strict';
const assert = require('node:assert/strict');
const test = require('node:test');
const core = require('../src/js/adapters/native-media-export-core.js');
const contract = require('../src/js/adapters/native-media-export-contract.js');
const { oracle, names, receipts } = require('./native-media-export-support.cjs');

test('file names are the baseline pad4 / frame_%04d rule, 1-based and never truncated (oracle)', () => {
  assert.equal(oracle.baselineCommit, 'fbe4b8a33e6d5a229f014154df2ba21988a9fabc');
  for (const [n, text] of oracle.pad4) assert.equal(core.pad4(n), text, `pad4(${n})`);
  for (const [ordinal, name] of oracle.svg) assert.equal(core.frameName(ordinal, 'svg'), name);
  for (const row of oracle.png) {
    const count = row.names.length;
    assert.deepEqual(names(count, 'png'), row.names);
    assert.deepEqual(Array.from({ length: count }, (_, i) => core.frameName(i + 1, 'png')), row.names);
  }
  assert.equal(core.frameName(10000, 'svg'), 'frame_10000.svg');
  assert.equal(core.frameName(1, 'png'), 'frame_0001.png');
  assert.equal(oracle.png.find((row) => row.start === 7 && row.end === 6).names.length, 0, 'the baseline PNG loop writes nothing for start > end');
});

test('the range rule is the baseline invalid_range condition for every oracle row', () => {
  assert.equal(oracle.ruleCondition, '!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end < start');
  for (const row of oracle.rule) {
    const num = (v) => (v === 'NaN' ? NaN : v);
    const payload = {
      documentSnapshotId: 'snap', contextId: 'scene-root', outputHandle: 'output/a',
      output: { kind: 'video', frameFormat: 'png' }, range: { start: num(row.start), end: num(row.end) },
      outputSpec: { ...contract.FIXED_SPEC, width: 64, height: 36 },
    };
    let refused = false;
    try { contract.payload(contract.MEDIA_BEGIN, payload); } catch (error) { refused = true; assert.ok(error instanceof TypeError); }
    assert.equal(refused, row.refused, JSON.stringify(row));
  }
});

test('rejectRaw refuses pixels, binary, callbacks, accessors, cycles and foreign objects, and admits plain JSON', () => {
  const cyclic = {}; cyclic.self = cyclic;
  const accessor = {}; Object.defineProperty(accessor, 'x', { enumerable: true, get: () => 1 });
  class Handle { constructor() { this.id = 1; } }
  for (const bad of [
    'data:image/png;base64,AAAA', 'blob:https://x/1', 'DATA:text/plain,x', { a: ['data:,x'] }, new Uint8Array(2), new ArrayBuffer(2),
    Buffer.from('x'), () => 1, accessor, cyclic, new Handle(), { canvas: {} }, { getContext: 1 }, Symbol('s'), 1n,
  ]) assert.throws(() => core.rejectRaw(bad), TypeError, String(bad && bad.constructor && bad.constructor.name));
  assert.doesNotThrow(() => core.rejectRaw({ a: [1, 'two', null, true, { b: undefined }], c: 'output/handle' }));
  assert.doesNotThrow(() => core.rejectRaw(Object.create(null)));
});

test('exact refuses unknown, missing and accessor members and non-plain values', () => {
  assert.throws(() => core.exact({ a: 1, b: 2 }, ['a'], [], 'x'), /missing or unknown/);
  assert.throws(() => core.exact({}, ['a'], [], 'x'), /missing or unknown/);
  assert.throws(() => core.exact([], [], [], 'x'), /plain object/);
  assert.throws(() => core.exact(new Map(), [], [], 'x'), /plain object/);
  const accessor = {}; Object.defineProperty(accessor, 'a', { enumerable: true, get: () => 1 });
  assert.throws(() => core.exact(accessor, ['a'], [], 'x'), /accessor/);
  assert.equal(core.exact({ a: 1 }, ['a'], ['b'], 'x').a, 1);
});

test('identifiers and revisions are bounded; frames are u32', () => {
  for (const bad of ['', ' a', '-a', 'a b', 'a'.repeat(129), 7, null]) assert.throws(() => core.id(bad, 'x'), TypeError, String(bad));
  assert.equal(core.id('a'.repeat(128), 'x').length, 128);
  assert.equal(core.id('output/a:1.2-3_', 'x'), 'output/a:1.2-3_');
  for (const bad of [-1, 1.5, NaN, 2 ** 53, '1']) assert.throws(() => core.revision(bad, 'x'), TypeError);
  assert.equal(core.revision(2 ** 53 - 1, 'x'), 2 ** 53 - 1);
  assert.equal(core.frame(4294967295, 'x'), 4294967295);
  assert.throws(() => core.frame(4294967296, 'x'), /u32/);
});

test('an oversized staged request is refused before it can be minted', () => {
  const current = core.identity({ instanceId: 'i', documentId: 'd', contentRevision: 0 });
  const small = core.request(current, 'r-1', 'job.export.svg.begin', { frames: [0] }, 0);
  assert.ok(Object.isFrozen(small) && Object.isFrozen(small.payload));
  assert.equal(small.expectedRevision, 0);
  assert.equal(core.request(current, 'r-2', 'job.export.svg.status', { jobId: 'j' }).expectedRevision, undefined);
  assert.throws(() => core.request(current, 'r-3', 'job.export.svg.begin', { frames: Array.from({ length: 2000 }, (_, i) => 100000 + i) }, 0), RangeError);
});

test('error details are a closed record: seven typed reasons and a u32 frame', () => {
  assert.deepEqual([...core.REASONS], ['link_missing', 'link_permission', 'media_facts_unavailable', 'media_placement_refused', 'media_plan_invalid', 'media_source_frame_invalid', 'media_decoder_unavailable']);
  for (const reason of core.REASONS) {
    const parsed = core.error({ code: 'not_found', message: 'm', details: { reason, frame: 12 } }, core.commonErrors, 'x');
    assert.deepEqual(parsed.details, { reason, frame: 12 });
    assert.ok(Object.isFrozen(parsed) && Object.isFrozen(parsed.details));
  }
  assert.deepEqual(core.error({ code: 'unavailable', message: 'm', details: {} }, core.commonErrors, 'x').details, {});
  for (const bad of [{ reason: 'other' }, { reason: 'link_missing', path: '/x' }, { frame: -1 }, { frame: 4294967296 }, { reason: 7 }]) {
    assert.throws(() => core.error({ code: 'not_found', message: 'm', details: bad }, core.commonErrors, 'x'), TypeError, JSON.stringify(bad));
  }
  assert.throws(() => core.error({ code: 'frame_failed', message: 'm' }, core.commonErrors, 'x'), /not admitted/);
  assert.throws(() => core.error({ code: 'not_found', message: 4 }, core.commonErrors, 'x'), TypeError);
});

const ok = (value) => core.terminalInvariant(core.receipt(value));
const bad = (value, pattern, label) => assert.throws(() => ok(value), pattern, label);

test('terminal invariants: running', () => {
  ok(receipts.running());
  bad(receipts.running({ artifact: { target: 'output/a', files: [] } }), /running job/);
  bad(receipts.running({ cleanup: { status: 'complete' } }), /running job/);
  bad(receipts.running({ cleanup: { status: 'not_required' } }), /running job/);
  bad(receipts.running({ externalEffectDisposition: 'committed' }), /running job/);
  bad(receipts.running({ error: { code: 'frame_failed', message: 'm' } }), /running job/);
});

test('terminal invariants: succeeded', () => {
  ok(receipts.succeeded(names(2, 'svg')));
  bad(receipts.succeeded(names(2, 'svg'), { progress: 0.99 }), /successful job/);
  bad(receipts.succeeded(names(2, 'svg'), { artifact: null }), /successful job/);
  bad(receipts.succeeded(names(2, 'svg'), { cleanup: { status: 'pending' } }), /successful job/);
  bad(receipts.succeeded(names(2, 'svg'), { cleanup: { status: 'not_required' } }), /successful job/);
  bad(receipts.succeeded(names(2, 'svg'), { externalEffectDisposition: 'contained' }), /successful job/);
  bad(receipts.succeeded(names(2, 'svg'), { externalEffectDisposition: 'none' }), /successful job/);
  bad(receipts.succeeded(names(2, 'svg'), { error: { code: 'frame_failed', message: 'm' } }), /successful job/);
});

test('terminal invariants: failed and cancelled', () => {
  ok(receipts.failed());
  bad(receipts.failed({ error: undefined }), /failed or cancelled/);
  bad(receipts.failed({ artifact: { target: 'output/a', files: [] } }), /failed or cancelled/);
  bad(receipts.failed({ cleanup: { status: 'pending' } }), /failed or cancelled/);
  bad(receipts.failed({ cleanup: { status: 'not_required' } }), /cleanup must be complete or failed/);
  bad(receipts.failed({ externalEffectDisposition: 'committed' }), /contained failed job/);
  bad(receipts.failed({ externalEffectDisposition: 'contained' }), /contained failed job/);
  bad(receipts.failed({ externalEffectDisposition: 'indeterminate' }), /contained failed job/);
  const debt = { code: 'cleanup_failed', message: 'debt' };
  ok(receipts.failed({ cleanup: { status: 'failed', error: debt }, externalEffectDisposition: 'indeterminate', error: debt }));
  bad(receipts.failed({ cleanup: { status: 'failed', error: debt }, externalEffectDisposition: 'none', error: debt }), /cleanup_failed requires/);
  bad(receipts.failed({ cleanup: { status: 'failed', error: debt }, externalEffectDisposition: 'indeterminate' }), /cleanup_failed requires/);
  bad(receipts.failed({ cleanup: { status: 'complete' }, externalEffectDisposition: 'indeterminate', error: debt }), /cleanup_failed requires/);
  bad(receipts.failed({ cleanup: { status: 'failed', error: { code: 'write_failed', message: 'm' } }, externalEffectDisposition: 'indeterminate', error: debt }), /cleanup_failed requires/);
  ok(receipts.cancelled());
  ok(receipts.cancelled({ error: { code: 'document_replaced', message: 'replaced' } }));
  bad(receipts.cancelled({ error: { code: 'frame_failed', message: 'm' } }), /cancelled job requires/);
  bad(receipts.cancelled({ cleanup: { status: 'pending' } }), /failed or cancelled/);
  bad(receipts.cancelled({ cleanup: { status: 'failed', error: debt }, externalEffectDisposition: 'indeterminate', error: debt }), /cancelled job requires/);
  bad(receipts.cancelled({ externalEffectDisposition: 'committed' }), /cancelled job requires/);
  bad(receipts.cancelled({ artifact: { target: 'output/a', files: [] } }), /failed or cancelled/);
});

test('the receipt shape is closed and frozen', () => {
  const parsed = core.receipt(receipts.succeeded(names(2, 'svg')));
  assert.ok(Object.isFrozen(parsed) && Object.isFrozen(parsed.artifact) && Object.isFrozen(parsed.artifact.files) && Object.isFrozen(parsed.cleanup));
  assert.deepEqual(parsed.artifact.files, ['frame_0001.svg', 'frame_0002.svg']);
  const source = receipts.succeeded(names(1, 'svg'));
  core.receipt(source);
  source.artifact.files.push('late.svg');
  assert.equal(core.receipt(receipts.succeeded(names(1, 'svg'))).artifact.files.length, 1, 'receipts never alias the wire value');
  for (const [label, value] of [
    ['unknown member', receipts.running({ extra: 1 })],
    ['missing member', (() => { const r = receipts.running(); delete r.cleanup; return r; })()],
    ['progress above 1', receipts.running({ progress: 1.1 })],
    ['progress NaN', receipts.running({ progress: NaN })],
    ['progress string', receipts.running({ progress: '0' })],
    ['unknown status', receipts.running({ status: 'queued' })],
    ['unknown disposition', receipts.running({ externalEffectDisposition: 'maybe' })],
    ['unknown cleanup status', receipts.running({ cleanup: { status: 'later' } })],
    ['cleanup error without failure', receipts.running({ cleanup: { status: 'pending', error: { code: 'cleanup_failed', message: 'm' } } })],
    ['file not a string', receipts.succeeded([1])],
    ['artifact extra member', receipts.succeeded(['a'], { artifact: { target: 'output/a', files: [], bytes: 3 } })],
    ['common code as job error', receipts.failed({ error: { code: 'unavailable', message: 'm' } })],
    ['bad job id', receipts.running({ jobId: 'job id' })],
    ['negative pinned revision', receipts.running({ pinnedRevision: -1 })],
  ]) assert.throws(() => core.receipt(value), TypeError, label);
});

test('the published files are checked against the plan, never against the encoder', () => {
  const svg = { kind: 'frames', ext: 'svg', count: 3 };
  core.checkArtifact(svg, names(3, 'svg'));
  assert.throws(() => core.checkArtifact(svg, names(2, 'svg')), /count/);
  assert.throws(() => core.checkArtifact(svg, names(4, 'svg')), /count/);
  assert.throws(() => core.checkArtifact(svg, names(3, 'png')), /names/);
  assert.throws(() => core.checkArtifact(svg, ['frame_0001.svg', 'frame_0003.svg', 'frame_0002.svg']), /names/);
  assert.throws(() => core.checkArtifact(svg, ['frame_0000.svg', 'frame_0001.svg', 'frame_0002.svg']), /names/);
  assert.throws(() => core.checkArtifact(svg, ['frame_1.svg', 'frame_2.svg', 'frame_3.svg']), /names/);
  assert.throws(() => core.checkArtifact(svg, ['/etc/a.svg', 'frame_0002.svg', 'frame_0003.svg']), /names/);
  assert.throws(() => core.checkArtifact({ kind: 'frames', ext: 'png', count: 0 }, []), /1..120/);
  assert.throws(() => core.checkArtifact({ kind: 'frames', ext: 'png', count: null }, names(121, 'png')), /1..120/);
  core.checkArtifact({ kind: 'frames', ext: 'png', count: null }, names(120, 'png'));
  core.checkArtifact({ kind: 'frames', ext: 'png', count: null }, names(1, 'png'));
  assert.throws(() => core.checkArtifact({ kind: 'frames', ext: 'png', count: null }, ['frame_0002.png']), /names/);
  core.checkArtifact({ kind: 'video' }, ['out.mp4']);
  assert.throws(() => core.checkArtifact({ kind: 'video' }, []), /exactly one/);
  assert.throws(() => core.checkArtifact({ kind: 'video' }, ['a.mp4', 'b.mp4']), /exactly one/);
});

test('an absent host route is typed unavailability with a stable code', () => {
  const failure = core.unavailable(core.UNAVAILABLE_REASON);
  assert.equal(failure.name, 'NativeExportUnavailable');
  assert.equal(failure.code, 'unavailable');
  assert.equal(failure.reason, 'export_route_unavailable');
  assert.ok(failure instanceof Error);
});
