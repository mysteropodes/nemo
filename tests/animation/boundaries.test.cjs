'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const profile = require('../../engineering/boundaries/profiles/app-js.profile.json');
const { checkProfile } = require('../../scripts/nemo/lib/boundaries.cjs');

test('the extracted animation domain obeys size, import and global-state boundaries', () => {
  const modules = profile.modules.filter(module => module.id === 'app.animation.curve');
  assert.equal(modules.length, 1, 'the production kernel must remain explicitly declared');
  assert.equal(modules[0].layer, 'domain');
  const report = checkProfile({ ...profile, modules, exceptions: [] }, {
    root: path.resolve(__dirname, '../..'),
  });
  assert.equal(report.ok, true, JSON.stringify(report.violations));
});
