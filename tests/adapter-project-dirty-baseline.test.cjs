'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const { create } = require('../src/js/adapters/project-dirty-baseline.js');

const SAVED = '{"label":"☃\\r\\n","opacity":[20,80]}\r\n';

test('uninitialized baseline is clean without asking a serializer', () => {
  const baseline = create();
  assert.equal(baseline.isDirty(() => { throw Error('must not read'); }), false);
  assert.equal(baseline.isDirtyBytes('edited'), false);
});

test('saved opaque bytes use exact equality, including whitespace and line endings', () => {
  const baseline = create();
  baseline.markSaved(SAVED);
  assert.equal(baseline.isDirty(() => SAVED), false);
  assert.equal(baseline.isDirtyBytes(SAVED), false);
  assert.equal(baseline.isDirtyBytes(SAVED.trim()), true);
  assert.equal(baseline.isDirty(() => SAVED.replace('20,80', '40,80')), true);
  assert.equal(baseline.isDirty(() => SAVED), false, 'undo to saved bytes is clean');
});

test('unavailable serialization stays dirty without clearing the saved baseline', () => {
  const baseline = create();
  baseline.markSaved(SAVED);
  assert.equal(baseline.isDirty(() => { throw Error('native persistence fenced'); }), true);
  assert.equal(baseline.isDirty(() => SAVED), false);
});

test('tab restore retains the pre-import dirty boolean and normalized clean bytes', () => {
  const baseline = create();
  baseline.markSaved('prior');
  baseline.restoreTab(true, () => { throw Error('dirty restore must not serialize'); });
  assert.equal(baseline.isDirtyBytes(SAVED), true);
  assert.equal(baseline.isDirtyBytes(''), false, 'existing dirty-tab sentinel semantics');
  baseline.restoreTab(false, () => SAVED);
  assert.equal(baseline.isDirtyBytes(SAVED), false);
  assert.equal(baseline.isDirtyBytes('old unnormalized bytes'), true);
});

test('failed clean tab snapshot does not replace the previous saved baseline', () => {
  const baseline = create();
  baseline.markSaved(SAVED);
  assert.throws(() => baseline.restoreTab(false, () => { throw Error('read failed'); }), /read failed/);
  assert.equal(baseline.isDirtyBytes(SAVED), false);
});

test('each project has isolated UI metadata; null can reset its comparison', () => {
  const a = create(), b = create();
  a.markSaved(SAVED);
  assert.equal(a.isDirtyBytes('changed'), true);
  assert.equal(b.isDirtyBytes('changed'), false);
  a.markSaved(null);
  assert.equal(a.isDirty(() => { throw Error('must not read'); }), false);
  assert.equal(Object.isFrozen(a), true);
});

test('the production classic-script load exposes the same UI-only contract', () => {
  const context = vm.createContext({});
  vm.runInContext(fs.readFileSync(require.resolve('../src/js/adapters/project-dirty-baseline.js'), 'utf8'), context,
    { filename: require.resolve('../src/js/adapters/project-dirty-baseline.js') });
  const api = context.NemoProjectDirtyBaseline;
  assert.equal(Object.isFrozen(api), true);
  const baseline = api.create();
  baseline.markSaved(SAVED);
  assert.equal(baseline.isDirtyBytes(SAVED), false);
  assert.equal(baseline.isDirtyBytes('changed'), true);
});
