'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), vm = require('node:vm');
const Registry = require('../src/js/adapters/labs-registry-model.js');
function harness(native, stored = new Map()) {
  const calls = [], area = {style: {filter: ''}};
  const context = {localStorage: {getItem: k => stored.get(k) ?? null, setItem: (k,v) => stored.set(k,String(v))},
    document: {readyState: 'complete', getElementById: () => area}, console, state: {currentFrame: 7}};
  context.window = context;
  context.SMLabs = Registry.create({...context.localStorage, warn() {}, toast() {}, describe: x => x});
  if (native) {
    context.__TAURI__ = {};
    context.NemoNativeObjectProject = {desktop: root => {
      assert.equal(root.state, context.state);
      return {isActive: () => true, renderPreview: frame => calls.push([frame, context.SMLabs.getViewFilter()])};
    }};
  }
  vm.runInNewContext(fs.readFileSync(require.resolve('../src/js/labs/view-filter.js'), 'utf8'), context);
  return {labs: context.SMLabs, calls, area, stored};
}
test('native value check uses ordinary preview, closed presets and restores color without CSS layout effects', () => {
  const h = harness(true);
  assert.equal(h.labs.getViewFilter(), 'none');
  h.labs.enable('view-filter');
  assert.equal(h.labs.getViewFilter(), 'values');
  h.labs.setViewFilter('contrast');
  h.labs.setViewFilter('grayscale'); // existing Settings/API alias
  h.labs.setViewFilter('dim');
  assert.throws(() => h.labs.setViewFilter('blur(2px)'), /unavailable/);
  assert.equal(h.labs.getViewFilter(), 'dim');
  h.labs.disable('view-filter');
  assert.deepEqual(h.calls, [[7,'values'], [7,'contrast'], [7,'values'], [7,'dim'], [7,'none']]);
  assert.equal(h.area.style.filter, '');
});
test('same-data native reload reads the selected filter on first preview without a startup document action', () => {
  const h = harness(true); h.labs.enable('view-filter'); h.labs.setViewFilter('dim');
  const reopened = harness(true, h.stored);
  assert.equal(reopened.labs.getViewFilter(), 'dim');
  assert.deepEqual(reopened.calls, []);
  reopened.labs.disable('view-filter');
  assert.equal(harness(true, h.stored).labs.getViewFilter(), 'none');
});
test('browser prototype preserves CSS presets/custom filters and persisted startup display', () => {
  const h = harness(false); h.labs.enable('view-filter');
  assert.equal(h.area.style.filter, 'grayscale(1)');
  h.labs.setViewFilter('contrast'); assert.equal(harness(false, h.stored).area.style.filter, 'contrast(1.6) saturate(1.1)');
  h.labs.setViewFilter('blur(2px)'); assert.equal(h.area.style.filter, 'blur(2px)');
  h.labs.enable('view-filter'); assert.equal(h.area.style.filter, 'blur(2px)');
  h.labs.disable('view-filter'); assert.equal(h.area.style.filter, '');
});
