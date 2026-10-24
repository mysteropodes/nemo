'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ROOT = path.resolve(__dirname, '..');
const R = require('../scripts/nemo/lib/boundaries-rust.cjs');
const read = name => JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/boundaries/profiles', name), 'utf8'));

test('U30 native display filters retain private viewport ownership and exact enum ports', () => {
  const profile = read('rust.profile.json'), policy = read('nemo-desktop.edges.json');
  const members = [
    ['rust.desktop.native.viewport', 'native_viewport_display_filter.rs'],
    ['rust.desktop.native.application.tests', 'native_viewport_display_filter_tests.rs'],
    ['rust.desktop.native.host.release.tests', 'native_object_display_filter_tests.rs'],
  ];
  for (const [id, file] of members) {
    const module = profile.modules.find(entry => entry.id === id);
    assert.ok(module.files.includes(file));
    assert.equal(module.publicApi.includes(file), false);
    const removed = structuredClone(profile);
    removed.modules.find(entry => entry.id === id).files = module.files.filter(entry => entry !== file);
    const coverage = require('../scripts/nemo/lib/boundaries-coverage.cjs').checkSourceCoverage(removed, { root: ROOT, sourcePaths: ['src-tauri/src/' + file] });
    assert.ok(coverage.violations.some(v => v.rule === 'coverage-unprofiled-source' && v.file === 'src-tauri/src/' + file));
  }
  const item = 'desktop_viewport::ViewportDisplayFilter';
  for (const id of ['rust.desktop.native.viewport', 'rust.desktop.native.application.commands',
    'rust.desktop.native.dispatch', 'rust.desktop.native.host.release.tests']) {
    const ports = policy.externalCratePorts.native_engine.moduleItems[id];
    assert.ok(ports.includes(item));
    const denied = structuredClone(policy);
    denied.externalCratePorts.native_engine.moduleItems[id] = ports.filter(entry => entry !== item);
    assert.ok(R.checkRustCrate(profile, denied, { root: ROOT }).violations.some(v =>
      v.rule === 'private-port-access' && v.module === id && v.detail.path === item));
  }
});
