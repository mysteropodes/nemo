'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const R = require('./lib/boundaries-rust.cjs');
const C = require('./lib/boundaries-coverage.cjs');
const root = path.resolve(__dirname, '../..');
const read = f => fs.readFileSync(path.join(root, f), 'utf8');
const profile = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json'));
const policy = JSON.parse(read('engineering/boundaries/profiles/native-engine.edges.json'));
const paths = ['native-engine/src/object_order_document.rs',
  'native-engine/src/object_order_codec.rs', 'native-engine/tests/object_order_codec.rs'];

test('strict v2 children retain existing facades, graph and exact file census', () => {
  const document = profile.modules.find(m => m.id === 'rust.native.engine.document');
  const codec = profile.modules.find(m => m.id === 'rust.native.engine.codec');
  assert.deepEqual(document.publicApi, ['document.rs', 'object_document.rs']);
  assert.deepEqual(codec.publicApi, ['codec.rs', 'project_structure.rs', 'object_codec.rs']);
  for (const source of paths) {
    assert.equal(profile.modules.flatMap(m => m.files.map(f => `${m.dir}/${f}`)).filter(f => f === source).length, 1);
    const omitted = structuredClone(profile);
    for (const module of omitted.modules) module.files = module.files.filter(f => `${module.dir}/${f}` !== source);
    assert.ok(C.checkSourceCoverage(omitted, { root, sourcePaths: [source] }).violations
      .some(v => v.rule === 'coverage-unprofiled-source' && v.file === source));
  }
  assert.match(read('native-engine/src/object_document.rs'), /#\[path = "object_order_document.rs"\]/);
  assert.match(read('native-engine/src/object_codec.rs'), /#\[path = "object_order_codec.rs"\]/);
  assert.match(read('native-engine/tests/object_codec.rs'), /#\[path = "object_order_codec.rs"\]/);
  const tooling = JSON.parse(read('engineering/boundaries/profiles/scripts-nemo.profile.json'));
  const owner = tooling.modules.find(m => m.id === 'nemo.test.boundariesRust');
  assert.equal(owner.files.filter(f => f === path.basename(__filename)).length, 1);
  assert.deepEqual(owner.publicApi, ['boundaries-rust.test.cjs']);
});

test('adopted native graph stays exact and codec cannot import application authority', () => {
  const result = R.checkRustCrate(profile, policy, { root });
  assert.equal(result.ok, true, JSON.stringify(result.violations));
  assert.equal(result.moduleCount, 18);
  assert.equal(result.edges.length, 50);
  assert.deepEqual(result.unsupported, []);
  assert.deepEqual(result.exceptionsApplied, []);
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-order-boundary-'));
  try {
    fs.cpSync(path.join(root, 'native-engine/src'), path.join(scratch, 'native-engine/src'), { recursive: true });
    fs.appendFileSync(path.join(scratch, paths[1]), '\nuse crate::application::NativeApplication;\n');
    const refused = R.checkRustCrate(profile, policy, { root: scratch });
    assert.equal(refused.ok, false);
    assert.ok(refused.violations.some(v => v.rule === 'layer-violation' && v.file === paths[1]
      && v.detail.targetModule === 'rust.native.engine.application'));
  } finally { fs.rmSync(scratch, { recursive: true, force: true }); }
});
