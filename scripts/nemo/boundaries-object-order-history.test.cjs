'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const R = require('./lib/boundaries-rust.cjs');
const C = require('./lib/boundaries-coverage.cjs');
const root = path.resolve(__dirname, '../..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8');
const profile = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json'));
const policy = JSON.parse(read('engineering/boundaries/profiles/native-engine.edges.json'));
const source = 'native-engine/tests/object_order_history.rs';

test('versioned history stays in one admitted image and canonical host v1 stays separate', () => {
  assert.equal(profile.modules.flatMap(m => m.files.map(f => `${m.dir}/${f}`)).filter(f => f === source).length, 1);
  const omitted = structuredClone(profile);
  for (const module of omitted.modules) module.files = module.files.filter(f => `${module.dir}/${f}` !== source);
  assert.ok(C.checkSourceCoverage(omitted, { root, sourcePaths: [source] }).violations
    .some(v => v.rule === 'coverage-unprofiled-source' && v.file === source));
  assert.match(read('native-engine/tests/object_history.rs'), /#\[path = "object_order_history.rs"\]/);
  assert.match(read('native-engine/src/object_revision.rs'), /document: Arc<ObjectDocument>/);
  assert.match(read('native-engine/src/object_fill.rs'), /snapshot.document\(\).clone\(\)/);
  assert.match(read('native-engine/src/object_snapshot.rs'), /encode_storage_project\(self.document\(\)\)/);
  assert.match(read('native-engine/src/object_codec.rs'), /let wire: LegacyDocument = parse\(bytes\)\?/);
  assert.match(read('src-tauri/src/native_object_bootstrap.rs'), /use native_engine::object_codec::decode_project;/);
  const tooling = JSON.parse(read('engineering/boundaries/profiles/scripts-nemo.profile.json'));
  const owner = tooling.modules.find(m => m.id === 'nemo.test.boundariesRust');
  assert.equal(owner.files.filter(f => f === path.basename(__filename)).length, 1);
  assert.deepEqual(owner.publicApi, ['boundaries-rust.test.cjs']);
});

test('actual native graph and revision-layer authority controls stay unchanged', () => {
  const result = R.checkRustCrate(profile, policy, { root });
  assert.equal(result.ok, true, JSON.stringify(result.violations));
  assert.equal(result.moduleCount, 18); assert.equal(result.edges.length, 44);
  assert.deepEqual(result.unsupported, []); assert.deepEqual(result.exceptionsApplied, []);
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-order-image-boundary-'));
  try {
    fs.cpSync(path.join(root, 'native-engine/src'), path.join(scratch, 'native-engine/src'), { recursive: true });
    fs.appendFileSync(path.join(scratch, 'native-engine/src/object_revision.rs'), '\nuse crate::application::NativeApplication;\n');
    const refused = R.checkRustCrate(profile, policy, { root: scratch });
    assert.equal(refused.ok, false);
    assert.ok(refused.violations.some(v => v.rule === 'layer-violation'
      && v.file === 'native-engine/src/object_revision.rs' && v.detail.targetModule === 'rust.native.engine.application'));
  } finally { fs.rmSync(scratch, { recursive: true, force: true }); }
});
